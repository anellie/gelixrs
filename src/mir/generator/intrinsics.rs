/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/26/19 10:08 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

//! This module contains things related to intrinsics: things that
//! bridge the gap between the compiler and the language.
//! An example would be the translation of operator
//! overloading interfaces into actually changing the behavior
//! of the expression.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use crate::ast::Module;
use crate::lexer::token::TType;
use crate::mir::{MIRModule, MutRc};
use crate::mir::generator::builder::MIRBuilder;
use crate::mir::generator::MIRGenerator;
use crate::mir::nodes::{Interface, InterfacePrototype};

thread_local! {
    pub static INTRINSICS: RefCell<Intrinsics> = RefCell::new(Intrinsics::default());
}

/// Contains all data structures that require some sort of special treatment.
#[derive(Default)]
pub struct Intrinsics {
    ops: HashMap<TType, MutRc<InterfacePrototype>>
}

impl Intrinsics {
    pub fn get_op_iface(&self, ty: TType) -> MutRc<InterfacePrototype> {
        Rc::clone(&self.ops[&ty])
    }

    pub fn populate(&mut self, modules: &mut Vec<(Module, MIRGenerator)>) {
        for (ast_mod, gen) in modules {
            if **ast_mod.path[0] == *"std" && **ast_mod.path[1] == *"ops" {
                // This is the std/ops module, containing all operator interfaces
                self.fill_ops_table(&gen.builder)
            }
        }
    }

    fn fill_ops_table(&mut self, builder: &MIRBuilder) {
        for (name, iface) in builder.prototypes.interfaces.iter() {
            let iface = Rc::clone(iface);
            match &name[..] {
                "Add" => self.ops.insert(TType::Plus, iface),
                "Sub" => self.ops.insert(TType::Minus, iface),
                "Mul" => self.ops.insert(TType::Star, iface),
                "Div" => self.ops.insert(TType::Slash, iface),
                "Equal" => {
                    self.ops.insert(TType::EqualEqual, Rc::clone(&iface));
                    self.ops.insert(TType::BangEqual, iface)
                }
                _ => None
            };
        }
    }
}