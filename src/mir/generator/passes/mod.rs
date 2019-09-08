/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/8/19, 3:47 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::Res;
use crate::ast::module::FileModule;

pub(super) mod declare;
pub(super) mod fill_struct;

/// A trait for passes that modify the AST or the generator before MIR creation.
pub(super) trait PreMIRPass {
    fn run(self, list: &mut FileModule) -> Res<()>;
}
