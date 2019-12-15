/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 4:19 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::error::Res;
use crate::mir::{MModule, MutRc};
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::generator::passes::{ModulePass, PassType};

/// This pass validates the intrinsics.
pub struct ValidateIntrinsics();

impl ModulePass for ValidateIntrinsics {
    fn get_type(&self) -> PassType {
        PassType::GlobalInspect
    }

    fn run_inspect(&mut self, _modules: &Vec<MutRc<MModule>>) -> Res<()> {
        INTRINSICS.with(|i| i.borrow_mut().validate())
    }
}
