/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::CSTNode;
use syntax::kind::SyntaxKind;

/// An enum containing all literals possible in Gelix.
/// TODO: Rework production of this, use generator similar to Type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Literal {
    pub data: LiteralData,
    pub cst: CSTNode
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralData {
    Any,
    None,
    True,
    False,
    Int,
    Float,
    String,
    Closure,
}

impl Literal {
    #[allow(unused)]
    pub fn cast(node: CSTNode) -> Option<Self> {
        if node.kind() == SyntaxKind::Literal {
            let child = node.children_with_tokens().next().unwrap();
            let data = match child.as_token().unwrap().kind() {
                SyntaxKind::False => LiteralData::False,
                SyntaxKind::True => LiteralData::True,
                SyntaxKind::Int => LiteralData::Int,
                SyntaxKind::Float => LiteralData::Float,
                SyntaxKind::String => LiteralData::String,
                SyntaxKind::ClosureLiteral => LiteralData::Closure,
                _ => panic!("AST encountered unknown CST literal")
            };

            Some(Self {
                data,
                cst: node
            })
        } else {
            None
        }
    }
}
