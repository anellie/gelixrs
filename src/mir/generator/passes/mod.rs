/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 9:49 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ast::declaration::Type;
use crate::lexer::token::Token;

pub(super) mod declare_class;
pub(super) mod declare_func;
pub(super) mod declare_interface;
pub(super) mod fill_class;
//pub(super) mod iface_impl;
pub(super) mod import;

thread_local! {
    // A constant used by some passes that is simply gelix's None type.
    static NONE_CONST: Type = Type::Ident(Token::generic_identifier("None".to_string()));
}
