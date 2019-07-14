//! This module contains all 'helper' functions of the parser.
//! These functions do not generate AST themselves, and are only used by other functions
//! to manipulate the stream of tokens.

use super::super::{
    ast::statement::Statement,
    token::{Token, Type},
    tokenizer::Tokenizer,
};
use super::Parser;
use std::mem;

impl<'p> Parser<'p> {
    /// Parses the tokens and returns a full AST.
    pub fn parse(&mut self) -> Vec<Statement> {
        let mut statements: Vec<Statement> = Vec::new();
        self.advance();

        while !self.is_at_end() {
            if let Some(f) = self.declaration() {
                statements.push(f)
            }
        }

        statements
    }

    /// Checks if the current token is the given type. If yes, it consumes it.
    pub fn match_token(&mut self, t_type: Type) -> bool {
        let matches = self.check(t_type);
        if matches {
            self.advance();
        }
        matches
    }

    /// Same as [match_token], but checks for multiple types.
    pub fn match_tokens(&mut self, types: &[Type]) -> bool {
        types.iter().any(|&t| self.match_token(t))
    }

    /// Consumes the current token if it is the type given.
    /// Will return None if the token was not the one that was expected.
    pub fn consume(&mut self, t_type: Type, message: &'static str) -> Option<Token<'p>> {
        if self.check(t_type) {
            Some(self.advance())
        } else {
            self.error_at_current(message);
            None
        }
    }

    /// Sets self.current to the next token and returns the last token.
    /// If at the end of tokens, self.current is set to an EndOfFile token.
    /// Advancing after the end will simply return EndOfFile tokens indefinitely.
    pub fn advance(&mut self) -> Token<'p> {
        if let Some(next) = self.tokens.next() {
            let mut previous = mem::replace(&mut self.current, next);

            // TODO: This way of skipping over newlines is kinda ugly...
            self.hit_newline = false;
            if self.current.t_type == Type::Newline {
                self.hit_newline = true;
                while let Some(next) = self.tokens.next() {
                    previous = mem::replace(&mut self.current, next);
                    if self.current.t_type != Type::Newline {
                        break;
                    }
                }
            }

            previous
        } else {
            mem::replace(
                &mut self.current,
                Token {
                    t_type: Type::EndOfFile,
                    lexeme: "\0",
                    line: 0,
                },
            )
        }
    }

    /// Is the current token the given token?
    pub fn check(&mut self, t_type: Type) -> bool {
        !self.is_at_end() && self.current.t_type == t_type
    }

    /// Is the parser at the end of the token stream?
    pub fn is_at_end(&mut self) -> bool {
        self.current.t_type == Type::EndOfFile
    }

    /// Causes an error at the current token with the given message; see fn below.
    pub fn error_at_current(&mut self, message: &str) {
        self.error_at(self.current.line, message)
    }

    /// Displays an error message at the given line
    /// and sets appropriate state to allow for error recovery.
    pub fn error_at(&mut self, line: usize, message: &str) {
        if self.waiting_for_sync {
            return;
        }

        eprint!("[Line {}] Error", line);
        eprintln!(": {}", message);

        self.had_error = true;
        self.waiting_for_sync = true;
    }

    /// Will attempt to sync after an error to allow compilation to continue.
    /// This allows displaying more than 1 error at a time.
    /// To resync, the parser looks for tokens that could indicate the start of a new declaration.
    pub fn syncronize(&mut self) {
        self.waiting_for_sync = false;

        while !self.is_at_end() {
            if self.hit_newline {
                return;
            }

            match self.current.t_type {
                Type::Class
                | Type::Error
                | Type::Func
                | Type::Var
                | Type::Val
                | Type::For
                | Type::If
                | Type::Return => return,
                _ => (),
            }
        }

        self.advance();
    }

    /// Creates a new parser for parsing the given tokens.
    pub fn new(tokens: Tokenizer<'p>) -> Parser<'p> {
        Parser {
            tokens,

            current: Token {
                t_type: Type::Newline,
                lexeme: "\n",
                line: 0,
            },
            hit_newline: false,

            had_error: false,
            waiting_for_sync: false,
        }
    }
}
