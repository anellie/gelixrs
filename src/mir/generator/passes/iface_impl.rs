/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/3/19 6:25 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{ASTType, FuncSignature, Function, FunctionArg, IFaceImpl, Variable};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{MIRFunction, MIRIFaceMethod, MIRType, MIRVariable};
use crate::mir::ToMIRResult;

/// This pass checks and defines all interface impl blocks.
pub fn iface_impl_pass(gen: &mut MIRGenerator, list: &mut Module) -> Res<()> {
    for iface in list.iface_impls.iter_mut() {
        iface_impl(gen, iface)?;
    }
    Ok(())
}

fn iface_impl(gen: &mut MIRGenerator, iface_impl: &mut IFaceImpl) -> Res<()> {
    gen.builder.add_this_alias(&iface_impl.class);
    let class = gen.builder.find_class(&iface_impl.class.lexeme).or_err(
        gen,
        &iface_impl.class,
        "Unknown class.",
    )?;
    let mut class = class.borrow_mut();

    let iface_cell = gen
        .builder
        .find_interface(&iface_impl.iface.lexeme)
        .or_err(gen, &iface_impl.iface, "Unknown interface.")?;
    let iface = iface_cell.borrow();

    add_generic_aliases(
        gen,
        &iface.generics,
        &iface_impl.iface_generics,
        &iface_impl.iface,
    )?;

    for method in iface_impl.methods.iter_mut() {
        let iface_method = iface.methods.get(&method.sig.name.lexeme).or_err(
            gen,
            &method.sig.name,
            "Method is not defined in interface.",
        )?;
        let this_arg = FunctionArg::this_arg(&iface_impl.class);

        let old_name = Rc::clone(&method.sig.name.lexeme);
        method.sig.name.lexeme = Rc::new(format!(
            "{}-{}",
            iface_impl.class.lexeme, method.sig.name.lexeme
        ));
        method.sig.parameters.insert(0, this_arg);

        let mir_method = create_function(gen, &method.sig)?;
        class.methods.insert(old_name, Rc::clone(&mir_method));

        check_equal_signature(gen, method, mir_method, iface_method)?;
    }

    if iface.methods.len() > iface_impl.methods.len() {
        Err(gen.error(
            &iface_impl.iface,
            &iface_impl.iface,
            "Missing methods in interface impl.",
        ))
    } else {
        class.interfaces.push(Rc::clone(&iface_cell));

        gen.builder.remove_this_alias();
        for g_iface in iface.generics.iter() {
            gen.builder.remove_alias(g_iface);
        }

        Ok(())
    }
}

/// Registers all generic parameter aliases; also does checking to ensure they are valid.
fn add_generic_aliases(
    gen: &mut MIRGenerator,
    iface: &Vec<Rc<String>>,
    impl_: &Vec<Token>,
    iface_tok: &Token,
) -> Res<()> {
    if iface.len() != impl_.len() {
        return Err(gen.error(
            iface_tok,
            iface_tok,
            &format!(
                "Wrong amount of interface generic parameters (expected {}; got {})",
                iface.len(),
                impl_.len()
            ),
        ));
    }
    for (g_impl, g_iface) in impl_.iter().zip(iface.iter()) {
        gen.builder
            .add_alias(g_iface, &ASTType::Token(g_impl.clone()));
    }
    Ok(())
}

/// Ensures that the implemented interface method matches the expected signature.
fn check_equal_signature(
    gen: &mut MIRGenerator,
    method: &Function,
    mir_method: Rc<MIRVariable>,
    iface_method: &MIRIFaceMethod,
) -> Res<()> {
    let mir_method = if let MIRType::Function(method) = &mir_method.type_ {
        method
    } else {
        panic!()
    };
    let mir_method = mir_method.borrow();

    if mir_method.ret_type != gen.builder.translate_generic(&iface_method.ret_type) {
        let tok = method
            .sig
            .return_type
            .as_ref()
            .map(|t| t.get_token())
            .flatten();
        return Err(MIRGenerator::anon_err(
            gen,
            tok,
            "Incorrect return type on interface method.",
        ));
    }

    for (i, (method_param, iface_param)) in mir_method
        .parameters
        .iter()
        .zip(iface_method.parameters.iter())
        .enumerate()
    {
        if method_param.type_ != gen.builder.translate_generic(iface_param) {
            let tok = &method.sig.parameters[i].name;
            return Err(MIRGenerator::error(
                gen,
                tok,
                tok,
                "Incorrect parameter type on interface method.",
            ));
        }
    }

    Ok(())
}
