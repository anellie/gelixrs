/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/16/19 9:25 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::error::Errors;
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::{MModule, MutRc};

/// This pass validates the intrinsics.
pub struct ValidateIntrinsics();

impl ModulePass for ValidateIntrinsics {
    fn get_type(&self) -> PassType {
        PassType::Globally
    }

    fn run_globally(&self, _modules: &[MutRc<MModule>]) -> Result<(), Vec<Errors>> {
        INTRINSICS
            .with(|i| i.borrow_mut().validate())
            .map_err(|e| vec![Errors(vec![e], Rc::new("".to_string()))])
    }
}
