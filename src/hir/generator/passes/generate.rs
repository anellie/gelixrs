use std::{collections::HashMap, rc::Rc};

use crate::{
    ast::Constructor,
    hir::{
        generator::HIRGenerator,
        get_or_create_iface_impls,
        nodes::{
            declaration::{Declaration, Field, Function, ADT},
            expression::Expr,
            types::{Instance, Type},
        },
    },
    lexer::token::Token,
    mir::MutRc,
};

impl HIRGenerator {
    pub fn generate(&mut self, decl: Declaration) {
        match decl {
            Declaration::Function(func) => self.generate_function(&func),

            Declaration::Adt(adt_rc) => {
                {
                    let adt = adt_rc.borrow();
                    self.generate_constructors(&adt);
                    for method in adt.methods.values() {
                        self.generate_function(method);
                    }
                }

                let inst = Instance::new(adt_rc);
                self.generate_impl(&Type::Value(inst.clone()));
                self.generate_impl(&Type::WeakRef(inst.clone()));
                self.generate_impl(&Type::StrongRef(inst));
            }
        }
    }

    pub fn generate_primitive(&mut self) {
        for ty in &Type::primitives() {
            self.generate_impl(ty);
        }
    }

    pub fn generate_impl(&mut self, ty: &Type) {
        let impls = get_or_create_iface_impls(ty);
        let impls = impls.borrow();

        for im in impls.interfaces.values() {
            self.switch_module(Rc::clone(&im.module));
            for method in im.methods.values() {
                self.generate_function(method);
            }
        }
    }

    fn generate_function(&mut self, function: &MutRc<Function>) {
        self.prepare_function(&function);
        let ast = Rc::clone(&function.borrow().ast);
        let body = self.expression(&ast.borrow().body.as_ref().unwrap()); // todo?

        let ret_type = function.borrow().ret_type.clone();
        let (body, success) = self.resolver.try_cast(body, &ret_type);
        if !success {
            self.err(
                &ast.borrow().sig.name.clone(),
                format!(
                    "Function return type ({}) does not match body type ({}).",
                    ret_type,
                    body.get_type()
                ),
            );
        }

        self.insert_at_ptr(Expr::ret(body));
        self.end_scope();
    }

    fn generate_constructors(&mut self, adt: &ADT) {
        let ast = adt.ast.borrow();
        if let Some(constructors) = ast.constructors() {
            for (ast, constructor) in constructors.iter().zip(adt.constructors.iter()) {
                self.prepare_function(constructor);
                self.set_uninitialized_members(ast, &adt.fields);
                if let Some(body) = &ast.body {
                    let body = self.expression(body);
                    self.insert_at_ptr(body);
                }
                self.end_scope();
                self.check_no_uninitialized(&adt.name);
            }

            self.uninitialized_this_fields.clear();
        }
    }

    /// Sets all fields that are uninitialized before the constructor
    /// and must be set by it.
    fn set_uninitialized_members(
        &mut self,
        constructor: &Constructor,
        class_mems: &HashMap<Rc<String>, Rc<Field>>,
    ) {
        self.uninitialized_this_fields.clear();
        for (name, mem) in class_mems.iter() {
            let initialized = constructor
                .parameters
                .iter()
                .filter(|p| p.1.is_none())
                .any(|p| &p.0.lexeme == name);
            if !initialized && mem.initializer.is_none() {
                self.uninitialized_this_fields.insert(Rc::clone(&mem));
            }
        }
    }

    /// Creates an error if any field is still uninitialized after a constructor.
    fn check_no_uninitialized(&mut self, err_tok: &Token) {
        if !self.uninitialized_this_fields.is_empty() {
            self.err(
                err_tok,
                "Cannot have uninitialized fields after constructor.".to_string(),
            )
        }
    }

    /// Will append an 'entry' block to the fn and set the pointer at
    /// that location, then insert all parameters as variables.
    fn prepare_function(&mut self, function: &MutRc<Function>) {
        self.set_pointer(Rc::clone(function));
        self.begin_scope();
        let func = function.borrow();
        for param in &func.parameters {
            self.insert_variable(&param, false);
        }
    }
}
