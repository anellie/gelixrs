/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::fmt;

use super::{super::lexer::token::Token, expression::Expression};

/// Visibilities of a declaration.
/// Most declarations default to 'module'
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
#[derive(Debug, Clone)]
pub struct Class {
    pub name: Token,
    pub visibility: Visibility,
    pub generics: Option<Vec<Token>>,
    pub variables: Vec<ClassMember>,
    pub methods: Vec<Function>,
    pub constructors: Vec<Constructor>,
}

/// A constructor in a class.
#[derive(Debug, Clone)]
pub struct Constructor {
    pub visibility: Visibility,
    pub parameters: Vec<ConstructorParam>,
    pub body: Expression,
}

pub type ConstructorParam = (Token, Option<Type>);

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
#[derive(Debug, Clone)]
pub struct Interface {
    pub name: Token,
    pub visibility: Visibility,
    pub generics: Option<Vec<Token>>,
    pub methods: Vec<Function>,
}

/// An interface implementation for a class.
#[derive(Debug, Clone)]
pub struct IFaceImpl {
    pub iface: Type,
    pub implementor: Type,
    pub methods: Vec<Function>,
}

/// A function inside an interface, where the body is the default implementation and optional
#[derive(Debug, Clone)]
pub struct InterfaceFunc {
    pub sig: FuncSignature,
    pub body: Option<Expression>,
}

/// A function signature.
#[derive(Debug, Clone)]
pub struct FuncSignature {
    pub name: Token,
    pub visibility: Visibility,
    pub generics: Option<Vec<Token>>,
    pub return_type: Option<Type>,
    pub parameters: Vec<FunctionParam>,
    pub variadic: bool,
}

/// A function argument.
#[derive(Debug, Clone)]
pub struct FunctionParam {
    pub type_: Type,
    pub name: Token,
}

impl FunctionParam {
    /// Used to create the implicit 'this' parameter in class & iface methods.
    pub fn this_param(ty: &Token) -> FunctionParam {
        FunctionParam {
            name: Token::generic_identifier("this".to_string()),
            type_: Type::Ident(ty.clone()),
        }
    }

    /// See above.
    pub fn this_param_(ty: &Type) -> FunctionParam {
        FunctionParam {
            name: Token::generic_identifier("this".to_string()),
            type_: ty.clone(),
        }
    }

    /// This is used for the implicit parameter on closure functions.
    /// Since this parameter is deconstructed into separate variables,
    /// the user should not be able to access it, so it contains a '-'
    /// to prevent the user from referencing it as a variable.
    pub fn captured_param() -> FunctionParam {
        FunctionParam {
            type_: Type::Ident(Token::generic_identifier("CLOSURE-CAPTURED".to_string())),
            name: Token::generic_identifier("captured-vars".to_string()),
        }
    }
}

/// A function definition.
#[derive(Debug, Clone)]
pub struct Function {
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
#[derive(Clone, Debug, EnumAsGetters, EnumIsA)]
pub enum Type {
    /// Just an identifier, primitive type, class, or interface
    Ident(Token),

    /// An array of a type, written [$type]
    Array(Box<Type>),

    /// A closure signature, written (param1: $ty1, param2: $ty2): $ret_type
    Closure {
        params: Vec<Type>,
        ret_type: Option<Box<Type>>,
        closing_paren: Token,
    },

    /// An identifier with additional generic parameters, a prototype
    /// instantiation: Prototype<TypeA, TypeB>
    Generic { token: Token, types: Vec<Type> },
}

impl Type {
    pub fn get_token(&self) -> &Token {
        match self {
            Type::Ident(tok) => tok,
            Type::Array(type_) => type_.get_token(),
            Type::Closure { closing_paren, .. } => closing_paren,
            Type::Generic { token, .. } => token,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Type::Ident(tok) => write!(f, "{}", tok.lexeme),

            Type::Array(type_) => write!(f, "[{}]", type_),

            Type::Closure {
                params, ret_type, ..
            } => {
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
