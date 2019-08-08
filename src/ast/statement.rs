use super::super::lexer::token::Token;
use super::expression::Expression;

/// An enum with all statements that can be in an Gelix AST.
#[derive(Debug)]
pub enum Statement<'s> {
    /// A class definition.
    Class {
        name: Token<'s>,
        variables: Vec<Variable<'s>>,
        methods: Vec<Function<'s>>,
    },

    /// An enum definition.
    Enum {
        name: Token<'s>,
        variants: Vec<Token<'s>>,
    },

    /// 'error' keyword.
    Error(Option<Expression<'s>>),

    /// An [Expression].
    Expression(Expression<'s>),

    /// A for loop. Only conditional loops are in the AST; iteration loops are unrolled.
    For {
        condition: Expression<'s>,
        body: Box<Statement<'s>>,
    },

    /// A function definition.
    Function(Function<'s>),

    /// 'return' keyword.
    Return(Option<Expression<'s>>),

    /// A variable definition.
    Variable(Variable<'s>),
}

// The below structs are not in the enum directly to allow using them in the 'Class' variant.

/// A function definition.
#[derive(Debug)]
pub struct Function<'f> {
    pub name: Token<'f>,
    pub return_type: Option<Token<'f>>,
    pub parameters: Vec<(Token<'f>, Token<'f>)>,
    pub body: Box<Statement<'f>>,
}

/// A variable definition.
#[derive(Debug)]
pub struct Variable<'v> {
    pub name: Token<'v>,
    pub is_val: bool,
    pub initializer: Expression<'v>,
}
