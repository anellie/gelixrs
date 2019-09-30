/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/30/19 2:26 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{ASTType, Class, FuncSignature, FunctionArg};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::passes::PreMIRPass;

/// This pass declares all classes.
/// It does not fill them; they are kept empty.
pub struct DeclareClassPass<'p> {
    gen: &'p mut MIRGenerator,
    none_const: ASTType,
}

impl<'p> PreMIRPass for DeclareClassPass<'p> {
    fn run(mut self, module: &mut Module) -> Res<()> {
        for class in module.classes.iter_mut() {
            self.create_class(class)?;
        }

        Ok(())
    }
}

impl<'p> DeclareClassPass<'p> {
    fn create_class(&mut self, class: &mut Class) -> Res<()> {
        // Create class (filled in another pass)
        let mir_class = self
            .gen
            .builder
            .create_class(Rc::clone(&class.name.lexeme))
            .ok_or_else(|| {
                MIRGenerator::error(
                    self.gen,
                    &class.name,
                    &class.name,
                    "Class was already defined!",
                )
            })?;

        let this_arg = FunctionArg {
            name: Token::generic_identifier("this".to_string()),
            _type: ASTType::Token(class.name.clone()),
        };

        // Declare all class methods
        let name = &class.name.lexeme;
        let mut mir_class = mir_class.borrow_mut();

        // Create init function
        let init_fn_name_tok =
            Token::generic_identifier(format!("{}-internal-init", &class.name.lexeme));
        let init_fn = create_function(
            self.gen,
            &FuncSignature {
                name: init_fn_name_tok.clone(),
                return_type: None,
                parameters: vec![this_arg.clone()],
            },
            &self.none_const,
        )?;
        mir_class.methods.insert(init_fn_name_tok.lexeme, init_fn);

        // Do all user-defined methods
        for method in class.methods.iter_mut() {
            let old_name = Rc::clone(&method.sig.name.lexeme);
            method.sig.name.lexeme = Rc::new(format!("{}-{}", name, method.sig.name.lexeme));
            method.sig.parameters.insert(0, this_arg.clone());

            let mir_method = create_function(self.gen, &method.sig, &self.none_const)?;
            mir_class.methods.insert(old_name, mir_method);
        }

        Ok(())
    }

    pub fn new(gen: &'p mut MIRGenerator) -> DeclareClassPass<'p> {
        Self {
            gen,
            none_const: ASTType::Token(Token::generic_identifier("None".to_string())),
        }
    }
}
