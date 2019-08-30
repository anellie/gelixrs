/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/30/19 6:21 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use super::super::nodes::{MIRFunction, MIRType};
use crate::ast::literal::Literal;
use crate::lexer::token::Type;
use crate::mir::nodes::{MIRExpression, MIRFlow, MIRStruct, MIRStructMem, MIRVariable};
use crate::mir::{mutrc_new, MutRc};
use std::collections::HashMap;
use std::rc::Rc;

pub struct MIRBuilder {
    position: Option<Pointer>,
    functions: HashMap<Rc<String>, MutRc<MIRFunction>>,
    types: HashMap<Rc<String>, MutRc<MIRStruct>>,

    /// Simply a const of the string "tmp".
    /// Used for temporary variables needed for class init.
    tmp_const: Rc<String>,
}

impl MIRBuilder {
    pub fn create_struct(&mut self, name: Rc<String>) -> Option<MutRc<MIRStruct>> {
        let class = mutrc_new(MIRStruct {
            name: Rc::clone(&name),
            members: HashMap::new(),
            member_order: Vec::new(),
            super_struct: None,
        });

        if !self.types.contains_key(&name) {
            self.types.insert(Rc::clone(&name), Rc::clone(&class));
            Some(class)
        } else {
            // Struct already exists
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

        if !self.functions.contains_key(&name) {
            self.functions
                .insert(Rc::clone(&name), Rc::clone(&function));
            Some(function)
        } else {
            None
        }
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

    pub fn build_bitcast(&self, obj: MIRExpression, goal: &MutRc<MIRStruct>) -> MIRExpression {
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

    pub fn build_constructor(&mut self, class_ref: MutRc<MIRStruct>) -> MIRExpression {
        let class = class_ref.borrow();
        let var = Rc::new(MIRVariable::new(
            Rc::clone(&self.tmp_const),
            MIRType::Struct(Rc::clone(&class_ref)),
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

    pub fn build_phi(
        &self,
        first: (MIRExpression, Rc<String>),
        second: (MIRExpression, Rc<String>),
    ) -> MIRExpression {
        MIRExpression::Phi(vec![first, second])
    }

    pub fn build_literal(&self, literal: Literal) -> MIRExpression {
        MIRExpression::Literal(literal)
    }

    pub fn build_struct_get(
        &self,
        object: MIRExpression,
        field: Rc<MIRStructMem>,
    ) -> MIRExpression {
        MIRExpression::StructGet {
            object: Box::new(object),
            index: field.index,
        }
    }

    pub fn build_struct_set(
        &self,
        object: MIRExpression,
        field: Rc<MIRStructMem>,
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
            block.last = ret
        }
    }

    pub fn find_type(&self, name: &String) -> Option<MIRType> {
        Some(match &name[..] {
            "None" => MIRType::None,
            "bool" => MIRType::Bool,
            "i64" => MIRType::Int,
            "f64" => MIRType::Double,
            "String" => MIRType::String,
            _ => MIRType::Struct(self.find_struct(name)?),
        })
    }

    pub fn find_struct(&self, name: &String) -> Option<MutRc<MIRStruct>> {
        Some(Rc::clone(self.types.get(name)?))
    }

    pub fn find_function(&self, name: &String) -> Option<MutRc<MIRFunction>> {
        Some(Rc::clone(self.functions.get(name)?))
    }

    pub fn set_pointer(&mut self, function: MutRc<MIRFunction>, block: Rc<String>) {
        self.position = Some(Pointer { function, block })
    }

    pub fn set_block(&mut self, block: &Rc<String>) {
        if let Some(pos) = self.position.as_mut() {
            pos.block = Rc::clone(block)
        }
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

    pub fn get_types(self) -> Vec<MutRc<MIRStruct>> {
        self.types.into_iter().map(|(_, v)| v).collect()
    }

    pub fn new() -> MIRBuilder {
        MIRBuilder {
            position: None,
            types: HashMap::new(),
            functions: HashMap::new(),
            tmp_const: Rc::new("tmp".to_string()),
        }
    }
}

pub struct Pointer {
    pub function: MutRc<MIRFunction>,
    block: Rc<String>,
}
