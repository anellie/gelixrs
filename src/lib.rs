/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/30/19 7:18 PM.
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
use lexer::token::Token;

type Res<T> = Result<T, Error>;

/// An error produced by all parts of the compiler.
pub struct Error {
    pub lines: (usize, usize),
    pub start: usize,
    pub len: usize,
    pub producer: &'static str,
    pub message: String,
}

impl Error {
    pub fn new(
        start_tok: &Token,
        end_tok: &Token,
        producer: &'static str,
        message: String,
    ) -> Error {
        Error {
            lines: (start_tok.line, end_tok.line),
            start: start_tok.index - start_tok.lexeme.len(),
            len: end_tok.index - (start_tok.index - start_tok.lexeme.len()),
            producer,
            message,
        }
    }

    /// Produces a nice looking string representation to be shown to the user.
    pub fn to_string(&self, source: &String) -> String {
        let (start_line, end_line) = self.lines;
        let mut result = format!("[{}] {}", self.producer, self.message);

        if start_line == 0 {
            // Line 0 means there is nothing relevant to show
            result = format!("{}\n  ?? | <unknown>", result);
        } else if start_line == end_line {
            let line = source
                .lines()
                .nth(start_line - 1)
                .unwrap_or("<end of file>");
            result = format!(
                "{}\n     |\n{:04} | {}\n     |{}{}",
                result,
                start_line,
                line,
                std::iter::repeat(' ').take(self.start).collect::<String>(),
                std::iter::repeat('^').take(self.len).collect::<String>(),
            )
        } else {
            for (i, line) in source
                .lines()
                .skip(start_line - 1)
                .take((end_line - start_line) + 1)
                .enumerate()
            {
                result = format!("{}\n{:04} | {}", result, (i + start_line), line);
            }
        }

        result
    }
}

pub fn parse_source(code: &str) -> Result<DeclarationList, Vec<Error>> {
    let lexer = lexer::Lexer::new(code);
    let parser = parser::Parser::new(lexer);
    parser.parse()
}

pub fn compile_ir(declarations: DeclarationList) -> Res<Module> {
    let mir = mir::generator::MIRGenerator::new().generate(declarations)?;
    Ok(ir::IRGenerator::new().generate(mir))
}
