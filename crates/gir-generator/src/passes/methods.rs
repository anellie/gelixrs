use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use smol_str::SmolStr;
use crate::GIRGenerator;

impl GIRGenerator {
    pub fn declare_methods(&mut self, adt: &MutRc<ADT>) {
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
        let ast = Rc::clone(&adt.borrow().ast);
        let mut ast = ast.borrow_mut();
        let is_iface = matches!(ast.ty, ast::ADTType::Interface);
        let this_param = FunctionParam::this_param_g(&ast);

        for method in ast.methods.drain(..) {
            let mut this_param = this_param.clone();
            this_param.type_ = if !is_iface
                && !method
                    .sig
                    .modifiers
                    .iter()
                    .any(|t| t.t_type == TType::Strong)
            {
                ast::Type::Weak(Box::new(this_param.type_))
            } else {
                this_param.type_
            };

            let name = method.sig.name.lexeme.clone();
            let gir_method = eat!(
                self,
                self.generate_gir_fn(
                    method,
                    Some(this_param.clone()),
                    Some(&adt.borrow().type_parameters),
                )
            );
            adt.borrow_mut().methods.insert(name, gir_method);
        }

        if let Some(constructors) = ast.constructors() {
            self.declare_constructors(&adt, &ast, &this_param, constructors);
        }
    }

