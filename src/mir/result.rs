/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/14/19 11:03 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ast::Type as ASTType;
use crate::lexer::token::Token;
use crate::error::Res;

pub trait ToMIRResult<T> {
    fn or_err(self, error_token: &Token, msg: &str) -> Res<T>;

    fn or_anon_err(self, error_token: Option<&Token>, msg: &str, ) -> Res<T>;

    fn or_type_err(self, error_ty: &Option<ASTType>, msg: &str, ) -> Res<T>;
}

impl<T> ToMIRResult<T> for Option<T> {
    #[inline(always)]
    fn or_err(self, error_token: &Token, msg: &str) -> Res<T> {
        self.ok_or_else(|| gen.error(error_token, error_token, msg))
    }

    #[inline(always)]
    fn or_anon_err(
        self,
        error_token: Option<&Token>,
        msg: &str,
    ) -> Res<T> {
        self.ok_or_else(|| gen.anon_err(error_token, msg))
    }

    #[inline(always)]
    fn or_type_err(
        self,
        error_ty: ASTType,
        msg: &str,
    ) -> Res<T> {
        self.ok_or_else(|| gen.anon_err(error_ty.as_ref().map(|t| t.get_token()).flatten_(), msg))
    }
}
