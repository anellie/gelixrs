/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/1/19 6:18 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */


use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::declaration::{ASTType, FunctionArg, IFaceImpl};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::generator::passes::declare_func::create_function;
use crate::mir::generator::passes::PreMIRPass;

/// This pass fills all classes with their members
/// and creates their internal init function.
pub struct IfaceImplPass<'p> {
    gen: &'p mut MIRGenerator,
    none_const: ASTType,
}

impl<'p> PreMIRPass for IfaceImplPass<'p> {
    fn run(mut self, list: &mut Module) -> Res<()> {
        for iface in list.iface_impls.iter_mut() {
            self.iface_impl(iface)?;
        }
        Ok(())
    }
}

impl<'p> IfaceImplPass<'p> {
    fn iface_impl(&mut self, iface_impl: &mut IFaceImpl) -> Res<()> {
        let class = self.gen.builder
            .find_class(&iface_impl.class.lexeme)
            .ok_or_else(|| self.gen.error(&iface_impl.class, &iface_impl.class, "Unknown class."))?;
        let mut class = class.borrow_mut();

        let iface = self.gen.builder
            .find_interface(&iface_impl.iface.lexeme)
            .ok_or_else(|| self.gen.error(&iface_impl.iface, &iface_impl.iface, "Unknown interface."))?;
        let iface = iface.borrow();

        for method in iface_impl.methods.iter_mut() {
            if !iface.methods.contains_key(&method.sig.name.lexeme) {
                return Err(self.gen.error(&method.sig.name, &method.sig.name, "Method is not defined in interface."))?;
            }

            let this_arg = FunctionArg {
                name: Token::generic_identifier("this".to_string()),
                _type: ASTType::Token(iface_impl.class.clone()),
            };

            let old_name = Rc::clone(&method.sig.name.lexeme);
            method.sig.name.lexeme = Rc::new(format!("{}-{}", iface_impl.class.lexeme, method.sig.name.lexeme));
            method.sig.parameters.insert(0, this_arg);

            let mir_method = create_function(self.gen, &method.sig, &self.none_const)?;
            class.methods.insert(old_name, mir_method);
        }

        if iface.methods.len() > iface_impl.methods.len() {
            Err(self.gen.error(&iface_impl.iface, &iface_impl.iface, "Missing methods in interface impl."))
        } else {
            Ok(())
        }
    }
}

impl<'p> IfaceImplPass<'p> {
    pub fn new(gen: &'p mut MIRGenerator) -> IfaceImplPass<'p> {
        Self {
            gen,
            none_const: ASTType::Token(Token::generic_identifier("None".to_string())),
        }
    }
}
