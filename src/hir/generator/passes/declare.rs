use std::{collections::HashMap, rc::Rc};

use crate::{
    ast,
    ast::declaration::FunctionParam,
    error::Res,
    hir::{
        generator::{intrinsics::INTRINSICS, module::HIRModuleGenerator, HIRGenerator},
        get_or_create_iface_impls,
        nodes::{
            declaration::{
                ast_generics_to_hir, ADTType, Declaration, Function, LocalVariable, ADT,
            },
            module::Module,
            types::{IFaceImpl, Type},
        },
        result::EmitHIRError,
    },
    lexer::token::Token,
    mir::{mutrc_new, MutRc},
};

impl HIRModuleGenerator {
    pub fn declare_adts(&mut self, module: MutRc<Module>, ast: &mut ast::Module) {
        for ast in ast.adts.drain(..) {
            let name = Rc::clone(&ast.name.lexeme);
            self.generator.try_reserve_name(&ast.name);
            let adt = ADT::from_ast(&self.generator, ast);

            let mut module = module.borrow_mut();
            module
                .declarations
                .insert(name, Declaration::Adt(Rc::clone(&adt)));

            if let ADTType::Enum { cases } = &adt.borrow().ty {
                for case in cases.values() {
                    module.declarations.insert(
                        Rc::clone(&case.borrow().name.lexeme),
                        Declaration::Adt(Rc::clone(case)),
                    );
                }
            };
        }
    }

    pub fn declare_iface_impls(&mut self, _module: MutRc<Module>, ast: &mut ast::Module) {
        for im in ast.iface_impls.drain(..) {
            self.generator.declare_impl(im)
        }
    }

    pub fn declare_functions(&mut self, _module: MutRc<Module>, ast: &mut ast::Module) {
        for ast in ast.functions.drain(..) {
            eatc!(self.generator.create_function(ast, None));
        }
    }
}

impl HIRGenerator {
    /// Creates a function.
    /// `this_arg` indicates that the function is a method
    /// with some kind of receiver, with the 'this' parameter
    /// added to the 0th position of the parameters and the
    /// function renamed to '$receiver-$name'.
    pub fn create_function(
        &mut self,
        func: ast::Function,
        this_param: Option<FunctionParam>,
    ) -> Res<MutRc<Function>> {
        let name = func.sig.name.clone();
        self.try_reserve_name(&func.sig.name);

        let function = self.generate_mir_fn(func, this_param)?;
        self.try_reserve_name(&name);
        self.module.borrow_mut().declarations.insert(
            Rc::clone(&name.lexeme),
            Declaration::Function(Rc::clone(&function)),
        );
        self.maybe_set_main_fn(&function, &name);
        Ok(function)
    }

    fn generate_mir_fn(
        &self,
        func: ast::Function,
        this_param: Option<FunctionParam>,
    ) -> Res<MutRc<Function>> {
        let signature = &func.sig;
        let ret_type = signature
            .return_type
            .as_ref()
            .map_or(Ok(Type::None), |ty| self.resolver.find_type(ty))?;

        if !ret_type.can_escape() {
            self.err(
                signature.return_type.as_ref().unwrap().token(),
                "Cannot return a weak reference".to_string(),
            );
        }

        let mut parameters = Vec::with_capacity(signature.parameters.len());
        for param in this_param.iter().chain(signature.parameters.iter()) {
            parameters.push(Rc::new(LocalVariable {
                name: param.name.clone(),
                ty: self.resolver.find_type(&param.type_)?,
                mutable: false,
            }));
        }

        Ok(mutrc_new(Function {
            name: signature.name.clone(),
            parameters,
            type_parameters: ast_generics_to_hir(&self, &signature.generics),
            exprs: Vec::with_capacity(4),
            variables: Default::default(),
            ret_type,
            ast: mutrc_new(func),
        }))
    }

    fn maybe_set_main_fn(&self, func: &MutRc<Function>, err_tok: &Token) {
        if &func.borrow().name.lexeme[..] == "main" {
            eat!(INTRINSICS
                .with(|i| i.borrow_mut().set_main_fn(func))
                .on_err(&self.path, err_tok, "Can't define main multiple times."));
        }
    }

    fn declare_impl(&self, iface_impl: ast::IFaceImpl) {
        let implementor = eat!(self.resolver.find_type(&iface_impl.implementor));

        let iface = eat!(self.resolver.find_type(&iface_impl.iface));
        if !iface.is_value() || !iface.as_value().ty.borrow().ty.is_interface() {
            self.err(&iface_impl.iface.token(), "Not an interface".to_string());
        }

        let err_tok = iface_impl.iface.token().clone();
        let impls = get_or_create_iface_impls(&implementor);
        let mir_impl = IFaceImpl {
            implementor,
            iface: iface.as_value().clone(),
            methods: HashMap::with_capacity(iface_impl.methods.len()),
            module: Rc::clone(&self.module),
            ast: mutrc_new(iface_impl),
        };
        let already_defined = impls
            .borrow_mut()
            .interfaces
            .insert(iface, mir_impl)
            .is_some();
        if already_defined {
            self.err(&err_tok, "Interface already defined for type".to_string());
        }
    }
}
