/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:54 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use crate::{
    lexer::token::{TType, TType::EndOfFile, Token},
    Error, ModulePath,
};
use std::collections::VecDeque;

mod parsing;

/// A parser that turns a stream of [Token]s into an AST.
pub struct Parser {
    /// The path of the module this parser is parsing.
    /// Required for error display.
    module_path: Rc<ModulePath>,

    /// The tokens left to parse.
    tokens: VecDeque<Token>,

    /// The line of the token before the current one.
    previous_line: usize,

    /// If an error occurred while creating a declaration, it will be put in this Vec.
    /// If it is empty, parsing was successful.
    errors: Vec<Error>,

    /// Stores the modifiers of the current global declaration.
    modifiers: Vec<Token>,
}

/// This impl block contains all 'helper' functions of the parser.
/// These functions do not generate AST themselves, and are only used by other functions
/// to manipulate the stream of tokens.
impl Parser {
    /// Checks if the current token is the given type. If yes, it consumes it.
    fn matches(&mut self, t_type: TType) -> bool {
        let matches = self.check(t_type);
        if matches {
            self.advance();
        }
        matches
    }

    /// Same as `match_token`, but checks for multiple types. Returns the token consumed.
    fn match_tokens(&mut self, types: &[TType]) -> Option<Token> {
        if types.iter().any(|&t| self.check(t)) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// Consumes the current token if it is the type given.
    /// Will return None if the token was not the one that was expected.
    fn consume(&mut self, t_type: TType, message: &'static str) -> Option<Token> {
        if self.check(t_type) {
            Some(self.advance())
        } else {
            self.error_at_current(message);
            None
        }
    }

    /// Same as consume, but consumes a separator (`;` or newline).
    /// Also does not return a token, since newlines are not tokens.
    /// (This special function is needed because of this)
    fn consume_separator(&mut self, message: &'static str) -> Option<()> {
        if self.matches(TType::Semicolon) || self.previous_line != self.current_line() {
            Some(())
        } else {
            self.error_at_current(message);
            None
        }
    }

    /// Advances to the next token, returning the previous one.
    /// Advancing after the end will simply return `EndOfFile` tokens indefinitely.
    fn advance(&mut self) -> Token {
        self.previous_line = self.current_line();
        self.tokens
            .pop_front()
            .unwrap_or_else(|| Token::eof_token(self.current_line() + 1))
    }

    /// Is the current token the given token?
    fn check(&self, t_type: TType) -> bool {
        self.current() == t_type
    }

    /// Is the next token the given token?
    fn check_next(&mut self, t_type: TType) -> bool {
        self.peek() == t_type
    }

    /// Same as check, but checks for separators between expressions (`;` or newline)
    fn check_separator(&mut self) -> bool {
        self.check(TType::Semicolon) || self.previous_line != self.current_line()
    }

    /// Is the parser at the end of the token stream?
    fn is_at_end(&self) -> bool {
        self.current() == TType::EndOfFile
    }

    fn current(&self) -> TType {
        self.tokens.get(0).map(|t| t.t_type).unwrap_or(EndOfFile)
    }

    fn current_line(&self) -> usize {
        self.tokens
            .get(0)
            .map(|t| t.line)
            .unwrap_or(self.previous_line)
    }

    fn current_full(&self) -> Token {
        self.tokens
            .get(0)
            .cloned()
            .unwrap_or_else(|| Token::eof_token(self.previous_line))
    }

    fn peek(&self) -> TType {
        self.tokens.get(1).map(|t| t.t_type).unwrap_or(EndOfFile)
    }

    /// Causes an error at the current token with the given message.
    /// Will set appropriate state.
    /// Returns None; allows returning from calling function with ?
    fn error_at_current(&mut self, message: &str) -> Option<()> {
        let error = Error::new(
            &self.current_full(),
            "Parser",
            message.to_string(),
            &self.module_path,
        );
        self.errors.push(error);
        None
    }

    /// Will attempt to sync after an error to allow compilation to continue.
    /// This allows displaying more than 1 error at a time.
    /// To re-sync, the parser looks for tokens that could indicate the start of a new declaration.
    fn synchronize(&mut self) {
        // Prevents not properly syncing when a declaration was inside another one
        self.advance();

        while !self.is_at_end() {
            match self.current() {
                TType::Impl | TType::Import | TType::Class | TType::Enum => return,
                _ => (),
            }
            self.advance();
        }
    }

    /// Creates a new parser for parsing the given tokens.
    pub fn new(tokens: VecDeque<Token>, module_path: Rc<ModulePath>) -> Parser {
        Parser {
            module_path,
            tokens,

            previous_line: 0,

            errors: vec![],
            modifiers: vec![],
        }
    }
}
