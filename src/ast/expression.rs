use std::collections::HashMap;

use super::super::token::Token;
use super::statement::Statement;
use super::literal::Literal;

/// An enum with all expression types in Gelix.
pub enum Expression<'e> {
    /// Assigment a la x = 5
    Assignment {
        name: Token<'e>,
        value: Box<Expression<'e>>
    },

    /// Binary operations like 5 + 5
    Binary {
        left: Box<Expression<'e>>,
        operator: Token<'e>,
        right: Box<Expression<'e>>
    },

    /// A block of code; last statement is value
    Block(Vec<Statement<'e>>),

    /// A method/function call.
    Call {
        callee: Box<Expression<'e>>,
        token: Token<'e>,
        arguments: Vec<Expression<'e>>
    },

    /// A getter (x.y)
    Get {
        object: Box<Expression<'e>>,
        name: Token<'e>
    },

    /// A grouping of expressions. (5 + 5) / 5
    Grouping(Box<Expression<'e>>),

    /// An if expression. Value is the value of the expression of the chosen branch.
    If {
        condition: Box<Expression<'e>>,
        then_branch: Box<Expression<'e>>,
        else_branch: Option<Box<Expression<'e>>>
    },

    /// A simple [Literal].
    Literal(Literal),

    /// Logical binary expression (5 > 3)
    Logical {
        left: Box<Expression<'e>>,
        operator: Token<'e>,
        right: Box<Expression<'e>>
    },

    /// A setter (x.y = 5)
    Set {
        object: Box<Expression<'e>>,
        name: Token<'e>,
        value: Box<Expression<'e>>
    },

    /// 'this' keyword
    This(Token<'e>),

    /// A unary operation. (!false)
    Unary {
        operator: Token<'e>,
        right: Box<Expression<'e>>
    },

    /// Simply a variable.
    Variable(Token<'e>),

    /// A when expression.
    When {
        value: Box<Expression<'e>>,
        body: HashMap<Expression<'e>, Statement<'e>>
    }
}

