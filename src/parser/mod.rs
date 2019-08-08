mod helpers;
mod parser;

use super::lexer::{token::Token, Lexer};
use std::iter::Peekable;

/// A parser that turns a stream of [Token]s into an AST.
pub struct Parser<'p> {
    // The token stream used.
    tokens: Peekable<Lexer<'p>>,

    // The token currently being processed.
    current: Token<'p>,
    // The line of the token before the current one.
    previous_line: usize,

    // If an error occured while creating a statement. Will signal the compiler that it should not compile.
    had_error: bool,
    // If the parser is waiting to 'sync' after a parsing error occured.
    waiting_for_sync: bool,
}
