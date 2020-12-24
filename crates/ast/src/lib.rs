pub use generated_nodes::*;
pub use literal::LiteralType;
pub use module::Module;
pub use types::TypeE;

use parser::SyntaxNode;
use syntax::language::GelixLang;

mod generated_nodes;
mod literal;
mod module;
mod types;

pub type CSTNode = SyntaxNode<GelixLang>;
