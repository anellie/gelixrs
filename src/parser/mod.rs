mod helpers;
mod parser;

use super::{token::Token, tokenizer::Tokenizer};

/// A parser that turns a stream of [Token]s into an AST.
pub struct Parser<'p> {
    // The token stream used.
    tokens: Tokenizer<'p>,

    // The token currently being processed.
    current: Token<'p>,
    // If there was a newline token before the current token.
    // This is somewhat inelegant, but the simplest solution I could think of.
    hit_newline: bool,

    // If an error occured while creating a statement. Will signal the compiler that it should not compile.
    had_error: bool,
    // If the parser is waiting to 'sync' after a parsing error occured.
    waiting_for_sync: bool,
}
