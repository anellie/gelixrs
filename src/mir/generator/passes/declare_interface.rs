/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/3/19 3:08 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{ASTType, FunctionArg, Interface};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRError, MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::passes::NONE_CONST;
use crate::mir::generator::passes::THIS_CONST;
use crate::mir::nodes::{MIRIFaceMethod, MIRType};

/// This pass declares all interfaces.
pub fn declare_interface_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    for interface in module.interfaces.iter_mut() {
        create_interface(gen, interface)?
    }

    Ok(())
}

fn create_interface(gen: &mut MIRGenerator, interface: &mut Interface) -> Res<()> {
    let mir_iface = gen
        .builder
        .create_interface(&interface.name.lexeme)
        .ok_or(gen.error(
            &interface.name,
            &interface.name,
            "Interface with the same name already defined.",
        ))?;
    let mut mir_iface = mir_iface.borrow_mut();

    gen.builder.add_alias(&THIS_CONST.with(|c| c.clone()), &ASTType::Token(interface.name.clone()));
    for generic in interface.generics.iter() {
        mir_iface.generics.push(Rc::clone(&generic.lexeme))
    }

    for method in interface.methods.iter_mut() {
        let ret_type = gen
            .builder
            .find_type(
                method
                    .sig
                    .return_type
                    .as_ref()
                    .unwrap_or(&NONE_CONST.with(|c| c.clone())),
            );
        let ret_type = check_for_generic_type(gen, &mir_iface.generics, method.sig.return_type.as_ref(), ret_type)?;

        let mut parameters = Vec::with_capacity(method.sig.parameters.len());
        for param in method.sig.parameters.iter() {
            let ty = check_for_generic_type(gen, &mir_iface.generics, Some(&param.type_), gen.builder.find_type(&param.type_))?;
            parameters.push(ty);
        }

        mir_iface.methods.insert(Rc::clone(&method.sig.name.lexeme), MIRIFaceMethod {
            name: Rc::clone(&method.sig.name.lexeme),
            parameters,
            ret_type,
            default_impl: method.body.take(),
        });
    }

    gen.builder.remove_alias(&THIS_CONST.with(|c| c.clone()));
    Ok(())
}

/// Takes an option of a MIRType as well as the ASTType it came from.
/// If the MIRType is none, it checks if it is a generic type; an error is raised if it isn't.
fn check_for_generic_type(
    gen: &mut MIRGenerator,
    generics: &Vec<Rc<String>>,
    ty: Option<&ASTType>,
    current: Option<MIRType>,
) -> Res<MIRType> {
    match current {
        Some(ty) => Ok(ty),
        None => {
            if let ASTType::Token(tok) = ty.unwrap() {
                if generics.contains(&tok.lexeme) {
                    return Ok(MIRType::Generic(Rc::clone(&tok.lexeme)))
                }
            }
            let tok = ty.unwrap().get_token();
            Err(MIRGenerator::anon_err(gen, tok, "Unknown type"))
        }
    }
}