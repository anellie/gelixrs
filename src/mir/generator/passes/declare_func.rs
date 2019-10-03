/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/3/19 5:13 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::FuncSignature;
use crate::ast::module::Module;
use crate::mir::generator::passes::NONE_CONST;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{MIRType, MIRVariable};
use crate::mir::ToMIRResult;

/// This pass defines all functions in MIR.
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
        .or_type_err(gen, &func_sig.return_type, "Unknown function return type")?;

    let mut parameters = Vec::with_capacity(func_sig.parameters.len());
    for param in func_sig.parameters.iter() {
        parameters.push(Rc::new(MIRVariable {
            mutable: false,
            name: Rc::clone(&param.name.lexeme),
            type_: gen.builder.find_type(&param.type_).or_err(
                gen,
                &param.name,
                "Function parameter has unknown type",
            )?,
        }));
    }

    let function = gen
        .builder
        .create_function(
            Rc::clone(&func_sig.name.lexeme),
            ret_type.clone(),
            parameters,
        )
        .or_err(gen, &func_sig.name, "Function was declared twice")?;

    let global = Rc::new(MIRVariable {
        name: Rc::clone(&func_sig.name.lexeme),
        type_: MIRType::Function(Rc::clone(&function)),
        mutable: false,
    });
    gen.builder
        .add_global(Rc::clone(&func_sig.name.lexeme), Rc::clone(&global));

    Ok(global)
}
