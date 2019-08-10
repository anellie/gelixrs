#![feature(bind_by_move_pattern_guards)]
#![feature(associated_type_bounds)]

#[macro_use]
extern crate plain_enum;

pub mod ast;
pub mod codegen;
pub mod parser;
pub mod lexer;

use inkwell::module::Module;
use ast::declaration::Declaration;

pub fn parse_source<'s>(code: &'s String) -> Option<Vec<Declaration<'s>>> {
    let lexer = lexer::Lexer::new(code);
    let parser = parser::Parser::new(lexer);
    parser.parse()
}

pub fn compile_ir(mut declarations: Vec<Declaration>) -> Option<Module> {
    let mut resolver = codegen::resolver::Resolver::new();
    resolver.resolve(&mut declarations)?;
    let generator = resolver.into_generator(declarations);
    generator.generate()
}
