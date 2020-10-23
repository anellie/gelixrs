/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{ast::module::ModulePath, error::Res, hir::hir_err, lexer::token::Token};

pub trait EmitHIRError<T> {
    fn on_err(self, module: &Rc<ModulePath>, error_token: &Token, msg: &str) -> Res<T>;
}

impl<T> EmitHIRError<T> for Option<T> {
    #[inline(always)]
    fn on_err(self, module: &Rc<ModulePath>, error_token: &Token, msg: &str) -> Res<T> {
        if let Some(i) = self {
            Ok(i)
        } else {
            Err(hir_err(error_token, msg.to_string(), module))
        }
    }
}

#[macro_use]
mod eat {
    /// This macro is used to generate binary operator parsing functions.
    /// The parser is a recursive descent parser.
    /// name is the name of the binary operation, next is the descending function name.
    /// matching is an array literal of the tokens that should match.
    macro_rules! eat {
        ($res:expr) => {{
            let macro_result = $res;
            if macro_result.is_ok() {
                macro_result.ok().unwrap()
            } else {
                return;
            }
        }};
    }

    macro_rules! eatc {
        ($res:expr) => {{
            let macro_result = $res;
            if macro_result.is_ok() {
                macro_result.ok().unwrap()
            } else {
                continue;
            }
        }};
    }
}
