/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/16/19 4:30 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast;
use crate::ast::Module;
use crate::error::{Error, Errors, Res};
use crate::mir::generator::builder::MIRBuilder;
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::nodes::{IFaceImpls, Type};
use crate::mir::{mutrc_new, MModule, MutRc, IFACE_IMPLS};

/// This pass inserts all iface impls in the global impl
/// table. It only validates that the type implementing for
/// exists, no other checks are performed.
pub struct DeclareIfaceImpls();

impl PreMIRPass for DeclareIfaceImpls {
    fn run(
        &mut self,
        ast: &mut Module,
        module: MutRc<MModule>,
        _modules: &[MutRc<MModule>],
    ) -> Result<(), Errors> {
        let mut errs = Vec::new();
        let mut builder = MIRBuilder::new(&module);

        for im in ast.iface_impls.drain(..) {
            declare_impl(im, &mut builder)
                .map_err(|e| errs.push(e))
                .ok();
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(Errors(errs, Rc::clone(&module.borrow().src)))
        }
    }
}

fn declare_impl(iface_impl: ast::IFaceImpl, builder: &mut MIRBuilder) -> Res<()> {
    if let ast::Type::Generic { token, .. } = &iface_impl.implementor {
        return Err(Error::new(
            &token,
            "MIR",
            "Generic arguments on implementor not supported yet.".to_string(),
            &builder.path,
        ));
    }

    let implementor = builder.find_type(&iface_impl.implementor)?;
    let impls = get_or_create_iface_impls(&implementor);
    impls.borrow_mut().ast.push(iface_impl);

    Ok(())
}

/// Gets the interfaces implemented by a type.
fn get_or_create_iface_impls(ty: &Type) -> MutRc<IFaceImpls> {
    match IFACE_IMPLS.with(|impls| impls.borrow().get(ty).cloned()) {
        Some(impls) => impls,
        None => IFACE_IMPLS.with(|impls| {
            let iface_impls = mutrc_new(IFaceImpls {
                implementor: ty.clone(),
                interfaces: HashSet::with_capacity(2),
                methods: HashMap::with_capacity(2),
                ast: Vec::with_capacity(2),
            });
            impls
                .borrow_mut()
                .insert(ty.clone(), Rc::clone(&iface_impls));
            iface_impls
        }),
    }
}
