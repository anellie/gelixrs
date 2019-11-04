/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 11:02 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{Class as ASTClass, FuncSignature, FunctionArg};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::mutrc_new;
use crate::mir::nodes::{Class, ClassPrototype};

/// This pass declares all classes.
/// It does not fill them; they are kept empty.
pub fn declare_class_pass(gen: &mut MIRGenerator, module: &mut Module) -> Res<()> {
    for class in module.classes.iter_mut() {
        create_class(gen, class)?;
    }

    Ok(())
}

fn create_class(gen: &mut MIRGenerator, class: &mut ASTClass) -> Res<()> {
    gen.builder.try_reserve_name(&class.name)?;
    gen.builder.add_this_alias(&class.name);

    let this_arg = FunctionArg::this_arg(&class.name);
    let init_fn_name = Token::generic_identifier(format!("{}-internal-init", &class.name.lexeme));
    let init_fn_sig = FuncSignature {
        name: init_fn_name.clone(),
        generics: class.generics.clone(),
        return_type: None,
        parameters: vec![this_arg.clone()],
    };

    // If the AST class has generic parameters, it must be compiled to
    // a class prototype instead of an actual class.
    if let Some(generics) = &class.generics {
        gen.builder.set_generic_types(generics);

        let mut mir_class = mutrc_new(ClassPrototype {
            name: Rc::clone(&class.name.lexeme),
            generic_args: gen.builder.generic_types.iter().cloned().collect(),
            ..Default::default()
        });
        gen.builder.prototypes.classes.insert(Rc::clone(&class.name.lexeme), Rc::clone(&mir_class));
        let mut mir_class = mir_class.borrow_mut();

        let init_fn = create_function(gen, &init_fn_sig, false)?.right().unwrap();
        mir_class.methods.insert(init_fn_name.lexeme, init_fn);

        // Do all user-defined methods
        for method in class.methods.iter_mut() {
            let method_name = Rc::clone(&method.sig.name.lexeme);
            // Change the method name to $class-$method to prevent name collisions
            method.sig.name.lexeme =
                Rc::new(format!("{}-{}", class.name.lexeme, method.sig.name.lexeme));
            method.sig.parameters.insert(0, this_arg.clone());

            let mir_method = create_function(gen, &method.sig, false)?.right().unwrap();
            mir_class.methods.insert(method_name, mir_method);
        }

        gen.builder.generic_types.clear();
    } else {
        let mut mir_class = mutrc_new(Class {
            name: Rc::clone(&class.name.lexeme),
            ..Default::default()
        });
        gen.builder.module.classes.insert(Rc::clone(&class.name.lexeme), Rc::clone(&mir_class));
        let mut mir_class = mir_class.borrow_mut();

        let init_fn = create_function(gen, &init_fn_sig, false)?.left().unwrap();
        mir_class.methods.insert(init_fn_name.lexeme, init_fn);

        // Do all user-defined methods
        for method in class.methods.iter_mut() {
            let method_name = Rc::clone(&method.sig.name.lexeme);
            // Change the method name to $class-$method to prevent name collisions
            method.sig.name.lexeme =
                Rc::new(format!("{}-{}", class.name.lexeme, method.sig.name.lexeme));
            method.sig.parameters.insert(0, this_arg.clone());

            let mir_method = create_function(gen, &method.sig, false)?.left().unwrap();
            mir_class.methods.insert(method_name, mir_method);
        }
    }

    gen.builder.remove_this_alias();
    Ok(())
}
