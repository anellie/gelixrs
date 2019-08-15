use super::declaration::Variable;
use super::expression::Expression;

/// An enum with all statements that can be in an Gelix AST.
/// A statement is a language construct that does not return a value, and cannot appear in top-level.
#[derive(Debug)]
pub enum Statement {
    /// 'error' keyword.
    Error(Option<Expression>),

    /// An [Expression].
    Expression(Expression),

    /// A for loop. Only conditional loops are in the AST; iteration loops are unrolled.
    For {
        condition: Expression,
        body: Expression,
    },

    /// A variable definition.
    Variable(Variable),
}
