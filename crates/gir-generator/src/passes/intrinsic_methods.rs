use std::rc::Rc;

use smol_str::SmolStr;
use syntax::kind::SyntaxKind;

use crate::{eat, GIRGenerator};
use common::MutRc;
use gir_nodes::{
    declaration::ADTType,
    expression::CastType,
    types::{ToInstance, TypeKind},
    Expr, Function, Type, ADT,
};
use std::collections::HashMap;

use super::declare::FnSig;
use gir_nodes::declaration::Visibility;

impl GIRGenerator {
    /// Declare lifecycle methods on ADTs.
    pub(super) fn declare_lifecycle_methods(&mut self, adt: &MutRc<ADT>) {
        let type_params = Rc::clone(&adt.borrow().type_parameters);
        let this_param = (SmolStr::new_inline("this"), Type::Adt(adt.to_inst()));
        let is_value = adt.borrow().type_kind == TypeKind::Value;

        // TODO: Replace this vec with an array once array.into_iter() is stabilized
        let mut fns: Vec<_> = vec![
            FnSig {
                name: "new-instance".into(),
                visibility: Visibility::Private,
                params: box vec![this_param.clone()].into_iter().map(Ok),
                type_parameters: type_params.clone(),
                ret_type: None,
                ast: None,
            },
            FnSig {
                name: "free-instance".into(),
                visibility: Visibility::Private,
                params: if is_value {
                    box vec![this_param.clone()].into_iter().map(Ok)
                } else {
                    box vec![this_param.clone(), ("refcount_zero".into(), Type::Bool)]
                        .into_iter()
                        .map(Ok)
                },
                type_parameters: type_params.clone(),
                ret_type: None,
                ast: None,
            },
        ];

        if is_value {
            fns.push(FnSig {
                name: "copy-instance".into(),
                visibility: Visibility::Private,
                params: box vec![this_param].into_iter().map(Ok),
                type_parameters: type_params,
                ret_type: None,
                ast: None,
            })
        }

        for func in fns.into_iter() {
            let name = func.name.clone();
            let gir_fn = eat!(self, self.create_function(func));
            adt.borrow_mut()
                .methods
                .insert(name.clone(), Rc::clone(&gir_fn));
        }
    }

    /// Generate the lifecycle methods on ADTs. This is not immediately done after
    /// declaring them since some of them depend on them being declared on other
    /// types, most notably enum parents need children to have them declared during generation.
    pub(super) fn generate_lifecycle_methods(&mut self, adt: &MutRc<ADT>) {
        let is_value = adt.borrow().type_kind == TypeKind::Value;

        // TODO: Replace this vec with an array once array.into_iter() is stabilized
        let mut fns: Vec<(_, fn(&mut _, &_, _, _))> = vec![
            ("new-instance", Self::generate_instantiator),
            ("free-instance", Self::generate_destructor),
        ];

        if is_value {
            fns.push(("copy-instance", Self::generate_copier));
        }

        for (name, generator) in fns.into_iter() {
            generator(self, adt, Rc::clone(&adt.borrow().methods[name]), is_value);
        }
    }

    fn generate_instantiator(&mut self, adt: &MutRc<ADT>, func: MutRc<Function>, _is_value: bool) {
        self.set_pointer(&func);
        let var = Rc::clone(&func.borrow().parameters[0]);

        for field in adt.borrow().fields.values() {
            if let Some(init) = field.initializer.take() {
                self.insert_at_ptr(Expr::store(Expr::load(Expr::lvar(&var), field), init, true))
            }
        }

        // Insert at the end of the instantiator to prevent
        // an edge case of an empty instantiator, which IR would interpret
        // incorrectly as an external function
        self.insert_at_ptr(Expr::none_const())
    }

    fn generate_destructor(&mut self, adt: &MutRc<ADT>, func: MutRc<Function>, is_value: bool) {
        self.set_pointer(&func);
        let adt_var = Rc::clone(&func.borrow().parameters[0]);

        let mut exprs = Vec::with_capacity(4);

        let maybe_free = self.maybe_call_free_impl(Expr::lvar(&adt_var));
        exprs.push(maybe_free);

        match &adt.borrow().ty {
            // Interface, TODO
            ADTType::Interface => (),

            ADTType::Enum { cases } => {
                let dest = Self::build_enum_destructor(Expr::lvar(&adt_var), cases);
                exprs.push(dest);
            }

            _ => {
                // Generic; class or enum case, just decrementing all refs is enough
                for field in adt.borrow().fields.values().filter(|f| f.ty.is_adt()) {
                    exprs.push(Expr::dec_rc(Expr::load(Expr::lvar(&adt_var), field)));
                }
            }
        }

        if is_value {
            self.insert_at_ptr(Expr::Block(exprs));
        } else {
            let dealloc_cond = Rc::clone(&func.borrow().parameters[1]);
            exprs.push(Expr::free(Expr::lvar(&adt_var)));

            self.insert_at_ptr(Expr::if_(
                Expr::lvar(&dealloc_cond),
                Expr::Block(exprs),
                Expr::none_const(),
                None,
            ));
        }
    }

    fn generate_copier(&mut self, adt: &MutRc<ADT>, func: MutRc<Function>, is_value: bool) {
        assert!(is_value);
        self.set_pointer(&func);
        let adt_var = Rc::clone(&func.borrow().parameters[0]);

        for field in adt.borrow().fields.values().filter(|f| f.ty.is_adt()) {
            self.insert_at_ptr(Expr::inc_rc(Expr::load(Expr::lvar(&adt_var), field)));
        }
    }

    fn maybe_call_free_impl(&mut self, adt: Expr) -> Expr {
        if self.flags.no_std {
            return Expr::none_const();
        }

        let ty = adt.get_type();
        let free_iface = self.intrinsics.free_iface.clone().unwrap();
        let impls = self.get_iface_impls(&ty);
        let free_method = impls
            .borrow()
            .interfaces
            .get(&free_iface.to_type())
            .map(|iface| iface.methods.values().next().map(Rc::clone))
            .flatten();
        if let Some(method) = free_method {
            Expr::call(Expr::fvar(&method), vec![adt])
        } else {
            Expr::none_const()
        }
    }

    /// Builds an enum destructor. Instead of just decrementing all
    /// members, an enum destructor must instead switch on itself
    /// to figure out which case destructor to call.
    fn build_enum_destructor(enu: Expr, cases: &HashMap<SmolStr, MutRc<ADT>>) -> Expr {
        let mut when_brs = Vec::with_capacity(cases.len());
        for case in cases.values() {
            let case_ty = case.to_type();
            let then = Expr::call(
                Expr::fvar(&case.borrow().methods["free-instance"]),
                vec![Expr::cast(enu.clone(), case_ty.clone(), CastType::Bitcast)],
            );
            let cond = Expr::binary(SyntaxKind::Is, enu.clone(), Expr::TypeGet(case_ty));
            when_brs.push((cond, then))
        }
        Expr::switch(when_brs, Expr::none_const(), None)
    }
}
