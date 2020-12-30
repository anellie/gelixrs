use std::{collections::HashSet, rc::Rc};

use crate::{eat, eatc, result::EmitGIRError, GIRGenerator};
use ast::CSTNode;
use common::MutRc;
use error::{GErr, Res};
use gir_nodes::{
    declaration::{ADTType, LocalVariable},
    gir_err,
    types::TypeArguments,
    Expr, Function, IFaceImpls, Instance, Type, ADT,
};
use smol_str::SmolStr;
use syntax::kind::SyntaxKind;

use super::declare::FnSig;
use gir_nodes::{declaration::Visibility, types::TypeVariable};
use std::collections::HashMap;

impl GIRGenerator {
    pub(super) fn declare_methods(&mut self, adt: &MutRc<ADT>) {
        self.declare_user_methods(&adt);

        // TODO: This is a little ugly... it works i guess?
        if let ADTType::Enum { cases } = &adt.borrow().ty {
            for case in cases.values() {
                let mut case = case.borrow_mut();
                case.methods.reserve(adt.borrow().methods.len());
                for method in &adt.borrow().methods {
                    case.methods.insert(method.0.clone(), Rc::clone(method.1));
                }
            }
        }
    }

    fn declare_user_methods(&mut self, adt: &MutRc<ADT>) {
        let ast = adt.borrow().ast.clone();
        let is_iface = matches!(adt.borrow().ty, ADTType::Interface);

        // This instance with type arguments, like SomeADT[T], as just SomeADT with
        // missing parameters causes issues method generics resolution
        let this_inst = Instance::new(
            Rc::clone(&adt),
            Rc::new(
                adt.borrow()
                    .type_parameters
                    .iter()
                    .map(TypeVariable::from_param)
                    .map(Type::Variable)
                    .collect(),
            ),
        );

        for method in ast.methods() {
            let name = method.sig().name().name();
            let strong_mod = method.modifiers().any(|m| m == SyntaxKind::Strong);
            let this_type = if is_iface || strong_mod {
                Type::StrongRef(this_inst.clone())
            } else {
                Type::WeakRef(this_inst.clone())
            };

            let gir_method = eat!(
                self,
                self.function_from_ast(
                    method,
                    Some(("this".into(), this_type)),
                    Some(Rc::clone(&adt.borrow().type_parameters))
                )
            );
            adt.borrow_mut().methods.insert(name, gir_method);
        }

        self.declare_constructors(adt, &ast, this_inst);
    }

    fn declare_constructors(&mut self, adt: &MutRc<ADT>, ast: &ast::Adt, this_inst: Instance<ADT>) {
        let mut constructor_parameter_list = HashSet::new();

        let mut add_constructor = |func: &MutRc<Function>, cst: Option<CSTNode>| -> Res<()> {
            adt.borrow_mut().constructors.push(Rc::clone(&func));
            let params = func
                .borrow()
                .parameters
                .iter()
                .skip(1)
                .map(|p| &p.ty)
                .cloned()
                .collect::<Vec<_>>();
            if !constructor_parameter_list.insert(params) {
                Err(gir_err(cst.unwrap(), GErr::E312))
            } else {
                Ok(())
            }
        };

        let res = self
            .maybe_default_constructor(adt, &ast, &this_inst)
            .map(|sig| -> Res<_> {
                let func = self.create_function(sig)?;
                add_constructor(&func, None)
            });
        self.eat(res.transpose());

        for constructor in ast.constructors() {
            let this_param = Some(Ok((
                SmolStr::new_inline("this"),
                Type::WeakRef(this_inst.clone()),
            )));
            let ast_sig = constructor.sig();
            let parameters = ast_sig.parameters().map(|param| {
                let name = param.name();
                let type_ = param
                    .maybe_type()
                    .or_else(|| Self::get_field_ty_by_name(ast, &name))
                    .or_err(&param.cst, GErr::E311)?;
                let type_ = self.find_type(&type_)?;
                Ok((name, type_))
            });

            let sig = FnSig {
                name: "constructor".into(),
                visibility: self.visibility_from_modifiers(constructor.modifiers()),
                params: box this_param.into_iter().chain(parameters),
                type_parameters: Rc::clone(&adt.borrow().type_parameters),
                ret_type: None,
                ast: Some(constructor),
            };

            let func = eatc!(self, self.create_function(sig));
            self.eat(add_constructor(&func, Some(ast_sig.cst)));
        }
    }

    fn get_field_ty_by_name(adt: &ast::Adt, name: &SmolStr) -> Option<ast::Type> {
        adt.members()
            .find(|mem| &mem.name() == name)
            .map(|m| m._type())
            .flatten()
    }

    /// Will return a default constructor with no parameters
    /// should the ADT not contain a constructor and
    /// all members have default values.
    fn maybe_default_constructor(
        &self,
        adt: &MutRc<ADT>,
        ast: &ast::Adt,
        this_inst: &Instance<ADT>,
    ) -> Option<FnSig> {
        let no_uninitialized_members = || !ast.members().any(|v| v.maybe_initializer().is_none());
        if ast.constructors().next().is_none() && no_uninitialized_members() {
            Some(FnSig {
                name: "DEFAULT-constructor".into(),
                visibility: Visibility::Public,
                params: box Some(Ok(("this".into(), Type::WeakRef(this_inst.clone())))).into_iter(),
                type_parameters: Rc::clone(&adt.borrow().type_parameters),
                ret_type: None,
                ast: None,
            })
        } else {
            None
        }
    }

