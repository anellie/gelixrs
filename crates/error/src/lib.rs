/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{
    fmt::{Display, Error as FmtErr, Formatter},
    rc::Rc,
};

use ansi_term::{
    ANSIString, ANSIStrings,
    Color::{Blue, Red},
    Style,
};
use lexer::{Lexer, Span};
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
            writeln!(f, "{}\n", err.to_string(self.src.as_ref(), &self.origin))?;
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
#[derive(Debug)]
pub struct Error {
    pub index: ErrorSpan,
    pub code: &'static str,
    pub message: String,
}

impl Error {
    /// Produces a nice looking string representation to be shown to the user.
    pub fn to_string<'a>(&self, source: Option<&'a Rc<String>>, origin: &str) -> String {
        let regular = Style::new();
        let bold = regular.bold();
        let dimmed = regular.dimmed();
        let italic = regular.italic();
        let red_ul = Red.underline();

        if let Some(source) = source {
            let span = self.index.get_span(source);
            let (line, start, len) = span_to_info(source, span);

            let result = format!(
                "\n{}: {}\n{} {} L{}:{}",
                Red.bold().paint(format!("Error[{}]", self.code)),
                bold.paint(&self.message),
                Blue.dimmed().paint("-->"),
                italic.paint(origin),
                line,
                start
            );

            let prev_line = source.lines().nth(line.wrapping_sub(2)).unwrap_or("");
            let next_line = source.lines().nth(line).unwrap_or("");
            let line_str = source
                .lines()
                .nth(line.wrapping_sub(1))
                .unwrap_or("<unexpected end of file>");

            let line_start = line_str.chars().take(start - 1).collect::<String>();
            let line_marked = line_str
                .chars()
                .skip(start - 1)
                .take(len)
                .collect::<String>();
            let line_end = line_str.chars().skip(start + len - 1).collect::<String>();

            let formatted: &[ANSIString<'a>] = &[
                regular.paint(result),
                dimmed.paint(format!("\n     |\n{:4} | ", line - 1)),
                regular.paint(prev_line),
                dimmed.paint(format!("\n{:4} | ", line)),
                regular.paint(line_start),
                red_ul.paint(line_marked),
                regular.paint(line_end),
                dimmed.paint(format!("\n{:4} | ", line + 1)),
                regular.paint(next_line),
                dimmed.paint("\n     |"),
            ];

            ANSIStrings(formatted).to_string()
        } else {
            format!(
                "\n{}: {}\n{} {}",
                Red.bold().paint("Error"),
                bold.paint(&self.message),
                Blue.dimmed().paint("-->"),
                italic.paint(origin),
            )
        }
    }
}

fn span_to_info(src: &str, span: Span) -> (usize, usize, usize) {
    let (line, line_offset) = src[0..span.start]
        .lines()
        .rev()
        .skip(1)
        .fold((0, 0), |(lc, offs), line| (lc + 1, offs + line.len() + 1));
    (
        line + 1,
        span.start - line_offset + 1,
        span.end - span.start,
    )
}

#[derive(Debug)]
pub enum ErrorSpan {
    Token(usize),
    None,
}

impl ErrorSpan {
    fn get_span(&self, src: &str) -> Span {
        match self {
            Self::Token(index) => {
                let mut lex = Lexer::new(src);
                for _ in 0..(*index - 1) {
                    lex.next();
                }
                lex.span()
            }

            Self::None => panic!("Not supposed to have a source"),
        }
    }
}
