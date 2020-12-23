use crate::{
    declaration::{Field, LocalVariable, Variable},
    expression::CastType,
    Expr, Function, Literal, Type,
};
use common::MutRc;
use error::Res;
use std::rc::Rc;
use syntax::kind::SyntaxKind;

pub trait Visitor {
    fn visit_block(&mut self, _block: &mut Vec<Expr>) -> Res<()> {
        Ok(())
    }

    fn visit_literal(&mut self, _literal: &mut Literal) -> Res<()> {
        Ok(())
    }

    fn visit_variable(&mut self, _variable: &mut Variable) -> Res<()> {
        Ok(())
    }

    fn visit_allocation(
        &mut self,
        _ty: &mut Type,
        _constructor: &mut MutRc<Function>,
        _args: &mut Vec<Expr>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_load(&mut self, _object: &mut Expr, _field: &Rc<Field>) -> Res<()> {
        Ok(())
    }

    fn visit_store(
        &mut self,
        _object: &mut Expr,
        _value: &mut Expr,
        _first_store: &mut bool,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_binary(
        &mut self,
        _left: &mut Expr,
        _op: &mut SyntaxKind,
        _right: &mut Expr,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_unary(&mut self, _op: &mut SyntaxKind, _right: &mut Expr) -> Res<()> {
        Ok(())
    }

    fn visit_call(&mut self, _callee: &mut Expr, _arguments: &mut Vec<Expr>) -> Res<()> {
        Ok(())
    }

    fn visit_if(
        &mut self,
        _condition: &mut Expr,
        _then_br: &mut Expr,
        _else_br: &mut Expr,
        _phi_type: &mut Option<Type>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_switch(
        &mut self,
        _branches: &mut Vec<(Expr, Expr)>,
        _else_br: &mut Expr,
        _phi_type: &mut Option<Type>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_loop(
        &mut self,
        _condition: &mut Expr,
        _body: &mut Expr,
        _else_br: &mut Expr,
        _phi_type: &mut Option<Type>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_break(&mut self, _inner: &mut Expr) -> Res<()> {
        Ok(())
    }

    fn visit_return(&mut self, _inner: &mut Expr) -> Res<()> {
        Ok(())
    }

    fn visit_cast(&mut self, _inner: &mut Expr, _to: &mut Type, _method: &mut CastType) -> Res<()> {
        Ok(())
    }

    fn visit_closure(
        &mut self,
        _function: &mut MutRc<Function>,
        _captured: &mut Rc<Vec<Rc<LocalVariable>>>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_type_get(&mut self, _inner: &mut Type) -> Res<()> {
        Ok(())
    }
}
