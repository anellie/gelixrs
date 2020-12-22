#![feature(box_patterns)]
#![feature(box_syntax)]

pub mod declaration;
pub mod expression;
mod iface_impls;
pub mod module;
mod printer;
pub mod types;
pub mod literal;
mod visitor;

use ast::CSTNode;
pub use declaration::{Declaration, Function, ADT};
use error::{Error, ErrorSpan};
pub use iface_impls::get_iface_impls;
pub use expression::Expr;
pub use literal::Literal;
pub use module::Module;
pub use types::{Type, Instance};
pub use visitor::Visitor;

/// Produces a new error for the GIR.
pub fn gir_err(cst: CSTNode, message: String) -> Error {
    Error {
        index: ErrorSpan::Span(cst.text_range().into()),
        code: "G001", // TODO error codes
        message,
    }
}
