/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/1/19 5:04 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{ASTType, Class, FuncSignature, FunctionArg};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::{MIRGenerator, Res};

/// This pass declares all classes.
/// It does not fill them; they are kept empty.
pub fn declare_class_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    for class in module.classes.iter_mut() {
        create_class(gen, class)?;
    }

    Ok(())
}

fn create_class(gen: &mut MIRGenerator, class: &mut Class) -> Res<()> {
    // Create class (filled in another pass)
    let mir_class = gen
        .builder
        .create_class(Rc::clone(&class.name.lexeme))
        .ok_or_else(|| {
            MIRGenerator::error(gen, &class.name, &class.name, "Class was already defined!")
        })?;

    let this_arg = FunctionArg::this_arg(&class.name);
    let name = &class.name.lexeme;
    let mut mir_class = mir_class.borrow_mut();

    // Create init function
    let init_fn_name = Token::generic_identifier(format!("{}-internal-init", &class.name.lexeme));
    let init_fn = create_function(
        gen,
        &FuncSignature {
            name: init_fn_name.clone(),
            return_type: None,
            parameters: vec![this_arg.clone()],
        },
    )?;
    mir_class.methods.insert(init_fn_name.lexeme, init_fn);

    // Do all user-defined methods
    for method in class.methods.iter_mut() {
        let old_name = Rc::clone(&method.sig.name.lexeme);
        method.sig.name.lexeme = Rc::new(format!("{}-{}", name, method.sig.name.lexeme));
        method.sig.parameters.insert(0, this_arg.clone());

        let mir_method = create_function(gen, &method.sig)?;
        mir_class.methods.insert(old_name, mir_method);
    }

    Ok(())
}
