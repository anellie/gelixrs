/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/24/19 3:58 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ast::declaration::Type;

use super::declaration::Variable;
use super::literal::Literal;
use super::super::lexer::token::{Token, TType};

/// All binary operand types that return a bool instead of the types of their values.
pub static LOGICAL_BINARY: [TType; 6] = [
    TType::Greater,
    TType::Less,
    TType::GreaterEqual,
    TType::LessEqual,
    TType::EqualEqual,
    TType::BangEqual,
];

/// An enum with all expression types in Gelix.
/// An expression is a language construct that returns a value of any type and cannot appear top-level.
/// Currently, everything not top-level is an expression. However, some are not allowed in certain contexts;
/// see the bottom of this enum.
/// Expressions appear as part of a declaration.
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
    /// The value produced is the value of the body on the last iteration, or the else branch if the condition was never true.
    For {
        condition: Box<Expression>,
        body: Box<Expression>,
        else_b: Option<Box<Expression>>,
    },

    /// A getter (x.y)
    Get {
        object: Box<Expression>,
        name: Token,
    },

    /// An if expression. Value is the value of the expression of the chosen branch.
    /// If no else branch is present or either branch does not return an expression,
    /// None is returned.
    If {
        condition: Box<Expression>,
        then_branch: Box<Expression>,
        else_branch: Option<Box<Expression>>,
    },

    /// A simple literal.
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

    // Below are all 'higher expressions'. These are differentiated in the parser.
    // They are only allowed to appear as top-level inside a block.
    // All of them always produce None as a value.
    /// A variable definition.
    VarDef(Box<Variable>),
}

impl Expression {
    /// Returns a token that is part of the expression to be used for error display.
    /// Can also return None if the expression does not contain any tokens.
    pub fn get_token(&self) -> Option<&Token> {
        Some(match self {
            Expression::Assignment { name, .. } => name,
            Expression::Binary { operator, .. } => operator,
            Expression::Block(vec) => vec.first()?.get_token()?,
            Expression::Break(expr) => (*expr).as_ref()?.get_token()?,
            Expression::Call { callee, .. } => callee.get_token()?,
            Expression::For { condition, .. } => condition.get_token()?,
            Expression::Get { name, .. } => name,
            Expression::If { condition, .. } => condition.get_token()?,
            Expression::Literal(_) => None?,
            Expression::Return(expr) => (*expr).as_ref()?.get_token()?,
            Expression::Set { name, .. } => name,
            Expression::Unary { operator, .. } => operator,
            Expression::Variable(name) => name,
            Expression::When { value, .. } => value.get_token()?,
            Expression::VarDef(var) => &var.name,
        })
    }
}
