#![feature(bind_by_move_pattern_guards)]

#[macro_use]
extern crate plain_enum;

pub mod ast;
pub mod parser;
pub mod token;
pub mod tokenizer;

use std::fs;

fn main() {
    let file = fs::read_to_string("examples/testing.gelix").unwrap();
    let tokenizer = tokenizer::Tokenizer::new(&file);
    let mut parser = parser::Parser::new(tokenizer);

    for statement in parser.parse() {
        println!("{:#?}", statement);
    }
}
