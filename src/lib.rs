#![feature(bind_by_move_pattern_guards)]
#![feature(associated_type_bounds)]

#[macro_use]
extern crate plain_enum;

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
    let generator = resolver.into_generator(declarations);
    generator.generate()
}
