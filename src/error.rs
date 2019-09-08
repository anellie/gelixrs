/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/8/19, 6:09 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use crate::lexer::token::Token;

/// A collection of errors inside a file.
pub struct FileErrors {
    pub errors: Vec<Error>,
    pub source: String,
    pub file_name: String,
}

impl FileErrors {
    pub fn new(errors: Vec<Error>, source: &str, file_name: String) -> FileErrors {
        FileErrors {
            errors,
            source: source.to_string(),
            file_name,
        }
    }
}

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
    pub fn to_string(&self, source: &str) -> String {
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
