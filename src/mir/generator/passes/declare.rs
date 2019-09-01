/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/1/19 6:32 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::{Class, DeclarationList, FuncSignature, FunctionArg};
use crate::lexer::token::Token;
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::generator::MIRGenerator;
use crate::mir::nodes::{MIRType, MIRVariable, MIRFunction};
use crate::Res;
use std::rc::Rc;
use crate::mir::MutRc;

/// This pass declares all structs and functions in the AST.
/// It does not fill structs; they are kept empty.
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
    /// Declare all classes.
    fn classes(&mut self, list: &mut DeclarationList) -> Res<()> {
        for class in list.classes.iter_mut() {
            self.create_class(class)?;
        }

        Ok(())
    }

    fn create_class(&mut self, class: &mut Class) -> Res<()> {
        // Create struct (filled in another pass)
        let mir_class = self.gen
            .builder
            .create_struct(Rc::clone(&class.name.lexeme))
            .ok_or_else(|| {
                MIRGenerator::error(&class.name, &class.name, "Class was already defined!")
            })?;

        let this_arg = FunctionArg {
            name: Token::generic_identifier("this".to_string()),
            _type: class.name.clone(),
        };

        // Create init function
        self.create_function(&FuncSignature {
            name: Token::generic_identifier(format!("{}-internal-init", &class.name.lexeme)),
            return_type: None,
            parameters: vec![this_arg.clone()],
        })?;

        // Declare all class methods
        let name = &class.name.lexeme;
        let mut mir_class = mir_class.borrow_mut();
        for method in class.methods.iter_mut() {
            let old_name = Rc::clone(&method.sig.name.lexeme);
            method.sig.name.lexeme = Rc::new(format!("{}-{}", name, method.sig.name.lexeme));
            method.sig.parameters.insert(0, this_arg.clone());

            let mir_method = self.create_function(&method.sig)?;
            mir_class.methods.insert(old_name, mir_method);
        }

        Ok(())
    }

    /// Declare all functions / create their signatures
    fn functions(&mut self, list: &mut DeclarationList) -> Res<()> {
        for function in list
            .ext_functions
            .iter()
            .chain(list.functions.iter().map(|f| &f.sig))
        {
            self.create_function(&function)?;
        }

        Ok(())
    }

    fn create_function(&mut self, func_sig: &FuncSignature) -> Res<MutRc<MIRFunction>> {
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
            .ok_or_else(|| {
                MIRGenerator::error(
                    func_sig.return_type.as_ref().unwrap(),
                    func_sig.return_type.as_ref().unwrap(),
                    "Unknown function return type",
                )
            })?;

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
                        MIRGenerator::error(
                            &func_sig.name,
                            func_sig.return_type.as_ref().unwrap_or(&func_sig.name),
                            "Function parameter has unknown type",
                        )
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
            .ok_or_else(|| {
                MIRGenerator::error(
                    &func_sig.name,
                    &func_sig.name,
                    "Function was declared twice",
                )
            })?;

        self.gen.environments.first_mut().unwrap().insert(
            Rc::clone(&func_sig.name.lexeme),
            Rc::new(MIRVariable::new(
                Rc::clone(&func_sig.name.lexeme),
                MIRType::Function(Rc::clone(&function)),
                false,
            )),
        );

        Ok(function)
    }

    pub fn new(gen: &'p mut MIRGenerator) -> DeclarePass<'p> {
        DeclarePass {
            gen,
            none_const: Rc::new("None".to_string()),
        }
    }
}
