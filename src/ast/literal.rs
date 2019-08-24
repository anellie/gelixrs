/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/12/19 8:35 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use std::fmt::{Display, Error, Formatter};

/// An enum containing all literals possible in Gelix.
#[derive(Debug)]
pub enum Literal {
    None,
    Bool(bool),
    Int(i64),
    Float(f32),
    Double(f64),
    Char(char),
    String(String),
    Array(Vec<Literal>),
}

impl Display for Literal {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            Literal::None => write!(f, "None"),
            Literal::Bool(val) => write!(f, "{}", val),
            Literal::Int(val) => write!(f, "{}", val),
            Literal::Float(val) => write!(f, "{}", val),
            Literal::Double(val) => write!(f, "{}", val),
            Literal::Char(val) => write!(f, "{}", val),
            Literal::String(val) => write!(f, "\"{}\"", val),
            Literal::Array(val) => write!(f, "{:?}", val),
        }
    }
}
