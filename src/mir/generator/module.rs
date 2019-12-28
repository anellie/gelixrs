/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/28/19 10:20 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{cell::RefCell, rc::Rc};

use crate::{
    ast::module::Module,
    error::Errors,
    mir::{
        generator::{
            builder::MIRBuilder,
            intrinsics::INTRINSICS,
            passes::{
                declaring_globals::DeclareGlobals,
                declaring_iface_impls::DeclareIfaceImpls,
                declaring_methods::DeclareMethods,
                declaring_types::DeclareTypes,
                fill_impls::FillIfaceImpls,
                filter_prototypes::FilterPrototypes,
                gc_variable_escape::GCMarkEscapeVariables,
                generate::Generate,
                generate_impls::GenerateImpls,
                imports::{ImportGlobals, ImportTypes},
                insert_members::InsertClassMembers,
                populate_intrinsics::PopulateIntrinsics,
                validate::ValidateIntrinsics,
                ModulePass, PassType, PreMIRPass,
            },
            MIRGenerator,
        },
        mutrc_new, MModule, MutRc, IFACE_IMPLS,
    },
};

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
            Box::new(PopulateIntrinsics()),
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
            Box::new(Generate()),
            Box::new(GenerateImpls()),
            Box::new(ValidateIntrinsics()),
            Box::new(GCMarkEscapeVariables()),
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
        let mut errors = Vec::new();

        match DONE_PASSES.with(|d| d.borrow().last().unwrap().get_type()) {
            PassType::Globally => {
                pass.run_globally(&self.modules)?;
            }

            PassType::Module => {
                for module in self.modules.iter() {
                    gen.switch_module(module);
                    pass.run_mod(gen)
                        .map_err(|e| errors.push(Errors(e, Rc::clone(&module.borrow().src))))
                        .ok();
                }
            }

            PassType::Type => {
                let types = self
                    .modules
                    .iter()
                    .map(|module| {
                        (
                            module.borrow().types.values().cloned().collect::<Vec<_>>(),
                            Rc::clone(module),
                        )
                    })
                    .collect::<Vec<_>>();

                for (types, module) in types {
                    let mut errs = Vec::new();

                    gen.switch_module(&module);
                    for ty in types {
                        ty.context().map(|c| gen.builder.context = c);
                        pass.run_type(gen, ty).map_err(|e| errs.push(e)).ok();
                    }

                    if !errs.is_empty() {
                        errors.push(Errors(errs, Rc::clone(&module.borrow().src)));
                    }
                }
            }

            PassType::GlobalVar => {
                let globals = self
                    .modules
                    .iter()
                    .map(|module| {
                        (
                            module
                                .borrow()
                                .globals
                                .values()
                                .cloned()
                                .collect::<Vec<_>>(),
                            Rc::clone(module),
                        )
                    })
                    .collect::<Vec<_>>();

                for (globals, module) in globals {
                    let mut errs = Vec::new();

                    gen.switch_module(&module);
                    for global in globals {
                        pass.run_global_var(gen, global)
                            .map_err(|e| errs.push(e))
                            .ok();
                    }

                    if !errs.is_empty() {
                        errors.push(Errors(errs, Rc::clone(&module.borrow().src)));
                    }
                }
            }
        }

        if !errors.is_empty() {
            Err(errors)
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
