/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use ast::CSTNode;
use error::{GErr, Res};
use gir_nodes::gir_err;

pub trait EmitGIRError<T> {
    fn or_err(self, cst: &CSTNode, err: GErr) -> Res<T>;
    fn or_error<F: FnOnce() -> GErr>(self, cst: &CSTNode, err: F) -> Res<T>;
}

impl<T> EmitGIRError<T> for Option<T> {
    #[inline(always)]
    fn or_err(self, cst: &CSTNode, err: GErr) -> Res<T> {
        if let Some(i) = self {
            Ok(i)
        } else {
            Err(gir_err(cst.clone(), err))
        }
    }

    #[inline(always)]
    fn or_error<F: FnOnce() -> GErr>(self, cst: &CSTNode, err: F) -> Res<T> {
        if let Some(i) = self {
            Ok(i)
        } else {
            Err(gir_err(cst.clone(), err()))
        }
    }
}

#[macro_use]
mod eat {
    #[macro_export]
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
    #[macro_export]
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
