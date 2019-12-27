/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{
    ast::{Module, Type as ASTType},
    error::{Error, Errors, Res},
    lexer::token::Token,
    mir::{
        generator::MIRGenerator,
        nodes::{Type, Variable},
        MModule, MutRc,
    },
};

pub mod declaring_globals;
pub mod declaring_iface_impls;
pub mod declaring_methods;
pub mod declaring_types;
pub mod fill_impls;
pub mod filter_prototypes;
pub mod generate;
pub mod generate_impls;
pub mod imports;
pub mod insert_members;
pub mod populate_intrinsics;
pub mod validate;

thread_local! {
    // A constant used by some passes that is simply gelix's None type.
    static NONE_CONST: ASTType = ASTType::Ident(Token::generic_identifier("None".to_string()));
    // An RC of the string 'internal-init'
    static INIT_CONST: Rc<String> = Rc::new("internal-init".to_string());
}

/// A pass that runs before the AST is discarded.
pub trait PreMIRPass {
    fn run(
        &mut self,
        ast: &mut Module,
        module: MutRc<MModule>,
        modules: &[MutRc<MModule>],
    ) -> Result<(), Errors>;
}

/// A pass that takes a MIR module and performs some kind of transformation
/// on the module.
/// The way these modules are called depends on their type,
/// see the PassType struct.
/// These modules are collected and executed in order inside mir/generator/module.rs.
pub trait ModulePass {
    fn get_type(&self) -> PassType;
    fn run_globally(&self, _modules: &[MutRc<MModule>]) -> Result<(), Vec<Errors>> {
        Ok(())
    }
    fn run_mod(&self, _gen: &mut MIRGenerator) -> Result<(), Vec<Error>> {
        Ok(())
    }
    fn run_type(&self, _gen: &mut MIRGenerator, _ty: Type) -> Res<()> {
        Ok(())
    }
    fn run_global_var(&self, _gen: &mut MIRGenerator, _global: Rc<Variable>) -> Res<()> {
        Ok(())
    }
}

/// Defines the type of a pass, and the way the pass will be called.
/// The reason for this pass implementation is that prototypes
/// require 'catching up' when instanced later.
/// By specifying which pass affects them, its easy to do so.
pub enum PassType {
    /// This pass runs on the all modules.
    /// Most of these passes do some form of
    /// validation or data collection;
    /// import resolution also belongs in here.
    Globally,
    /// This pass runs on the whole module and modifies
    /// said module.
    Module,
    /// This pass only modifies a specific type in the module.
    /// It should not modify anything else in the module.
    /// Note that the [Context] of non-primitive types (class/iface/func)
    /// is automatically attached.
    Type,
    /// This pass only modifies a specific global/function
    /// in the module.
    GlobalVar,
}
