use super::super::lexer::token::{Token, Type};
use super::literal::Literal;
use super::statement::Statement;

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
    /// Last statement is return value, if it is not an Expression, None is returned.
    Block(Vec<Statement>),

    /// A method/function call.
    Call {
        callee: Box<Expression>,
        token: Token,
        arguments: Vec<Expression>,
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

    /// 'return' keyword.
    Return(Option<Box<Expression>>),

    /// A setter (x.y = 5)
    Set {
        object: Box<Expression>,
        name: Token,
        value: Box<Expression>,
    },

    /// 'take' keyword
    Take {
        value: Box<Expression>,
        else_branch: Option<Box<Expression>>,
    },

    /// 'this' keyword
    This(Token),

    /// A unary operation. (!false)
    Unary {
        operator: Token,
        right: Box<Expression>,
    },

    /// Simply a variable.
    Variable(Token),

    /// A when expression.
    When {
        value: Box<Expression>,
        branches: Vec<(Expression, Expression)>,
        else_branch: Box<Expression>,
    },
}
