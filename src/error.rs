/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/15/19 2:07 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::fmt::{Display, Error as FmtErr, Formatter};
use std::rc::Rc;

use crate::ast::module::ModulePath;
use crate::lexer::token::Token;

pub type Res<T> = Result<T, Error>;

/// A struct for a list of errors that occurred along with the source.
pub struct Errors(pub Vec<Error>, pub String);

impl Display for Errors {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtErr> {
        for err in self.0.iter() {
            writeln!(f, "{}", err.to_string(&self.1))?;
        }
        Ok(())
    }
}

/// An error produced by all parts of the compiler.
pub struct Error {
    pub line: usize,
    pub start: usize,
    pub len: usize,
    pub producer: &'static str,
    pub message: String,
    pub module: Rc<ModulePath>,
}

impl Error {
    pub fn new(
        tok: &Token,
        producer: &'static str,
        message: String,
        module: &Rc<ModulePath>
    ) -> Error {
        Error {
            line: tok.line,
            start: tok.index - tok.lexeme.len(),
            len: tok.lexeme.len(),
            producer,
            message,
            module: Rc::clone(module)
        }
    }

    /// Produces a nice looking string representation to be shown to the user.
    pub fn to_string(&self, source: &str) -> String {
        let mut result = format!("[{}] {}\n--> {} L{}:{}", self.producer, self.message, self.module, self.line, self.start);
        let line = source
            .lines()
            .nth(self.line - 1)
            .unwrap_or("<unexpected end of file>");
        format!(
            "{}\n     |\n{:04} | {}\n     |{}{}",
            result,
            self.line,
            line,
            std::iter::repeat(' ').take(self.start).collect::<String>(),
            std::iter::repeat('^').take(self.len).collect::<String>(),
        )
    }
}
