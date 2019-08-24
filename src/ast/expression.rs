/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/22/19 8:19 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use super::super::lexer::token::{Token, Type};
use super::declaration::Variable;
use super::literal::Literal;
use std::fmt::{Display, Formatter, Error};
use std::borrow::Borrow;

// All binary operand types that return a bool instead of the types of their values.
pub static LOGICAL_BINARY: [Type; 6] = [
    Type::Greater,
    Type::Less,
    Type::GreaterEqual,
    Type::LessEqual,
    Type::EqualEqual,
    Type::BangEqual,
];

/// An enum with all expression types in Gelix.
/// An expression is a language construct that returns a value of any type and cannot appear top-level.
/// Currently, everything not top-level is an expression. However, some are not allowed in certain contexts;
/// see the bottom of this enum.
#[derive(Debug)]
pub enum Expression {
    /// Assignment a la x = 5
    Assignment { name: Token, value: Box<Expression> },

    /// Binary operations like 5 + 5
    Binary {
        left: Box<Expression>,
        operator: Token,
        right: Box<Expression>,
    },

    /// A block of code.
    /// Last expression is the return value.
    Block(Vec<Expression>),

    /// 'break' keyword. Always produces None as a value.
    Break(Option<Box<Expression>>),

    /// A method/function call.
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },

    /// A for loop. Only conditional loops are in the AST; iteration loops are unrolled.
    /// The value produced is the value of the body on the last iteration.
    For {
        condition: Box<Expression>,
        body: Box<Expression>,
    },

    /// A getter (x.y)
    Get {
        object: Box<Expression>,
        name: Token,
    },

    /// A grouping of expressions. (5 + 5) / 5
    Grouping(Box<Expression>),

    /// An if expression. Value is the value of the expression of the chosen branch.
    /// If no else branch is present or either branch does not return an expression,
    /// None is returned.
    If {
        condition: Box<Expression>,
        then_branch: Box<Expression>,
        else_branch: Option<Box<Expression>>,
    },

    /// A simple [Literal].
    Literal(Literal),

    /// 'return' keyword. Always produces None as a value.
    Return(Option<Box<Expression>>),

    /// A setter (x.y = 5)
    Set {
        object: Box<Expression>,
        name: Token,
        value: Box<Expression>,
    },

    /// A unary operation. (!false)
    Unary {
        operator: Token,
        right: Box<Expression>,
    },

    /// Simply a variable use.
    Variable(Token),

    /// A when expression.
    When {
        value: Box<Expression>,
        branches: Vec<(Expression, Expression)>,
        else_branch: Box<Expression>,
    },

    /// Below are all 'higher expressions'. These are differentiated in the parser.
    /// They are only allowed to appear as top-level inside a block.
    /// All of them always produce None as a value.

    /// A variable definition.
    VarDef(Box<Variable>),
}

impl Expression {
    /// Returns the line the expression is on.
    /// Can also return None if the expression does not contain information on its line.
    pub fn get_line(&self) -> Option<usize> {
        Some(match self {
            Expression::Assignment { name, .. } => name.line,
            Expression::Binary { operator , .. } => operator.line,
            Expression::Block(vec) => vec.first()?.get_line()?,
            Expression::Break(expr) => (*expr).as_ref()?.get_line()?,
            Expression::Call { callee, .. } => callee.get_line()?,
            Expression::For { condition, .. } => condition.get_line()?,
            Expression::Get { name, .. } => name.line,
            Expression::Grouping(expr) => expr.get_line()?,
            Expression::If { condition, .. } => condition.get_line()?,
            Expression::Literal(_) => None?,
            Expression::Return(expr) => (*expr).as_ref()?.get_line()?,
            Expression::Set { name, .. } => name.line,
            Expression::Unary { operator, .. } => operator.line,
            Expression::Variable(name) => name.line,
            Expression::When { value, .. } => value.get_line()?,
            Expression::VarDef(var) => var.name.line,
        })
    }
}

impl Display for Expression {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Expression::Assignment { name, value } =>
                write!(f, "{} = {}", name.lexeme, value),

            Expression::Binary { left, operator, right } =>
                write!(f, "{} {} {}", left, operator.lexeme, right),

            Expression::Block(_) => write!(f, "{{ ... }}"),

            Expression::Break(expr) =>
                if let Some(expr) = expr {
                    write!(f, "break {}", expr)
                } else {
                    write!(f, "break")
                },

            Expression::Call { callee, arguments } =>
                write!(f, "{}({:?})", callee, arguments),

            Expression::For { condition, body } =>
                write!(f, "for ({}) {}", condition, body),

            Expression::Get { object, name } =>
                write!(f, "{}.{}", object, name.lexeme),

            Expression::Grouping(expr) => write!(f, "({})", expr),

            Expression::If { condition, then_branch, else_branch } =>
                if let Some(else_branch) = else_branch {
                    write!(f, "if ({}) {} else {}", condition, then_branch, else_branch)
                } else {
                    write!(f, "if ({}) {}", condition, then_branch)
                },

            Expression::Literal(literal) =>
                write!(f, "{}", literal),

            Expression::Return(expr) =>
                if let Some(expr) = expr {
                    write!(f, "return {}", expr)
                } else {
                    write!(f, "return")
                },

            Expression::Set { object, name, value } =>
                write!(f, "{}.{} = {}", object, name.lexeme, value),

            Expression::Unary { operator, right } =>
                write!(f, "{}{}", operator.lexeme, right),

            Expression::Variable(var) => write!(f, "{}", var.lexeme),

            Expression::When { value, .. } =>
                write!(f, "when ({}) {{ ... }}", value),

            Expression::VarDef(var) =>
                write!(f, "var {} = {}", var.name.lexeme, var.initializer),
        }
    }
}
