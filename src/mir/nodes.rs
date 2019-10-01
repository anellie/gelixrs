/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/2/19 1:40 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::expression::LOGICAL_BINARY;
use crate::ast::literal::Literal;
use crate::lexer::token::Type;
use crate::mir::MutRc;

/// All types in Gelix.
#[derive(Debug, Clone)]
pub enum MIRType {
    Any,
    None,
    Bool,

    I8,
    I16,
    I32,
    I64,

    F32,
    F64,

    String,

    Array(Box<MIRType>),

    Function(MutRc<MIRFunction>),

    Class(MutRc<MIRClass>),

    Interface(MutRc<MIRInterface>),
}

impl PartialEq for MIRType {
    fn eq(&self, other: &Self) -> bool {
        if let MIRType::Any = other {
            return true;
        }

        match self {
            MIRType::Function(func) => {
                if let MIRType::Function(other) = other {
                    func == other
                } else {
                    false
                }
            }

            MIRType::Class(class) => {
                if let MIRType::Class(other) = other {
                    class == other
                } else {
                    false
                }
            }

            MIRType::Any => true,

            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

impl Display for MIRType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            MIRType::Array(arr) => write!(f, "[{}]", arr),
            MIRType::Function(func) => write!(f, "<func {}>", func.borrow().name),
            MIRType::Class(class) => write!(f, "{}", class.borrow().name),
            MIRType::Interface(iface) => write!(f, "{}", iface.borrow().name),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[derive(Debug)]
pub struct MIRClass {
    pub name: Rc<String>,
    /// All class members.
    pub members: IndexMap<Rc<String>, Rc<MIRClassMember>>,
    /// All class methods. Inserted as "doThing", not "Name-doThing".
    pub methods: HashMap<Rc<String>, Rc<MIRVariable>>,
}

impl PartialEq for MIRClass {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// A member of a class.
#[derive(Debug)]
pub struct MIRClassMember {
    pub mutable: bool,
    pub _type: MIRType,
    pub index: u32,
}

/// An abstract interface defining all its methods.
#[derive(Debug)]
pub struct MIRInterface {
    pub name: Rc<String>,
    // A map of all methods. If the method does not have a default implementation, it has no blocks/variables.
    pub methods: IndexMap<Rc<String>, Rc<MIRVariable>>,
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
    /// Returns the name of it (should be used since a change can be needed due to colliding names).
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
        default: Rc<String>,
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

    /// Casts a class to another class type.
    Bitcast {
        object: Box<MIRExpression>,
        goal: MutRc<MIRClass>,
    },

    /// A function call.
    Call {
        callee: Box<MIRExpression>,
        arguments: Vec<MIRExpression>,
    },

    /// An expression indicating that any code after it should be discarded,
    /// and that the next statement in the block should be the terminator ([MIRFlow]).
    DoRet,

    /// Simply produces the function as a value.
    Function(MutRc<MIRFunction>),

    /// A Phi node. Returns a different value based on
    /// which block the current block was reached from.
    Phi(Vec<(MIRExpression, Rc<String>)>),

    /// Gets a member of a class struct.
    StructGet {
        object: Box<MIRExpression>,
        index: u32,
    },

    /// Sets a member of a class struct.
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

            MIRExpression::Bitcast { goal, .. } => MIRType::Class(Rc::clone(goal)),

            MIRExpression::Call { callee, .. } => {
                if let MIRType::Function(func) = callee.get_type() {
                    RefCell::borrow(&func).ret_type.clone()
                } else {
                    panic!("non-function call type")
                }
            }

            MIRExpression::DoRet => MIRType::None,

            MIRExpression::Function(func) => MIRType::Function(func.clone()),

            MIRExpression::Phi(branches) => branches.first().unwrap().0.get_type(),

            MIRExpression::StructGet { object, index } => {
                MIRExpression::type_from_struct_get(object, *index)
            }

            MIRExpression::StructSet { object, index, .. } => {
                MIRExpression::type_from_struct_get(object, *index)
            }

            MIRExpression::Literal(literal) => match literal {
                Literal::Any => MIRType::Any,
                Literal::None => MIRType::None,
                Literal::Bool(_) => MIRType::Bool,
                Literal::I8(_) => MIRType::I8,
                Literal::I16(_) => MIRType::I16,
                Literal::I32(_) => MIRType::I32,
                Literal::I64(_) => MIRType::I64,
                Literal::F32(_) => MIRType::F32,
                Literal::F64(_) => MIRType::F64,
                Literal::String(_) => MIRType::String,
                Literal::Array(arr) => {
                    MIRType::Array(Box::new(arr.as_ref().right().unwrap().type_.clone()))
                }
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
        if let MIRType::Class(class) = object {
            RefCell::borrow(&class)
                .members
                .iter()
                .find(|(_, mem)| mem.index == index)
                .unwrap()
                .1
                ._type
                .clone()
        } else {
            panic!("non-class struct get")
        }
    }
}

/// An array literal in MIR. See ast/literal.rs for usage.
#[derive(Debug, Clone)]
pub struct MIRArray {
    pub values: Vec<MIRExpression>,
    pub type_: MIRType,
}
