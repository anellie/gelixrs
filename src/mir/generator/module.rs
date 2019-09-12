/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/12/19, 9:11 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::module::Module;
use crate::mir::generator::passes::declare::{DeclareClassPass, DeclareFuncPass};
use crate::mir::generator::passes::fill_struct::FillStructPass;
use crate::mir::generator::passes::import::{ImportClassPass, ImportFuncPass};
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::generator::{MIRError, MIRGenerator, Res};
use crate::mir::MIRModule;
use std::rc::Rc;

/// A set of [MIRGenerator]s.
/// Takes a list of module ASTs and transforms them into
/// MIR modules.
pub struct MIRModuleGenerator {
    modules: Vec<(Module, MIRGenerator)>,
}

impl MIRModuleGenerator {
    pub fn execute(mut self) -> Result<Vec<MIRModule>, Vec<MIRError>> {
        self.run_for_all(Box::new(|(module, gen)| {
            DeclareClassPass::new(gen).run(module)
        }))?;
        ImportClassPass::new(&mut self.modules).run();
        self.run_for_all(Box::new(|(module, gen)| {
            DeclareFuncPass::new(gen).run(module)
        }))?;
        ImportFuncPass::new(&mut self.modules).run()?;
        self.run_for_all(Box::new(|(module, gen)| {
            FillStructPass::new(gen).run(module)
        }))?;

        self.modules
            .into_iter()
            .map(|(module, mut gen)| {
                gen.generate_mir(&module)?;
                Ok(gen.builder.consume_module())
            })
            .collect::<Result<Vec<MIRModule>, MIRError>>()
            .map_err(|e| vec![e])
    }

    fn run_for_all(
        &mut self,
        mut func: Box<dyn FnMut(&mut (Module, MIRGenerator)) -> Res<()>>,
    ) -> Result<(), Vec<MIRError>> {
        let mut errors = Vec::new();
        for module in self.modules.iter_mut() {
            let result = func(module);
            if let Err(err) = result {
                errors.push(err)
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn new(modules: Vec<Module>) -> Self {
        let modules = modules.into_iter().map(|module| {
            let path_clone = Rc::clone(&module.path);
            (module, MIRGenerator::new(path_clone))
        });

        Self {
            modules: modules.collect(),
        }
    }
}
