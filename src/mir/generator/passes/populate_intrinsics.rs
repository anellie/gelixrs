/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::{
    ast::Module,
    error::Errors,
    mir::{
        generator::{intrinsics::INTRINSICS, passes::PreMIRPass},
        MModule, MutRc,
    },
};

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
            // This is the prelude, containing the array and string classes
            INTRINSICS.with(|i| {
                i.borrow_mut().array_proto = module.find_prototype(&"Array".to_string());
                i.borrow_mut().string_type = module.find_type(&"String".to_string());
            })
        }
        Ok(())
    }
}
