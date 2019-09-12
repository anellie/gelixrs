/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/11/19, 6:57 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::module::Module;
use crate::mir::generator::Res;

pub(super) mod declare;
pub(super) mod fill_struct;
pub(super) mod import;

/// A trait for passes that modify the AST or the generator before MIR creation.
pub(super) trait PreMIRPass {
    fn run(self, module: &mut Module) -> Res<()>;
}
