/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/16/19 3:28 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::Module;
use crate::error::Errors;
use crate::mir::{MModule, MutRc, mutrc_new};
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::nodes::{
    ClassPrototype, FunctionPrototype, InterfacePrototype, Prototype, Prototypes,
};

/// This pass removes all types/functions with generic parameters
/// from the AST list, since they are handled separately.
pub struct FilterPrototypes();

impl PreMIRPass for FilterPrototypes {
    fn run(&mut self, ast: &mut Module, module: MutRc<MModule>, _modules: &Vec<MutRc<MModule>>) -> Result<(), Errors> {
        let mut module = module.borrow_mut();
        let mut errs = Vec::new();

        for class in ast.classes.drain_filter(|c| c.generics.is_some()) {
            module
                .try_reserve_name(&class.name)
                .map_err(|e| errs.push(e))
                .ok();
            module.protos.insert(
                Rc::clone(&class.name.lexeme),
                Prototype {
                    name: Rc::clone(&class.name.lexeme),
                    proto: Prototypes::Class(mutrc_new(ClassPrototype {
                        ast: class,
                        impls: vec![],
                        instances: RefCell::new(Default::default()),
                    })),
                },
            );
        }

        for iface in ast.interfaces.drain_filter(|i| i.generics.is_some()) {
            module
                .try_reserve_name(&iface.name)
                .map_err(|e| errs.push(e))
                .ok();
            module.protos.insert(
                Rc::clone(&iface.name.lexeme),
                Prototype {
                    name: Rc::clone(&iface.name.lexeme),
                    proto: Prototypes::Interface(mutrc_new(InterfacePrototype {
                        ast: iface,
                        impls: vec![],
                        instances: RefCell::new(Default::default()),
                    })),
                },
            );
        }

        for func in ast.functions.drain_filter(|f| f.sig.generics.is_some()) {
            module
                .try_reserve_name(&func.sig.name)
                .map_err(|e| errs.push(e))
                .ok();
            module.protos.insert(
                Rc::clone(&func.sig.name.lexeme),
                Prototype {
                    name: Rc::clone(&func.sig.name.lexeme),
                    proto: Prototypes::Function(mutrc_new(FunctionPrototype {
                        ast: func,
                        instances: RefCell::new(Default::default()),
                    })),
                },
            );
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(Errors(errs, Rc::clone(&module.src)))
        }
    }
}
