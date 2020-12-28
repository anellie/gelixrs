/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 2:57 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::{
    declaration::{Field, LocalVariable, Variable},
    types::ToInstance,
    Function, Literal, Type, Visitor,
};
use common::MutRc;
use error::Res;
use std::rc::Rc;
use syntax::kind::SyntaxKind;

/// All binary operand types that return a bool instead of the types of their values.
static LOGICAL_BINARY: [SyntaxKind; 10] = [
    SyntaxKind::Greater,
    SyntaxKind::Less,
    SyntaxKind::GreaterEqual,
    SyntaxKind::LessEqual,
    SyntaxKind::EqualEqual,
    SyntaxKind::BangEqual,
    SyntaxKind::Bang,
    SyntaxKind::Is,
    SyntaxKind::And,
    SyntaxKind::Or,
];

/// An expression in gelix.
/// GIR expressions are an intermediate between AST and LLVM IR;
/// they contain semantic info but are mostly high-level with some lowering.
/// The expression set is slightly bigger than AST to allow for
/// some operations.
/// Compared to AST, GIR can contain undefined behavior if malformed.
#[derive(Clone, Debug)]
pub enum Expr {
    /// A block of expressions. Mainly kept around for lifetimes.
    /// Guaranteed to contain at least one expression.
    Block(Vec<Expr>),

    /// A simple literal, producing itself as value.
    Literal(Literal),

    /// Simply a variable use/load.
    Variable(Variable),

    /// Allocate a value of the given type,
    /// usually [Type::WeakRef] or [Type::StrongRef].
    Allocate {
        ty: Type,
        constructor: MutRc<Function>,
        args: Vec<Expr>,
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
        operator: SyntaxKind,
        right: Box<Expr>,
    },

    /// A unary operation. (!false)
    Unary {
        operator: SyntaxKind,
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

    /// A cast of a value to another type. See [CastType] for various methods of casting.
    Cast {
        inner: Box<Expr>,
        to: Type,
        method: CastType,
    },

    /// A closure construction out of a function and captured variables.
    /// The function is required to have a receiver parameter of type
    /// ClosureCaptured, fitting [captured].
    Closure {
        function: MutRc<Function>,
        captured: Rc<Vec<Rc<LocalVariable>>>,
    },

    /// A 'type get', similar to a variable but resolves to a type.
    /// Produced when the user uses an ADT like a variable
    TypeGet(Type),

    /// An intrinsic that is only ever produced by compiler code.
    /// Therefore, some methods like get_token or get_type do not
    /// need to be implemented for this.
    Intrinsic(Intrinsic),
}

impl Expr {
    pub fn none_const() -> Expr {
        Expr::Literal(Literal::None)
    }

    pub fn type_get(inner: Type) -> Expr {
        match inner {
            Type::None => Expr::none_const(),
            _ => Expr::TypeGet(inner),
        }
    }

    pub fn literal(literal: Literal) -> Expr {
        Expr::Literal(literal)
    }

    pub fn var(var: Variable) -> Expr {
        Expr::Variable(var)
    }

    pub fn fvar(func: &MutRc<Function>) -> Expr {
        Expr::Variable(Variable::Function(func.to_inst()))
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

    pub fn binary(operator: SyntaxKind, left: Expr, right: Expr) -> Expr {
        Expr::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        }
    }

