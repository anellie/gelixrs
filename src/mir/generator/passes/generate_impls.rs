/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{
    error::Errors,
    mir::{
        generator::{
            builder::MIRBuilder,
            passes::{ModulePass, PassType},
            MIRGenerator,
        },
        nodes::IFaceImpls,
        MModule, MutRc, IFACE_IMPLS,
    },
};

/// This pass generates all functions inside iface impls.
pub struct GenerateImpls();

impl ModulePass for GenerateImpls {
    fn get_type(&self) -> PassType {
        PassType::Globally
    }

    fn run_globally(&self, modules: &[MutRc<MModule>]) -> Result<(), Vec<Errors>> {
        // Actual module does not matter for now; will be
        // set as needed when generating impls
        let mut gen = MIRGenerator::new(MIRBuilder::new(&modules[0]));

        IFACE_IMPLS.with(|iface_impls| {
            let iface_impls = iface_impls.borrow();
            for (_, impls) in iface_impls.iter() {
                gen_impl_for_type(&mut gen, impls).map_err(|e| vec![e])?
            }
            Ok(())
        })
    }
}

pub fn gen_impl_for_type(gen: &mut MIRGenerator, impls: &MutRc<IFaceImpls>) -> Result<(), Errors> {
    let impls = impls.borrow();
    for im in impls.interfaces.values() {
        let ast = Rc::clone(&im.ast);
        gen.switch_module(&im.module);

        for (i, (_, method)) in im.methods.iter().enumerate() {
            gen.generate_function(&ast.methods[i], Some(method.type_.as_function()))
                .map_err(|e| Errors(vec![e], Rc::clone(&im.module.borrow().src)))?;
        }
    }

    Ok(())
}
