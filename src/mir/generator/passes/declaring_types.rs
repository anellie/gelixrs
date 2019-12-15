/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 3:47 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::error::{Error, Res};
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::nodes::{Class, Interface, Type};

/// This pass defines all types inside the module; currently classes and interfaces.
/// It only creates a stub MIR definition and inserts it as a type;
/// nothing is filled or created.
pub struct DeclareTypes();

impl ModulePass for DeclareTypes {
    fn get_type(&self) -> PassType { PassType::Module }

    fn run_mod(&mut self, module: MutRc<MModule>) -> Result<(), Vec<Error>> {
        let mut module = module.borrow_mut();
        let mut errs = Vec::new();

        for i in 0..module.ast.classes.len() {
            let name = module.ast.classes[i].name.clone();
            module.try_reserve_name(&name).map_err(|e| errs.push(e));

            let mir_class_rc = mutrc_new(Class {
                name: Rc::clone(&name.lexeme),
                ..Default::default()
            });

            module
                .types
                .insert(Rc::clone(&name.lexeme), Type::Class(mir_class_rc));
        }

        for i in 0..module.ast.interfaces.len() {
            let name = module.ast.interfaces[i].name.clone();
            module.try_reserve_name(&name).map_err(|e| errs.push(e));

            let mir_iface_rc = mutrc_new(Interface {
                name: Rc::clone(&name.lexeme),
                ..Default::default()
            });

            module
                .types
                .insert(Rc::clone(&name.lexeme), Type::Interface(mir_iface_rc));
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}