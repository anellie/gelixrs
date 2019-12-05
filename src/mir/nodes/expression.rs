/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/5/19 9:46 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::fmt::{Display, Error, Formatter};
use std::rc::Rc;

use crate::ast::expression::LOGICAL_BINARY;
use crate::ast::Literal;
use crate::lexer::token::TType;
use crate::mir::nodes::{Type, Variable};

/// All expressions in MIR. All of them produce a value.
/// Expressions are in blocks in functions. Gelix does not have statements.
#[derive(Debug, Clone)]
pub enum Expression {
    /// Simply a binary operation between numbers.
    Binary {
        left: Box<Expression>,
        operator: TType,
        right: Box<Expression>,
    },

    /// A function call.
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },

    /// A 'flow' expression, which changes control flow. See [Flow] enum
    Flow(Box<Flow>),

    /// A Phi node. Returns a different value based on
    /// which block the current block was reached from.
    Phi(Vec<(Expression, Rc<String>)>),

    /// Gets a member of a class struct.
    StructGet { object: Box<Expression>, index: u32 },

    /// Sets a member of a class struct.
    StructSet {
        object: Box<Expression>,
        index: u32,
        value: Box<Expression>,
    },

    /// Simply produces the literal as value.
    Literal(Literal),

    /// A unary expression on numbers.
    Unary {
        operator: TType,
        right: Box<Expression>,
    },

    /// Returns a variable.
    VarGet(Rc<Variable>),

    /// Stores a value inside a variable.
    VarStore {
        var: Rc<Variable>,
        value: Box<Expression>,
    },
}

impl Expression {
    /// Returns the type of this MIRExpression.
    /// Note that this function does not do type validation, and calling this function
    /// on malformed expressions is undefined behavior that can lead to panics.
    pub fn get_type(&self) -> Type {
        match self {
            Expression::Binary { left, operator, .. } => {
                if LOGICAL_BINARY.contains(&operator) {
                    Type::Bool
                } else {
                    left.get_type()
                }
            }

            Expression::Call { callee, .. } => {
                if let Type::Function(func) = callee.get_type() {
                    RefCell::borrow(&func).ret_type.clone()
                } else {
                    panic!("non-function call type")
                }
            }

            Expression::Flow(_) => Type::None,

            Expression::Phi(branches) => branches.first().unwrap().0.get_type(),

            Expression::StructGet { object, index } => Self::type_from_struct_get(object, *index),

            Expression::StructSet { object, index, .. } => {
                Self::type_from_struct_get(object, *index)
            }

            Expression::Literal(literal) => match literal {
                Literal::Any => Type::Any,
                Literal::None => Type::None,
                Literal::Bool(_) => Type::Bool,
                Literal::I8(_) => Type::I8,
                Literal::I16(_) => Type::I16,
                Literal::I32(_) => Type::I32,
                Literal::I64(_) => Type::I64,
                Literal::F32(_) => Type::F32,
                Literal::F64(_) => Type::F64,
                Literal::String(_) => Type::String,
                _ => panic!("unknown literal"),
            },

            Expression::Unary { operator, right } => match operator {
                TType::Bang => Type::Bool,
                TType::Minus => right.get_type(),
                _ => panic!("invalid unary"),
            },

            Expression::VarGet(var) => var.type_.clone(),

            Expression::VarStore { var, .. } => var.type_.clone(),
        }
    }

    /// Returns the type of a struct member.
    fn type_from_struct_get(object: &Expression, index: u32) -> Type {
        let object = object.get_type();
        if let Type::Class(class) = object {
            class
                .borrow()
                .members
                .iter()
                .find(|(_, mem)| mem.index == index)
                .unwrap()
                .1
                .type_
                .clone()
        } else {
            panic!("non-class struct get")
        }
    }
}

impl Display for Expression {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Expression::Binary {
                left,
                operator,
                right,
            } => write!(f, "({}) {:?} ({})", left, operator, right),

            Expression::Call { callee, arguments } => {
                write!(f, "call {}", callee)?;
                if !arguments.is_empty() {
                    write!(f, " with ")?;
                }
                for arg in arguments.iter() {
                    write!(f, "({}) ", arg)?;
                }
                Ok(())
            }

            Expression::Flow(flow) => write!(f, "{}", flow),

            Expression::Phi(nodes) => {
                write!(f, "phi {{ ")?;
                for (expr, block) in nodes.iter() {
                    write!(f, "{}: ({}), ", block, expr)?;
                }
                write!(f, "}}")
            }

            Expression::StructGet { object, index } => write!(f, "get {} from ({})", index, object),

            Expression::StructSet {
                object,
                index,
                value,
            } => write!(f, "set {} of ({}) to ({})", index, object, value),

            Expression::Literal(literal) => write!(f, "{}", literal),

            Expression::Unary { right, .. } => write!(f, "neg ({})", right),

            Expression::VarGet(var) => write!(f, "{}", var.name),

            Expression::VarStore { var, value } => write!(f, "store ({}) in {}", value, var.name),
        }
    }
}

/// An 'expression' that always yields None, and changes control flow.
#[derive(Clone, Debug)]
pub enum Flow {
    /// Return void from function
    None,

    /// Return a value from function
    Return(Expression),

    /// Jump to another block
    Jump(Rc<String>),

    /// Jump to another block conditionally
    Branch {
        condition: Expression,
        then_b: Rc<String>,
        else_b: Rc<String>,
    },

    /// Same as branch, but with a list of conditions.
    /// Jumps to the first that matches.
    Switch {
        cases: Vec<(Expression, Rc<String>)>,
        default: Rc<String>,
    },
}

impl Display for Flow {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Flow::None => write!(f, "return"),

            Flow::Return(expr) => write!(f, "return ({})", expr),

            Flow::Jump(goal) => write!(f, "jump {}", goal),

            Flow::Branch {
                condition,
                then_b,
                else_b,
            } => write!(f, "jump {} if ({}) else {}", then_b, condition, else_b),

            Flow::Switch { cases, default } => {
                write!(f, "switch {{ ")?;
                for (expr, block) in cases.iter() {
                    write!(f, "{}: ({}), ", block, expr)?;
                }
                write!(f, "else {}", default)?;
                write!(f, "}}")
            }
        }
    }
}

/// An array literal in MIR. See ast/literal.rs for usage.
#[derive(Debug, Clone)]
pub struct ArrayLiteral {
    pub values: Vec<Expression>,
    pub type_: Type,
}
