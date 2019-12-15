/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 2:11 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ast::module::Module;
use crate::error::{Error, Errors};
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::IFACE_IMPLS;

/// A set of [MIRGenerator]s.
/// Takes a list of module ASTs and transforms them into
/// MIR modules.
pub struct MIRModuleGenerator {
    modules: Vec<MutRc<MModule>>,
}

impl MIRModuleGenerator {
    pub fn execute(mut self) -> Result<Vec<MutRc<MModule>>, Vec<Errors>> {
        reset_mir();
        INTRINSICS.with(|i| i.borrow_mut().populate(&mut self.modules));
        INTRINSICS.with(|i| i.borrow_mut().validate()).map_err(|e| vec![Errors(vec![e], "".to_string())])?;
        Ok(self.modules)
    }

    pub fn new(modules: Vec<Module>) -> Self {
        Self {
            modules: modules.into_iter().map(MModule::new).map(mutrc_new).collect(),
        }
    }
}

/// This function resets MIR global state.
/// Called before starting a new compile.
fn reset_mir() {
    INTRINSICS.with(|i| i.borrow_mut().reset());
    IFACE_IMPLS.with(|i| i.borrow_mut().clear());
}