    pub fn unary(operator: SyntaxKind, right: Expr) -> Expr {
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

    pub fn dec_rc(val: Expr) -> Expr {
        Expr::Intrinsic(Intrinsic::DecRc(box val))
    }

    pub fn free(val: Expr) -> Expr {
        Expr::Intrinsic(Intrinsic::Free(box val))
    }

    pub fn iface_call(callee: Expr, index: usize, arguments: Vec<Expr>) -> Expr {
        Expr::Intrinsic(Intrinsic::IfaceCall {
            iface: box callee,
            index,
            arguments,
        })
    }

    pub fn get_type(&self) -> Type {
        match self {
            Expr::Block(exprs) => exprs.last().unwrap().get_type(),

            Expr::Literal(literal) => literal.get_type(),

            Expr::Variable(var) => var.get_type(),

            Expr::Load { object, field } => {
                field.ty.resolve(&object.get_type().type_args().unwrap())
            }

            Expr::Store { value, .. } => value.get_type(),

            Expr::Binary {
                right, operator, ..
            }
            | Expr::Unary {
                right, operator, ..
            } => {
                if LOGICAL_BINARY.contains(&operator) {
                    Type::Bool
                } else {
                    right.get_type()
                }
            }

            Expr::Call { callee, .. } => match callee.get_type() {
                Type::Function(func) => func.ty.borrow().ret_type.resolve(func.args()),
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

            Expr::Closure { function, .. } => function.borrow().to_closure_type(),

            Expr::TypeGet(ty) => Type::Type(Box::new(ty.clone())),

            Expr::Intrinsic(_) => Type::Any,
        }
    }

    pub fn get_type_get_type(&self) -> Type {
        if let Expr::TypeGet(ty) = self {
            ty.clone()
        } else {
            panic!("Not TypeGet");
        }
    }

    /// Simple helper for `gen_expr` call match arms.
    /// Done instead of deriving `EnumIsA` to save compilation time.
    pub fn is_struct_get(&self) -> bool {
        matches!(self, Expr::Load { .. })
    }

    /// If this expression can be assigned to, aka used as a location for
    /// a store expression.
    pub fn assignable(&self) -> bool {
        match self {
            Expr::Block(exprs) => exprs.last().map(Expr::assignable).unwrap_or(false),

            Expr::Variable(var) => var.assignable(),

            Expr::Load { field, .. } => field.mutable,

            Expr::If {
                then_branch,
                else_branch,
                phi_type,
                ..
            } => phi_type.is_some() && then_branch.assignable() && else_branch.assignable(),

            Expr::Switch {
                branches,
                else_branch,
                phi_type,
            } => {
                phi_type.is_some()
                    && else_branch.assignable()
                    && branches.iter().all(|(_, br)| br.assignable())
            }

            _ => false,
        }
    }

    /// A 'human readable' name used for error reporting.
    /// For example, when the user tries assigning to a non-assignable value,
    /// the error message would be "Cannot assign to {{ expr.human_name() }}."
    pub fn human_name(&self) -> &'static str {
        match self {
            Expr::Block(_) => "block",
            Expr::Literal(_) => "literal",
            Expr::Variable(var) => match var {
                Variable::Function(_) => "function",
                Variable::Local(var) => {
                    if var.mutable {
                        "mutable local variable"
                    } else {
                        "immutable local variable"
                    }
                }
            },
            Expr::Allocate { .. } => "allocation",
            Expr::Load { field, .. } if field.mutable => "mutable field",
            Expr::Load { .. } => "immutable field",
            Expr::Store { .. } => "assignment",
            Expr::Binary { .. } => "infix operation",
            Expr::Unary { .. } => "prefix operation",
            Expr::Call { .. } => "call",
            Expr::If { .. } => "if expression",
            Expr::Switch { .. } => "when expression",
            Expr::Loop { .. } => "loop",
            Expr::Break(_) => "break",
            Expr::Return(_) => "return",
            Expr::Cast { .. } => "cast",
            Expr::Closure { .. } => "closure literal",
            Expr::TypeGet(_) => "type access",
            Expr::Intrinsic(_) => "<intrinsic>",
        }
    }

    pub fn visit<T: Visitor>(&mut self, v: &mut T) -> Res<()> {
        match self {
            Expr::Block(block) => {
                for expr in block.iter_mut() {
                    expr.visit(v)?;
                }
                v.visit_block(block)
            }

            Expr::Literal(literal) => v.visit_literal(literal),

            Expr::Variable(var) => v.visit_variable(var),

            Expr::Allocate {
                ty,
                constructor,
                args,
                ..
            } => {
                for expr in args.iter_mut() {
                    expr.visit(v)?;
                }
                v.visit_allocation(ty, constructor, args)
            }

            Expr::Load { object, field } => {
                object.visit(v)?;
                v.visit_load(object, field)
            }

            Expr::Store {
                location,
                value,
                first_store,
            } => {
                location.visit(v)?;
                value.visit(v)?;
                v.visit_store(location, value, first_store)
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                left.visit(v)?;
                right.visit(v)?;
                v.visit_binary(left, operator, right)
            }

            Expr::Unary { operator, right } => {
                right.visit(v)?;
                v.visit_unary(operator, right)
            }

            Expr::Call { callee, arguments } => {
                callee.visit(v)?;
                for expr in arguments.iter_mut() {
                    expr.visit(v)?;
                }
                v.visit_call(callee, arguments)
            }

            Expr::If {
                condition,
                then_branch,
                else_branch,
                phi_type,
            } => {
                condition.visit(v)?;
                then_branch.visit(v)?;
                else_branch.visit(v)?;
                v.visit_if(condition, then_branch, else_branch, phi_type)
            }

            Expr::Switch {
                branches,
                else_branch,
                phi_type,
            } => {
                for br in branches.iter_mut() {
                    br.0.visit(v)?;
                    br.1.visit(v)?;
                }
                else_branch.visit(v)?;
                v.visit_switch(branches, else_branch, phi_type)
            }

            Expr::Loop {
                condition,
                body,
                else_branch,
                phi_type,
            } => {
                condition.visit(v)?;
                body.visit(v)?;
                else_branch.visit(v)?;
                v.visit_loop(condition, body, else_branch, phi_type)
            }

            Expr::Break(expr) => {
                expr.visit(v)?;
                v.visit_break(expr)
            }

            Expr::Return(expr) => {
                expr.visit(v)?;
                v.visit_return(expr)
            }

            Expr::Cast { inner, to, method } => {
                inner.visit(v)?;
                v.visit_cast(inner, to, method)
            }

            Expr::Closure { function, captured } => v.visit_closure(function, captured),

            Expr::TypeGet(ty) => v.visit_type_get(ty),

            Expr::Intrinsic(_) => Ok(()), // TODO?
        }
    }
}

#[derive(Clone, Debug)]
pub enum Intrinsic {
    /// Manually increment a value's refcount.
    IncRc(Box<Expr>),
    /// Manually decrement a value's refcount.
    DecRc(Box<Expr>),
    /// Manually free a value. Only calls free,
    /// does not do anything else.
    Free(Box<Expr>),
    /// Perform a virtual call. callee must
    /// be a type with an embedded vtable
    /// (currently only interfaces).
    IfaceCall {
        iface: Box<Expr>,
        index: usize,
        arguments: Vec<Expr>,
    },
}

#[derive(Clone, Debug)]
pub enum CastType {
    /// A numeric cast between any number type
    Number,
    /// A cast of strong ref to weak
    StrongToWeak,
    /// A cast of strong or weak ref to value
    ToValue,
    /// A bitcast ( = reinterpretation of bits)
    Bitcast,
    // Type is the implementor type
    ToInterface(Type),
}
