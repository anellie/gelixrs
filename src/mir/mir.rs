/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/28/19 4:40 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::expression::LOGICAL_BINARY;
use crate::ast::literal::Literal;
use crate::lexer::token::{Token, Type};
use crate::mir::MutRc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
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
    Struct(MutRc<MIRStruct>),
}

#[derive(Debug)]
pub struct MIRStruct {
    pub name: Rc<String>,
    pub members: HashMap<Rc<String>, Rc<MIRStructMem>>,
    pub member_order: Vec<Rc<MIRStructMem>>,
    pub super_struct: Option<MutRc<MIRStruct>>,
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
    pub parameters: Vec<Rc<MIRVariable>>,
    pub blocks: HashMap<Rc<String>, MIRBlock>,
    pub variables: HashMap<Rc<String>, Rc<MIRVariable>>,
    pub ret_type: MIRType,
}

impl PartialEq for MIRFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl MIRFunction {
    pub fn append_block(&mut self, mut name: String) -> Rc<String> {
        if self.blocks.contains_key(&name) {
            name = format!("{}-{}", name, self.blocks.len());
        }
        let rc = Rc::new(name);
        self.blocks.insert(
            Rc::clone(&rc),
            MIRBlock {
                expressions: Vec::with_capacity(5),
                last: MIRFlow::None,
            },
        );
        rc
    }

    pub fn insert_var(&mut self, mut name: Rc<String>, var: Rc<MIRVariable>) -> Rc<String> {
        if self.variables.contains_key(&name) {
            name = Rc::new(format!("{}-{}", name, self.variables.len()));
        }
        self.variables.insert(Rc::clone(&name), var);
        name
    }
}

#[derive(Debug)]
pub struct MIRVariable {
    pub mutable: bool,
    pub _type: MIRType,
    pub name: Rc<String>,
}

impl MIRVariable {
    pub fn new(name: Rc<String>, _type: MIRType, mutable: bool) -> MIRVariable {
        MIRVariable {
            name,
            _type,
            mutable,
        }
    }
}

impl Hash for MIRVariable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

#[derive(Debug)]
pub struct MIRBlock {
    pub expressions: Vec<MIRExpression>,
    pub last: MIRFlow,
}

#[derive(Debug)]
pub enum MIRFlow {
    None,

    Jump(Rc<String>),

    Branch {
        condition: MIRExpression,
        then_b: Rc<String>,
        else_b: Rc<String>,
    },

    Return(MIRExpression),
}

#[derive(Debug)]
pub enum MIRExpression {
    // Maybe turn this into a function call? » left.add(right) «
    Binary {
        left: Box<MIRExpression>,
        operator: Token,
        right: Box<MIRExpression>,
    },

    Call {
        callee: Box<MIRExpression>,
        arguments: Vec<MIRExpression>,
    },

    Function(MutRc<MIRFunction>),

    Phi(Vec<(MIRExpression, Rc<String>)>),

    StructGet {
        object: Box<MIRExpression>,
        index: u32,
    },

    StructSet {
        object: Box<MIRExpression>,
        index: u32,
        value: Box<MIRExpression>,
    },

    Literal(Literal),

    Unary {
        operator: Token,
        right: Box<MIRExpression>,
    },

    VarGet(Rc<MIRVariable>),

    VarStore {
        var: Rc<MIRVariable>,
        value: Box<MIRExpression>,
    },
}

impl MIRExpression {
    /// Returns the type of this MIRExpression.
    /// Note that this function does not do type validation, and calling this function
    /// on malformed expressions is undefined behavior that can lead to panics.
    pub(super) fn get_type(&self) -> MIRType {
        match self {
            MIRExpression::Binary { left, operator, .. } => {
                if LOGICAL_BINARY.contains(&operator.t_type) {
                    MIRType::Bool
                } else {
                    left.get_type()
                }
            }

            MIRExpression::Call { callee, .. } => {
                if let MIRType::Function(func) = callee.get_type() {
                    RefCell::borrow(&func).ret_type.clone()
                } else {
                    panic!("non-function call type")
                }
            }

            MIRExpression::Function(func) => func.borrow().ret_type.clone(),

            MIRExpression::Phi(branches) => branches.first().unwrap().0.get_type(),

            MIRExpression::StructGet { object, index } => {
                MIRExpression::type_from_struct_get(object, index)
            }

            MIRExpression::StructSet { object, index, .. } => {
                MIRExpression::type_from_struct_get(object, index)
            }

            MIRExpression::Literal(literal) => match literal {
                Literal::None => MIRType::None,
                Literal::Bool(_) => MIRType::Bool,
                Literal::Int(_) => MIRType::Int,
                Literal::Float(_) => MIRType::Float,
                Literal::Double(_) => MIRType::Double,
                Literal::String(_) => MIRType::String,
                _ => panic!("unknown literal"),
            },

            MIRExpression::Unary { operator, right } => match operator.t_type {
                Type::Bang => MIRType::Bool,
                Type::Minus => right.get_type(),
                _ => panic!("invalid unary"),
            },

            MIRExpression::VarGet(var) => var._type.clone(),

            MIRExpression::VarStore { var, .. } => var._type.clone(),
        }
    }

    fn type_from_struct_get(object: &MIRExpression, index: &u32) -> MIRType {
        let object = object.get_type();
        if let MIRType::Struct(struc) = object {
            RefCell::borrow(&struc)
                .members
                .iter()
                .find(|(_, mem)| mem.index == *index)
                .unwrap()
                .1
                ._type
                .clone()
        } else {
            panic!("non-struct struct get")
        }
    }
}
