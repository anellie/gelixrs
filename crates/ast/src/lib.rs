pub use generated_nodes::*;
pub use literal::Literal;
pub use module::Module;

use parser::SyntaxNode;
use syntax::language::GelixLang;

mod generated_nodes;
mod literal;
mod module;
mod types;

pub type CSTNode = SyntaxNode<GelixLang>;
