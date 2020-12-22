/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::Literal;
use syntax::kind::SyntaxKind;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralType {
    True,
    False,
    Int,
    Float,
    String,
}

impl Literal {
    #[allow(unused)]
    pub fn get(&self) -> (SmolStr, LiteralType) {
        let child = self.cst.children_with_tokens().next().unwrap();
        let token = child.as_token().unwrap();
        let kind = match token.kind() {
            SyntaxKind::False => LiteralType::False,
            SyntaxKind::True => LiteralType::True,
            SyntaxKind::Int => LiteralType::Int,
            SyntaxKind::Float => LiteralType::Float,
            SyntaxKind::String => LiteralType::String,
            _ => panic!("AST encountered unknown CST literal")
        };
        (token.text().clone(), kind)
    }
}
