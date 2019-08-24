/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/12/19 8:35 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

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
