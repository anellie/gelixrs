/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/13/19, 3:45 PM.
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

    // The Rust representation of these integers can be unsigned
    // since literals themselves are always unsigned.
    // (A negative literal is just a unary negated literal)
    I8(u8),
    I16(u8),
    I32(u16),
    I64(u32),

    F32(f32),
    F64(f64),

    Char(char),
    String(Rc<String>),

    Array(Vec<Literal>),
    Closure(Rc<Function>),
}
