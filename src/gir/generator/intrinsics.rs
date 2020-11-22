/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 2/3/20 3:23 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

//! This module contains things related to intrinsics: things that
//! bridge the gap between the compiler and the language.
//! An example would be the translation of operator
//! overloading interfaces into actually changing the behavior
//! of the expression.

use std::{
    cell::{Ref, RefCell},
    collections::HashMap,
    rc::Rc,
};

use crate::{
    error::{Error, Res},
    gir::{
        nodes::{
            declaration::{Function, ADT},
            module::Module,
        },
        MutRc, Type,
    },
    lexer::token::TType,
};

thread_local! {
    pub static INTRINSICS: RefCell<Intrinsics> = RefCell::new(Intrinsics::default());
}

/// Contains all data structures that require some sort of special treatment.
#[derive(Default)]
pub struct Intrinsics {
    /// Contains prototypes of all operators.
    /// For binary operators, the key is the TType of the operator.
    /// For IndexGet, its LeftBracket.
    /// For IndexSet, its RightBracket.
    ops: HashMap<TType, MutRc<ADT>>,
    /// String type, used for string literals.
    pub string_type: Option<Type>,
    /// `std/iter/Iter` prototype
    pub iter_proto: Option<MutRc<ADT>>,
    /// `std/iter/ToIter` prototype
    pub to_iter_proto: Option<MutRc<ADT>>,
    /// The Free interface, used while compiling a class destructor.
    pub free_iface: Option<MutRc<ADT>>,
    /// libc free.
    pub libc_free: Option<MutRc<Function>>,
    /// The entry point of the program - more than one function
    /// named main is a compile error
    pub main_fn: Option<MutRc<Function>>,
    /// A list of functions required for compilation.
    /// Currently main_fn and a few intrinsics.
    pub required_compile_fns: Vec<MutRc<Function>>,
}

impl Intrinsics {
    /// Returns the interface corresponding with this binary operator
    pub fn get_op_iface(&self, ty: TType) -> Option<MutRc<ADT>> {
        self.ops.get(&ty).cloned()
    }

    /// Only call this with the std/ops module, containing all operator interfaces;
    /// fills self.ops
    pub fn fill_ops_table(&mut self, module: Ref<Module>) {
        for (name, iface) in &module.declarations {
            let iface = Rc::clone(iface.as_adt());
            match &name[..] {
                "Add" => self.ops.insert(TType::Plus, iface),
                "Sub" => self.ops.insert(TType::Minus, iface),
                "Mul" => self.ops.insert(TType::Star, iface),
                "Div" => self.ops.insert(TType::Slash, iface),
                "Equal" => {
                    self.ops.insert(TType::EqualEqual, Rc::clone(&iface));
                    self.ops.insert(TType::BangEqual, iface)
                }
                "IndexGet" => self.ops.insert(TType::LeftBracket, iface),
                "IndexSet" => self.ops.insert(TType::RightBracket, iface),
                _ => None,
            };
        }
    }

    /// Sets the main fn. Returns success, None indicates that
    /// a main function already existed
    pub fn set_main_fn(&mut self, func: &MutRc<Function>) -> Option<()> {
        if self.main_fn.is_some() {
            None
        } else {
            self.main_fn = Some(Rc::clone(func));
            self.required_compile_fns.push(Rc::clone(&func));
            Some(())
        }
    }

    /// Report any errors or issues with intrinsics.
    pub fn validate(&mut self) -> Res<()> {
        if self.main_fn.is_none() {
            return Err(Error {
                line: 0,
                start: 0,
                len: 0,
                producer: "GIR",
                message: "Could not find main function.".to_string(),
                module: Rc::new(Default::default()),
            });
        }
        Ok(())
    }
}
