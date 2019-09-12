/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/12/19, 5:58 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::module::Module;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{MIRVariable, MIRStruct};
use crate::mir::MutRc;
use std::rc::Rc;
use crate::ModulePath;

/// This pass tries to resolve all imports to a class.
pub struct ImportClassPass<'p> {
    modules: &'p mut Vec<(Module, MIRGenerator)>,
}

impl<'p> ImportClassPass<'p> {
    pub fn run(mut self) {
        // This piece of black magic iterates every module.
        // To allow for mutating it while accessing other modules immutably,
        // the module is temporarily removed.
        // This is done using swap_remove to prevent any array shifting or allocations.
        for i in 0..self.modules.len() {
            let (mut module, mut gen) = self.modules.swap_remove(i);

            module.imports.drain_filter(|import| {
                let class = self.find_class(&import.path, &import.symbol);
                class.and_then(|class| gen.builder.add_imported_struct(class)).is_some()
            }).count();

            self.modules.push((module, gen))
        }
    }

    fn find_class(&mut self, path: &ModulePath, name: &String) -> Option<MutRc<MIRStruct>> {
        let module = self.modules.iter().find(|(module, _)| &*module.path == path)?;
        module.1.builder.find_struct(name)
    }

    pub fn new(modules: &'p mut Vec<(Module, MIRGenerator)>) -> ImportClassPass<'p> {
        Self { modules }
    }
}

/// This pass tries to resolve all imports to a function.
pub struct ImportFuncPass<'p> {
    modules: &'p mut Vec<(Module, MIRGenerator)>,
}

impl<'p> ImportFuncPass<'p> {
    pub fn run(mut self) -> Res<()> {
        // See ImportClassPass::run for explanation on this magic.
        for i in 0..self.modules.len() {
            let (mut module, mut gen) = self.modules.swap_remove(i);

            module.imports.drain_filter(|import| {
                let func = self.find_func(&import.path, &import.symbol);
                func.and_then(|func| gen.builder.add_imported_function(func)).is_some()
            }).count();

            self.modules.push((module, gen))
        }

        // TODO: Yell at the user for invalid imports
        Ok(())
    }

    fn find_func(&mut self, path: &ModulePath, name: &String) -> Option<Rc<MIRVariable>> {
        let module = self.modules.iter().find(|(module, _)| &*module.path == path)?;
        module.1.builder.find_global(name)
    }

    pub fn new(modules: &'p mut Vec<(Module, MIRGenerator)>) -> ImportFuncPass<'p> {
        Self { modules }
    }
}
