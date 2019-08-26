/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/26/19 6:49 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::PostMIRPass;

pub struct TypecheckPass<'p> {
    gen: &'p mut MIRGenerator
}

impl<'p> PostMIRPass for TypecheckPass<'p> {
    fn run(mut self) -> Res<()> {
        unimplemented!()
    }
}

impl<'p> TypecheckPass<'p> {
    pub fn new(gen: &'p mut MIRGenerator) -> TypecheckPass<'p> {
        TypecheckPass { gen }
    }
}
