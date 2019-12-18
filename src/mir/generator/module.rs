/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/18/19 11:08 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::module::Module;
use crate::error::{Error, Errors};
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::builder::MIRBuilder;
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::generator::MIRGenerator;
use crate::mir::generator::passes::{ModulePass, PassType, PreMIRPass};
use crate::mir::generator::passes::declaring_globals::DeclareGlobals;
use crate::mir::generator::passes::declaring_iface_impls::DeclareIfaceImpls;
use crate::mir::generator::passes::declaring_methods::DeclareMethods;
use crate::mir::generator::passes::declaring_types::DeclareTypes;
use crate::mir::generator::passes::fill_impls::FillIfaceImpls;
use crate::mir::generator::passes::filter_prototypes::FilterPrototypes;
use crate::mir::generator::passes::generate::Generate;
use crate::mir::generator::passes::generate_impls::GenerateImpls;
use crate::mir::generator::passes::imports::{ImportGlobals, ImportTypes};
use crate::mir::generator::passes::insert_members::InsertClassMembers;
use crate::mir::generator::passes::populate_intrinsics::PopulateIntrinsics;
use crate::mir::generator::passes::validate::ValidateIntrinsics;
use crate::mir::IFACE_IMPLS;
use crate::mir::nodes::{Type, Variable};

thread_local! {
    /// A map containing passes that have run, and the currently running pass.
    /// This is global state since it is shared across modules.
    /// TODO: This would be better implemented as a lazy_static,
    /// but the compiler does not currently support multithreading.
    pub static DONE_PASSES: RefCell<Vec<Box<dyn ModulePass>>> = RefCell::new(Vec::new());
}

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
            Box::new(ImportTypes()),
            Box::new(DeclareGlobals()),
            Box::new(ImportGlobals()),
            Box::new(DeclareIfaceImpls()),
        ];

        for mut pass in passes.drain(..) {
            let mut errs = Vec::new();
            for (ast, module) in modules.iter_mut().zip(self.modules.iter()) {
                pass.run(ast, Rc::clone(&module), &self.modules)
                    .map_err(|e| errs.push(e))
                    .ok();
            }
            if !errs.is_empty() {
                return Err(errs);
            }
        }

        let mut passes: Vec<Box<dyn ModulePass>> = vec![
            Box::new(DeclareMethods()),
            Box::new(FillIfaceImpls(RefCell::new(true))),
            Box::new(InsertClassMembers()),
            Box::new(PopulateIntrinsics()),
            Box::new(Generate()),
            Box::new(GenerateImpls()),
            Box::new(ValidateIntrinsics()),
        ];
        let mut generator = MIRGenerator::new(MIRBuilder::new(&self.modules[0]));
        for pass in passes.drain(..) {
            // The pass needs to be put into DONE_PASSES before running.
            DONE_PASSES.with(|d| d.borrow_mut().push(pass));
            DONE_PASSES.with(|d| self.run_pass(&**d.borrow().last().unwrap(), &mut generator))?;
        }

        Ok(self.modules)
    }

    pub fn run_pass(
        &self,
        pass: &dyn ModulePass,
        gen: &mut MIRGenerator,
    ) -> Result<(), Vec<Errors>> {
        match DONE_PASSES.with(|d| d.borrow().last().unwrap().get_type()) {
            PassType::Globally => {
                pass.run_globally(&self.modules)?;
            }

            _ => {
                let mut errs = Vec::new();

                for module in self.modules.iter() {
                    gen.switch_module(module);
                    self.run_module_pass(gen, pass)
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
        gen: &mut MIRGenerator,
        pass: &dyn ModulePass,
    ) -> Result<(), Vec<Error>> {
        let mut errs = Vec::new();

        match pass.get_type() {
            PassType::Module => {
                pass.run_mod(gen).map_err(|mut e| errs.append(&mut e)).ok();
            }

            PassType::Type => {
                let types_iter = gen
                    .module
                    .borrow()
                    .types
                    .values()
                    .cloned()
                    .collect::<Vec<Type>>();
                for ty in types_iter {
                    ty.context().map(|c| gen.builder.context = c);
                    pass.run_type(gen, ty).map_err(|e| errs.push(e)).ok();
                }
            }

            PassType::GlobalVar => {
                let globals_iter = gen
                    .module
                    .borrow()
                    .globals
                    .values()
                    .cloned()
                    .collect::<Vec<Rc<Variable>>>();
                for global in globals_iter {
                    pass.run_global_var(gen, global)
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
            modules: modules.iter().map(MModule::new).map(mutrc_new).collect(),
        }
    }
}

/// This function resets MIR global state.
/// Called before starting a new compile.
fn reset_mir() {
    INTRINSICS.with(|i| i.borrow_mut().reset());
    IFACE_IMPLS.with(|i| i.borrow_mut().clear());
    DONE_PASSES.with(|d| d.borrow_mut().clear());
}
