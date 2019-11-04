/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 8:03 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use either::Either;
use either::Either::{Left, Right};

use crate::ast::declaration::FuncSignature;
use crate::ast::module::Module;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{Function, FunctionPrototype, Type, Variable};
use crate::mir::{mutrc_new, MutRc, ToMIRResult};

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
) -> Res<Either<Rc<Variable>, MutRc<FunctionPrototype>>> {
    gen.builder.try_reserve_name(&func_sig.name)?;
    func_sig
        .generics
        .as_ref()
        .map(|g| gen.builder.set_generic_types(&g));

    let name = gen.builder.get_function_name(&func_sig.name.lexeme);
    let ret_type = func_sig
        .return_type
        .as_ref()
        .map(|ty| {
            gen.builder.find_type(ty).or_type_err(
                gen,
                &func_sig.return_type,
                "Unknown function return type",
            )
        })
        .unwrap_or(Ok(Type::None))?;

    let mut parameters = Vec::with_capacity(func_sig.parameters.len());
    for param in func_sig.parameters.iter() {
        parameters.push(Rc::new(Variable {
            mutable: false,
            name: Rc::clone(&param.name.lexeme),
            type_: gen.builder.find_type(&param.type_).or_err(
                gen,
                &param.name,
                "Function parameter has unknown type",
            )?,
        }));
    }

    if func_sig.generics.is_some() {
        let function = mutrc_new(FunctionPrototype {
            name,
            parameters,
            generic_args: gen.builder.generic_types.iter().cloned().collect(),
            ret_type,
            ..Default::default()
        });

        gen.builder
            .prototypes
            .functions
            .insert(Rc::clone(&func_sig.name.lexeme), Rc::clone(&function));
        Ok(Right(function))
    } else {
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

        gen.builder
            .module
            .functions
            .insert(Rc::clone(&func_sig.name.lexeme), Rc::clone(&global));
        Ok(Left(global))
    }
}
