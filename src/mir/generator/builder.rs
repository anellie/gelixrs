/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 9:38 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::{HashMap, HashSet};
use std::mem::discriminant;
use std::rc::Rc;

use either::Either;
use either::Either::{Left, Right};

use crate::ast::Type as ASTType;
use crate::error::Error;
use crate::lexer::token::{TType, Token};
use crate::mir::generator::{MIRError, MIRGenerator, Res};
use crate::mir::nodes::{
    Class, ClassMember, ClassPrototype, Expression, Flow, FunctionPrototype, Interface,
    InterfacePrototype, Variable,
};
use crate::mir::{MIRModule, MutRc};
use crate::{module_path_to_string, ModulePath};

use super::super::nodes::{Function, Type};

/// A builder for assisting in creating MIR.
pub struct MIRBuilder {
    /// The current insertion position.
    position: Option<Pointer>,

    /// The module the builder is inserting into.
    pub module: MIRModule,

    /// Things imported into this module by 'import' declarations.
    pub imports: Imports,
    /// All prototypes in this module.
    pub prototypes: Prototypes,

    /// A list of all global names (classes/interfaces/functions) in this module.
    /// Used to ensure that no naming collision occurs.
    used_names: HashSet<Rc<String>>,
    /// A list of type aliases, where the key is a type that will be translated to the value.
    /// Currently only used for the 'This' type alias inside classes/interfaces.
    type_aliases: HashMap<Rc<String>, ASTType>,
    /// All generic types that should resolve to a generic MIR type.
    /// Used when compiling prototypes.
    pub generic_types: HashSet<Rc<String>>,

    /// Simply a const of the string "tmp".
    /// Used for temporary variables needed for class init.
    tmp_const: Rc<String>,
    /// Simply a const of the string "This".
    /// Used for the This alias inside classes/interfaces.
    this_const: Rc<String>,
}

impl MIRBuilder {
    /// Tries to reserve the given name for the current module. If the name
    /// is already used, returns an error.
    pub fn try_reserve_name(&mut self, name: &Token) -> Res<()> {
        self.try_reserve_name_rc(&name.lexeme, name)
    }

    pub fn try_reserve_name_rc(&mut self, name: &Rc<String>, tok: &Token) -> Res<()> {
        if self.used_names.insert(Rc::clone(name)) {
            Ok(())
        } else {
            Err(MIRError {
                error: Error::new(
                    tok,
                    tok,
                    "MIRGenerator",
                    format!("Name {} already defined in this module", name),
                ),
                module: self.module_path(),
            })
        }
    }

    /// Will create the variable in the current function.
    pub fn add_function_variable(&mut self, variable: Rc<Variable>) {
        let func = self.cur_fn();
        func.borrow_mut()
            .insert_var(Rc::clone(&variable.name), variable);
    }

