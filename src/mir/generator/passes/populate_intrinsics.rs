/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/20/19 7:43 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::error::Errors;
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::{MModule, MutRc};
use crate::ast::Module;

/// This pass populates the intrinsics struct.
pub struct PopulateIntrinsics();

impl PreMIRPass for PopulateIntrinsics {
    fn run(
        &mut self,
        _ast: &mut Module,
        module: MutRc<MModule>,
        _modules: &[MutRc<MModule>],
    ) -> Result<(), Errors> {
        let module = module.borrow();
        if **module.path.0[0] == *"std" && **module.path.0[1] == *"ops" {
            // This is the std/ops module, containing all operator interfaces
            INTRINSICS.with(|i| i.borrow_mut().fill_ops_table(module))
        } else if **module.path.0[0] == *"std" && **module.path.0[1] == *"prelude" {
            // This is the prelude, containing the array class
            INTRINSICS.with(|i| i.borrow_mut().array_proto = module.find_prototype(&"Array".to_string()))
        }
        Ok(())
    }
}
