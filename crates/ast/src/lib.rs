pub use generated_nodes::*;
pub use literal::LiteralType;
use smol_str::SmolStr;
pub use types::TypeE;

use parser::{Node, Token};
use syntax::kind::SyntaxKind;

pub type CSTNode = Node;

mod generated_nodes;
mod literal;
mod types;

impl Import {
    pub fn is_export(&self) -> bool {
        self.cst
            .children_with_tokens()
            .any(|c| c.as_token().map(Token::kind) == Some(SyntaxKind::Export))
    }

    pub fn parts(&self) -> impl Iterator<Item = SmolStr> + '_ {
        self.cst
            .children_with_tokens()
            .filter(|c| {
                matches!(
                    c.as_token().map(Token::kind),
                    Some(SyntaxKind::Identifier) | Some(SyntaxKind::Plus)
                )
            })
            .map(|c| c.as_token().unwrap().text().clone())
    }
}

impl Function {
    pub fn cast_constructor(node: CSTNode) -> Option<Self> {
        if let SyntaxKind::Constructor = node.kind() {
            Some(Self { cst: node })
        } else {
            None
        }
    }
}
