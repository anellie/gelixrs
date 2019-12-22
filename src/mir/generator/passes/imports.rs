/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/22/19 4:09 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefMut;
use std::mem;
use std::rc::Rc;

use crate::ast::{Import, Module};
use crate::error::{Error, Errors, Res};
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::result::ToMIRResult;
use crate::mir::{MModule, MutRc};

/// This pass imports all types.
pub struct ImportTypes();

impl PreMIRPass for ImportTypes {
    fn run(
        &mut self,
        ast: &mut Module,
        module: MutRc<MModule>,
        modules: &[MutRc<MModule>],
    ) -> Result<(), Errors> {
        module.borrow_mut().imports.ast = mem::replace(&mut ast.imports, vec![]);
        drain_mod_imports(modules, module, &mut |modules, module, import| {
            let src_module_rc = modules
                .iter()
                .find(|m| {
                    m.try_borrow().ok().map(|m| Rc::clone(&m.path)) == Some(Rc::clone(&import.path))
                })
                .or_err(&module.path, &import.symbol, "Unknown module.")?;

            let src_module = src_module_rc.borrow();
            match &import.symbol.lexeme[..] {
                "+" => {
                    for name in src_module.types.keys().chain(src_module.protos.keys()) {
                        module.try_reserve_name_rc(name, &import.symbol)?;
                    }
                    module
                        .imports
                        .modules
                        .insert(Rc::clone(&src_module.path), Rc::clone(src_module_rc));
                    Ok(false)
                }

                _ => {
                    let name = Rc::clone(&import.symbol.lexeme);
                    let ty = src_module.types.get(&name);

                    if let Some(ty) = ty {
                        module.imports.types.insert(name, ty.clone());
                    } else {
                        let proto = src_module.protos.get(&name);
                        match proto {
                            Some(p) => module.imports.protos.insert(name, p.clone()),
                            None => return Ok(false),
                        };
                    }

                    module.try_reserve_name(&import.symbol)?;
                    Ok(true)
                }
            }
        })
    }
}

/// This pass imports all globals.
pub struct ImportGlobals();

impl PreMIRPass for ImportGlobals {
    fn run(
        &mut self,
        _ast: &mut Module,
        module: MutRc<MModule>,
        modules: &[MutRc<MModule>],
    ) -> Result<(), Errors> {
        drain_mod_imports(modules, module, &mut |modules, module, import| {
            let src_module_rc = modules
                .iter()
                .find(|m| {
                    m.try_borrow().ok().map(|m| Rc::clone(&m.path)) == Some(Rc::clone(&import.path))
                })
                .or_err(&module.path, &import.symbol, "Unknown module.")?;

            let src_module = src_module_rc.borrow();
            match &import.symbol.lexeme[..] {
                "+" => {
                    for name in src_module.globals.keys() {
                        module.try_reserve_name_rc(name, &import.symbol)?;
                    }
                    Ok(true)
                }

                _ => {
                    module.try_reserve_name(&import.symbol)?;
                    let name = Rc::clone(&import.symbol.lexeme);
                    let global = src_module.globals.get(&name);

                    if let Some(global) = global {
                        module.imports.globals.insert(name, global.clone());
                        Ok(true)
                    } else {
                        Err(Error::new(
                            &import.symbol,
                            "MIR",
                            "Unknown module member.".to_string(),
                            &module.path,
                        ))
                    }
                }
            }
        })
    }
}

/// This function runs drain_filter on all imports in the given module, using the given function as a filter.
/// If the filter returns Err, the function exits prematurely and returns the error.
fn drain_mod_imports(
    modules: &[MutRc<MModule>],
    module: MutRc<MModule>,
    cond: &mut dyn FnMut(&[MutRc<MModule>], &mut RefMut<MModule>, &Import) -> Res<bool>,
) -> Result<(), Errors> {
    let mut errs = Vec::new();
    let mut module = module.borrow_mut();

    // This can be replaced with drain_filter once stabilized:
    // https://github.com/rust-lang/rust/issues/43244
    let mut i = 0;
    while i != module.imports.ast.len() {
        let import = module.imports.ast.remove(i);
        if !cond(modules, &mut module, &import)
            .map_err(|e| errs.push(e))
            .unwrap_or(false)
        {
            module.imports.ast.insert(i, import);
            i += 1;
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(Errors(errs, Rc::clone(&module.src)))
    }
}
