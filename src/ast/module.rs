/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/8/19, 5:01 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::{Class, Enum, FuncSignature, Function};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug)]
pub struct Module {
    pub name: Rc<String>,
    pub content: ModuleContent
}

#[derive(Debug, Default)]
pub struct FileModule {
    pub classes: Vec<Class>,
    pub enums: Vec<Enum>,
    pub ext_functions: Vec<FuncSignature>,
    pub functions: Vec<Function>,
}

#[derive(Debug)]
pub enum ModuleContent {
    Submodules(HashMap<Rc<String>, Module>),
    File(FileModule)
}