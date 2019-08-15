use super::super::lexer::token::Token;
use super::expression::Expression;

/// An enum with all declarations that can be in an Gelix AST.
/// A declaration is a language construct that can only appear in top-level and does not produce a value.
#[derive(Debug)]
pub enum Declaration {
    /// A declaration of an external function.
    ExternFunction(FuncSignature),

    /// A class definition.
    Class(Class),

    /// An enum definition.
    Enum {
        name: Token,
        variants: Vec<Token>,
    },

    /// A function definition.
    Function(Function),
}

// The below structs are not in the enum directly to allow reuse.

/// A class definition.
#[derive(Debug)]
pub struct Class {
    pub name: Token,
    pub variables: Vec<Variable>,
    pub methods: Vec<Function>,
}

/// A function signature.
#[derive(Debug)]
pub struct FuncSignature {
    pub name: Token,
    pub return_type: Option<Token>,
    pub parameters: Vec<FunctionArg>,
}

/// A function argument.
#[derive(Debug)]
pub struct FunctionArg {
    pub _type: Token,
    pub name: Token,
}

/// A function definition.
#[derive(Debug)]
pub struct Function {
    pub sig: FuncSignature,
    pub body: Expression,
}

/// A variable definition.
#[derive(Debug)]
pub struct Variable {
    pub name: Token,
    pub is_val: bool,
    pub initializer: Expression,
}
