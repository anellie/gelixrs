/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/26/19 7:56 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/26/19 6:48 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/24/19 5:50 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use super::super::mir::{MIRFunction, MIRType};
use crate::mir::mir::{MIRStruct, MIRVariable, MIRExpression};
use std::collections::HashMap;
use crate::mir::{MIR, MutRc, mutrc_new};
use std::rc::Rc;
use crate::lexer::token::Token;

pub struct MIRBuilder {
    position: Option<Pointer>,
    functions: HashMap<Rc<String>, MutRc<MIRFunction>>,
    types: HashMap<Rc<String>, MutRc<MIRStruct>>,
}

impl MIRBuilder {
    pub fn create_struct(&mut self, name: Rc<String>) -> Option<MutRc<MIRStruct>> {
        let class = mutrc_new(MIRStruct {
            name: Rc::clone(&name),
            members: HashMap::new(),
            super_struct: None
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
        parameters: Vec<Rc<MIRVariable>>
    ) -> Option<MutRc<MIRFunction>> {
        let function = mutrc_new(MIRFunction {
            name: Rc::clone(&name),
            parameters,
            blocks: HashMap::new(),
            variables: HashMap::new(),
            ret_type
        });

        if !self.functions.contains_key(&name) {
            self.functions.insert(Rc::clone(&name), Rc::clone(&function));
            Some(function)
        } else {
            None
        }
    }

    /// Will create the variable in the current function.
    pub fn add_function_variable(&mut self, variable: Rc<MIRVariable>) {
        let func = self.cur_fn();
        func.borrow_mut().variables.insert(Rc::clone(&variable.name), variable);
    }

    pub fn build_binary(
        &self,
        left: MIRExpression,
        operator: Token,
        right: MIRExpression
    ) -> MIRExpression {
        MIRExpression::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right)
        }
    }

    pub fn build_call(&mut self, callee: MIRExpression, args: Vec<MIRExpression>) -> MIRExpression {
        MIRExpression::Call {
            callee: Box::new(callee),
            arguments: args
        }
    }
    
    pub fn build_store(&self, var: Rc<MIRVariable>, value: MIRExpression) -> MIRExpression {
        MIRExpression::VarStore {
            var,
            value: Box::new(value)
        }
    }

    pub fn build_load(&self, var: Rc<MIRVariable>) -> MIRExpression {
        MIRExpression::VarGet(var)
    }

    pub fn find_type(&self, name: &String) -> Option<MIRType> {
        Some(match &name[..] {
            "None" => MIRType::None,
            "bool" => MIRType::Bool,
            "i64" => MIRType::Int,
            "f64" => MIRType::Double,
            "String" => MIRType::String,
            _ => MIRType::Struct(self.find_struct(name)?)
        })
    }

    pub fn find_struct(&self, name: &String) -> Option<MutRc<MIRStruct>> {
        Some(Rc::clone(self.types.get(name)?))
    }

    pub fn find_function(&self, name: &String) -> Option<MutRc<MIRFunction>> {
        Some(Rc::clone(self.functions.get(name)?))
    }

    pub fn set_pointer(&mut self, function: MutRc<MIRFunction>, block: Rc<String>) {
        self.position = Some(Pointer {
            function,
            block
        })
    }

    pub fn insert_at_ptr(&mut self, expr: MIRExpression) {
        let func = self.cur_fn();
        let mut func = func.borrow_mut();
        func.blocks.get_mut(&self.position.as_ref().unwrap().block).unwrap().expressions.push(expr);
    }

    fn cur_fn(&self) -> MutRc<MIRFunction> {
        Rc::clone(&self.position.as_ref().unwrap().function)
    }

    pub fn get_mir(self) -> MIR {
        MIR {
            types: self.types.into_iter().map(|(_, v)| v).collect(),
            functions: self.functions
        }
    }

    pub fn new() -> MIRBuilder {
        MIRBuilder {
            position: None,
            types: HashMap::new(),
            functions: HashMap::new(),
        }
    }
}

pub struct Pointer {
    pub function: MutRc<MIRFunction>,
    block: Rc<String>,
}