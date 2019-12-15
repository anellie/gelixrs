/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 3:36 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::module::Module;
use crate::error::{Error, Errors, Res};
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::generator::passes::declaring_types::DeclareTypes;
use crate::mir::generator::passes::filter_prototypes::FilterPrototypes;
use crate::mir::generator::passes::populate_intrinsics::PopulateIntrinsics;
use crate::mir::generator::passes::validate::ValidateIntrinsics;
use crate::mir::IFACE_IMPLS;

/// Responsible for collecting all passes that run on the MIR.
/// MIR is built purely by running many transformation passes,
/// it is considered compiled once the last pass
/// has been run.
pub struct PassRunner {
    /// All the modules in this compilation run.
    modules: Vec<MutRc<MModule>>,
}

impl PassRunner {
    pub fn execute(mut self) -> Result<Vec<MutRc<MModule>>, Vec<Errors>> {
        reset_mir();
        let mut passes: Vec<Box<dyn ModulePass>> = vec![
            Box::new(FilterPrototypes()),
            Box::new(DeclareTypes()),
            Box::new(PopulateIntrinsics()),
            Box::new(ValidateIntrinsics())
        ];

        for mut pass in passes.drain(..) {
            self.run_pass(&mut pass)?;
        }

        Ok(self.modules)
    }

    pub fn run_pass(&self, pass: &mut Box<dyn ModulePass>) -> Result<(), Vec<Errors>> {
        match pass.get_type() {
            PassType::GlobalInspect => {
                pass.run_inspect(&self.modules).map_err(|e| vec![Errors(vec![e], Rc::new("".to_string()))])?;
            }

            _ => {
                let mut errs = Vec::new();

                for module in self.modules.iter() {
                    self.run_module_pass(module, pass)
                        .map_err(|e| errs.push(Errors(e, Rc::clone(&module.borrow().ast.src))));
                }

                if !errs.is_empty() {
                    return Err(errs)
                }
            }
        }
        Ok(())
    }

    pub fn run_module_pass(&self, module: &MutRc<MModule>, pass: &mut Box<dyn ModulePass>) -> Result<(), Vec<Error>> {
        let mut errs = Vec::new();

        match pass.get_type() {
            PassType::Module => {
                pass.run_mod(Rc::clone(module)).map_err(|mut e| errs.append(&mut e));
            },

            PassType::Type => {
                for (name, ty) in module.borrow().types.iter() {
                    pass.run_type(module.borrow(), ty).map_err(|e| errs.push(e));
                }
            }

            PassType::Global => {
                for (name, global) in module.borrow().globals.iter() {
                    pass.run_global(module.borrow(), global).map_err(|e| errs.push(e));
                }
            }

            _ => panic!("Unknown pass type")
        }

        if !errs.is_empty() {
            Err(errs)
        } else {
            Ok(())
        }
    }

    pub fn new(modules: Vec<Module>) -> Self {
        Self {
            modules: modules.into_iter().map(MModule::new).map(mutrc_new).collect()
        }
    }
}

/// This function resets MIR global state.
/// Called before starting a new compile.
fn reset_mir() {
    INTRINSICS.with(|i| i.borrow_mut().reset());
    IFACE_IMPLS.with(|i| i.borrow_mut().clear());
}
