/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 2:07 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::Type;
use crate::error::Res;
use crate::lexer::token::Token;
use crate::mir::{MModule, MutRc};

pub mod declaring_types;
pub mod filter_prototypes;

thread_local! {
    // A constant used by some passes that is simply gelix's None type.
    static NONE_CONST: Type = Type::Ident(Token::generic_identifier("None".to_string()));
    // An RC of the string 'internal-init'
    static INIT_CONST: Rc<String> = Rc::new("internal-init".to_string());
}

/// A pass that takes a MIR module and performs some kind of transformations
/// on the module.
/// These modules are collected and executed in order inside mir/generator/module.rs.
pub trait ModulePass {
    fn run(&mut self, module: MutRc<MModule>) -> Res<()>;
}
