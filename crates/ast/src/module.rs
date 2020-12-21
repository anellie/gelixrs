/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use common::{ModPath, ModulePath};
use parser::ParseResult;
use std::rc::Rc;

/// A module, containing both the CST and AST.
/// Simply a file at this stage.
#[derive(Debug)]
pub struct Module {
    pub path: ModulePath,
    pub src: Rc<String>,
    pub cst: ParseResult, // TODO: AST
}

impl Module {
    pub fn new(path: &ModPath, src: &Rc<String>, cst: ParseResult) -> Self {
        Self {
            path: Rc::new(path.clone()),
            src: Rc::clone(src),
            cst,
        }
    }
}
