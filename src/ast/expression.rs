use super::super::lexer::token::Token;
use super::literal::Literal;
use super::statement::Statement;

/// An enum with all expression types in Gelix.
/// An expression is a language construct that returns a value of any type and cannot appear top-level.
#[derive(Debug)]
pub enum Expression<'e> {
    /// Assigment a la x = 5
    Assignment {
        name: Token<'e>,
        value: Box<Expression<'e>>,
    },

    /// Binary operations like 5 + 5
    Binary {
        left: Box<Expression<'e>>,
        operator: Token<'e>,
        right: Box<Expression<'e>>,
    },

    /// A block of code; similar to the statement of the same name.
    /// Difference is that this one has an expression as its last statement; which is its value.
    Block(Vec<Statement<'e>>),

    /// A method/function call.
    Call {
        callee: Box<Expression<'e>>,
        token: Token<'e>,
        arguments: Vec<Expression<'e>>,
    },

    /// A getter (x.y)
    Get {
        object: Box<Expression<'e>>,
        name: Token<'e>,
    },

    /// A grouping of expressions. (5 + 5) / 5
    Grouping(Box<Expression<'e>>),

    /// An if-else ternary-like expression. Value is the value of the expression of the chosen branch.
    IfElse {
        condition: Box<Expression<'e>>,
        then_branch: Box<Expression<'e>>,
        else_branch: Box<Expression<'e>>,
    },

    /// A simple [Literal].
    Literal(Literal),

    /// Logical binary expression (5 > 3)
    Logical {
        left: Box<Expression<'e>>,
        operator: Token<'e>,
        right: Box<Expression<'e>>,
    },

    /// A setter (x.y = 5)
    Set {
        object: Box<Expression<'e>>,
        name: Token<'e>,
        value: Box<Expression<'e>>,
    },

    /// 'take' keyword
    Take {
        value: Box<Expression<'e>>,
        else_branch: Option<Box<Expression<'e>>>,
    },

    /// 'this' keyword
    This(Token<'e>),

    /// A unary operation. (!false)
    Unary {
        operator: Token<'e>,
        right: Box<Expression<'e>>,
    },

    /// Simply a variable.
    Variable(Token<'e>),

    /// A when expression.
    When {
        value: Box<Expression<'e>>,
        branches: Vec<(Expression<'e>, Expression<'e>)>,
        else_branch: Box<Expression<'e>>,
    },
}
