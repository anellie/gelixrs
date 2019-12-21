/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/20/19 7:27 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

//! This module contains things related to intrinsics: things that
//! bridge the gap between the compiler and the language.
//! An example would be the translation of operator
//! overloading interfaces into actually changing the behavior
//! of the expression.

use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::error::{Error, Res};
use crate::lexer::token::{TType, Token};
use crate::mir::nodes::{Prototype, Type, Variable};
use crate::mir::MModule;

thread_local! {
    pub static INTRINSICS: RefCell<Intrinsics> = RefCell::new(Intrinsics::default());
}

/// Contains all data structures that require some sort of special treatment.
#[derive(Default)]
pub struct Intrinsics {
    ops: HashMap<TType, Rc<Prototype>>,
    pub array_proto: Option<Rc<Prototype>>,
    pub main_fn: Option<Rc<Variable>>,
}

impl Intrinsics {
    pub fn get_op_iface(&self, ty: TType) -> Rc<Prototype> {
        Rc::clone(&self.ops[&ty])
    }

    pub fn get_array_type(&self, ty: Type, tok: Option<Token>) -> Res<Type> {
        let proto = self.array_proto.as_ref().cloned().unwrap();
        let err_tok = tok.unwrap_or_else(|| Token::generic_token(TType::Identifier));
        proto.build(vec![ty], &err_tok, Rc::clone(&proto))
    }

    // Only call this with the std/ops module, containing all operator interfaces
    pub fn fill_ops_table(&mut self, module: Ref<MModule>) {
        for (name, iface) in module.protos.iter() {
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
                _ => None,
            };
        }
    }

    pub fn set_main_fn(&mut self, func: &Rc<Variable>) -> Option<()> {
        match self.main_fn {
            Some(_) => None,
            None => {
                self.main_fn = Some(Rc::clone(func));
                Some(())
            }
        }
    }

    pub fn validate(&mut self) -> Res<()> {
        if self.main_fn.is_none() {
            return Err(Error {
                line: 0,
                start: 0,
                len: 0,
                producer: "MIR",
                message: "Could not find main function.".to_string(),
                module: Rc::new(Default::default()),
            });
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        self.ops.clear();
        self.array_proto = None;
        self.main_fn = None;
    }
}
