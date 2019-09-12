/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/12/19, 8:58 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::{Class, FuncSignature, FunctionArg};
use crate::ast::module::Module;
use crate::lexer::token::Token;
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::generator::{MIRGenerator, Res};
use crate::mir::nodes::{MIRType, MIRVariable};
use std::rc::Rc;

fn create_function(
    gen: &mut MIRGenerator,
    func_sig: &FuncSignature,
    none_const: &Rc<String>,
) -> Res<Rc<MIRVariable>> {
    let ret_type = gen
        .builder
        .find_type(
            func_sig
                .return_type
                .as_ref()
                .map(|t| &t.lexeme)
                .unwrap_or(none_const),
        )
        .ok_or_else(|| {
            MIRGenerator::error(
                gen,
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
            _type: gen.builder.find_type(&param._type.lexeme).ok_or_else(|| {
                MIRGenerator::error(
                    gen,
                    &func_sig.name,
                    func_sig.return_type.as_ref().unwrap_or(&func_sig.name),
                    "Function parameter has unknown type",
                )
            })?,
        }))
    }

    let function = gen
        .builder
        .create_function(
            Rc::clone(&func_sig.name.lexeme),
            ret_type.clone(),
            parameters,
        )
        .ok_or_else(|| {
            MIRGenerator::error(
                gen,
                &func_sig.name,
                &func_sig.name,
                "Function was declared twice",
            )
        })?;

    let global = Rc::new(MIRVariable::new(
        Rc::clone(&func_sig.name.lexeme),
        MIRType::Function(Rc::clone(&function)),
        false,
    ));
    gen.builder
        .add_global(Rc::clone(&func_sig.name.lexeme), Rc::clone(&global));

    Ok(global)
}

/// This pass declares all classes.
/// It does not fill them; they are kept empty.
pub struct DeclareClassPass<'p> {
    gen: &'p mut MIRGenerator,
    none_const: Rc<String>,
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
        // Create struct (filled in another pass)
        let mir_class = self
            .gen
            .builder
            .create_struct(Rc::clone(&class.name.lexeme))
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
            _type: class.name.clone(),
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
            none_const: Rc::new("None".to_string()),
        }
    }
}

/// This pass declares all functions.
pub struct DeclareFuncPass<'p> {
    gen: &'p mut MIRGenerator,
    none_const: Rc<String>,
}

impl<'p> PreMIRPass for DeclareFuncPass<'p> {
    fn run(self, module: &mut Module) -> Res<()> {
        for function in module
            .ext_functions
            .iter()
            .chain(module.functions.iter().map(|f| &f.sig))
        {
            create_function(self.gen, &function, &self.none_const)?;
        }

        Ok(())
    }
}

impl<'p> DeclareFuncPass<'p> {
    pub fn new(gen: &'p mut MIRGenerator) -> DeclareFuncPass<'p> {
        Self {
            gen,
            none_const: Rc::new("None".to_string()),
        }
    }
}