    pub fn fill_impls(&mut self) {
        // TODO maybe optimize
        let clone = self.iface_impls.values().cloned().collect::<Vec<_>>();
        for impls in clone {
            self.fill_impls_(impls)
        }
    }

    #[allow(clippy::map_entry)] // False positive
    fn fill_impls_(&mut self, impls: MutRc<IFaceImpls>) {
        let mut impls = impls.borrow_mut();

        let mut methods: HashMap<SmolStr, _> = HashMap::with_capacity(impls.interfaces.len() * 2);
        for iface_impl in impls
            .interfaces
            .values_mut()
            .filter(|im| !im.module.borrow().compiled)
        {
            self.switch_module(Rc::clone(&iface_impl.module));

            let ast = &iface_impl.ast;
            let iface = Rc::clone(&iface_impl.iface.ty);

            for ast_method in ast.methods() {
                let iface = iface.borrow();
                let name = ast_method.sig().name();
                let iface_method = eatc!(
                    self,
                    iface
                        .methods
                        .get(&name.name())
                        .or_err(&name.cst, GErr::E313)
                );

                let this_type = iface_impl.implementor.clone();
                // TODO: also insert into ADTs?
                // TODO: Type parameters on impls
                let impl_method = eatc!(
                    self,
                    self.function_from_ast(ast_method, Some(("this".into(), this_type)), None)
                );
                iface_impl
                    .methods
                    .insert(name.name(), Rc::clone(&impl_method));
                if methods.contains_key(&name.name()) {
                    methods.remove(&name.name());
                } else {
                    methods.insert(name.name(), Rc::clone(&impl_method));
                }

                self.check_equal_signature(&impl_method, iface_method, iface_impl.iface.args());
            }

            // Account for the 3 intrinsic methods already present on precompiled interfaces
            let iface_was_compiled = iface.borrow().module.borrow().compiled;
            let iface_method_len = iface.borrow().methods.len() - (iface_was_compiled as usize * 3);
            if iface_method_len > iface_impl.methods.len() {
                self.err(
                    ast.iface().cst,
                    GErr::E314(
                        iface
                            .borrow()
                            .methods
                            .keys()
                            .filter(|m| !iface_impl.methods.contains_key(*m))
                            .cloned()
                            .collect(),
                    ),
                );
            }
        }

        impls.methods = methods;
    }

    /// Ensures that the implemented interface method matches the expected signature.
    fn check_equal_signature(
        &self,
        impl_method: &MutRc<Function>,
        iface_method: &MutRc<Function>,
        iface_args: &Rc<TypeArguments>,
    ) {
        let impl_method = impl_method.borrow();
        let iface_method = iface_method.borrow();
        let ast = impl_method.ast.clone().unwrap();

        if impl_method.ret_type != iface_method.ret_type.resolve(iface_args) {
            let sig = ast.sig();
            let tok = sig.ret_type().map_or_else(|| sig.name().cst, |r| r.cst);
            self.err(tok, GErr::E315);
        }

        for (i, (method_param, iface_param)) in impl_method
            .parameters
            .iter()
            .zip(iface_method.parameters.iter())
            .skip(1)
            .enumerate()
        {
            let iface_ty = iface_param.ty.resolve(iface_args);
            if method_param.ty != iface_ty {
                let cst = ast.sig().parameters().nth(i).unwrap().cst;
                self.err(
                    cst,
                    GErr::E316 {
                        expected: iface_ty.to_string(),
                        was: method_param.ty.to_string(),
                    },
                );
            }
        }
    }

    /// Insert all constructor 'setter' parameters into the entry
    /// block of their GIR function.
    /// This has to be a separate pass since constructors are declared
    /// before fields are.
    pub fn constructor_setters(&mut self, adt: &MutRc<ADT>) {
        let adt = adt.borrow();
        for (constructor, func) in adt.ast.constructors().zip(adt.constructors.iter()) {
            let exprs = eatc!(
                self,
                self.insert_constructor_setters(&adt, &constructor, &func.borrow().parameters)
            );
            // (local variable to prevent 'already borrowed' panic)
            func.borrow_mut().exprs = exprs;
        }
    }

    fn insert_constructor_setters(
        &mut self,
        adt: &ADT,
        constructor: &ast::Function,
        gir_fn_params: &[Rc<LocalVariable>],
    ) -> Res<Vec<Expr>> {
        let mut block = Vec::new();
        for (index, param) in constructor
            .sig()
            .parameters()
            .enumerate()
            .filter(|(_, param)| param.maybe_type().is_none())
        {
            let field = adt
                .fields
                .get(&param.name())
                .or_err(&param.cst, GErr::E317)?;
            block.push(Expr::store(
                Expr::load(Expr::lvar(&gir_fn_params[0]), field),
                Expr::lvar(&gir_fn_params[index + 1]),
                true,
            ))
        }
        block.push(Expr::none_const());
        Ok(block)
    }
}
