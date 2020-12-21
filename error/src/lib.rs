/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    fmt::{Display, Error as FmtErr, Formatter},
    rc::Rc,
};

use lexer::Span;
use std::fmt::Debug;

pub type Res<T> = Result<T, Error>;

/// A struct for a list of errors that occurred at a given location.
pub struct Errors {
    // All errors that occurred.
    pub errors: Vec<Error>,
    // The source code of the origin, if any
    pub src: Option<Rc<String>>,
    // The origin of the error, usually a module (path), can be anything
    pub origin: String,
}

impl Display for Errors {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtErr> {
        writeln!(f, "Errors inside {}:", self.origin)?;
        for err in &self.errors {
            writeln!(f, "{}\n", err.to_string(self.src.as_ref()))?;
        }
        Ok(())
    }
}

impl Debug for Errors {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtErr> {
        <Self as Display>::fmt(self, f)
    }
}

/// An error produced by all parts of the compiler.
pub struct Error {
    pub index: ErrorSpan,
    pub code: &'static str,
    pub message: String,
}

impl Error {
    /// Produces a nice looking string representation to be shown to the user.
    pub fn to_string<'a>(&self, _source: Option<&Rc<String>>) -> String {
        /*
        let regular = Style::new();
        let bold = regular.bold();
        let dimmed = regular.dimmed();
        let italic = regular.italic();
        let red_ul = Red.underline();

        let result = format!(
            "\n{}: {}\n{} {} L{}:{}",
            Red.bold().paint("Error"),
            bold.paint(&self.message),
            Blue.dimmed().paint("-->"),
            italic.paint(&self.module.to_string()),
            self.line,
            self.start
        );

        let prev_line = source.lines().nth(self.line.wrapping_sub(2)).unwrap_or("");
        let next_line = source.lines().nth(self.line).unwrap_or("");
        let line = source
            .lines()
            .nth(self.line.wrapping_sub(1))
            .unwrap_or("<unexpected end of file>");

        let line_start = line.chars().take(self.start - 1).collect::<String>();
        let line_marked = line
            .chars()
            .skip(self.start - 1)
            .take(self.len)
            .collect::<String>();
        let line_end = line
            .chars()
            .skip(self.start + self.len - 1)
            .collect::<String>();

        let formatted: &[ANSIString<'a>] = &[
            regular.paint(result),
            dimmed.paint(format!("\n     |\n{:4} | ", self.line - 1)),
            regular.paint(prev_line),
            dimmed.paint(format!("\n{:4} | ", self.line)),
            regular.paint(line_start),
            red_ul.paint(line_marked),
            regular.paint(line_end),
            dimmed.paint(format!("\n{:4} | ", self.line + 1)),
            regular.paint(next_line),
            dimmed.paint("\n     |"),
        ];

        ANSIStrings(formatted).to_string()
                 */
        todo!()
    }
}

pub enum ErrorSpan {
    Token(usize),
    Span(Span),
}
