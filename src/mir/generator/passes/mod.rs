/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 4:19 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::Ref;
use std::rc::Rc;

use crate::ast::Type as ASTType;
use crate::error::{Error, Res};
use crate::lexer::token::Token;
use crate::mir::nodes::{Type, Variable};
use crate::mir::{MModule, MutRc};

pub mod declaring_globals;
pub mod declaring_types;
pub mod filter_prototypes;
pub mod populate_intrinsics;
pub mod validate;

thread_local! {
    // A constant used by some passes that is simply gelix's None type.
    static NONE_CONST: ASTType = ASTType::Ident(Token::generic_identifier("None".to_string()));
    // An RC of the string 'internal-init'
    static INIT_CONST: Rc<String> = Rc::new("internal-init".to_string());
}

/// A pass that takes a MIR module and performs some kind of transformation
/// on the module.
/// The way these modules are called depends on their type,
/// see the PassType struct.
/// These modules are collected and executed in order inside mir/generator/module.rs.
pub trait ModulePass {
    fn get_type(&self) -> PassType;
    fn run_inspect(&mut self, _modules: &Vec<MutRc<MModule>>) -> Res<()> {
        Ok(())
    }
    fn run_mod(&mut self, _module: MutRc<MModule>) -> Result<(), Vec<Error>> {
        Ok(())
    }
    fn run_type(&mut self, _module: Ref<MModule>, _ty: &Type) -> Res<()> {
        Ok(())
    }
    fn run_global(&mut self, _module: Ref<MModule>, _global: &Rc<Variable>) -> Res<()> {
        Ok(())
    }
}

/// Defines the type of a pass, and the way the pass will be called.
/// The reason for this pass implementation is that prototypes
/// require 'catching up' when instanced later.
/// By specifying which pass affects them, its easy to do so.
pub enum PassType {
    /// This pass runs on the all modules and does not
    /// modify them. Most of these passes do some form of
    /// validation or data collection.
    GlobalInspect,
    /// This pass runs on the whole module and modifies
    /// said module.
    Module,
    /// This pass only modifies a specific type in the module.
    /// It should not modify anything else in the module.
    Type,
    /// This pass only modifies a specific global/function
    /// in the module.
    Global,
}
