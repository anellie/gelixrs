/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 4:39 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::FuncSignature;
use crate::ast::module::ModulePath;
use crate::error::{Error, Res};
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::builder::MIRBuilder;
use crate::mir::generator::intrinsics::INTRINSICS;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::nodes::{Function, Type, Variable};
use crate::mir::result::ToMIRResult;

/// This pass defines all globals inside the module; currently only functions.
/// It only creates a signature and inserts it into the module;
/// no actual body/code is generated at this stage.
/// Since it requires types for its signature, this pass has to run
/// after [DeclareTypes].
pub struct DeclareGlobals();

impl ModulePass for DeclareGlobals {
    fn get_type(&self) -> PassType {
        PassType::Module
    }

    fn run_mod(&mut self, module: MutRc<MModule>) -> Result<(), Vec<Error>> {
        let mut errs = Vec::new();
        let builder = MIRBuilder::new(&module);

        // TODO: This is a sin
        // The borrow checker can be annoying...
        let len = module.borrow().ast.functions.len();
        for i in 0..len {
            let func_sig = module.borrow().ast.functions[i].sig.clone();
            let is_ext = module.borrow().ast.functions[i].body.is_none();
            create_function(&module, &builder, &func_sig, is_ext)
                .map_err(|e| errs.push(e))
                .ok();
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}

pub fn create_function(
    module: &MutRc<MModule>,
    builder: &MIRBuilder,
    func_sig: &FuncSignature,
    is_external: bool,
) -> Res<Rc<Variable>> {
    module.borrow_mut().try_reserve_name(&func_sig.name)?;

    let name = if is_external {
        String::clone(&func_sig.name.lexeme)
    } else {
        get_function_name(&module.borrow().path, &func_sig.name.lexeme)
    };

    let ret_type = func_sig
        .return_type
        .as_ref()
        .map(|ty| builder.find_type(ty))
        .unwrap_or(Ok(Type::None))?;

    let mut parameters = Vec::with_capacity(func_sig.parameters.len());
    for param in func_sig.parameters.iter() {
        parameters.push(Rc::new(Variable {
            mutable: false,
            name: Rc::clone(&param.name.lexeme),
            type_: builder.find_type(&param.type_)?,
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
            .or_err(
                &module.borrow().path,
                &func_sig.name,
                "Can't define main multiple times.",
            )?;
    }

    module
        .borrow_mut()
        .globals
        .insert(Rc::clone(&func_sig.name.lexeme), Rc::clone(&global));
    Ok(global)
}

pub fn get_function_name(path: &Rc<ModulePath>, func_name: &Rc<String>) -> String {
    if func_name.as_ref() != "main" {
        format!("{}:{}", path, func_name)
    } else {
        func_name.to_string()
    }
}
