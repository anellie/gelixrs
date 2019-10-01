/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/29/19 10:35 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::rc::Rc;

use either::Either;

use crate::ast::module::{Import, Module};
use crate::mir::generator::{MIRError, MIRGenerator};
use crate::mir::nodes::{MIRClass, MIRVariable};
use crate::mir::MutRc;
use crate::{module_path_to_string, ModulePath};

type ModulesRef<'t> = &'t mut Vec<(Module, MIRGenerator)>;

/// This pass tries to resolve all imports to a class.
pub fn class_imports(modules: ModulesRef) {
    drain_mod_imports(modules, &mut |modules, gen, import| {
        match find_class(modules, &import.path, &import.symbol) {
            Either::Left(class) => {
                class.and_then(|class| gen.builder.add_imported_class(class, true))
            }

            Either::Right(classes) => {
                // Do not import class methods.
                // They are imported later in function imports, as they appear
                // as regular functions in the module
                classes.iter().try_for_each(|(_, class)| {
                    gen.builder.add_imported_class(Rc::clone(class), false)
                });
                None // Functions still need to be imported!
            }
        }
        .is_some()
    });
}

fn find_class<'t>(
    modules: ModulesRef<'t>,
    path: &ModulePath,
    name: &String,
) -> Either<Option<MutRc<MIRClass>>, &'t HashMap<Rc<String>, MutRc<MIRClass>>> {
    let module = modules.iter().find(|(module, _)| &*module.path == path);

    if let Some(module) = module {
        match &name[..] {
            "+" => Either::Right(&module.1.builder.module.types),
            _ => Either::Left(module.1.builder.find_class(name)),
        }
    } else {
        Either::Left(None)
    }
}

/// This pass tries to resolve all imports to a function.
pub fn function_imports(modules: ModulesRef) {
    drain_mod_imports(modules, &mut |modules, gen, import| {
        match find_func(modules, &import.path, &import.symbol) {
            Either::Left(func) => func.and_then(|func| gen.builder.add_imported_function(func)),

            Either::Right(funcs) => funcs
                .iter()
                .try_for_each(|(_, func)| gen.builder.add_imported_function(Rc::clone(func))),
        }
        .is_some()
    });
}

fn find_func<'t>(
    modules: ModulesRef<'t>,
    path: &ModulePath,
    name: &String,
) -> Either<Option<Rc<MIRVariable>>, &'t HashMap<Rc<String>, Rc<MIRVariable>>> {
    let module = modules.iter().find(|(module, _)| &*module.path == path);

    if let Some(module) = module {
        match &name[..] {
            "+" => Either::Right(&module.1.builder.module.functions),
            _ => Either::Left(module.1.builder.find_global(name)),
        }
    } else {
        Either::Left(None)
    }
}

/// Returns 'Unresolved import' errors on any imports left.
pub fn ensure_no_imports(modules: &mut Vec<(Module, MIRGenerator)>) -> Result<(), Vec<MIRError>> {
    let mut errors = Vec::new();

    for (module, gen) in modules.iter() {
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
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// This function runs drain_filter on all imports in all modules, using the given function as a filter.
fn drain_mod_imports(
    modules: &mut Vec<(Module, MIRGenerator)>,
    cond: &mut dyn FnMut(&mut Vec<(Module, MIRGenerator)>, &mut MIRGenerator, &mut Import) -> bool,
) {
    // This piece of black magic iterates every module.
    // To allow for mutating it while accessing other modules immutably,
    // the module is temporarily removed.
    // This is done using swap_remove to prevent any array shifting or allocations.
    for i in 0..(modules.len() + 1) {
        let i = if i == modules.len() { 0 } else { i };
        let (mut module, mut gen) = modules.swap_remove(i);

        module
            .imports
            .drain_filter(|i| cond(modules, &mut gen, i))
            .count();
        modules.push((module, gen))
    }
}
