/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/3/19 2:36 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::mem::discriminant;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::declaration::ASTType;
use crate::ast::literal::Literal;
use crate::lexer::token::Type;
use crate::mir::{MIRModule, MutRc, mutrc_new};
use crate::mir::nodes::{MIRClass, MIRClassMember, MIRExpression, MIRFlow, MIRInterface, MIRVariable};
use crate::ModulePath;

use super::super::nodes::{MIRFunction, MIRType};

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
}

impl MIRBuilder {
    pub fn create_class(&mut self, name: Rc<String>) -> Option<MutRc<MIRClass>> {
        let class = mutrc_new(MIRClass {
            name: Rc::clone(&name),
            members: IndexMap::new(),
            methods: HashMap::new(),
            interfaces: Vec::new(),
        });

        if self.find_class(&name).is_none() {
            self.module
                .classes
                .insert(Rc::clone(&name), Rc::clone(&class));
            Some(class)
        } else {
            // Class already exists
            None
        }
    }

    pub fn add_imported_class(
        &mut self,
        class: MutRc<MIRClass>,
        import_methods: bool,
    ) -> Option<()> {
        let name = Rc::clone(&class.borrow().name);
        if self.find_class(&name).is_none() {
            self.imports.classes
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
        ret_type: MIRType,
        parameters: Vec<Rc<MIRVariable>>,
    ) -> Option<MutRc<MIRFunction>> {
        let function = mutrc_new(MIRFunction {
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

    pub fn add_imported_function(&mut self, func: Rc<MIRVariable>) -> Option<()> {
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

    pub fn add_imported_iface(
        &mut self,
        iface: MutRc<MIRInterface>,
    ) -> Option<()> {
        let name = Rc::clone(&iface.borrow().name);
        if self.find_interface(&name).is_none() {
            self.imports.interfaces
                .insert(Rc::clone(&name), Rc::clone(&iface));
            Some(())
        } else {
            // Interface already exists
            None
        }
    }

    pub fn create_interface(&mut self, name: &Rc<String>) -> Option<MutRc<MIRInterface>> {
        let iface = mutrc_new(MIRInterface {
            name: Rc::clone(name),
            methods: IndexMap::new(),
            generics: Vec::new()
        });

        if self.find_interface(name).is_none() {
            self.module.interfaces.insert(Rc::clone(name), Rc::clone(&iface));
            Some(iface)
        } else {
            // Interface already exists
            None
        }
    }

    pub fn add_global(&mut self, name: Rc<String>, variable: Rc<MIRVariable>) {
        if let MIRType::Function(_) = variable._type {
            self.module.functions.insert(name, variable);
        } else {
            panic!("Invalid global")
        }
    }

    pub fn find_global(&self, name: &String) -> Option<Rc<MIRVariable>> {
        self.module
            .functions
            .get(name)
            .or_else(|| self.module.imported_func.get(name))
            .map(Rc::clone)
    }

    /// Will create the variable in the current function.
    pub fn add_function_variable(&mut self, variable: Rc<MIRVariable>) {
        let func = self.cur_fn();
        func.borrow_mut()
            .insert_var(Rc::clone(&variable.name), variable);
    }

    pub fn build_binary(
        &self,
        left: MIRExpression,
        operator: Type,
        right: MIRExpression,
    ) -> MIRExpression {
        MIRExpression::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        }
    }

    pub fn build_unary(&self, right: MIRExpression, op: Type) -> MIRExpression {
        MIRExpression::Unary {
            operator: op,
            right: Box::new(right),
        }
    }

    pub fn build_bitcast(&self, obj: MIRExpression, goal: &MutRc<MIRClass>) -> MIRExpression {
        MIRExpression::Bitcast {
            object: Box::new(obj),
            goal: Rc::clone(&goal),
        }
    }

    pub fn build_call(&mut self, callee: MIRExpression, args: Vec<MIRExpression>) -> MIRExpression {
        MIRExpression::Call {
            callee: Box::new(callee),
            arguments: args,
        }
    }

    pub fn build_constructor(&mut self, class_ref: MutRc<MIRClass>) -> MIRExpression {
        let class = class_ref.borrow();
        let var = Rc::new(MIRVariable::new(
            Rc::clone(&self.tmp_const),
            MIRType::Class(Rc::clone(&class_ref)),
            false,
        ));
        self.cur_fn()
            .borrow_mut()
            .insert_var(Rc::clone(&self.tmp_const), Rc::clone(&var));

        let init_fn = self
            .find_function(&format!("{}-internal-init", &class.name))
            .unwrap();
        let init_call = MIRExpression::Call {
            callee: Box::new(MIRExpression::Function(init_fn)),
            arguments: vec![MIRExpression::VarGet(Rc::clone(&var))],
        };
        self.insert_at_ptr(init_call);

        let user_init = self.find_function(&format!("{}-init", &class.name));
        if let Some(user_init) = user_init {
            let init_call = MIRExpression::Call {
                callee: Box::new(MIRExpression::Function(user_init)),
                arguments: vec![MIRExpression::VarGet(Rc::clone(&var))],
            };
            self.insert_at_ptr(init_call);
        }

        MIRExpression::VarGet(var)
    }

    pub fn build_phi(&self, nodes: Vec<(MIRExpression, Rc<String>)>) -> MIRExpression {
        // Filter all nodes that return Any.
        // A node might return Any if it does not produce a value;
        // but instead branches away from the phi.
        let filtered_nodes = nodes
            .into_iter()
            .filter(|node| {
                let type_ = node.0.get_type();
                discriminant(&MIRType::Any) != discriminant(&type_)
            })
            .collect();

        MIRExpression::Phi(filtered_nodes)
    }

    pub fn build_literal(&self, literal: Literal) -> MIRExpression {
        MIRExpression::Literal(literal)
    }

    pub fn build_struct_get(
        &self,
        object: MIRExpression,
        field: Rc<MIRClassMember>,
    ) -> MIRExpression {
        MIRExpression::StructGet {
            object: Box::new(object),
            index: field.index,
        }
    }

    pub fn build_struct_set(
        &self,
        object: MIRExpression,
        field: Rc<MIRClassMember>,
        value: MIRExpression,
    ) -> MIRExpression {
        MIRExpression::StructSet {
            object: Box::new(object),
            index: field.index,
            value: Box::new(value),
        }
    }

    pub fn build_store(&self, var: Rc<MIRVariable>, value: MIRExpression) -> MIRExpression {
        MIRExpression::VarStore {
            var,
            value: Box::new(value),
        }
    }

    pub fn build_load(&self, var: Rc<MIRVariable>) -> MIRExpression {
        MIRExpression::VarGet(var)
    }

    pub fn build_branch(&mut self, cond: MIRExpression, then: &Rc<String>, else_: &Rc<String>) {
        self.set_return(MIRFlow::Branch {
            condition: cond,
            then_b: Rc::clone(&then),
            else_b: Rc::clone(&else_),
        });
    }

    pub fn build_jump(&mut self, to: &Rc<String>) {
        self.set_return(MIRFlow::Jump(Rc::clone(to)))
    }

    pub fn append_block(&mut self, name: &str) -> Rc<String> {
        self.cur_fn().borrow_mut().append_block(name.to_string())
    }

    pub fn set_return(&mut self, ret: MIRFlow) {
        let cur_fn = self.cur_fn();
        let mut cur_fn = cur_fn.borrow_mut();
        let mut block = cur_fn
            .blocks
            .get_mut(&self.position.as_ref().unwrap().block)
            .unwrap();
        // If the return type is not None, it was already overridden by something else
        // (return or break expression mostly) and should not be changed.
        // (discriminant returns a unique id of an enum variant)
        if std::mem::discriminant(&block.last) == std::mem::discriminant(&MIRFlow::None) {
            block.last = ret;
            drop(cur_fn);
            self.insert_at_ptr(MIRExpression::DoRet)
        }
    }

    pub fn find_type(&self, ast: &ASTType) -> Option<MIRType> {
        Some(match ast {
            ASTType::Token(tok) => {
                if let Some(alias) = self.type_aliases.get(&tok.lexeme) {
                    return self.find_type(alias);
                }

                match &tok.lexeme[..] {
                    "None" => MIRType::None,
                    "bool" => MIRType::Bool,

                    "i8" => MIRType::I8,
                    "i16" => MIRType::I16,
                    "i32" => MIRType::I32,
                    "i64" => MIRType::I64,

                    "f32" => MIRType::F32,
                    "f64" => MIRType::F64,

                    "String" => MIRType::String,

                    _ => self
                        .find_class(&tok.lexeme)
                        .map(|c| MIRType::Class(c))
                        .or_else(|| Some(MIRType::Interface(self.find_interface(&tok.lexeme)?)))?,
                }
            },

            ASTType::Array(type_) => MIRType::Array(Box::new(self.find_type(type_)?)),

            ASTType::Closure { .. } => unimplemented!(),

            ASTType::Generic { .. } => unimplemented!(),
        })
    }

    pub fn find_class(&self, name: &String) -> Option<MutRc<MIRClass>> {
        Some(Rc::clone(
            self.module
                .classes
                .get(name)
                .or_else(|| self.imports.classes.get(name))?,
        ))
    }

    pub fn find_function(&self, name: &String) -> Option<MutRc<MIRFunction>> {
        Some(Rc::clone(
            self.module
                .functions
                .get(name)
                .or_else(|| self.module.imported_func.get(name))
                .map(|f| {
                    if let MIRType::Function(f) = &f._type {
                        f
                    } else {
                        panic!("Not a function!")
                    }
                })?,
        ))
    }

    pub fn find_interface(&self, name: &String) -> Option<MutRc<MIRInterface>> {
        Some(Rc::clone(self.module.interfaces.get(name)?))
    }

    pub fn set_pointer(&mut self, function: MutRc<MIRFunction>, block: Rc<String>) {
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

    pub fn remove_alias(&mut self, name: &Rc<String>) {
        self.type_aliases.remove(name);
    }

    pub fn insert_at_ptr(&mut self, expr: MIRExpression) {
        let func = self.cur_fn();
        let mut func = func.borrow_mut();
        func.blocks
            .get_mut(&self.position.as_ref().unwrap().block)
            .unwrap()
            .expressions
            .push(expr);
    }

    pub fn cur_fn(&self) -> MutRc<MIRFunction> {
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
        }
    }
}

pub struct Pointer {
    pub function: MutRc<MIRFunction>,
    block: Rc<String>,
}

#[derive(Default)]
pub struct Imports {
    classes: HashMap<Rc<String>, MutRc<MIRClass>>,
    interfaces: HashMap<Rc<String>, MutRc<MIRInterface>>,
}
