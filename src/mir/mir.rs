/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/24/19 5:49 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use std::collections::HashMap;
use crate::ast::literal::Literal;
use crate::lexer::token::Token;
use inkwell::values::PointerValue;
use crate::mir::MutRc;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum MIRType {
    None,
    Bool,
    Int,
    Float,
    Double,
    String,
    Function(MutRc<MIRFunction>),
    Struct(MutRc<MIRStruct>)
}

#[derive(Debug)]
pub struct MIRStruct {
    pub name: Rc<String>,
    pub members: HashMap<Rc<String>, Rc<MIRStructMem>>,
    pub super_struct: Option<MutRc<MIRStruct>>
}

#[derive(Debug)]
pub struct MIRStructMem {
    pub mutable: bool,
    pub _type: MIRType,
    pub index: u32,
}

impl PartialEq for MIRStruct {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

#[derive(Debug)]
pub struct MIRFunction {
    pub name: Rc<String>,
    pub parameters: Vec<MIRFuncArg>,
    pub blocks: HashMap<Rc<String>, MIRBlock>,
    pub variables: HashMap<Rc<String>, Rc<MIRVariable>>,
    pub ret_type: MIRType
}

impl PartialEq for MIRFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl MIRFunction {
    pub fn append_block(&mut self, mut name: String) -> Rc<String> {
        if self.blocks.contains_key(&name) {
            name = format!("name-{}", self.blocks.len());
        }
        let rc = Rc::new(name);
        self.blocks.insert(Rc::clone(&rc), MIRBlock {
            expressions: Vec::with_capacity(5),
            last: MIRFlow::Return(None)
        });
        rc
    }
}

#[derive(Debug)]
pub struct MIRFuncArg {
    pub name: Rc<String>,
    pub _type: MIRType
}

#[derive(Debug)]
pub struct MIRVariable {
    pub mutable: bool,
    pub _type: MIRType,
    pub name: Rc<String>,
    pub alloca: Option<PointerValue>
}

impl MIRVariable {
    pub fn new(name: Rc<String>, _type: MIRType, mutable: bool) -> MIRVariable {
        MIRVariable { name, _type, mutable, alloca: None }
    }
}

#[derive(Debug)]
pub struct MIRBlock {
    pub expressions: Vec<MIRExpression>,
    pub last: MIRFlow
}

#[derive(Debug)]
pub enum MIRFlow {
    Jump(String),

    Branch {
        condition: MIRExpression,
        then_b: String,
        else_b: String
    },

    Return(Option<MIRExpression>)
}

#[derive(Debug)]
pub enum MIRExpression {
    // Maybe turn this into a function call? » left.add(right) «
    Binary {
        left: Box<MIRExpression>,
        operator: Token,
        right: Box<MIRExpression>
    },

    // Maybe already resolve the callee to a function?
    Call {
        callee: Box<MIRExpression>,
        arguments: Vec<MIRExpression>
    },

    StructGet {
        object: Box<MIRExpression>,
        index: u32
    },

    StructSet {
        object: Box<MIRExpression>,
        index: u32,
        value: Box<MIRExpression>
    },

    Literal(Literal),

    Unary {
        operator: Token,
        right: Box<MIRExpression>
    },

    VarGet(MIRVariable),

    VarStore {
        var: Rc<String>,
        value: Box<MIRExpression>
    }
}