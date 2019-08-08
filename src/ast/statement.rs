use super::expression::Expression;
use super::declaration::Variable;

/// An enum with all statements that can be in an Gelix AST.
/// A statement is a language construct that does not return a value, and cannot appear in top-level.
#[derive(Debug)]
pub enum Statement<'s> {
    /// A block of code.
    Block(Vec<Statement<'s>>),

    /// 'error' keyword.
    Error(Option<Expression<'s>>),

    /// An [Expression].
    Expression(Expression<'s>),

    /// A for loop. Only conditional loops are in the AST; iteration loops are unrolled.
    For {
        condition: Expression<'s>,
        body: Box<Statement<'s>>,
    },

    /// An if statement. Also see IfElse in [Expression], with is similar.
    If {
        condition: Box<Expression<'s>>,
        then_branch: Box<Statement<'s>>,
        else_branch: Option<Box<Statement<'s>>>,
    },

    /// 'return' keyword.
    Return(Option<Expression<'s>>),

    /// A variable definition.
    Variable(Variable<'s>),
}