#![feature(box_patterns)]
#![feature(box_syntax)]

pub mod declaration;
pub mod expression;
mod iface_impls;
pub mod literal;
pub mod module;
mod printer;
pub mod types;
mod visitor;

use ast::CSTNode;
pub use declaration::{Declaration, Function, ADT};
use error::{Error, ErrorSpan, GErr};
pub use expression::Expr;
pub use iface_impls::{IFaceImpl, IFaceImpls};
pub use literal::Literal;
pub use module::Module;
pub use types::{Instance, Type};
pub use visitor::Visitor;

/// Produces a new error for the GIR.
pub fn gir_err(cst: CSTNode, err: GErr) -> Error {
    Error {
        index: ErrorSpan::Span(cst.text_range().into()),
        kind: err,
    }
}
