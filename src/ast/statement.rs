use super::super::token::Token;
use super::expression::Expression;

/// An enum with all statements that can be in an Gelix AST.
pub enum Statement<'s> {
    /// A class definition.
    Class {
        name: Token<'s>,
        variables: Vec<Variable<'s>>,
        methods: Vec<Function<'s>>
    },

    /// An enum definition.
    Enum {
        name: Token<'s>,
        variants: Vec<Token<'s>>
    },

    /// 'error' keyword.
    Error {
        keyword: Token<'s>,
        value: Option<Expression<'s>>
    },

    /// An [Expression].
    Expression(Expression<'s>),

    /// A for loop. Only conditional loops are in the AST; iteration loops are unrolled.
    For {
        condition: Expression<'s>,
        body: Box<Statement<'s>>
    },

    /// A function definition.
    Function(Function<'s>),

    /// 'return' keyword.
    Return {
        keyword: Token<'s>,
        value: Option<Expression<'s>>
    },

    /// 'take' keyword.
    Take {
        keyword: Token<'s>,
        value: Expression<'s>,
        else_branch: Box<Statement<'s>>
    },

    /// A variable definition.
    Variable(Variable<'s>)
}

// The below structs are not in the enum directly to allow using them in the 'Class' variant.

/// A function definition.
pub struct Function<'f> {
    name: Token<'f>,
    parameters: Vec<Token<'f>>,
    body: Vec<Statement<'f>>
}

/// A variable definition.
pub struct Variable<'v> {
    name: Token<'v>,
    is_val: bool,
    initializer: Option<Expression<'v>>
}