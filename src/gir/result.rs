/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{ast::module::ModulePath, error::Res, gir::gir_err, lexer::token::Token, ast};
use crate::error::Error;

pub trait EmitGIRError<T> {
    fn on_err(self, module: &Rc<ModulePath>, error_token: &Token, msg: &str) -> Res<T>;
}

impl<T> EmitGIRError<T> for Option<T> {
    #[inline(always)]
    fn on_err(self, module: &Rc<ModulePath>, error_token: &Token, msg: &str) -> Res<T> {
        if let Some(i) = self {
            Ok(i)
        } else {
            Err(gir_err(error_token, msg.to_string(), module))
        }
    }
}

pub trait ToGIRResult<T> {
    fn or_err(self, module: &Rc<ModulePath>, error_token: &Token, msg: &str) -> Res<T>;
    fn or_type_err(self, module: &Rc<ModulePath>, error_ty: &ast::Type, msg: &str) -> Res<T>;
}

impl<T> ToGIRResult<T> for Option<T> {
    #[inline(always)]
    fn or_err(self, module: &Rc<ModulePath>, error_token: &Token, msg: &str) -> Res<T> {
        self.ok_or_else(|| Error::new(error_token, "GIR", msg.to_string(), module))
    }

    #[inline(always)]
    fn or_type_err(self, module: &Rc<ModulePath>, error_ty: &ast::Type, msg: &str) -> Res<T> {
        self.ok_or_else(|| Error::new(error_ty.token(), "GIR", msg.to_string(), module))
    }
}

#[macro_use]
mod eat {
    macro_rules! eat {
        ($gen:expr, $res:expr) => {{
            match $res {
                Ok(r) => r,
                Err(e) => {
                    $gen.error(e);
                    return;
                }
            }
        }};
    }

    macro_rules! eatc {
        ($gen:expr, $res:expr) => {{
            match $res {
                Ok(r) => r,
                Err(e) => {
                    $gen.error(e);
                    continue;
                }
            }
        }};
    }
}
