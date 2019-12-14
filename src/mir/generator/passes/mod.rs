/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/14/19 5:02 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::Type;
use crate::lexer::token::Token;

pub mod declare_class;
pub mod declare_func;
pub mod declare_interface;
pub mod fill_class;
pub mod iface_impl;
pub mod import;

thread_local! {
    // A constant used by some passes that is simply gelix's None type.
    static NONE_CONST: Type = Type::Ident(Token::generic_identifier("None".to_string()));
    // An RC of the string 'internal-init'
    static INIT_CONST: Rc<String> = Rc::new("internal-init".to_string());
}
