/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 11:02 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use either::Either::{Left, Right};

use crate::module_path_to_string;
use crate::ast::module::{Import, Module};
use crate::mir::generator::{MIRError, MIRGenerator, Res};
use crate::mir::ToMIRResult;

type ModulesRef<'t> = &'t mut Vec<(Module, MIRGenerator)>;

#[macro_use]
mod import_macro {
    /// This macro generates a function for importing something.
    /// This works since the builder has separate fields for different
    /// things, but they can all be handled the same when importing.
    #[macro_export]
    macro_rules! import {
        ($fn_name: ident, $name:ident, $getter:ident) => {
            pub fn $fn_name(modules: ModulesRef) -> Result<(), Vec<MIRError>> {
                drain_mod_imports(modules, &mut |modules, gen, import| {
                    let (module, mod_gen) = modules
                        .iter()
                        .find(|(module, _)| *module.path == import.path)
                        .or_err(gen, &import.symbol, "Unknown module.")?;

                    Ok(match &import.symbol.lexeme[..] {
                        "+" => {
                            gen.builder.prototypes.$name.extend(
                                mod_gen
                                    .builder
                                    .prototypes
                                    .$name
                                    .iter()
                                    .map(|(a, b)| (a.clone(), b.clone())),
                            );
                            gen.builder.imports.$name.extend(
                                mod_gen
                                    .builder
                                    .module
                                    .$name
                                    .iter()
                                    .map(|(a, b)| (a.clone(), b.clone())),
                            );
                            for name in mod_gen
                                .builder
                                .prototypes
                                .$name
                                .keys()
                                .chain(mod_gen.builder.module.$name.keys())
                            {
                                gen.builder.try_reserve_name_rc(name, &import.symbol)?
                            }
                            false
                        }

                        _ => {
                            let thing = mod_gen.builder.$getter(&import.symbol.lexeme);
                            let success = match thing {
                                Some(Left(thing)) => gen
                                    .builder
                                    .imports
                                    .$name
                                    .insert(Rc::clone(&import.symbol.lexeme), thing)
                                    .is_none(),
                                Some(Right(proto)) => gen
                                    .builder
                                    .prototypes
                                    .$name
                                    .insert(Rc::clone(&import.symbol.lexeme), proto)
                                    .is_none(),
                                _ => false,
                            };

                            if success {
                               gen.builder.try_reserve_name(&import.symbol)?
                            }
                            success
                        }
                    })
                })
            }
        };
    }
}

import!(class_imports, classes, find_class_or_proto);
import!(interface_imports, interfaces, find_iface_or_proto);
import!(function_imports, functions, find_func_or_proto);

/// Returns 'Unresolved import' errors on any imports left.
pub fn ensure_no_imports(modules: &mut Vec<(Module, MIRGenerator)>) -> Result<(), Vec<MIRError>> {
    let mut errors = Vec::new();
    for (module, gen) in modules.iter() {
        for import in &module.imports {
            if import.symbol.lexeme.as_ref() == "+" {
                continue
            }

            errors.push(gen.anon_err(
                None,
                &format!(
                    "Symbol '{}' not found in module {:?}",
                    import.symbol.lexeme,
                    module_path_to_string(&import.path)
                ),
            ))
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// This function runs drain_filter on all imports in all modules, using the given function as a filter.
/// If the filter returns Err, the function exits prematurely and returns the error.
fn drain_mod_imports(
    modules: &mut Vec<(Module, MIRGenerator)>,
    cond: &mut dyn FnMut(
        &mut Vec<(Module, MIRGenerator)>,
        &mut MIRGenerator,
        &mut Import,
    ) -> Res<bool>,
) -> Result<(), Vec<MIRError>> {
    // This piece of black magic iterates every module.
    // To allow for mutating it while accessing other modules immutably,
    // the module is temporarily removed.
    // This is done using swap_remove to prevent any array shifting or allocations.
    for i in 0..modules.len() {
        let i = if i == (modules.len() - 1) { 0 } else { i };
        let (mut module, mut gen) = modules.swap_remove(i);

        // This can be replaced with drain_filter once stabilized:
        // https://github.com/rust-lang/rust/issues/43244
        let mut i = 0;
        while i != module.imports.len() {
            if cond(modules, &mut gen, &mut module.imports[i]).map_err(|e| vec![e])? {
                module.imports.remove(i);
            } else {
                i += 1;
            }
        }

        modules.push((module, gen))
    }
    Ok(())
}
