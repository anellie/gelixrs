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

use common::MutRc;
use error::{Error, ErrorSpan, GErr, Res};
use gir_nodes::{Function, Module, Type, ADT};
use std::{cell::Ref, collections::HashMap, rc::Rc};
use syntax::kind::SyntaxKind;

/// Contains all data structures that require some sort of special treatment.
#[derive(Clone, Default)]
pub struct Intrinsics {
    /// Contains prototypes of all operators.
    /// For binary operators, the key is the SyntaxKind of the operator.
    /// For IndexGet, its LeftBracket.
    /// For IndexSet, its RightBracket.
    ops: HashMap<SyntaxKind, MutRc<ADT>>,
    /// String type, used for string literals.
    pub string_type: Option<Type>,
    /// `std/iter/Iter` prototype
    pub(crate) iter_proto: Option<MutRc<ADT>>,
    /// `std/iter/ToIter` prototype
    pub(crate) to_iter_proto: Option<MutRc<ADT>>,
    /// The Free interface, used while compiling a class destructor.
    pub(crate) free_iface: Option<MutRc<ADT>>,
    /// libc free.
    pub(crate) libc_free: Option<MutRc<Function>>,
    /// The entry point of the program - more than one function
    /// named main is a compile error
    pub(crate) main_fn: Option<MutRc<Function>>,
    /// std/prelude module for auto-import
    pub(crate) std_prelude: Option<MutRc<Module>>,
    /// A list of functions required for compilation.
    /// Currently main_fn and a few intrinsics.
    pub required_compile_fns: Vec<MutRc<Function>>,
}

impl Intrinsics {
    /// Returns the interface corresponding with this binary operator
    pub(crate) fn get_op_iface(&self, ty: SyntaxKind) -> Option<MutRc<ADT>> {
        self.ops.get(&ty).cloned()
    }

    /// Only call this with the std/ops module, containing all operator interfaces;
    /// fills self.ops
    pub(crate) fn fill_ops_table(&mut self, module: Ref<Module>) {
        for (name, iface) in &module.declarations {
            let iface = Rc::clone(iface.as_adt());
            match &name[..] {
                "Add" => self.ops.insert(SyntaxKind::Plus, iface),
                "Sub" => self.ops.insert(SyntaxKind::Minus, iface),
                "Mul" => self.ops.insert(SyntaxKind::Star, iface),
                "Div" => self.ops.insert(SyntaxKind::Slash, iface),
                "Equal" => {
                    self.ops.insert(SyntaxKind::EqualEqual, Rc::clone(&iface));
                    self.ops.insert(SyntaxKind::BangEqual, iface)
                }
                "IndexGet" => self.ops.insert(SyntaxKind::LeftBracket, iface),
                "IndexSet" => self.ops.insert(SyntaxKind::RightBracket, iface),
                _ => None,
            };
        }
    }

    /// Sets the main fn. Returns success, None indicates that
    /// a main function already existed
    pub(crate) fn set_main_fn(&mut self, func: &MutRc<Function>) -> Option<()> {
        if self.main_fn.is_some() {
            None
        } else {
            self.main_fn = Some(Rc::clone(func));
            self.required_compile_fns.push(Rc::clone(&func));
            Some(())
        }
    }

    /// Report any errors or issues with intrinsics.
    pub(crate) fn validate(&mut self, library: bool) -> Res<()> {
        if !library && self.main_fn.is_none() {
            return Err(Error {
                index: ErrorSpan::None,
                kind: GErr::E101,
            });
        }
        Ok(())
    }
}
