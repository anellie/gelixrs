/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/8/19 6:07 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use super::super::lexer::token::Token;
use super::expression::Expression;

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
