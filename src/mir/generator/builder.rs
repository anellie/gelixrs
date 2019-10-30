/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/30/19 8:03 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::mem::discriminant;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::literal::Literal;
use crate::ast::Type as ASTType;
use crate::lexer::token::{Token, TType};
use crate::mir::{MIRModule, MutRc, mutrc_new};
use crate::mir::nodes::{
    Class, ClassMember, Expression, Flow, Interface, Variable,
};
use crate::ModulePath;

use super::super::nodes::{Function, Type};

/// A builder for assisting in creating MIR.
pub struct MIRBuilder {
    /// The current insertion position.
    position: Option<Pointer>,

    /// The module the builder is inserting into.
    pub module: MIRModule,

    /// Things imported into this module by 'import' declarations.
    imports: Imports,

    /// A list of type aliases, where the key is a type that will be translated to the value.
    type_aliases: HashMap<Rc<String>, ASTType>,

    /// Simply a const of the string "tmp".
    /// Used for temporary variables needed for class init.
    tmp_const: Rc<String>,
    /// Simply a const of the string "This".
    /// Used for the This alias inside classes/interfaces.
    this_const: Rc<String>,
}

impl MIRBuilder {
    pub fn create_class(&mut self, name: &Token) -> Option<MutRc<Class>> {
        let class = mutrc_new(Class {
            name: Rc::clone(&name.lexeme),
            members: IndexMap::new(),
            methods: HashMap::new(),
            interfaces: IndexMap::new(),
        });

        if self.find_class(&name.lexeme).is_none() {
            self.module
                .classes
                .insert(Rc::clone(&name.lexeme), Rc::clone(&class));

            self.add_this_alias(name);
            Some(class)
        } else {
            // Class already exists
            None
        }
    }

    pub fn add_imported_class(
        &mut self,
        class: MutRc<Class>,
        import_methods: bool,
    ) -> Option<()> {
        let name = Rc::clone(&class.borrow().name);
        if self.find_class(&name).is_none() {
            self.imports
                .classes
                .insert(Rc::clone(&name), Rc::clone(&class));
            if import_methods {
                for (_, method) in class.borrow().methods.iter() {
                    self.add_imported_function(Rc::clone(method))?;
                }
            }
            Some(())
        } else {
            // Class already exists
            None
        }
    }

    pub fn create_function(
        &mut self,
        name: Rc<String>,
        ret_type: Type,
        parameters: Vec<Rc<Variable>>,
    ) -> Option<MutRc<Function>> {
        let function = mutrc_new(Function {
            name: Rc::clone(&name),
            parameters,
            blocks: HashMap::new(),
            variables: HashMap::new(),
            ret_type,
        });

        if self.find_function(&name).is_none() {
            Some(function)
        } else {
            None
        }
    }

    pub fn add_imported_function(&mut self, func: Rc<Variable>) -> Option<()> {
        let name = &func.name;
        if self.find_function(&name).is_none() {
            self.module
                .imported_func
                .insert(Rc::clone(&name), Rc::clone(&func));
            Some(())
        } else {
            // Function already exists
            None
        }
    }

    pub fn add_imported_iface(&mut self, iface: MutRc<Interface>) -> Option<()> {
        let name = Rc::clone(&iface.borrow().name);
        if self.find_interface(&name).is_none() {
            self.imports
                .interfaces
                .insert(Rc::clone(&name), Rc::clone(&iface));
            Some(())
        } else {
            // Interface already exists
            None
        }
    }

    pub fn create_interface(&mut self, name: &Rc<String>) -> Option<MutRc<Interface>> {
        let iface = mutrc_new(Interface {
            name: Rc::clone(name),
            methods: IndexMap::new(),
            generics: Vec::new(),
        });

        if self.find_interface(name).is_none() {
            self.module
                .interfaces
                .insert(Rc::clone(name), Rc::clone(&iface));
            Some(iface)
        } else {
            // Interface already exists
            None
        }
    }

    pub fn add_global(&mut self, name: Rc<String>, variable: Rc<Variable>) {
        if let Type::Function(_) = variable.type_ {
            self.module.functions.insert(name, variable);
        } else {
            panic!("Invalid global")
        }
    }

    pub fn find_global(&self, name: &String) -> Option<Rc<Variable>> {
        self.module
            .functions
            .get(name)
            .or_else(|| self.module.imported_func.get(name))
            .map(Rc::clone)
    }

    /// Will create the variable in the current function.
    pub fn add_function_variable(&mut self, variable: Rc<Variable>) {
        let func = self.cur_fn();
        func.borrow_mut()
            .insert_var(Rc::clone(&variable.name), variable);
    }

    pub fn build_binary(
        &self,
        left: Expression,
        operator: TType,
        right: Expression,
    ) -> Expression {
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
            .find_global(&format!("{}-internal-init", &class.name))
            .unwrap();
        let init_call = Expression::Call {
            callee: Box::new(Expression::VarGet(init_fn)),
            arguments: vec![Expression::VarGet(Rc::clone(&var))],
        };
        self.insert_at_ptr(init_call);

        let user_init = self.find_global(&format!("{}-init", &class.name));
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

    pub fn build_literal(&self, literal: Literal) -> Expression {
        Expression::Literal(literal)
    }

    pub fn build_struct_get(
        &self,
        object: Expression,
        field: Rc<ClassMember>,
    ) -> Expression {
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

    pub fn find_class(&self, name: &String) -> Option<MutRc<Class>> {
        Some(Rc::clone(
            self.module
                .classes
                .get(name)
                .or_else(|| self.imports.classes.get(name))?,
        ))
    }

    pub fn find_function(&self, name: &String) -> Option<MutRc<Function>> {
        Some(Rc::clone(
            self.module
                .functions
                .get(name)
                .or_else(|| self.module.imported_func.get(name))
                .map(|f| {
                    if let Type::Function(f) = &f.type_ {
                        f
                    } else {
                        panic!("Not a function!")
                    }
                })?,
        ))
    }

    pub fn find_interface(&self, name: &String) -> Option<MutRc<Interface>> {
        Some(Rc::clone(
            self.module.interfaces.get(name).or_else(|| self.imports.interfaces.get(name))?
        ))
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

    /// Will turn the passed in type into a concrete type, should it still be a generic one.
    pub fn translate_generic(&mut self, ty: &Type) -> Type {
        if let Type::Generic(name) = ty {
            self.find_type(self.type_aliases.get(name).unwrap())
                .unwrap()
        } else {
            ty.clone()
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
            type_aliases: HashMap::new(),
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
    classes: HashMap<Rc<String>, MutRc<Class>>,
    interfaces: HashMap<Rc<String>, MutRc<Interface>>,
}
