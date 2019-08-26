/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/26/19 6:47 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::DeclarationList;
use crate::mir::generator::Res;

pub(super) mod declare;
pub(super) mod fill_struct;
pub(super) mod typecheck;

pub(super) trait PreMIRPass {
    fn run(self, list: &mut DeclarationList) -> Res<()>;
}

pub(super) trait PostMIRPass {
    fn run(self) -> Res<()>;
}