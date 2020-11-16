use crate::{
    ast::Literal,
    error::Res,
    gir::{
        nodes::{
            declaration::{Field, LocalVariable, Variable},
            expression::{CastType, Expr},
        },
        Function, MutRc, Type,
    },
    lexer::token::Token,
};
use std::rc::Rc;

pub trait Visitor {
    fn visit_block(&mut self, block: &mut Vec<Expr>) -> Res<()> {
        Ok(())
    }

    fn visit_literal(&mut self, literal: &mut Literal) -> Res<()> {
        Ok(())
    }

    fn visit_variable(&mut self, variable: &mut Variable) -> Res<()> {
        Ok(())
    }

    fn visit_allocation(
        &mut self,
        ty: &mut Type,
        constructor: &mut MutRc<Function>,
        args: &mut Vec<Expr>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_load(&mut self, object: &mut Expr, field: &Rc<Field>) -> Res<()> {
        Ok(())
    }

    fn visit_store(
        &mut self,
        object: &mut Expr,
        value: &mut Expr,
        first_store: &mut bool,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_binary(&mut self, left: &mut Expr, op: &mut Token, right: &mut Expr) -> Res<()> {
        Ok(())
    }

    fn visit_unary(&mut self, op: &mut Token, right: &mut Expr) -> Res<()> {
        Ok(())
    }

    fn visit_call(&mut self, callee: &mut Expr, arguments: &mut Vec<Expr>) -> Res<()> {
        Ok(())
    }

    fn visit_if(
        &mut self,
        condition: &mut Expr,
        then_br: &mut Expr,
        else_br: &mut Expr,
        phi_type: &mut Option<Type>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_switch(
        &mut self,
        branches: &mut Vec<(Expr, Expr)>,
        else_br: &mut Expr,
        phi_type: &mut Option<Type>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_loop(
        &mut self,
        condition: &mut Expr,
        body: &mut Expr,
        else_br: &mut Expr,
        phi_type: &mut Option<Type>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_break(&mut self, inner: &mut Expr) -> Res<()> {
        Ok(())
    }

    fn visit_return(&mut self, inner: &mut Expr) -> Res<()> {
        Ok(())
    }

    fn visit_cast(&mut self, inner: &mut Expr, to: &mut Type, method: &mut CastType) -> Res<()> {
        Ok(())
    }

    fn visit_closure(
        &mut self,
        function: &mut MutRc<Function>,
        captured: &mut Rc<Vec<Rc<LocalVariable>>>,
    ) -> Res<()> {
        Ok(())
    }

    fn visit_type_get(&mut self, inner: &mut Type) -> Res<()> {
        Ok(())
    }
}
