/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/30/19 10:41 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::expression::LOGICAL_BINARY;
use crate::ast::literal::Literal;
use crate::lexer::token::Type;
use crate::mir::MutRc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// All types in Gelix.
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

impl Display for MIRType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            MIRType::None => write!(f, "None"),
            MIRType::Bool => write!(f, "bool"),
            MIRType::Int => write!(f, "i64"),
            MIRType::Float => write!(f, "f32"),
            MIRType::Double => write!(f, "f64"),
            MIRType::String => write!(f, "String"),
            MIRType::Function(_) => write!(f, "<func>"),
            MIRType::Struct(struc) => write!(f, "{}", struc.borrow().name),
        }
    }
}

/// A struct that represents a class.
#[derive(Debug)]
pub struct MIRStruct {
    pub name: Rc<String>,
    pub members: HashMap<Rc<String>, Rc<MIRStructMem>>,
    pub member_order: Vec<Rc<MIRStructMem>>,
    pub super_struct: Option<MutRc<MIRStruct>>,
}

impl PartialEq for MIRStruct {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// A member of a struct.
#[derive(Debug)]
pub struct MIRStructMem {
    pub mutable: bool,
    pub _type: MIRType,
    pub index: u32,
}

/// A function in MIR. Consists of blocks.
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
    /// Appends a new block; will returns the block name.
    /// The name can be different than the given one when it was already in use.
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

    /// Inserts a variable into the functions allocation table.
    pub fn insert_var(&mut self, mut name: Rc<String>, var: Rc<MIRVariable>) -> Rc<String> {
        if self.variables.contains_key(&name) {
            name = Rc::new(format!("{}-{}", name, self.variables.len()));
        }
        self.variables.insert(Rc::clone(&name), var);
        name
    }
}

/// A variable inside a function.
#[derive(Debug, Clone)]
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

/// A block inside a function.
#[derive(Debug)]
pub struct MIRBlock {
    pub expressions: Vec<MIRExpression>,
    pub last: MIRFlow,
}

/// The last part of a block.
#[derive(Debug)]
pub enum MIRFlow {
    /// Return void
    None,

    /// Jump to another block
    Jump(Rc<String>),

    /// Jump to another block conditionally
    Branch {
        condition: MIRExpression,
        then_b: Rc<String>,
        else_b: Rc<String>,
    },

    /// Same as branch, but with a list of conditions.
    /// Jumps to the first that matches.
    Switch {
        cases: Vec<(MIRExpression, Rc<String>)>,
        default: Rc<String>
    },

    /// Return a value
    Return(MIRExpression),
}

/// All expressions in MIR. All of them produce a value.
#[derive(Debug, Clone)]
pub enum MIRExpression {
    /// Simply a binary operation between numbers.
    Binary {
        left: Box<MIRExpression>,
        operator: Type,
        right: Box<MIRExpression>,
    },

    /// Casts a struct to another struct type.
    Bitcast {
        object: Box<MIRExpression>,
        goal: MutRc<MIRStruct>,
    },

    /// A function call.
    Call {
        callee: Box<MIRExpression>,
        arguments: Vec<MIRExpression>,
    },

    /// Simply produces the function as a value.
    Function(MutRc<MIRFunction>),

    /// A Phi node. Returns a different value based on
    /// which block the current block was reached from.
    Phi(Vec<(MIRExpression, Rc<String>)>),

    /// Gets a member of a struct.
    StructGet {
        object: Box<MIRExpression>,
        index: u32,
    },

    /// Sets a member of a struct.
    StructSet {
        object: Box<MIRExpression>,
        index: u32,
        value: Box<MIRExpression>,
    },

    /// Simply produces the literal as value.
    Literal(Literal),

    /// A unary expression on numbers.
    Unary {
        operator: Type,
        right: Box<MIRExpression>,
    },

    /// Returns a variable.
    VarGet(Rc<MIRVariable>),

    /// Stores a value inside a variable.
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
                if LOGICAL_BINARY.contains(&operator) {
                    MIRType::Bool
                } else {
                    left.get_type()
                }
            }

            MIRExpression::Bitcast { goal, .. } => MIRType::Struct(Rc::clone(goal)),

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
                MIRExpression::type_from_struct_get(object, *index)
            }

            MIRExpression::StructSet { object, index, .. } => {
                MIRExpression::type_from_struct_get(object, *index)
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

            MIRExpression::Unary { operator, right } => match operator {
                Type::Bang => MIRType::Bool,
                Type::Minus => right.get_type(),
                _ => panic!("invalid unary"),
            },

            MIRExpression::VarGet(var) => var._type.clone(),

            MIRExpression::VarStore { var, .. } => var._type.clone(),
        }
    }

    /// Returns the type of a struct member.
    fn type_from_struct_get(object: &MIRExpression, index: u32) -> MIRType {
        let object = object.get_type();
        if let MIRType::Struct(struc) = object {
            RefCell::borrow(&struc)
                .members
                .iter()
                .find(|(_, mem)| mem.index == index)
                .unwrap()
                .1
                ._type
                .clone()
        } else {
            panic!("non-struct struct get")
        }
    }
}