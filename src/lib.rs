/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/30/19 3:27 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

// https://github.com/rust-lang/rust/pull/63059
#![feature(bind_by_move_pattern_guards)]
// https://github.com/rust-lang/rust/pull/60256
#![feature(option_flattening)]

#[macro_use]
#[cfg(test)]
extern crate lazy_static;

pub mod ast;
pub mod ir;
pub mod lexer;
pub mod mir;
pub mod parser;

#[cfg(test)]
pub mod tests;

use ast::declaration::DeclarationList;
use inkwell::module::Module;

pub fn parse_source(code: &str) -> Result<DeclarationList, Vec<String>> {
    let lexer = lexer::Lexer::new(code);
    let parser = parser::Parser::new(lexer);
    parser.parse()
}

pub fn compile_ir(declarations: DeclarationList) -> Result<Module, String> {
    let mir = mir::generator::MIRGenerator::new().generate(declarations)?;
    Ok(ir::IRGenerator::new().generate(mir))
}
