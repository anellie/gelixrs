/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 10:02 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::module::Module;
use crate::error::{Error, Errors};
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::generator::passes::{ModulePass, PassType, PreMIRPass};
use crate::mir::generator::passes::declaring_globals::DeclareGlobals;
use crate::mir::generator::passes::declaring_types::DeclareTypes;
use crate::mir::generator::passes::filter_prototypes::FilterPrototypes;
use crate::mir::generator::passes::generate::Generate;
use crate::mir::generator::passes::populate_intrinsics::PopulateIntrinsics;
use crate::mir::generator::passes::validate::ValidateIntrinsics;
use crate::mir::IFACE_IMPLS;
use crate::mir::nodes::{Type, Variable};

/// Responsible for collecting all passes that run on the MIR.
/// MIR is built purely by running many transformation passes,
/// it is considered compiled once the last pass
/// has been run.
pub struct PassRunner {
    /// All the modules in this compilation run.
    modules: Vec<MutRc<MModule>>,
}

impl PassRunner {
    pub fn execute(self, mut modules: Vec<Module>) -> Result<Vec<MutRc<MModule>>, Vec<Errors>> {
        reset_mir();
        let mut passes: Vec<Box<dyn PreMIRPass>> = vec![
            Box::new(FilterPrototypes()),
            Box::new(DeclareTypes()),
            Box::new(DeclareGlobals()),
        ];

        for mut pass in passes.drain(..) {
            let mut errs = Vec::new();
            for (ast, module) in modules.iter_mut().zip(self.modules.iter()) {
                pass.run(ast, Rc::clone(&module)).map_err(|e| errs.push(e)).ok();
            }
            if !errs.is_empty() {
                return Err(errs);
            }
        }

        let mut passes: Vec<Box<dyn ModulePass>> = vec![
            Box::new(PopulateIntrinsics()),
            Box::new(Generate()),
            Box::new(ValidateIntrinsics()),
        ];

        for mut pass in passes.drain(..) {
            self.run_pass(&mut *pass)?;
        }

        Ok(self.modules)
    }

    pub fn run_pass(&self, pass: &mut dyn ModulePass) -> Result<(), Vec<Errors>> {
        match pass.get_type() {
            PassType::GlobalInspect => {
                pass.run_inspect(&self.modules)
                    .map_err(|e| vec![Errors(vec![e], Rc::new("".to_string()))])?;
            }

            _ => {
                let mut errs = Vec::new();

                for module in self.modules.iter() {
                    self.run_module_pass(module, pass)
                        .map_err(|e| errs.push(Errors(e, Rc::clone(&module.borrow().src))))
                        .ok();
                }

                if !errs.is_empty() {
                    return Err(errs);
                }
            }
        }
        Ok(())
    }

    pub fn run_module_pass(
        &self,
        module: &MutRc<MModule>,
        pass: &mut dyn ModulePass,
    ) -> Result<(), Vec<Error>> {
        let mut errs = Vec::new();

        match pass.get_type() {
            PassType::Module => {
                pass.run_mod(Rc::clone(module))
                    .map_err(|mut e| errs.append(&mut e))
                    .ok();
            }

            PassType::Type => {
                let types_iter = module.borrow().types.values().cloned().collect::<Vec<Type>>();
                for ty in types_iter {
                    pass.run_type(module, ty)
                        .map_err(|e| errs.push(e))
                        .ok();
                }
            }

            PassType::Global => {
                let globals_iter = module.borrow().globals.values().cloned().collect::<Vec<Rc<Variable>>>();
                for global in globals_iter {
                    pass.run_global(module, global)
                        .map_err(|e| errs.push(e))
                        .ok();
                }
            }

            _ => panic!("Unknown pass type"),
        }

        if !errs.is_empty() {
            Err(errs)
        } else {
            Ok(())
        }
    }

    pub fn new(modules: &[Module]) -> Self {
        Self {
            modules: modules
                .iter()
                .map(MModule::new)
                .map(mutrc_new)
                .collect(),
        }
    }
}

/// This function resets MIR global state.
/// Called before starting a new compile.
fn reset_mir() {
    INTRINSICS.with(|i| i.borrow_mut().reset());
    IFACE_IMPLS.with(|i| i.borrow_mut().clear());
}
