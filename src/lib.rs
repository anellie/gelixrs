/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/23/19 5:56 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

#![feature(bind_by_move_pattern_guards)]
#![feature(associated_type_bounds)]
#![feature(option_flattening)]

#[macro_use]
#[cfg(test)]
extern crate lazy_static;

pub mod ast;
pub mod codegen;
pub mod parser;
pub mod lexer;

#[cfg(test)]
pub mod tests;

use inkwell::module::Module;
use ast::declaration::DeclarationList;

pub fn parse_source(code: &String) -> Option<DeclarationList> {
    let lexer = lexer::Lexer::new(code);
    let parser = parser::Parser::new(lexer);
    parser.parse()
}

pub fn compile_ir(mut declarations: DeclarationList) -> Option<Module> {
    let mut resolver = codegen::resolver::Resolver::new();
    resolver.resolve(&mut declarations)?;
    let generator = resolver.into_generator();
    Some(generator.generate(declarations))
}
