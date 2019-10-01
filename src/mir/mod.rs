/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/2/19 1:22 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use nodes::{MIRClass, MIRVariable};

use crate::mir::nodes::MIRInterface;
use crate::ModulePath;

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
    pub classes: HashMap<Rc<String>, MutRc<MIRClass>>,
    pub interfaces: HashMap<Rc<String>, MutRc<MIRInterface>>,
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
