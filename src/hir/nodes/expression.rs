/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 2:57 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{
    ast::{expression::LOGICAL_BINARY, Literal},
    hir::{
        generator::intrinsics::INTRINSICS,
        nodes::{
            declaration::{Field, Function, LocalVariable, Variable},
            types::{Type, TypeArguments},
        },
    },
    lexer::token::Token,
    mir::MutRc,
};

/// An expression in gelix.
/// HIR expressions are an intermediate between AST and MIR;
/// they contain semantic info but are high-level with little lowering.
/// The expression set is slightly smaller than AST as some
/// things are unified.
#[derive(Clone, Debug)]
pub enum Expr {
    /// A block of expressions. Mainly kept around for lifetimes.
    /// Guaranteed to contain at least one expression.
    Block(Vec<Expr>),

    /// A simple literal, producing itself as value.
    Literal(Literal, Token),

    /// Simply a variable use/load.
    Variable(Variable),

    /// Allocate a value of the given type,
    /// usually [Type::WeakRef] or [Type::StrongRef].
    Allocate {
        ty: Type,
        constructor: MutRc<Function>,
        args: Vec<Expr>,
        tok: Token,
    },

    // A field getter on an ADT.
    Load {
        object: Box<Expr>,
        field: Rc<Field>,
    },

    /// Store into an ADT or variable.
    Store {
        location: Box<Expr>,
        value: Box<Expr>,
        first_store: bool,
    },

    /// Binary math like 5 + 5
    Binary {
        left: Box<Expr>,
        operator: Token,
        right: Box<Expr>,
    },

    /// A unary operation. (!false)
    Unary {
        operator: Token,
        right: Box<Expr>,
    },

    /// A method/function call.
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },

    /// An if expression. Value is the value of the expression of the chosen branch.
    /// If no else branch is present or either branch does not return an expression,
    /// None is returned.
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        /// Returned type, if returning a value
        phi_type: Option<Type>,
    },

    /// A switch, where each branch is tested and the first
    /// one whose condition is truthy will be run.
    /// Can be expression.
    Switch {
        branches: Vec<(Expr, Expr)>,
        else_branch: Box<Expr>,
        /// Returned type, if returning a value
        phi_type: Option<Type>,
    },

    /// A conditional (for) loop.
    /// The value produced is the value of the body on the last iteration, or the else branch if the condition was never true.
    Loop {
        condition: Box<Expr>,
        body: Box<Expr>,
        else_branch: Box<Expr>,
        /// Returned type, if returning a value
        phi_type: Option<Type>,
    },

    /// 'break' keyword. Always produces None as a value.
    Break(Box<Expr>),

    /// 'return' keyword. Always produces None as a value.
    Return(Box<Expr>),

    Cast {
        inner: Box<Expr>,
        to: Type,
        method: CastType,
    },

    TypeGet(Type),
}

impl Expr {
    pub fn none_const(tok: Token) -> Expr {
        Expr::Literal(Literal::None, tok)
    }

    pub fn none_const_() -> Expr {
        Expr::Literal(Literal::None, Token::eof_token(1))
    }

    pub fn literal(literal: Literal) -> Expr {
        Expr::Literal(literal, Token::eof_token(1))
    }

    pub fn var(var: Variable) -> Expr {
        Expr::Variable(var)
    }

    pub fn lvar(var: &Rc<LocalVariable>) -> Expr {
        Expr::Variable(Variable::Local(Rc::clone(var)))
    }

    pub fn load(obj: Expr, field: &Rc<Field>) -> Expr {
        Expr::Load {
            object: Box::new(obj),
            field: Rc::clone(field),
        }
    }

    pub fn store(loc: Expr, value: Expr, first_store: bool) -> Expr {
        Expr::Store {
            location: Box::new(loc),
            value: Box::new(value),
            first_store,
        }
    }

