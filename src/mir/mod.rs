/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/12/19 9:07 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ModulePath;
use nodes::{MIRStruct, MIRVariable};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub mod generator;
pub mod nodes;

type MutRc<T> = Rc<RefCell<T>>;

fn mutrc_new<T>(value: T) -> MutRc<T> {
    Rc::new(RefCell::new(value))
}

/// A struct produced by a generator. It contains the full MIR representation of a file/module.
#[derive(Debug, Default)]
pub struct MIRModule {
    pub path: Rc<ModulePath>,
    pub types: HashMap<Rc<String>, MutRc<MIRStruct>>,
    pub functions: HashMap<Rc<String>, Rc<MIRVariable>>,
    /// Imported functions will only be declared; not defined.
    /// They are defined when modules are linked/combined at the end of IR generation.
    pub imported_func: HashMap<Rc<String>, Rc<MIRVariable>>,
}

impl MIRModule {
    pub fn new(path: Rc<ModulePath>) -> MIRModule {
        Self {
            path,
            ..Default::default()
        }
    }
}
