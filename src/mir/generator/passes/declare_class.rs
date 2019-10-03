/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/3/19 5:13 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{ASTType, Class, FuncSignature, FunctionArg};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::ToMIRResult;

/// This pass declares all classes.
/// It does not fill them; they are kept empty.
pub fn declare_class_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    for class in module.classes.iter_mut() {
        create_class(gen, class)?;
    }

    Ok(())
}

fn create_class(gen: &mut MIRGenerator, class: &mut Class) -> Res<()> {
    let mir_class = gen
        .builder
        .create_class(&class.name)
        .or_err(gen, &class.name, "Class is already defined!")?;
    let mut mir_class = mir_class.borrow_mut();
    let this_arg = FunctionArg::this_arg(&class.name);

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
        let method_name = Rc::clone(&method.sig.name.lexeme);
        // Change the method name to $class-$method to prevent name collisions
        method.sig.name.lexeme = Rc::new(format!("{}-{}", class.name.lexeme, method.sig.name.lexeme));
        method.sig.parameters.insert(0, this_arg.clone());

        let mir_method = create_function(gen, &method.sig)?;
        mir_class.methods.insert(method_name, mir_method);
    }

    gen.builder.remove_this_alias();
    Ok(())
}
