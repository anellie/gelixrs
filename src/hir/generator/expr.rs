use crate::{
    ast::Expression,
    error::Res,
    hir::{generator::HIRGenerator, nodes::expression::Expr},
};

impl HIRGenerator {
    /// Generate a single expression inside the current module.
    /// Will set flags on the generator if applicable.
    pub fn expression(&mut self, ast: &Expression) -> Expr {
        todo!()
    }
}