    pub fn build_binary(&self, left: Expression, operator: TType, right: Expression) -> Expression {
        Expression::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        }
    }

    pub fn build_unary(&self, right: Expression, op: TType) -> Expression {
        Expression::Unary {
            operator: op,
            right: Box::new(right),
        }
    }

    pub fn build_call(&mut self, callee: Expression, args: Vec<Expression>) -> Expression {
        Expression::Call {
            callee: Box::new(callee),
            arguments: args,
        }
    }

    pub fn build_constructor(&mut self, class_ref: MutRc<Class>) -> Expression {
        let class = class_ref.borrow();
        let var = Rc::new(Variable {
            name: Rc::clone(&self.tmp_const),
            type_: Type::Class(Rc::clone(&class_ref)),
            mutable: false,
        });

        self.cur_fn()
            .borrow_mut()
            .insert_var(Rc::clone(&self.tmp_const), Rc::clone(&var));

        let init_fn = self
            .find_function_var(&format!("{}-internal-init", &class.name))
            .unwrap();
        let init_call = Expression::Call {
            callee: Box::new(Expression::VarGet(init_fn)),
            arguments: vec![Expression::VarGet(Rc::clone(&var))],
        };
        self.insert_at_ptr(init_call);

        let user_init = self.find_function_var(&format!("{}-init", &class.name));
        if let Some(user_init) = user_init {
            let init_call = Expression::Call {
                callee: Box::new(Expression::VarGet(user_init)),
                arguments: vec![Expression::VarGet(Rc::clone(&var))],
            };
            self.insert_at_ptr(init_call);
        }

        Expression::VarGet(var)
    }

    pub fn build_phi(&self, nodes: Vec<(Expression, Rc<String>)>) -> Expression {
        // Filter all nodes that return Any.
        // A node might return Any if it does not produce a value;
        // but instead branches away from the phi.
        let filtered_nodes = nodes
            .into_iter()
            .filter(|node| {
                let type_ = node.0.get_type();
                discriminant(&Type::Any) != discriminant(&type_)
            })
            .collect();

        Expression::Phi(filtered_nodes)
    }

    pub fn build_struct_get(&self, object: Expression, field: Rc<ClassMember>) -> Expression {
        Expression::StructGet {
            object: Box::new(object),
            index: field.index,
        }
    }

    pub fn build_struct_set(
        &self,
        object: Expression,
        field: Rc<ClassMember>,
        value: Expression,
    ) -> Expression {
        Expression::StructSet {
            object: Box::new(object),
            index: field.index,
            value: Box::new(value),
        }
    }

    pub fn build_store(&self, var: Rc<Variable>, value: Expression) -> Expression {
        Expression::VarStore {
            var,
            value: Box::new(value),
        }
    }

    pub fn build_load(&self, var: Rc<Variable>) -> Expression {
        Expression::VarGet(var)
    }

    pub fn build_branch(&mut self, cond: Expression, then: &Rc<String>, else_: &Rc<String>) {
        self.set_return(Flow::Branch {
            condition: cond,
            then_b: Rc::clone(&then),
            else_b: Rc::clone(&else_),
        });
    }

    pub fn build_jump(&mut self, to: &Rc<String>) {
        self.set_return(Flow::Jump(Rc::clone(to)))
    }

    pub fn append_block(&mut self, name: &str) -> Rc<String> {
        self.cur_fn().borrow_mut().append_block(name)
    }

    pub fn set_return(&mut self, ret: Flow) {
        self.insert_at_ptr(Expression::Flow(Box::new(ret)));
    }

    pub fn find_type(&self, ast: &ASTType) -> Option<Type> {
        Some(match ast {
            ASTType::Ident(tok) => {
                if let Some(alias) = self.type_aliases.get(&tok.lexeme) {
                    return self.find_type(alias);
                }

                match &tok.lexeme[..] {
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
                        .or_else(|| Some(Type::Interface(self.find_interface(&tok.lexeme)?)))?,
                }
            }

            ASTType::Array(_) => unimplemented!(),

            ASTType::Closure { .. } => unimplemented!(),

            ASTType::Generic { .. } => unimplemented!(),
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
            .map(|f| MIRGenerator::var_to_function(f))
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

    pub fn set_pointer(&mut self, function: MutRc<Function>, block: Rc<String>) {
        self.position = Some(Pointer { function, block })
    }

    pub fn set_block(&mut self, block: &Rc<String>) {
        if let Some(pos) = self.position.as_mut() {
            pos.block = Rc::clone(block)
        }
    }

    pub fn add_alias(&mut self, key: &Rc<String>, val: &ASTType) {
        self.type_aliases.insert(Rc::clone(key), val.clone());
    }

    pub fn add_this_alias(&mut self, name: &Token) {
        self.type_aliases
            .insert(Rc::clone(&self.this_const), ASTType::Ident(name.clone()));
    }

    pub fn remove_alias(&mut self, name: &Rc<String>) {
        self.type_aliases.remove(name);
    }

    pub fn remove_this_alias(&mut self) {
        self.type_aliases.remove(&self.this_const);
    }

    pub fn set_generic_types(&mut self, types: &Vec<Token>) {
        self.generic_types.clear();
        for ty in types.iter() {
            self.generic_types.insert(Rc::clone(&ty.lexeme));
        }
    }

    pub fn set_generic_types_rc(&mut self, types: &Vec<Rc<String>>) {
        self.generic_types.clear();
        for ty in types.iter() {
            self.generic_types.insert(Rc::clone(ty));
        }
    }

    pub fn insert_at_ptr(&mut self, expr: Expression) {
        let func = self.cur_fn();
        let mut func = func.borrow_mut();
        func.blocks
            .get_mut(&self.position.as_ref().unwrap().block)
            .unwrap()
            .push(expr);
    }

    pub fn cur_fn(&self) -> MutRc<Function> {
        Rc::clone(&self.position.as_ref().unwrap().function)
    }

    pub fn cur_block_name(&self) -> Rc<String> {
        Rc::clone(&self.position.as_ref().unwrap().block)
    }

    pub fn consume_module(self) -> MIRModule {
        self.module
    }

    pub fn module_path(&self) -> Rc<ModulePath> {
        Rc::clone(&self.module.path)
    }

    pub fn new(module: MIRModule) -> MIRBuilder {
        MIRBuilder {
            position: None,
            module,
            imports: Imports::default(),
            prototypes: Prototypes::default(),
            used_names: HashSet::with_capacity(3),
            type_aliases: HashMap::new(),
            generic_types: HashSet::with_capacity(3),
            tmp_const: Rc::new("tmp".to_string()),
            this_const: Rc::new("This".to_string()),
        }
    }
}

pub struct Pointer {
    pub function: MutRc<Function>,
    block: Rc<String>,
}

#[derive(Default)]
pub struct Imports {
    pub classes: HashMap<Rc<String>, MutRc<Class>>,
    pub interfaces: HashMap<Rc<String>, MutRc<Interface>>,
    pub functions: HashMap<Rc<String>, Rc<Variable>>,
}

/// A list of all prototypes.
/// Prototypes are things with generic parameters, that are
/// turned into a concrete implementation when used with
/// generic arguments.
/// Note that the prototypes in the builder also include imported
/// prototypes, since they do not end up in the final module either way.
#[derive(Default)]
pub struct Prototypes {
    pub classes: HashMap<Rc<String>, MutRc<ClassPrototype>>,
    pub interfaces: HashMap<Rc<String>, MutRc<InterfacePrototype>>,
    pub functions: HashMap<Rc<String>, MutRc<FunctionPrototype>>,
}
