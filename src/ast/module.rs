/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/4/19 9:44 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::ast::declaration::{Class, Enum, FuncSignature, Function, IFaceImpl, Interface};
use crate::lexer::token::Token;
use crate::ModulePath;

#[derive(Debug, Default)]
pub struct Module {
    pub path: Rc<ModulePath>,

    pub classes: Vec<Class>,
    pub interfaces: Vec<Interface>,
    pub iface_impls: Vec<IFaceImpl>,
    pub enums: Vec<Enum>,
    pub ext_functions: Vec<FuncSignature>,
    pub functions: Vec<Function>,
    pub imports: Vec<Import>,
}

impl Module {
    pub fn new(path: &ModulePath) -> Self {
        Self {
            path: Rc::new(path.clone()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: ModulePath,
    pub symbol: Token,
}
