/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/2/19 4:44 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{ASTType, FunctionArg, IFaceImpl};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::passes::THIS_CONST;

/// This pass checks and defines all interface impl blocks.
pub fn iface_impl_pass(gen: &mut MIRGenerator, list: &mut Module) -> Res<()> {
    for iface in list.iface_impls.iter_mut() {
        iface_impl(gen, iface)?;
    }
    Ok(())
}

fn iface_impl(gen: &mut MIRGenerator, iface_impl: &mut IFaceImpl) -> Res<()> {
    gen.builder.add_alias(&THIS_CONST.with(|c| c.clone()), &ASTType::Token(iface_impl.class.clone()));
    let class = gen
        .builder
        .find_class(&iface_impl.class.lexeme)
        .ok_or_else(|| gen.error(&iface_impl.class, &iface_impl.class, "Unknown class."))?;
    let mut class = class.borrow_mut();

    let iface_cell = gen
        .builder
        .find_interface(&iface_impl.iface.lexeme)
        .ok_or_else(|| gen.error(&iface_impl.iface, &iface_impl.iface, "Unknown interface."))?;
    let iface = iface_cell.borrow();

    for method in iface_impl.methods.iter_mut() {
        if !iface.methods.contains_key(&method.sig.name.lexeme) {
            return Err(gen.error(
                &method.sig.name,
                &method.sig.name,
                "Method is not defined in interface.",
            ))?;
        }
        let this_arg = FunctionArg::this_arg(&iface_impl.class);

        let old_name = Rc::clone(&method.sig.name.lexeme);
        method.sig.name.lexeme = Rc::new(format!(
            "{}-{}",
            iface_impl.class.lexeme, method.sig.name.lexeme
        ));
        method.sig.parameters.insert(0, this_arg);

        let mir_method = create_function(gen, &method.sig)?;
        class.methods.insert(old_name, mir_method);
    }

    if iface.methods.len() > iface_impl.methods.len() {
        Err(gen.error(
            &iface_impl.iface,
            &iface_impl.iface,
            "Missing methods in interface impl.",
        ))
    } else {
        class.interfaces.push(Rc::clone(&iface_cell));
        gen.builder.remove_alias(&THIS_CONST.with(|c| c.clone()));
        Ok(())
    }
}
