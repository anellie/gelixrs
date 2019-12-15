/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 11:34 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::error::Error;
use crate::mir::{MModule, MutRc};
use crate::mir::generator::MIRGenerator;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::nodes::Type;

/// This pass populates the intrinsics struct.
pub struct Generate();

impl ModulePass for Generate {
    fn get_type(&self) -> PassType {
        PassType::Module
    }

    fn run_mod(&mut self, module: MutRc<MModule>) -> Result<(), Vec<Error>> {
        let mut errs = Vec::new();
        // TODO: This is not ideal
        // It's not terrible, since all types are wrapped in a Rc,
        // but cloning a bunch of Rc is not ideal.
        let types: Vec<Type> = module.borrow().types.values().cloned().collect();

        let mut gen = MIRGenerator::new(&module);
        for ty in types {
            match ty {
                Type::Function(func) => {
                    let ast = func.borrow().ast.as_ref().cloned();
                    if let Some(ast) = ast {
                        gen.generate_function(&ast, None)
                            .map_err(|e| errs.push(e))
                            .ok();
                    }
                }

                Type::Class(class) => {
                    let ast = Rc::clone(&class.borrow().ast);
                    gen.generate_constructors(&ast)
                        .map_err(|e| errs.push(e))
                        .ok();

                    for method in ast.methods.iter() {
                        let mir = &class.borrow().methods[&method.sig.name.lexeme];
                        gen.generate_function(method, Some(mir.type_.as_function()))
                            .map_err(|e| errs.push(e))
                            .ok();
                    }
                }

                Type::Interface(_iface) => (),

                _ => panic!("Primitive type in module!"),
            }
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}
