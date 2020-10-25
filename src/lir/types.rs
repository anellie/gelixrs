use crate::mir::MutRc;
use std::rc::Rc;
use crate::lir::expr::Expr;

#[derive(Clone)]
pub enum Type {
    Void,
    I1,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,

    Function(MutRc<Function>),
    ADT(MutRc<ADT>),
    Pointer(Box<Type>),
}

pub struct Function {
    pub name: Rc<String>,
    pub parameters: Vec<Type>,
    pub return_type: Type,
    pub body: Vec<Expr>
}

pub struct ADT {
    pub name: Rc<String>,
    pub fields: Vec<Type>
}