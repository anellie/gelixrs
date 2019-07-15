#![feature(bind_by_move_pattern_guards)]

#[macro_use]
extern crate plain_enum;

pub mod ast;
pub mod parser;
pub mod lexer;

use std::{fs, process};

fn compile_and_run(code: String) {
    let lexer = lexer::Lexer::new(&code);
    let mut parser = parser::Parser::new(lexer);

    for statement in parser.parse() {
        println!("{:#?}", statement);
    }
}

pub fn run_file(path: &str) {
    let file = fs::read_to_string(path);
    match file {
        Ok(input) => compile_and_run(input),
        Err(_) => {
            println!("Failed to read file.");
            process::exit(74);
        }
    };
}