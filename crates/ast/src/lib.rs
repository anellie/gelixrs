pub use generated_nodes::*;
pub use literal::LiteralType;
pub use types::TypeE;

use parser::SyntaxNode;
use syntax::language::GelixLang;

mod generated_nodes;
mod literal;
mod types;

pub type CSTNode = SyntaxNode<GelixLang>;
