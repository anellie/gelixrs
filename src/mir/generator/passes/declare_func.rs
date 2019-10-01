/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/30/19 2:09 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::FuncSignature;
use crate::ast::module::Module;
use crate::mir::generator::passes::NONE_CONST;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{MIRType, MIRVariable};

/// This pass generates all functions.
pub fn declare_func_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    for function in module
        .ext_functions
        .iter()
        .chain(module.functions.iter().map(|f| &f.sig))
    {
        create_function(gen, &function)?;
    }

    Ok(())
}

pub(super) fn create_function(
    gen: &mut MIRGenerator,
    func_sig: &FuncSignature,
) -> Res<Rc<MIRVariable>> {
    let ret_type = gen
        .builder
        .find_type(
            func_sig
                .return_type
                .as_ref()
                .unwrap_or(&NONE_CONST.with(|c| c.clone())),
        )
        .ok_or_else(|| {
            let tok = func_sig.return_type.as_ref().unwrap().get_token();
            MIRGenerator::anon_err(gen, tok, "Unknown function return type")
        })?;

    let mut parameters = Vec::with_capacity(func_sig.parameters.len());
    for param in func_sig.parameters.iter() {
        parameters.push(Rc::new(MIRVariable {
            mutable: false,
            name: Rc::clone(&param.name.lexeme),
            _type: gen.builder.find_type(&param._type).ok_or_else(|| {
                MIRGenerator::error(
                    gen,
                    &func_sig.name,
                    &func_sig.name,
                    "Function parameter has unknown type",
                )
            })?,
        }))
    }

    let function = gen
        .builder
        .create_function(
            Rc::clone(&func_sig.name.lexeme),
            ret_type.clone(),
            parameters,
        )
        .ok_or_else(|| {
            MIRGenerator::error(
                gen,
                &func_sig.name,
                &func_sig.name,
                "Function was declared twice",
            )
        })?;

    let global = Rc::new(MIRVariable::new(
        Rc::clone(&func_sig.name.lexeme),
        MIRType::Function(Rc::clone(&function)),
        false,
    ));
    gen.builder
        .add_global(Rc::clone(&func_sig.name.lexeme), Rc::clone(&global));

    Ok(global)
}
