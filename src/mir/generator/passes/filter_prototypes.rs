/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 6:18 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::Error;
use crate::mir::generator::passes::{ModulePass, PassType};
use crate::mir::nodes::{
    ClassPrototype, FunctionPrototype, InterfacePrototype, Prototype, Prototypes,
};
use crate::mir::{mutrc_new, MModule, MutRc};

/// This pass removes all types/functions with generic parameters
/// from the AST list, since they are handled separately.
pub struct FilterPrototypes();

impl ModulePass for FilterPrototypes {
    fn get_type(&self) -> PassType {
        PassType::Module
    }

    fn run_mod(&mut self, module: MutRc<MModule>) -> Result<(), Vec<Error>> {
        let mut module = module.borrow_mut();
        let mut errs = Vec::new();

        let mut i = 0;
        while i != module.ast.classes.len() {
            if module.ast.classes[i].generics.is_some() {
                let class = module.ast.classes.remove(i);
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
            } else {
                i += 1;
            }
        }

        let mut i = 0;
        while i != module.ast.interfaces.len() {
            if module.ast.interfaces[i].generics.is_some() {
                let iface = module.ast.interfaces.remove(i);
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
            } else {
                i += 1;
            }
        }

        let mut i = 0;
        while i != module.ast.functions.len() {
            if module.ast.functions[i].sig.generics.is_some() {
                let func = module.ast.functions.remove(i);
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
            } else {
                i += 1;
            }
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}
