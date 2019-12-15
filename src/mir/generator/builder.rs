/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 2:07 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use either::Either;
use either::Either::{Left, Right};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::{module_path_to_string, ModulePath};
use crate::lexer::token::Token;
use crate::mir::{IFACE_IMPLS, MModule, MutRc};
use crate::mir::generator::MIRError;
use crate::mir::nodes::{
    Class, ClassPrototype, Expr, Flow, FunctionPrototype, Interface,
    InterfacePrototype, Variable,
};

use super::super::nodes::{Function, Type};

/// A builder for assisting in creating MIR.
pub struct MIRBuilder {
    /// The current insertion position.
    position: Option<Pointer>,

    /// Insertion positions saved for later.
    /// Used to store the previous insertion
    /// positions when generating a class from a prototype;
    /// the insertion position is saved here before
    /// and restored after compiling the class prototype
    /// has finished.
    saved_positions: Vec<Pointer>,

    /// The module the builder is inserting into.
    pub module: MModule,
}

impl MIRBuilder {
    /// Builds a class instance and returns an expression that loads the instance.
    /// The expression returned can be safely cloned to reuse the instance.
    pub fn build_class_inst(&mut self, class_ref: MutRc<Class>) -> Expr {
        let call = {
            let class = class_ref.borrow();
            Expr::call(Expr::load(&class.instantiator), vec![])
        };

        let var = Variable::new(true, Type::Class(class_ref), &Rc::new("tmp-constructor-var".to_string()));
        self.add_function_variable(Rc::clone(&var));
        self.insert_at_ptr(Expr::store(&var, call));

        Expr::load(&var)
    }

    /// Will append a block to the given function, always creating a new one.
    pub fn append_block(&mut self, name: &str) -> Rc<String> {
        self.cur_fn().borrow_mut().append_block(name, true)
    }

    pub fn find_type_by_name(&self, tok: &Token) -> Option<Type> {
        Some(match &tok.lexeme[..] {
            "None" => Type::None,
            "bool" => Type::Bool,

            "i8" => Type::I8,
            "i16" => Type::I16,
            "i32" => Type::I32,
            "i64" => Type::I64,

            "f32" => Type::F32,
            "f64" => Type::F64,

            "String" => Type::String,

            _ => self
                .find_class(&tok.lexeme)
                .map(Type::Class)
                .or_else(|| Some(Type::Interface(self.find_interface(&tok.lexeme)?)))
                .or_else(|| {
                    Some(Type::Generic(
                        self.generic_types.iter().position(|g| *g == tok.lexeme)?,
                    ))
                })?,
        })
    }

    /// Takes the name of a function and returns the name it should have as part of its definition.
    /// This name has the module path appended to the front, to prevent naming collisions
    /// inside IR.
    /// The only exception to this is 'main', as it may only appear once.
    pub fn get_function_name(&mut self, func_name: &Rc<String>) -> String {
        if func_name.as_ref() != "main" {
            format!("{}:{}", module_path_to_string(&self.module.path), func_name)
        } else {
            func_name.to_string()
        }
    }

    pub fn find_function(&self, name: &String) -> Option<MutRc<Function>> {
        self.module
            .functions
            .get(name)
            .or_else(|| self.imports.functions.get(name))
            .map(|f| Rc::clone(f.type_.as_function()))
    }

    pub fn find_function_var(&self, name: &String) -> Option<Rc<Variable>> {
        self.module
            .functions
            .get(name)
            .or_else(|| self.imports.functions.get(name))
            .map(Rc::clone)
    }

    pub fn find_func_or_proto(
        &self,
        name: &String,
    ) -> Option<Either<Rc<Variable>, MutRc<FunctionPrototype>>> {
        self.find_function_var(name)
            .map(Left)
            .or_else(|| self.prototypes.functions.get(name).cloned().map(Right))
    }

    pub fn find_class(&self, name: &String) -> Option<MutRc<Class>> {
        Some(Rc::clone(
            self.module
                .classes
                .get(name)
                .or_else(|| self.imports.classes.get(name))?,
        ))
    }

    pub fn find_class_or_proto(
        &self,
        name: &String,
    ) -> Option<Either<MutRc<Class>, MutRc<ClassPrototype>>> {
        self.find_class(name)
            .map(Left)
            .or_else(|| self.prototypes.classes.get(name).cloned().map(Right))
    }

    pub fn find_interface(&self, name: &String) -> Option<MutRc<Interface>> {
        Some(Rc::clone(
            self.module
                .interfaces
                .get(name)
                .or_else(|| self.imports.interfaces.get(name))?,
        ))
    }

    pub fn find_iface_or_proto(
        &self,
        name: &String,
    ) -> Option<Either<MutRc<Interface>, MutRc<InterfacePrototype>>> {
        self.find_interface(name)
            .map(Left)
            .or_else(|| self.prototypes.interfaces.get(name).cloned().map(Right))
    }

    /// Searches for an associated method on a type. Can be either an interface
    /// method or a class method.
    pub fn find_associated_method(&self, ty: Type, name: &Token) -> Option<Rc<Variable>> {
        let class_method = if let Type::Class(class) = &ty {
            class.borrow().methods.get(&name.lexeme).cloned()
        } else {
            None
        };

        class_method.or_else(|| {
            IFACE_IMPLS
                .with(|impls| impls.borrow().get(&ty).cloned())?
                .borrow()
                .methods
                .get(&name.lexeme)
                .cloned()
        })
    }

    pub fn set_pointer(&mut self, function: MutRc<Function>, block: Rc<String>) {
        self.position = Some(Pointer { function, block })
    }

    pub fn push_current_pointer(&mut self) {
        if let Some(ptr) = self.position.take() {
            self.saved_positions.push(ptr);
        }
    }

    pub fn load_last_pointer(&mut self) {
        self.position = self.saved_positions.pop();
    }

    pub fn set_block(&mut self, block: &Rc<String>) {
        if let Some(pos) = self.position.as_mut() {
            pos.block = Rc::clone(block)
        }
    }

    pub fn set_generic_types(&mut self, types: &Vec<Token>) {
        self.generic_types.clear();
        for ty in types.iter() {
            self.generic_types.push(Rc::clone(&ty.lexeme));
        }
    }

    pub fn set_generic_types_rc(&mut self, types: &Vec<Rc<String>>) {
        self.generic_types.clear();
        for ty in types.iter() {
            self.generic_types.push(Rc::clone(ty));
        }
    }

    pub fn insert_at_ptr(&mut self, expr: Expr) {
        let func = self.cur_fn();
        let mut func = func.borrow_mut();
        func.blocks
            .get_mut(&self.position.as_ref().unwrap().block)
            .unwrap()
            .push(expr)
    }

    pub fn cur_fn(&self) -> MutRc<Function> {
        self.position.as_ref().unwrap().function.clone()
    }

    pub fn cur_block_name(&self) -> Rc<String> {
        Rc::clone(&self.position.as_ref().unwrap().block)
    }

    pub fn consume_module(self) -> MModule {
        self.module
    }

    pub fn module_path(&self) -> Rc<ModulePath> {
        Rc::clone(&self.module.path)
    }

    pub fn new(module: MModule) -> MIRBuilder {
        MIRBuilder {
            position: None,
            saved_positions: Vec::with_capacity(3),
            module,
            imports: Imports::default(),
            prototypes: Prototypes::default(),
            used_names: HashSet::with_capacity(3),
            generic_types: Vec::with_capacity(3),
        }
    }
}

pub struct Pointer {
    pub function: MutRc<Function>,
    block: Rc<String>,
}
