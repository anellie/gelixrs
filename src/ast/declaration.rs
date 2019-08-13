use super::super::lexer::token::Token;
use super::expression::Expression;

/// An enum with all declarations that can be in an Gelix AST.
/// A declaration is a language construct that can only appear in top-level and does not produce a value.
#[derive(Debug)]
pub enum Declaration<'s> {
    /// A declaration of an external function.
    ExternFunction(FuncSignature<'s>),

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

// The below structs are not in the enum directly to allow reuse.

/// A function signature.
#[derive(Debug)]
pub struct FuncSignature<'f> {
    pub name: Token<'f>,
    pub return_type: Option<Token<'f>>,
    pub parameters: Vec<FunctionArg<'f>>,
}

/// A function argument.
#[derive(Debug)]
pub struct FunctionArg<'a> {
    pub _type: Token<'a>,
    pub name: Token<'a>
}

/// A function definition.
#[derive(Debug)]
pub struct Function<'f> {
    pub sig: FuncSignature<'f>,
    pub body: Expression<'f>,
}

/// A variable definition.
#[derive(Debug)]
pub struct Variable<'v> {
    pub name: Token<'v>,
    pub is_val: bool,
    pub initializer: Expression<'v>,
}