    fn declare_constructors(
        &mut self,
        adt: &MutRc<ADT>,
        ast: &ast::ADT,
        this_param: &FunctionParam,
        constructors: &[Constructor],
    ) {
        let mut constructor_parameter_list = HashSet::with_capacity(constructors.len());

        let default = self.maybe_default_constructor(&ast);
        let iter = constructors.iter().chain(default.iter());
        for constructor in iter {
            let sig = eatc!(
                self,
                self.get_constructor(&ast, constructor, this_param.clone())
            );
            let func = eatc!(
                self,
                self.generate_gir_fn(
                    ast::Function { sig, body: None },
                    None,
                    Some(&adt.borrow().type_parameters),
                )
            );
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
                self.err(
                    &ast.name,
                    "Class contains constructors with duplicate signatures.".to_string(),
                );
            }
        }
    }

    /// Returns the function signature of a constructor.
    fn get_constructor(
        &mut self,
        adt: &ast::ADT,
        constructor: &Constructor,
        mut this_param: FunctionParam,
    ) -> Res<FuncSignature> {
        this_param.type_ = ast::Type::Weak(Box::new(this_param.type_));
        let mut parameters = constructor
            .parameters
            .iter()
            .map(|(name, ty)| {
                let type_ = ty
                    .clone()
                    .or_else(|| Self::get_field_ty_by_name(adt, name))
                    .or_err(
                        &self.path,
                        name,
                        "Cannot infer type of field with default value (specify type explicitly.)",
                    )?;
                Ok(FunctionParam {
                    type_,
                    name: name.clone(),
                })
            })
            .collect::<Res<Vec<FunctionParam>>>()?;
        parameters.insert(0, this_param);
        Ok(FuncSignature {
            name: Token::generic_identifier("constructor"),
            visibility: constructor.visibility,
            generics: None,
            return_type: None,
            parameters,
            variadic: false,
            modifiers: vec![],
        })
    }

    fn get_field_ty_by_name(adt: &ast::ADT, name: &Token) -> Option<ast::Type> {
        adt.members()
            .unwrap()
            .iter()
            .find(|mem| mem.name.lexeme == name.lexeme)
            .map(|m| m.ty.clone())
            .flatten()
    }

    /// Will return a default constructor with no parameters
    /// should the ADT not contain a constructor and
    /// all members have default values.
    fn maybe_default_constructor(&self, adt: &ast::ADT) -> Option<Constructor> {
        let no_uninitialized_members = || {
            !adt.members()
                .unwrap()
                .iter()
                .any(|v| v.initializer.is_none())
        };
        if adt.constructors().unwrap().is_empty() && no_uninitialized_members() {
            Some(Constructor {
                parameters: vec![],
                visibility: Visibility::Public,
                body: None,
            })
        } else {
            None
        }
    }

    /// Insert all constructor 'setter' parameters into the entry
    /// block of their GIR function.
    /// This has to be a separate pass since constructors are declared
    /// before fields are.
    pub fn constructor_setters(&mut self, adt: &MutRc<ADT>) {
        if let Some(constructors) = adt.borrow().ast.borrow().constructors() {
            for (constructor, func) in constructors.iter().zip(adt.borrow().constructors.iter()) {
                let exprs = eatc!(
                    self,
                    self.insert_constructor_setters(
                        &adt.borrow(),
                        constructor,
                        &func.borrow().parameters
                    )
                );
                // (local variable to prevent 'already borrowed' panic)
                func.borrow_mut().exprs = exprs;
            }
        }
    }

    fn insert_constructor_setters(
        &mut self,
        adt: &ADT,
        constructor: &Constructor,
        gir_fn_params: &[Rc<LocalVariable>],
    ) -> Res<Vec<Expr>> {
        let mut block = Vec::new();
        for (index, (param, _)) in constructor
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, (_, ty))| ty.is_none())
        {
            let field =
                adt.fields
                    .get(&param.lexeme)
                    .or_err(&self.path, param, "Unknown class field.")?;
            block.push(Expr::store(
                Expr::load(Expr::lvar(&gir_fn_params[0]), field),
                Expr::lvar(&gir_fn_params[index + 1]),
                true,
            ))
        }
        block.push(Expr::none_const_());
        Ok(block)
    }

    pub fn fill_impls(&mut self) {
        IFACE_IMPLS.with(|i| {
            for impls in i.borrow().values() {
                self.fill_impls_(impls)
            }
        })
    }

    fn fill_impls_(&mut self, impls: &MutRc<IFaceImpls>) {
        let mut impls = impls.borrow_mut();

        let mut methods: HashMap<SmolStr, _> = HashMap::with_capacity(impls.interfaces.len() * 2);
        for iface_impl in impls.interfaces.values_mut() {
            self.switch_module(Rc::clone(&iface_impl.module));

            let ast = Rc::clone(&iface_impl.ast);
            let iface = Rc::clone(&iface_impl.iface.ty);
            let mut ast = ast.borrow_mut();
            let this_arg = FunctionParam::this_param_(&ast.implementor);

            for ast_method in ast.methods.drain(..) {
                let iface = iface.borrow();
                let name = ast_method.sig.name.lexeme.clone();
                let iface_method = eatc!(
                    self,
                    iface.methods.get(&name).on_err(
                        &self.path,
                        &ast_method.sig.name,
                        "Method is not defined in interface.",
                    )
                );

                // TODO: also insert into ADTs?
                // TODO: Type parameters on impls
                let impl_method = eatc!(
                    self,
                    self.generate_gir_fn(ast_method, Some(this_arg.clone()), None)
                );
                iface_impl
                    .methods
                    .insert(name.clone(), Rc::clone(&impl_method));
                if methods.contains_key(&name) {
                    methods.remove(&name);
                } else {
                    methods.insert(name.clone(), Rc::clone(&impl_method));
                }

                self.check_equal_signature(&impl_method, iface_method, iface_impl.iface.args());
            }

            if iface.borrow().methods.len() > iface_impl.methods.len() {
                self.err(
                    &ast.iface.token(),
                    "Missing methods in interface impl.".to_string(),
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
        let ast = impl_method.ast.borrow();

        if impl_method.ret_type != iface_method.ret_type.resolve(iface_args) {
            let tok = ast
                .sig
                .return_type
                .as_ref()
                .map_or(&ast.sig.name, ast::Type::token);
            self.err(
                tok,
                "Incorrect return type on interface method.".to_string(),
            );
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
                let tok = &ast.sig.parameters[i].name;
                self.err(
                    tok,
                    format!(
                        "Incorrect parameter type on interface method (Expected {}, was {}).",
                        iface_ty, method_param.ty
                    ),
                );
            }
        }
    }
}
