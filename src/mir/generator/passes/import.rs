/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/17/19 5:15 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::rc::Rc;

use either::Either;

use crate::{module_path_to_string, ModulePath};
use crate::ast::module::Module;
use crate::mir::generator::{MIRError, MIRGenerator};
use crate::mir::MutRc;
use crate::mir::nodes::{MIRStruct, MIRVariable};

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
        for i in 0..(self.modules.len() + 1) {
            let i = if i == self.modules.len() { 0 } else { i };
            let (mut module, mut gen) = self.modules.swap_remove(i);

            module
                .imports
                .drain_filter(|import| {
                    match self.find_class(&import.path, &import.symbol) {
                        Either::Left(class) => {
                            class.and_then(|class| gen.builder.add_imported_struct(class, true))
                        }

                        Either::Right(classes) => {
                            // Do not import class methods.
                            // They are imported later in ImportFuncPass, as they appear
                            // as regular functions in the module
                            classes.iter().try_for_each(|(_, class)| {
                                gen.builder.add_imported_struct(Rc::clone(class), false)
                            });
                            None // Functions still need to be imported!
                        }
                    }
                    .is_some()
                })
                .count();

            self.modules.push((module, gen))
        }
    }

    fn find_class(
        &mut self,
        path: &ModulePath,
        name: &String,
    ) -> Either<Option<MutRc<MIRStruct>>, &HashMap<Rc<String>, MutRc<MIRStruct>>> {
        let module = self
            .modules
            .iter()
            .find(|(module, _)| &*module.path == path);

        if let Some(module) = module {
            match &name[..] {
                "+" => Either::Right(&module.1.builder.module.types),
                _ => Either::Left(module.1.builder.find_struct(name)),
            }
        } else {
            Either::Left(None)
        }
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
        for i in 0..(self.modules.len() + 1) {
            let i = if i == self.modules.len() { 0 } else { i };
            let (mut module, mut gen) = self.modules.swap_remove(i);

            module
                .imports
                .drain_filter(|import| {
                    match self.find_func(&import.path, &import.symbol) {
                        Either::Left(func) => {
                            func.and_then(|func| gen.builder.add_imported_function(func))
                        }

                        Either::Right(funcs) => funcs.iter().try_for_each(|(_, func)| {
                            gen.builder.add_imported_function(Rc::clone(func))
                        }),
                    }
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

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn find_func(
        &mut self,
        path: &ModulePath,
        name: &String,
    ) -> Either<Option<Rc<MIRVariable>>, &HashMap<Rc<String>, Rc<MIRVariable>>> {
        let module = self
            .modules
            .iter()
            .find(|(module, _)| &*module.path == path);

        if let Some(module) = module {
            match &name[..] {
                "+" => Either::Right(&module.1.builder.module.functions),
                _ => Either::Left(module.1.builder.find_global(name)),
            }
        } else {
            Either::Left(None)
        }
    }

    pub fn new(modules: &'p mut Vec<(Module, MIRGenerator)>) -> ImportFuncPass<'p> {
        Self { modules }
    }
}
