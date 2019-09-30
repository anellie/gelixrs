/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/30/19 2:26 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{ASTType, FunctionArg, Interface};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::passes::PreMIRPass;

/// This pass declares all interfaces.
pub struct DeclareIFacePass<'p> {
    gen: &'p mut MIRGenerator,
    none_const: ASTType,
}

impl<'p> PreMIRPass for DeclareIFacePass<'p> {
    fn run(mut self, module: &mut Module) -> Res<()> {
        for interface in module.interfaces.iter_mut() {
            self.create_interface(interface)?
        }

        Ok(())
    }
}

impl<'p> DeclareIFacePass<'p> {
    fn create_interface(&mut self, interface: &mut Interface) -> Res<()> {
        let mir_iface = self.gen.builder.create_interface(&interface.name.lexeme)
            .ok_or(self.gen.error(&interface.name, &interface.name, "Interface with the same name already defined."))?;
        let mut mir_iface = mir_iface.borrow_mut();

        let this_arg = FunctionArg {
            name: Token::generic_identifier("this".to_string()),
            _type: ASTType::Token(interface.name.clone()),
        };

        for method in interface.methods.iter_mut() {
            let old_name = Rc::clone(&method.sig.name.lexeme);
            method.sig.name.lexeme = Rc::new(format!("{}-{}", interface.name.lexeme, method.sig.name.lexeme));
            method.sig.parameters.insert(0, this_arg.clone());

            let mir_method = create_function(self.gen, &method.sig, &self.none_const)?;
            mir_iface.methods.insert(old_name, mir_method);
        }

        Ok(())
    }

    pub fn new(gen: &'p mut MIRGenerator) -> DeclareIFacePass<'p> {
        Self {
            gen,
            none_const: ASTType::Token(Token::generic_identifier("None".to_string())),
        }
    }
}