    pub fn binary(operator: Token, left: Expr, right: Expr) -> Expr {
        Expr::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        }
    }

    pub fn unary(operator: Token, right: Expr) -> Expr {
        Expr::Unary {
            operator,
            right: Box::new(right),
        }
    }

    pub fn call(callee: Expr, arguments: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: Box::new(callee),
            arguments,
        }
    }

    pub fn if_(cond: Expr, then: Expr, else_: Expr, phi_type: Option<Type>) -> Expr {
        Expr::If {
            condition: Box::new(cond),
            then_branch: Box::new(then),
            else_branch: Box::new(else_),
            phi_type,
        }
    }

    pub fn switch(branches: Vec<(Expr, Expr)>, else_: Expr, phi_type: Option<Type>) -> Expr {
        Expr::Switch {
            branches,
            else_branch: Box::new(else_),
            phi_type,
        }
    }

    pub fn loop_(cond: Expr, body: Expr, else_: Expr, phi_type: Option<Type>) -> Expr {
        Expr::Loop {
            condition: Box::new(cond),
            body: Box::new(body),
            else_branch: Box::new(else_),
            phi_type,
        }
    }

    pub fn ret(val: Expr) -> Expr {
        Expr::Return(Box::new(val))
    }

    pub fn break_(val: Expr) -> Expr {
        Expr::Break(Box::new(val))
    }

    pub fn cast(val: Expr, to: Type, method: CastType) -> Expr {
        Expr::Cast {
            inner: Box::new(val),
            to,
            method,
        }
    }

    pub fn get_type(&self) -> Type {
        match self {
            Expr::Block(exprs) => exprs.last().unwrap().get_type(),

            Expr::Literal(literal, _) => match literal {
                Literal::Any => Type::Any,
                Literal::None => Type::None,
                Literal::Bool(_) => Type::Bool,
                Literal::I8(_) => Type::I8,
                Literal::I16(_) => Type::I16,
                Literal::I32(_) => Type::I32,
                Literal::I64(_) => Type::I64,
                Literal::U8(_) => Type::U8,
                Literal::U16(_) => Type::U16,
                Literal::U32(_) => Type::U32,
                Literal::U64(_) => Type::U64,
                Literal::F32(_) => Type::F32,
                Literal::F64(_) => Type::F64,
                Literal::Char(_) => unimplemented!(),
                Literal::String(_) => INTRINSICS.with(|i| i.borrow().string_type.clone().unwrap()),
                // Literal::Array(Right(arr)) => arr.type_.clone(), TODO
                Literal::Closure(_) | Literal::Array(_) => panic!("invalid literal"),
            },

            Expr::Variable(var) => var.get_type(),

            Expr::Load { object, field } => field
                .ty
                .resolve(object.get_type().type_args().unwrap())
                .clone(),

            Expr::Store { value, .. } => value.get_type(),

            Expr::Binary {
                right, operator, ..
            }
            | Expr::Unary {
                right, operator, ..
            } => {
                if LOGICAL_BINARY.contains(&operator.t_type) {
                    Type::Bool
                } else {
                    right.get_type()
                }
            }

            Expr::Call { callee, .. } => match callee.get_type() {
                Type::Function(func) => func.ty.borrow().ret_type.resolve(&func.args).clone(),
                Type::Closure(closure) => closure.ret_type.clone(),
                _ => panic!("Invalid callee"),
            },

            Expr::If { phi_type, .. }
            | Expr::Switch { phi_type, .. }
            | Expr::Loop { phi_type, .. } => {
                if let Some(ty) = phi_type {
                    ty.clone()
                } else {
                    Type::None
                }
            }

            Expr::Break(_) | Expr::Return(_) => Type::Any,

            Expr::Cast { to, .. } | Expr::Allocate { ty: to, .. } => to.clone(),

            Expr::TypeGet(ty) => Type::Type(Box::new(ty.clone())),
        }
    }

    /// Returns a token that is part of the expression to be used for error display.
    pub fn get_token(&self) -> Token {
        match self {
            Expr::Binary { operator: tok, .. }
            | Expr::Literal(_, tok)
            | Expr::Allocate { tok, .. }
            | Expr::Unary { operator: tok, .. } => tok.clone(),

            Expr::Store { location: ex, .. }
            | Expr::Call { callee: ex, .. }
            | Expr::Load { object: ex, .. }
            | Expr::Switch {
                else_branch: ex, ..
            }
            | Expr::Loop { condition: ex, .. }
            | Expr::If { condition: ex, .. }
            | Expr::Return(ex)
            | Expr::Break(ex)
            | Expr::Cast { inner: ex, .. } => ex.get_token(),

            Expr::Variable(var) => var.get_token(),

            Expr::Block(exprs) => exprs.first().unwrap().get_token(),

            Expr::TypeGet(_) => panic!("no token here!"),
        }
    }

    /// Simple helper for `gen_expr` call match arms.
    /// Done instead of deriving `EnumIsA` to save compilation time.
    /// TODO: is this used?
    pub fn is_variable(&self) -> bool {
        if let Expr::Variable(_) = self {
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug)]
pub enum CastType {
    Number,
    StrongToWeak,
    ToValue,
    Bitcast,
    ToInterface,
}
