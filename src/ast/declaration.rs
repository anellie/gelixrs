/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/12/19 11:08 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::fmt;

use crate::option::Flatten;

use super::expression::Expression;
use super::super::lexer::token::Token;

/// Visibilities of a declaration.
/// Most declaration default to 'module'
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Visibility {
    /// Visible and importable from anywhere.
    Public,
    /// Local to the given file/submodule.
    Private,
    /// Local to the given base module, 'std' for example.
    Module,
}

/// A class definition.
#[derive(Debug)]
pub struct Class {
    pub name: Token,
    pub visibility: Visibility,
    pub generics: Option<Vec<Token>>,
    pub variables: Vec<ClassMember>,
    pub methods: Vec<Function>,
    pub constructors: Vec<Constructor>,
}

pub type ConstructorParam = (Token, Option<Type>);

/// A constructor in a class.
#[derive(Debug)]
pub struct Constructor {
    pub visibility: Visibility,
    pub parameters: Vec<ConstructorParam>,
    pub body: Expression,
}

/// A member of a class.
#[derive(Debug, Clone)]
pub struct ClassMember {
    pub name: Token,
    pub visibility: Visibility,
    pub mutable: bool,
    pub ty: Option<Type>,
    pub initializer: Option<Expression>,
}

// An interface definition.
#[derive(Debug)]
pub struct Interface {
    pub name: Token,
    pub visibility: Visibility,
    pub generics: Option<Vec<Token>>,
    pub methods: Vec<InterfaceFunc>,
}

/// An interface implementation for a class.
#[derive(Debug)]
pub struct IFaceImpl {
    pub iface: Token,
    pub implementor: Type,
    pub iface_generics: Option<Vec<Token>>,
    pub methods: Vec<Function>,
}

/// An enum definition.
#[derive(Debug)]
pub struct Enum {
    pub name: Token,
    pub variants: Vec<Token>,
}

/// A function signature.
#[derive(Debug)]
pub struct FuncSignature {
    pub name: Token,
    pub visibility: Visibility,
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

    /// See above.
    pub fn this_arg_(ty: &Type) -> FunctionArg {
        FunctionArg {
            name: Token::generic_identifier("this".to_string()),
            type_: ty.clone(),
        }
    }
}

/// A function definition.
#[derive(Debug)]
pub struct Function {
    pub sig: FuncSignature,
    pub body: Option<Expression>,
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
