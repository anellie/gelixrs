/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/16/19 9:25 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::error::Errors;
use crate::mir::{IFACE_IMPLS, MModule, MutRc};
use crate::mir::generator::builder::MIRBuilder;
use crate::mir::generator::MIRGenerator;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::nodes::IFaceImpls;

/// This pass populates the intrinsics struct.
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
    for im in impls.interfaces.iter() {
        let ast = Rc::clone(&im.ast);
        gen.switch_module(&im.module);

        for (i, (_, method)) in im.methods.iter().enumerate() {
            gen.generate_function(&ast.methods[i], Some(method.type_.as_function()))
                .map_err(|e| Errors(vec![e], Rc::clone(&im.module.borrow().src)))?;
        }
    }

    Ok(())
}
