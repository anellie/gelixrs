/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/13/19 10:15 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;




use crate::ast::declaration::FuncSignature;
use crate::ast::module::Module;

use crate::mir::{mutrc_new, ToMIRResult};
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::nodes::{Function, FunctionPrototype, Type, Variable};

/// This pass defines all functions in MIR.
pub fn declare_func_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    // Remove all functions that contain generics from the list
    // so the generator won't bother trying to compile it later.
    for func in module.functions.drain_filter(|f| f.sig.generics.is_some()) {
        gen.builder.prototypes.functions.insert(Rc::clone(&func.sig.name.lexeme), mutrc_new(FunctionPrototype {
            ast: func,
            impls: vec![],
            instances: Default::default()
        }));
    }

    for func in module.functions.iter_mut() {
        create_function(gen, &func.sig, func.body.is_none())?;
    }

    Ok(())
}

pub(super) fn create_function(
    gen: &mut MIRGenerator,
    func_sig: &FuncSignature,
    is_external: bool
) -> Res<Rc<Variable>> {
    gen.builder.try_reserve_name(&func_sig.name)?;

    let name = if is_external {
        String::clone(&func_sig.name.lexeme)
    } else {
        gen.builder.get_function_name(&func_sig.name.lexeme)
    };
    let ret_type = func_sig
        .return_type
        .as_ref()
        .map(|ty| gen.find_type(ty))
        .unwrap_or(Ok(Type::None))?;

    let mut parameters = Vec::with_capacity(func_sig.parameters.len());
    for param in func_sig.parameters.iter() {
        parameters.push(Rc::new(Variable {
            mutable: false,
            name: Rc::clone(&param.name.lexeme),
            type_: gen.find_type(&param.type_)?,
        }));
    }

    let function = mutrc_new(Function {
        name,
        parameters,
        ret_type,
        ..Default::default()
    });

    let global = Rc::new(Variable {
        name: Rc::clone(&func_sig.name.lexeme),
        type_: Type::Function(Rc::clone(&function)),
        mutable: false,
    });

    if &func_sig.name.lexeme[..] == "main" {
        INTRINSICS
            .with(|i| i.borrow_mut().set_main_fn(&global))
            .or_err(&gen, &func_sig.name, "Can't define main multiple times.")?;
    }

    gen.builder
        .module
        .functions
        .insert(Rc::clone(&func_sig.name.lexeme), Rc::clone(&global));
    Ok(global)
}
