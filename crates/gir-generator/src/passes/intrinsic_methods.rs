use std::rc::Rc;

use smol_str::SmolStr;

use std::collections::HashMap;
use crate::GIRGenerator;

impl GIRGenerator {
    pub fn declare_lifecycle_methods(&mut self, adt: &MutRc<ADT>) {
        let ast = Rc::clone(&adt.borrow().ast);
        let this_param = FunctionParam::this_param_g(&ast.borrow());

        // TODO: Replace this vec with an array once array.into_iter() is stabilized
        let fns: Vec<(_, _)> = vec![
            (
                "new-instance",
                Self::get_instantiator_ast(this_param.clone()),
            ),
            (
                "free-wr",
                Self::get_destructor_ast(this_param.clone(), false),
            ),
            (
                "free-sr",
                Self::get_destructor_ast(this_param.clone(), true),
            ),
        ];

        for (name, func) in fns.into_iter() {
            let gir_fn = eat!(
                self,
                self.generate_gir_fn(func, None, Some(&adt.borrow().type_parameters))
            );
            adt.borrow_mut()
                .methods
                .insert(SmolStr::new_inline(name), Rc::clone(&gir_fn));
        }
    }

    pub fn generate_lifecycle_methods(&mut self, adt: &MutRc<ADT>) {
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
            if let Some(init) = field.initializer.take().take() {
                self.insert_at_ptr(Expr::store(
                    Expr::load(Expr::lvar(&var), field),
                    *init,
                    true,
                ))
            }
        }

        // Insert at the end of the instantiator to prevent
        // an edge case of an empty instantiator, which IR would interpret
        // incorrectly as an external function
        self.insert_at_ptr(Expr::none_const_())
    }

    fn generate_wr_destructor(&mut self, adt: &MutRc<ADT>, func: MutRc<Function>) {
        self.set_pointer(&func);
        let adt_var = Rc::clone(&func.borrow().parameters[0]);

        self.insert_at_ptr(Self::maybe_call_free_impl(Expr::lvar(&adt_var)));
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

        if_free_exprs.push(Self::maybe_call_free_impl(Expr::lvar(&adt_var)));
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
            Expr::none_const_(),
            None,
        ));
    }

    fn maybe_call_free_impl(adt: Expr) -> Expr {
        let ty = adt.get_type();
        let free_iface = INTRINSICS.with(|i| i.borrow().free_iface.clone()).unwrap();
        let free_method = get_iface_impls(&ty)
            .map(|impls| {
                impls
                    .borrow()
                    .interfaces
                    .get(&free_iface.to_type())
                    .map(|iface| Rc::clone(iface.methods.iter().next().unwrap().1))
            })
            .flatten();
        if let Some(method) = free_method {
            Expr::call(Expr::fvar(&method), vec![adt])
        } else {
            Expr::none_const_()
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
            let cond = Expr::binary(
                Token::generic_token(TType::Is),
                enu.clone(),
                Expr::TypeGet(case_ty),
            );
            when_brs.push((cond, then))
        }
        Expr::switch(when_brs, Expr::none_const_(), None)
    }

    /// Returns AST of the ADT instantiator.
    fn get_instantiator_ast(mut this_param: FunctionParam) -> ast::Function {
        this_param.type_ = ast::Type::Weak(Box::new(this_param.type_));
        ast::Function {
            sig: FuncSignature {
                name: Token::generic_identifier("new-instance"),
                visibility: Visibility::Public,
                generics: None,
                return_type: None,
                parameters: vec![this_param],
                variadic: false,
                modifiers: vec![],
            },
            body: None,
        }
    }

    /// Returns signature of the ADT destructor.
    fn get_destructor_ast(mut this_param: FunctionParam, strong_ref: bool) -> ast::Function {
        if !strong_ref {
            this_param.type_ = ast::Type::Weak(Box::new(this_param.type_));
        }

        let mut sig = FuncSignature {
            name: Token::generic_identifier(if strong_ref { "free-sr" } else { "free-wr" }),
            visibility: Visibility::Public,
            generics: None,
            return_type: None,
            parameters: vec![
                this_param,
                FunctionParam {
                    type_: ast::Type::Ident(Token::generic_identifier("bool")),
                    name: Token::generic_identifier("refcount_is_0"),
                },
            ],
            variadic: false,
            modifiers: vec![],
        };
        if !strong_ref {
            sig.parameters.pop();
        }
        ast::Function { sig, body: None }
    }
}
