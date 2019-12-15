/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 3:36 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::fmt::{Display, Error, Formatter};
use std::rc::Rc;

use crate::ast::declaration::{Class, Function, IFaceImpl, Interface};
use crate::lexer::token::Token;

#[derive(Clone, Debug, Default, PartialOrd, PartialEq, Eq, Hash)]
pub struct ModulePath(pub Vec<Rc<String>>);

impl Display for ModulePath {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.0.iter()
            .map(|rc| (&**rc).clone())
            .collect::<Vec<String>>()
            .join("/"))
    }
}

#[derive(Debug, Default)]
pub struct Module {
    pub path: Rc<ModulePath>,
    pub src: Rc<String>,

    pub classes: Vec<Class>,
    pub interfaces: Vec<Interface>,
    pub iface_impls: Vec<IFaceImpl>,
    pub functions: Vec<Function>,
    pub imports: Vec<Import>,
}

impl Module {
    pub fn new(path: &ModulePath, src: &Rc<String>) -> Self {
        Self {
            path: Rc::new(path.clone()),
            src: Rc::clone(src),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: Rc<ModulePath>,
    pub symbol: Token,
}
