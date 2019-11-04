/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 9:32 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::declaration::{Interface as ASTIFace, Type as ASTType};
use crate::ast::module::Module;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::NONE_CONST;
use crate::mir::nodes::{IFaceMethod, Interface, InterfacePrototype, Type};
use crate::mir::ToMIRResult;

/// This pass declares all interfaces.
pub fn declare_interface_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    for interface in module.interfaces.iter_mut() {
        create_interface(gen, interface)?
    }

    Ok(())
}

fn create_interface(gen: &mut MIRGenerator, interface: &mut ASTIFace) -> Res<()> {
    gen.builder.try_reserve_name(&interface.name)?;
    gen.builder.add_this_alias(&interface.name);
    interface.generics.as_ref().map(|g| gen.builder.set_generic_types(&g));

    let mut methods = IndexMap::with_capacity(interface.methods.len());
    for method in interface.methods.iter_mut() {
        let ast_ret_type = method.sig.return_type.as_ref();
        let ret_type = gen
            .builder
            .find_type(ast_ret_type.unwrap_or(&NONE_CONST.with(|c| c.clone())))
            .or_type_err(gen, &method.sig.return_type, "Unknown return type")?;

        let mut parameters = Vec::with_capacity(method.sig.parameters.len());
        for param in method.sig.parameters.iter() {
            let ty = gen
                .builder
                .find_type(&param.type_)
                .or_err(gen, &param.name, "Unknown parameter type")?;
            parameters.push(ty);
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

    if let Some(generics) = &interface.generics {
        gen.builder.set_generic_types(generics);

        let mut interface = InterfacePrototype {
            name: Rc::clone(&interface.name.lexeme),
            methods,
            generic_args: gen.builder.generic_types.iter().cloned().collect(),
            ..Default::default()
        };
    } else {
        let mut interface = Interface {
            name: Rc::clone(&interface.name.lexeme),
            methods,
        };
    }

    gen.builder.remove_this_alias();
    gen.builder.generic_types.clear();
    Ok(())
}