/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/12/19, 5:45 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::{Class, Enum, FuncSignature, Function};
use crate::ModulePath;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct Module {
    pub path: Rc<ModulePath>,

    pub classes: Vec<Class>,
    pub enums: Vec<Enum>,
    pub ext_functions: Vec<FuncSignature>,
    pub functions: Vec<Function>,
    pub imports: Vec<Import>
}

impl Module {
    pub fn new(path: &ModulePath) -> Self {
        Self {
            path: Rc::new(path.clone()),
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub struct Import {
    pub path: ModulePath,
    pub symbol: Rc<String>
}
