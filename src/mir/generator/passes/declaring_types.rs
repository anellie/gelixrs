/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 3:36 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::error::{Error, Res};
use crate::mir::{MModule, MutRc};
use crate::mir::generator::passes::{ModulePass, PassType};

/// This pass defines all types inside the module; currently classes and interfaces.
/// It only creates a stub MIR definition and inserts it as a type;
/// nothing is filled or created.
pub struct DeclareTypes();

impl ModulePass for DeclareTypes {
    fn get_type(&self) -> PassType { PassType::Module }

    fn run_mod(&mut self, module: MutRc<MModule>) -> Result<(), Vec<Error>> {
        let mut module = module.borrow_mut();

        for class in module.ast.classes.iter_mut() {
            // TODO
        }

        for iface in module.ast.interfaces.iter_mut() {
            // TODO
        }

        Ok(())
    }
}