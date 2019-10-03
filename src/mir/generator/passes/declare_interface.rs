/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/3/19 5:40 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{ASTType, FunctionArg, Interface};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRError, MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::passes::NONE_CONST;
use crate::mir::nodes::{MIRIFaceMethod, MIRType};
use crate::mir::ToMIRResult;

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
        .or_err(gen, &interface.name, "Interface with the same name already defined.")?;
    let mut mir_iface = mir_iface.borrow_mut();

    gen.builder.add_this_alias(&interface.name);
    mir_iface.generics = interface.generics.iter().map(|t| t.lexeme.clone()).collect();

    for method in interface.methods.iter_mut() {
        let ast_ret_type = method.sig.return_type.as_ref();
        let ret_type = gen
            .builder
            .find_type(ast_ret_type.unwrap_or(&NONE_CONST.with(|c| c.clone())))
            .or_else(|| try_resolve_generic_type(&mir_iface.generics, ast_ret_type))
            .or_type_err(gen, &method.sig.return_type, "Unknown return type")?;

        let mut parameters = Vec::with_capacity(method.sig.parameters.len());
        for param in method.sig.parameters.iter() {
            let ty = gen
                .builder
                .find_type(&param.type_)
                .or_else(|| try_resolve_generic_type(&mir_iface.generics, Some(&param.type_)))
                .or_err(gen, &param.name, "Unknown parameter type")?;
            parameters.push(ty);
        }

        mir_iface.methods.insert(Rc::clone(&method.sig.name.lexeme), MIRIFaceMethod {
            name: Rc::clone(&method.sig.name.lexeme),
            parameters,
            ret_type,
            default_impl: method.body.take(),
        });
    }

    gen.builder.remove_this_alias();
    Ok(())
}

/// Tries resolving an AST type to a generic.
fn try_resolve_generic_type(
    generics: &Vec<Rc<String>>,
    ty: Option<&ASTType>,
) -> Option<MIRType> {
    if let ASTType::Token(tok) = ty.unwrap() {
        if generics.contains(&tok.lexeme) {
            return Some(MIRType::Generic(Rc::clone(&tok.lexeme)))
        }
    }
    None
}