/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/8/19, 6:09 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::ast::declaration::Function;
use std::rc::Rc;

/// An enum containing all literals possible in Gelix.
#[derive(Debug, Clone)]
pub enum Literal {
    Any,
    None,
    Bool(bool),
    Int(i64),
    Float(f32),
    Double(f64),
    Char(char),
    String(Rc<String>),
    Array(Vec<Literal>),
    Closure(Rc<Function>),
}
