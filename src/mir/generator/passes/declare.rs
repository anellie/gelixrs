/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/26/19 6:45 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::{Class, DeclarationList, FuncSignature, FunctionArg};
use crate::lexer::token::Token;
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::generator::{Error, MIRGenerator, Res};
use crate::mir::nodes::{MIRType, MIRVariable};
use std::rc::Rc;

pub struct DeclarePass<'p> {
    gen: &'p mut MIRGenerator,
    none_const: Rc<String>,
}

impl<'p> PreMIRPass for DeclarePass<'p> {
    fn run(mut self, list: &mut DeclarationList) -> Res<()> {
        self.classes(list)?;
        self.functions(list)
    }
}

impl<'p> DeclarePass<'p> {
    /// This part of the pass declares all classes.
    fn classes(&mut self, list: &DeclarationList) -> Res<()> {
        for class in &list.classes {
            self.create_class(&class)?;
        }

        Ok(())
    }

    fn create_class(&mut self, class: &Class) -> Res<()> {
        // Create struct (filled later)
        self.gen
            .builder
            .create_struct(Rc::clone(&class.name.lexeme))
            .ok_or_else(|| {
                Error::new(
                    Some(class.name.line),
                    "Class was already defined!",
                    format!("class {} {{ ... }}", &class.name.lexeme),
                )
            })?;

        // Create init function
        self.create_function(&FuncSignature {
            name: Token::generic_identifier(format!("{}-internal-init", &class.name.lexeme)),
            return_type: None,
            parameters: vec![FunctionArg {
                name: Token::generic_identifier("this".to_string()),
                _type: class.name.clone(),
            }],
        })?;

        Ok(())
    }

    /// This part declares all functions (their signatures).
    fn functions(&mut self, list: &mut DeclarationList) -> Res<()> {
        for function in list
            .ext_functions
            .iter()
            .chain(list.functions.iter().map(|f| &f.sig))
        {
            self.create_function(&function)?;
        }

        for class in list.classes.iter_mut() {
            let name = &class.name.lexeme;
            for method in class.methods.iter_mut() {
                method.sig.name.lexeme = Rc::new(format!("{}-{}", name, method.sig.name.lexeme));
                self.create_function(&method.sig)?;
            }
        }

        Ok(())
    }

    fn create_function(&mut self, func_sig: &FuncSignature) -> Res<()> {
        let ret_type = &self
            .gen
            .builder
            .find_type(
                func_sig
                    .return_type
                    .as_ref()
                    .map(|t| &t.lexeme)
                    .unwrap_or(&self.none_const),
            )
            .ok_or_else(|| Error::new_fn("Unknown function return type", &func_sig))?;

        let mut parameters = Vec::with_capacity(func_sig.parameters.len());
        for param in func_sig.parameters.iter() {
            parameters.push(Rc::new(MIRVariable {
                mutable: false,
                name: Rc::clone(&param.name.lexeme),
                _type: self
                    .gen
                    .builder
                    .find_type(&param._type.lexeme)
                    .ok_or_else(|| {
                        Error::new_fn("Function parameter has unknown type", &func_sig)
                    })?,
            }))
        }

        let function = self
            .gen
            .builder
            .create_function(
                Rc::clone(&func_sig.name.lexeme),
                ret_type.clone(),
                parameters,
            )
            .ok_or_else(|| Error::new_fn("Function was declared twice", &func_sig))?;

        self.gen.environments.first_mut().unwrap().insert(
            Rc::clone(&func_sig.name.lexeme),
            Rc::new(MIRVariable::new(
                Rc::clone(&func_sig.name.lexeme),
                MIRType::Function(function),
                false,
            )),
        );

        Ok(())
    }

    pub fn new(gen: &'p mut MIRGenerator) -> DeclarePass<'p> {
        DeclarePass {
            gen,
            none_const: Rc::new("None".to_string()),
        }
    }
}
