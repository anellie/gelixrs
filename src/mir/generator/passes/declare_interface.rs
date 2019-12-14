/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/14/19 5:40 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::declaration::Interface as ASTIFace;
use crate::ast::module::Module;
use crate::mir::{MutRc, mutrc_new};
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::NONE_CONST;
use crate::mir::nodes::{IFaceMethod, Interface, InterfacePrototype};

/// This pass declares all interfaces.
pub fn declare_interface_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    // Remove all interfaces that contain generics from the list
    // so the generator won't bother trying to compile it later.
    for interface in module.interfaces.drain_filter(|f| f.generics.is_some()) {
        gen.builder.prototypes.interfaces.insert(
            Rc::clone(&interface.name.lexeme),
            mutrc_new(InterfacePrototype {
                ast: interface,
                impls: vec![],
                instances: Default::default(),
            }),
        );
    }

    for interface in module.interfaces.iter_mut() {
        create_interface(gen, interface)?;
    }

    Ok(())
}

pub fn create_interface(gen: &mut MIRGenerator, interface: &mut ASTIFace) -> Res<MutRc<Interface>> {
    gen.builder.try_reserve_name(&interface.name)?;

    let mut methods = IndexMap::with_capacity(interface.methods.len());
    for method in interface.methods.iter_mut() {
        let ast_ret_type = method.sig.return_type.as_ref();
        let ret_type = gen.find_type(ast_ret_type.unwrap_or(&NONE_CONST.with(|c| c.clone())))?;

        let mut parameters = Vec::with_capacity(method.sig.parameters.len());
        for param in method.sig.parameters.iter() {
            parameters.push(gen.find_type(&param.type_)?);
        }

        methods.insert(
            Rc::clone(&method.sig.name.lexeme),
            IFaceMethod {
                name: Rc::clone(&method.sig.name.lexeme),
                parameters,
                ret_type,
                default_impl: method.body.take(),
            },
        );
    }

    let mir_iface = mutrc_new(Interface {
        name: Rc::clone(&interface.name.lexeme),
        methods,
        proto: None,
    });
    gen.builder
        .module
        .interfaces
        .insert(Rc::clone(&interface.name.lexeme), Rc::clone(&mir_iface));

    Ok(mir_iface)
}
