pub use generated_nodes::*;
pub use literal::LiteralType;
use smol_str::SmolStr;
pub use types::TypeE;

use parser::{SyntaxNode, SyntaxToken};
use syntax::{kind::SyntaxKind, language::GelixLang};

mod generated_nodes;
mod literal;
mod types;

pub type CSTNode = SyntaxNode<GelixLang>;

impl Import {
    pub fn is_export(&self) -> bool {
        self.cst
            .children_with_tokens()
            .any(|c| c.as_token().map(SyntaxToken::kind) == Some(SyntaxKind::Export))
    }

    pub fn parts(&self) -> impl Iterator<Item = SmolStr> + '_ {
        self.cst
            .children_with_tokens()
            .filter(|c| {
                matches!(
                    c.as_token().map(SyntaxToken::<GelixLang>::kind),
                    Some(SyntaxKind::Identifier) | Some(SyntaxKind::Plus)
                )
            })
            .map(|c| c.as_token().unwrap().text().clone())
    }
}
