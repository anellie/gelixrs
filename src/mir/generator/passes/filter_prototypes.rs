/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/18/19 3:49 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::Module;
use crate::error::Errors;
use crate::mir::generator::passes::PreMIRPass;
use crate::mir::nodes::{ProtoAST, Prototype};
use crate::mir::{MModule, MutRc};

/// This pass removes all types/functions with generic parameters
/// from the AST list, since they are handled separately.
pub struct FilterPrototypes();

impl PreMIRPass for FilterPrototypes {
    fn run(
        &mut self,
        ast: &mut Module,
        module_rc: MutRc<MModule>,
        _modules: &[MutRc<MModule>],
    ) -> Result<(), Errors> {
        let mut module = module_rc.borrow_mut();
        let mut errs = Vec::new();

        let class_iter = ast
            .classes
            .drain_filter(|c| c.generics.is_some())
            .map(|c| (c.name.clone(), ProtoAST::Class(Rc::new(c))));
        let iface_iter = ast
            .interfaces
            .drain_filter(|i| i.generics.is_some())
            .map(|i| (i.name.clone(), ProtoAST::Interface(Rc::new(i))));
        let func_iter = ast
            .functions
            .drain_filter(|f| f.sig.generics.is_some())
            .map(|f| (f.sig.name.clone(), ProtoAST::Function(Rc::new(f))));

        for (name, ast) in class_iter.chain(iface_iter).chain(func_iter) {
            module
                .try_reserve_name(&name)
                .map_err(|e| errs.push(e))
                .ok();
            module.protos.insert(
                Rc::clone(&name.lexeme),
                Rc::new(Prototype {
                    name: name.lexeme,
                    instances: Default::default(),
                    impls: vec![],
                    module: Rc::clone(&module_rc),
                    ast,
                }),
            );
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(Errors(errs, Rc::clone(&module.src)))
        }
    }
}
