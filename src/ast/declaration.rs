/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/1/19 9:24 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use super::super::lexer::token::Token;
use super::expression::Expression;

/// A struct produced by the parser. It contains all types of declarations
/// that can be emitted during compilation.
/// A declaration is a language construct that can only appear in top-level and does not produce a value.
#[derive(Debug, Default)]
pub struct DeclarationList {
    pub classes: Vec<Class>,
    pub enums: Vec<Enum>,
    pub ext_functions: Vec<FuncSignature>,
    pub functions: Vec<Function>,
}

/// A class definition.
#[derive(Debug)]
pub struct Class {
    pub name: Token,
    pub superclass: Option<Token>,
    pub variables: Vec<Variable>,
    pub methods: Vec<Function>,
}

/// An enum definition.
#[derive(Debug)]
pub struct Enum {
    pub name: Token,
    pub variants: Vec<Token>,
}

/// A function signature.
/// Also doubles as the node for an external function, as a signature is all it consists of.
#[derive(Debug)]
pub struct FuncSignature {
    pub name: Token,
    pub return_type: Option<Token>,
    pub parameters: Vec<FunctionArg>,
}

/// A function argument.
#[derive(Debug, Clone)]
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
