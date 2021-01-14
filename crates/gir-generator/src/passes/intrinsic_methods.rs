use std::rc::Rc;

use smol_str::SmolStr;
use syntax::kind::SyntaxKind;

use crate::{eat, GIRGenerator};
use common::MutRc;
use gir_nodes::{
    declaration::ADTType, expression::CastType, types::ToInstance, Expr, Function, Type, ADT,
};
use std::collections::HashMap;

use super::declare::FnSig;
use gir_nodes::declaration::Visibility;

impl GIRGenerator {
    /// Declare lifecycle methods on ADTs.
    pub(super) fn declare_lifecycle_methods(&mut self, adt: &MutRc<ADT>) {
        let type_params = Rc::clone(&adt.borrow().type_parameters);
        let this_param = (SmolStr::new_inline("this"), Type::WeakRef(adt.to_inst()));
        let this_param_sr = (this_param.0.clone(), this_param.1.to_strong());

        // TODO: Replace this vec with an array once array.into_iter() is stabilized
        let fns: Vec<_> = vec![
            FnSig {
                name: "new-instance".into(),
                visibility: Visibility::Private,
                params: box vec![this_param.clone()].into_iter().map(Ok),
                type_parameters: type_params.clone(),
                ret_type: None,
                ast: None,
            },
            FnSig {
                name: "free-wr".into(),
                visibility: Visibility::Private,
                params: box vec![this_param].into_iter().map(Ok),
                type_parameters: type_params.clone(),
                ret_type: None,
                ast: None,
            },
            FnSig {
                name: "free-sr".into(),
                visibility: Visibility::Private,
                params: box vec![this_param_sr, ("refcount_zero".into(), Type::Bool)]
                    .into_iter()
                    .map(Ok),
                type_parameters: type_params,
                ret_type: None,
                ast: None,
            },
        ];

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
        // TODO: Replace this vec with an array once array.into_iter() is stabilized
        let fns: Vec<(_, fn(&mut _, &_, _))> = vec![
            ("new-instance", Self::generate_instantiator),
            ("free-wr", Self::generate_wr_destructor),
            ("free-sr", Self::generate_sr_destructor),
        ];

        for (name, generator) in fns.into_iter() {
            generator(self, adt, Rc::clone(&adt.borrow().methods[name]));
        }
    }

    fn generate_instantiator(&mut self, adt: &MutRc<ADT>, func: MutRc<Function>) {
        self.set_pointer(&func);
        let var = Rc::clone(&func.borrow().parameters[0]);

        for field in adt.borrow().fields.values() {
            if let Some(init) = field.initializer.take() {
                self.insert_at_ptr(Expr::store(
                    Expr::load(Expr::lvar(&var), field),
                    init,
                    true,
                ))
            }
        }

        // Insert at the end of the instantiator to prevent
        // an edge case of an empty instantiator, which IR would interpret
        // incorrectly as an external function
        self.insert_at_ptr(Expr::none_const())
    }

    fn generate_wr_destructor(&mut self, adt: &MutRc<ADT>, func: MutRc<Function>) {
        self.set_pointer(&func);
        let adt_var = Rc::clone(&func.borrow().parameters[0]);

        let maybe_free = self.maybe_call_free_impl(Expr::lvar(&adt_var));
        self.insert_at_ptr(maybe_free);

        match &adt.borrow().ty {
            // Interface, TODO
            ADTType::Interface => (),

            ADTType::Enum { cases } => {
                let dest = Self::build_enum_destructor(Expr::lvar(&adt_var), cases);
                self.insert_at_ptr(dest);
            }

            _ => {
                // Generic; class or enum case, just decrementing all refs is enough
                for field in adt
                    .borrow()
                    .fields
                    .values()
                    .filter(|f| f.ty.is_strong_ref())
                {
                    self.insert_at_ptr(Expr::dec_rc(Expr::load(Expr::lvar(&adt_var), field)));
                }
            }
        }
    }

    fn generate_sr_destructor(&mut self, adt: &MutRc<ADT>, func: MutRc<Function>) {
        self.set_pointer(&func);
        let adt_var = Rc::clone(&func.borrow().parameters[0]);
        let dealloc_cond = Rc::clone(&func.borrow().parameters[1]);

        let mut if_free_exprs = Vec::with_capacity(adt.borrow().fields.len() + 5);
        if_free_exprs.push(Expr::dec_rc(Expr::lvar(&adt_var)));

        if_free_exprs.push(self.maybe_call_free_impl(Expr::lvar(&adt_var)));
        if !adt.borrow().ty.is_interface() {
            if_free_exprs.push(Expr::call(
                Expr::fvar(&adt.borrow().methods["free-wr"]),
                vec![Expr::cast(
                    Expr::lvar(&adt_var),
                    adt_var.ty.to_weak(),
                    CastType::StrongToWeak,
                )],
            ));
            if_free_exprs.push(Expr::free(Expr::lvar(&adt_var)));
        } else {
            // TODO!!
        }

        self.insert_at_ptr(Expr::if_(
            Expr::lvar(&dealloc_cond),
            Expr::Block(if_free_exprs),
            Expr::none_const(),
            None,
        ));
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
            let case_ty = case.to_type().to_weak();
            let then = Expr::call(
                Expr::fvar(&case.borrow().methods["free-wr"]),
                vec![Expr::cast(enu.clone(), case_ty.clone(), CastType::Bitcast)],
            );
            let cond = Expr::binary(SyntaxKind::Is, enu.clone(), Expr::TypeGet(case_ty));
            when_brs.push((cond, then))
        }
        Expr::switch(when_brs, Expr::none_const(), None)
    }
}
