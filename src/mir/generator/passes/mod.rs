/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/2/19 4:44 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::ASTType;
use crate::lexer::token::Token;

pub(super) mod declare_class;
pub(super) mod declare_func;
pub(super) mod declare_interface;
pub(super) mod fill_class;
pub(super) mod iface_impl;
pub(super) mod import;

thread_local! {
    // A constant used by some passes that is simply gelix's None type.
    static NONE_CONST: ASTType = ASTType::Token(Token::generic_identifier("None".to_string()));
    // A constant of the 'This' type alias.
    static THIS_CONST: Rc<String> = Rc::new("This".to_string());
}
