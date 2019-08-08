#![feature(bind_by_move_pattern_guards)]
#![feature(associated_type_bounds)]

#[macro_use]
extern crate plain_enum;

pub mod ast;
pub mod codegen;
pub mod parser;
pub mod lexer;

use std::{fs, process};

fn parse_and_print(code: String) {
    let lexer = lexer::Lexer::new(&code);
    let mut parser = parser::Parser::new(lexer);

    for statement in parser.parse() {
        println!("{:#?}", statement);
    }
}

fn compile_and_run(code: String) {
    let lexer = lexer::Lexer::new(&code);
    let mut parser = parser::Parser::new(lexer);

    if let Some(declarations) = parser.parse() {
        let mut generator = codegen::IRGenerator::new(declarations);
        generator.generate();
    } else {
        eprint!("Encountered errors during parsing. Exiting.");
    }
}

pub fn do_file(path: &str, print_and_exit: bool) {
    let file = fs::read_to_string(path);
    match file {
        Ok(input) => if print_and_exit {
            parse_and_print(input);
        } else {
            compile_and_run(input);
        },
        Err(_) => {
            println!("Failed to read file.");
            process::exit(74);
        }
    };
}