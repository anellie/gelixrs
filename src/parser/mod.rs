mod parser;

use super::lexer::{
    token::{Token, Type},
    Lexer,
};
use std::{iter::Peekable, mem};

/// A parser that turns a stream of [Token]s into an AST.
pub struct Parser {
    /// The token stream used.
    tokens: Peekable<Lexer>,

    /// The token currently being processed.
    current: Token,
    /// The line of the token before the current one.
    previous_line: usize,

    /// If an error occurred while creating a statement.
    /// Will signal the compiler that it should not compile.
    had_error: bool,
}

/// This impl block contains all 'helper' functions of the parser.
/// These functions do not generate AST themselves, and are only used by other functions
/// to manipulate the stream of tokens.
impl Parser {
    /// Checks if the current token is the given type. If yes, it consumes it.
    fn match_token(&mut self, t_type: Type) -> bool {
        let matches = self.check(t_type);
        if matches {
            self.advance();
        }
        matches
    }

    /// Same as [match_token], but checks for multiple types. Returns the token consumed.
    fn match_tokens(&mut self, types: &[Type]) -> Option<Token> {
        if types.iter().any(|&t| self.check(t)) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// Consumes the current token if it is the type given.
    /// Will return None if the token was not the one that was expected.
    fn consume(&mut self, t_type: Type, message: &'static str) -> Option<Token> {
        if self.check(t_type) {
            Some(self.advance())
        } else {
            self.error_at_current(message);
            None
        }
    }

    /// Same as consume, but consumes semicolons or newlines.
    /// Also does not return a token, since newlines are not tokens.
    /// (This special function is needed because of this)
    fn consume_semi_or_nl(&mut self, message: &'static str) {
        if self.check(Type::Semicolon) {
            self.advance();
        } else if self.previous_line == self.current.line {
            self.error_at_current(message); // No newline
        }
    }

    /// Sets self.current to the next token and returns the last token.
    /// If at the end of tokens, self.current is set to an EndOfFile token.
    /// Advancing after the end will simply return EndOfFile tokens indefinitely.
    fn advance(&mut self) -> Token {
        self.previous_line = self.current.line;

        let old_token = if let Some(next) = self.tokens.next() {
            mem::replace(&mut self.current, next)
        } else {
            let line = self.current.line + 1;
            mem::replace(&mut self.current, Token::eof_token(line))
        };

        if self.check(Type::ScanError) {
            self.lexer_error();
        }

        old_token
    }

    /// Is the current token the given token?
    fn check(&mut self, t_type: Type) -> bool {
        self.current.t_type == t_type
    }

    /// Same as check, but checks for ; or newlines
    /// (This special function is needed since newlines are not a token)
    fn check_semi_or_nl(&mut self) -> bool {
        self.check(Type::Semicolon) || self.previous_line != self.current.line
    }

    /// Is the parser at the end of the token stream?
    fn is_at_end(&self) -> bool {
        self.current.t_type == Type::EndOfFile
    }

    /// Causes an error at the current token with the given message.
    /// Will set appropriate state.
    /// Returns None; allows returning from calling function with ?
    fn error_at_current(&mut self, message: &str) -> Option<()> {
        eprintln!(
            "[Line {}][Token '{}' / {:?}] {}",
            self.current.line, self.current.lexeme, self.current.t_type, message
        );
        self.had_error = true;
        None
    }

    /// Reports an error produced by the lexer.
    fn lexer_error(&mut self) {
        eprintln!(
            "[Line {}] Lexer error: {}",
            self.current.line, self.current.lexeme
        );
        self.had_error = true;
    }

    /// Will attempt to sync after an error to allow compilation to continue.
    /// This allows displaying more than 1 error at a time.
    /// To re-sync, the parser looks for tokens that could indicate the start of a new declaration.
    fn synchronize(&mut self) {
        // Prevents not properly syncing when a declaration was inside another one
        self.advance();

        while !self.is_at_end() {
            match self.current.t_type {
                Type::ExFn | Type::Class | Type::Func | Type::Enum => return,
                _ => (),
            }
            self.advance();
        }
    }

    /// Creates a new parser for parsing the given tokens.
    pub fn new(tokens: Lexer) -> Parser {
        let mut parser = Parser {
            tokens: tokens.peekable(),

            current: Token::eof_token(0),
            previous_line: 0,

            had_error: false,
        };

        // Set state correctly.
        parser.advance();
        parser
    }
}
