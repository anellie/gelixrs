use super::tokenizer::Tokenizer;

/// A parser that turns a stream of [Token]s into an AST.
struct Parser<'p> {
    tokens: Tokenizer<'p>
}

impl<'p> Parser<'p> {
    /// Parses the tokens and returns a full AST. TODO: Implement
    pub fn parse() {

    }

    /// Creates a new parser for parsing the given tokens.
    pub fn new(tokens: Tokenizer<'p>) -> Parser<'p> {
        Parser {
            tokens: tokens
        }
    }
}