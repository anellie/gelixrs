/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 9:58 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::collections::HashMap;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::Module;
use crate::error::{Error, Errors};
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::passes::{ModulePass, PassType, PreMIRPass};
use crate::mir::nodes::{Class, Interface, Type};

/// This pass defines all types inside the module; currently classes and interfaces.
/// It only creates a stub MIR definition and inserts it as a type;
/// nothing is filled or created.
pub struct DeclareTypes();

impl PreMIRPass for DeclareTypes {
    fn run(&mut self, ast: &mut Module, module: MutRc<MModule>) -> Result<(), Errors> {
        let mut module = module.borrow_mut();
        let mut errs = Vec::new();

        for class in ast.classes.drain(..) {
            let name = class.name.clone();
            module
                .try_reserve_name(&name)
                .map_err(|e| errs.push(e))
                .ok();

            let mir_class_rc = mutrc_new(Class {
                name: Rc::clone(&name.lexeme),
                members: IndexMap::with_capacity(class.variables.len()),
                methods: HashMap::with_capacity(class.methods.len()),
                instantiator: Rc::new(Default::default()),
                constructors: Vec::with_capacity(class.constructors.len()),
                ast: Rc::new(class)
            });

            module
                .types
                .insert(Rc::clone(&name.lexeme), Type::Class(mir_class_rc));
        }

        for iface in ast.interfaces.drain(..) {
            let name = iface.name.clone();
            module
                .try_reserve_name(&name)
                .map_err(|e| errs.push(e))
                .ok();

            let mir_iface_rc = mutrc_new(Interface {
                name: Rc::clone(&name.lexeme),
                methods: IndexMap::with_capacity(iface.methods.len()),
                proto: None,
                ast: Rc::new(iface)
            });

            module
                .types
                .insert(Rc::clone(&name.lexeme), Type::Interface(mir_iface_rc));
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(Errors(errs, Rc::clone(&module.src)))
        }
    }
}
