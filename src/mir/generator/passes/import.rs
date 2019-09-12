/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/12/19, 9:16 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::module::Module;
use crate::mir::generator::{MIRGenerator, MIRError};
use crate::mir::nodes::{MIRStruct, MIRVariable};
use crate::mir::MutRc;
use crate::{ModulePath, module_path_to_string};
use std::rc::Rc;

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

            module
                .imports
                .drain_filter(|import| {
                    self.find_class(&import.path, &import.symbol)
                        .and_then(|class| gen.builder.add_imported_struct(class))
                        .is_some()
                })
                .count();

            self.modules.push((module, gen))
        }
    }

    fn find_class(&mut self, path: &ModulePath, name: &String) -> Option<MutRc<MIRStruct>> {
        let module = self
            .modules
            .iter()
            .find(|(module, _)| &*module.path == path)?;
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
    pub fn run(mut self) -> Result<(), Vec<MIRError>> {
        let mut errors = Vec::new();

        // See ImportClassPass::run for explanation on this magic.
        for i in 0..self.modules.len() {
            let (mut module, mut gen) = self.modules.swap_remove(i);

            module
                .imports
                .drain_filter(|import| {
                    self.find_func(&import.path, &import.symbol)
                        .and_then(|func| gen.builder.add_imported_function(func))
                        .is_some()
                })
                .count();

            // Imports should be all resolved; if not, they were invalid and an error
            for import in &module.imports {
                let mut full_path = import.path.clone();
                full_path.push(import.symbol.clone());

                errors.push(gen.anon_err(
                    None,
                    &format!(
                        "Invalid import: {:?}\n(Either the specified symbol was not found, or the name already exists in the current module.)",
                        module_path_to_string(&full_path)
                    )))
            }

            self.modules.push((module, gen))
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    fn find_func(&mut self, path: &ModulePath, name: &String) -> Option<Rc<MIRVariable>> {
        let module = self
            .modules
            .iter()
            .find(|(module, _)| &*module.path == path)?;
        module.1.builder.find_global(name)
    }

    pub fn new(modules: &'p mut Vec<(Module, MIRGenerator)>) -> ImportFuncPass<'p> {
        Self { modules }
    }
}
