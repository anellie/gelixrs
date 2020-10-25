use crate::ast::Literal;
use crate::lir::types::Type;
use std::rc::Rc;
use crate::lexer::token::TType;
use crate::hir::nodes::expression::CastType;

/// An expression in gelix.
/// HIR expressions are an intermediate between AST and MIR;
/// they contain semantic info but are high-level with little lowering.
/// The expression set is slightly smaller than AST as some
/// things are unified.
#[derive(Clone)]
pub enum Expr {
    /// A block of expressions. Mainly kept around for lifetimes.
    /// Guaranteed to contain at least one expression.
    Block(Vec<Expr>),

    /// A simple literal, producing itself as value.
    Literal(Literal),

    /// An alloca get. Does not load.
    Alloca(Alloca),

    /// Loads the inner value. Value must be Type::Pointer.
    Load(Box<Expr>),

    /// Stores value into store. Store must be Type::Pointer.
    Store {
        store: Box<Expr>,
        value: Box<Expr>
    },

    /// Binary math like 5 + 5
    Binary {
        left: Box<Expr>,
        operator: TType,
        right: Box<Expr>,
    },

    /// A unary operation. (!false)
    Unary {
        operator: TType,
        right: Box<Expr>,
    },

    /// A method/function call.
    Call {
        function: Box<Expr>,
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
}

#[derive(Clone)]
pub struct Alloca {
    pub name: Rc<String>,
    pub ty: Type,
}