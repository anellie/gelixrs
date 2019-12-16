/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/16/19 4:10 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::declaration::{Function, FunctionArg, IFaceImpl, Type as ASTType};
use crate::ast::module::Module;
use crate::error::Error;
use crate::lexer::token::Token;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{
    IFaceImpl as MIRImpl, IFaceImpls, IFaceMethod, Interface, InterfacePrototype, Type, Variable,
};
use crate::mir::{mutrc_new, MModule, MutRc, IFACE_IMPLS};
use crate::option::Flatten;

/// This pass defines all methods on classes and interfaces.
pub struct FillIfaceImpls();

impl ModulePass for FillIfaceImpls {
    fn get_type(&self) -> PassType {
        PassType::Type
    }

    fn run_type(&mut self, module: &MutRc<MModule>, ty: Type) -> Result<(), Error> {
        unimplemented!()
    }
}
