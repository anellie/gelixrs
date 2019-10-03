/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/2/19 5:15 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::fmt;
use std::rc::Rc;

use super::super::lexer::token::Token;
use super::expression::Expression;

/// A class definition.
#[derive(Debug)]
pub struct Class {
    pub name: Token,
    pub generics: Vec<Token>,
    pub variables: Vec<Variable>,
    pub methods: Vec<Function>,
}

// An interface definition.
#[derive(Debug)]
pub struct Interface {
    pub name: Token,
    pub generics: Vec<Token>,
    pub methods: Vec<InterfaceFunc>,
}

/// An interface implementation for a class.
#[derive(Debug)]
pub struct IFaceImpl {
    pub iface: Token,
    pub class: Token,
    pub iface_generics: Vec<Token>,
    pub class_generics: Vec<Token>,
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
    pub return_type: Option<ASTType>,
    pub parameters: Vec<FunctionArg>,
}

/// A function argument.
#[derive(Debug, Clone)]
pub struct FunctionArg {
    pub type_: ASTType,
    pub name: Token,
}

impl FunctionArg {
    /// Used to create the implicit 'this' arg in class & iface methods.
    pub fn this_arg(ty: &Token) -> FunctionArg {
        FunctionArg {
            name: Token::generic_identifier("this".to_string()),
            type_: ASTType::Token(ty.clone()),
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
#[derive(Debug)]
pub struct Variable {
    pub name: Token,
    pub is_val: bool,
    pub initializer: Expression,
}

/// A type literal, like 'String' or '[i64]'
#[derive(Clone, Debug)]
pub enum ASTType {
    Token(Token),

    Array(Box<ASTType>),

    Closure {
        params: Vec<ASTType>,
        ret_type: Option<Box<ASTType>>,
    },

    Generic {
        token: Token,
        types: Vec<ASTType>,
    },
}

impl ASTType {
    pub fn get_token(&self) -> Option<&Token> {
        match self {
            ASTType::Token(tok) => Some(tok),
            ASTType::Array(type_) => type_.get_token(),
            ASTType::Closure { params, ret_type } => ret_type
                .as_ref()
                .map(|box_| &**box_)
                .or(params.first())
                .map(|t| t.get_token())
                .flatten(),
            ASTType::Generic { token, .. } => Some(token),
        }
    }
}

impl fmt::Display for ASTType {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            ASTType::Token(tok) => write!(f, "{}", tok.lexeme),

            ASTType::Array(type_) => write!(f, "[{}]", type_),

            ASTType::Closure { params, ret_type } => {
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

            ASTType::Generic { token, types } => {
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
