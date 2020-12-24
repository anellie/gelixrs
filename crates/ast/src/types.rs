use crate::{Type, Variable};
use parser::SyntaxToken;
use smol_str::SmolStr;
use syntax::kind::SyntaxKind;

impl Type {
    pub fn get(&self) -> TypeE {
        let token = self.cst.first_token().unwrap();
        match token.kind() {
            SyntaxKind::Identifier if self.cst.first_child().is_none() => {
                // No children => only identifier
                TypeE::Ident(token.text().clone())
            }

            SyntaxKind::Identifier => {
                // Has children => generic type
                let types = self.cst.children().filter_map(Type::cast);
                TypeE::Generic {
                    ident: token.text().clone(),
                    types: types.collect(),
                }
            }

            SyntaxKind::Tilde => {
                TypeE::Value(self.cst.first_child().map(Self::cast).unwrap().unwrap())
            }
            SyntaxKind::AndSym => {
                TypeE::Weak(self.cst.first_child().map(Self::cast).unwrap().unwrap())
            }
            SyntaxKind::Star => {
                TypeE::RawPtr(self.cst.first_child().map(Self::cast).unwrap().unwrap())
            }

            SyntaxKind::LeftParen => {
                let mut types: Vec<_> = self.cst.children().filter_map(Type::cast).collect();
                TypeE::Closure {
                    ret_type: types.pop(),
                    params: types,
                }
            }

            _ => panic!("Cannot parse type"),
        }
    }
}

pub enum TypeE {
    Ident(SmolStr),
    Value(Type),
    Weak(Type),
    RawPtr(Type),

    Closure {
        params: Vec<Type>,
        ret_type: Option<Type>,
    },

    Generic {
        ident: SmolStr,
        types: Vec<Type>,
    },
}

impl Variable {
    pub fn mutable(&self) -> bool {
        self.cst
            .children_with_tokens()
            .any(|c| c.as_token().map(SyntaxToken::kind) == Some(SyntaxKind::Var))
    }
}
