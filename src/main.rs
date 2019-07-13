#[macro_use]
extern crate plain_enum;

pub mod ast;
pub mod parser;
pub mod token;
pub mod tokenizer;

use std::fs;

fn main() {
    let file = fs::read_to_string("docs/gelix-syntax-examples.gelix").unwrap();
    let tokenizer = tokenizer::Tokenizer::new(&file);

    for token in tokenizer {
        println!("{:?}", token);
    }
}
