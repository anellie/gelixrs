use super::super::lexer::token::Token;
use super::expression::Expression;
use super::statement::Statement;

/// An enum with all declarations that can be in an Gelix AST.
/// A declaration is a language construct that can only appear in top-level and does not produce a value.
#[derive(Debug)]
pub enum Declaration<'s> {
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

    /// A function definition.
    Function(Function<'s>),
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
