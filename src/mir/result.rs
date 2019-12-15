/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/14/19 11:03 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ast::Type as ASTType;
use crate::lexer::token::Token;
use crate::error::{Res, Error};
use crate::ast::module::ModulePath;
use std::rc::Rc;

pub trait ToMIRResult<T> {
    fn or_err(self, module: &Rc<ModulePath>, error_token: &Token, msg: &str) -> Res<T>;
    fn or_anon_err(self, module: &Rc<ModulePath>, error_token: Option<&Token>, msg: &str, ) -> Res<T>;
    fn or_type_err(self, module: &Rc<ModulePath>, error_ty: ASTType, msg: &str, ) -> Res<T>;
}

impl<T> ToMIRResult<T> for Option<T> {
    #[inline(always)]
    fn or_err(self, module: &Rc<ModulePath>, error_token: &Token, msg: &str) -> Res<T> {
        self.ok_or_else(|| Error::new(error_token, "MIR", msg.to_string(), module))
    }

    #[inline(always)]
    fn or_anon_err(
        self,
        module: &Rc<ModulePath>,
        error_token: Option<&Token>,
        msg: &str,
    ) -> Res<T> {
        self.ok_or_else(|| Error::new(&Token::eof_token(0), "MIR", msg.to_string(), module))
    }

    #[inline(always)]
    fn or_type_err(
        self,
        module: &Rc<ModulePath>,
        error_ty: ASTType,
        msg: &str,
    ) -> Res<T> {
        self.ok_or_else(|| Error::new(error_ty.get_token(), "MIR", msg.to_string(), module))
    }
}
