/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 6:49 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::fmt;

use crate::option::Flatten;

use super::super::lexer::token::Token;
use super::expression::Expression;

/// A class definition.
#[derive(Debug)]
pub struct Class {
    pub name: Token,
    pub generics: Option<Vec<Token>>,
    pub variables: Vec<Variable>,
    pub methods: Vec<Function>,
}

// An interface definition.
#[derive(Debug)]
pub struct Interface {
    pub name: Token,
    pub generics: Option<Vec<Token>>,
    pub methods: Vec<InterfaceFunc>,
}

/// An interface implementation for a class.
#[derive(Debug)]
pub struct IFaceImpl {
    pub iface: Token,
    pub class: Token,
    pub iface_generics: Option<Vec<Token>>,
    pub class_generics: Option<Vec<Token>>,
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
    pub generics: Option<Vec<Token>>,
    pub return_type: Option<Type>,
    pub parameters: Vec<FunctionArg>,
}

/// A function argument.
#[derive(Debug, Clone)]
pub struct FunctionArg {
    pub type_: Type,
    pub name: Token,
}

impl FunctionArg {
    /// Used to create the implicit 'this' arg in class & iface methods.
    pub fn this_arg(ty: &Token) -> FunctionArg {
        FunctionArg {
            name: Token::generic_identifier("this".to_string()),
            type_: Type::Ident(ty.clone()),
        }
    }
}

/// A function definition.
#[derive(Debug)]
pub struct Function {
    pub sig: FuncSignature,
    pub body: Expression,
}

/// A function inside an interface, where the body is the default implementation and optional
#[derive(Debug)]
pub struct InterfaceFunc {
    pub sig: FuncSignature,
    pub body: Option<Expression>,
}

/// A variable definition.
#[derive(Debug, Clone)]
pub struct Variable {
    pub name: Token,
    pub mutable: bool,
    pub initializer: Expression,
}

/// A type literal, like 'String' or '[i64]'
#[derive(Clone, Debug)]
pub enum Type {
    Ident(Token),

    Array(Box<Type>),

    Closure {
        params: Vec<Type>,
        ret_type: Option<Box<Type>>,
    },

    Generic {
        token: Token,
        types: Vec<Type>,
    },
}

impl Type {
    pub fn get_token(&self) -> Option<&Token> {
        match self {
            Type::Ident(tok) => Some(tok),
            Type::Array(type_) => type_.get_token(),
            Type::Closure { params, ret_type } => ret_type
                .as_ref()
                .map(|box_| &**box_)
                .or_else(|| params.first())
                .map(|t| t.get_token())
                .flatten_(),
            Type::Generic { token, .. } => Some(token),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Type::Ident(tok) => write!(f, "{}", tok.lexeme),

            Type::Array(type_) => write!(f, "[{}]", type_),

            Type::Closure { params, ret_type } => {
                write!(f, "(")?;
                let mut iter = params.iter();
                if let Some(param) = iter.next() {
                    write!(f, "{}", param)?;
                }
                for param in iter {
                    write!(f, ", {}", param)?;
                }
                write!(f, ")")?;
                if let Some(ret_type) = ret_type {
                    write!(f, "-> {}", ret_type)?;
                }
                Ok(())
            }

            Type::Generic { token, types } => {
                write!(f, "{}<", token.lexeme)?;
                let mut iter = types.iter();
                if let Some(type_) = iter.next() {
                    write!(f, "{}", type_)?;
                }
                for type_ in iter {
                    write!(f, ", {}", type_)?;
                }
                write!(f, ">")
            }
        }
    }
}
