/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 9/21/19 4:30 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

use token::{Token, Type};

pub mod token;

/// A lexer is an iterator that turns gelix source code into [Token]s.
pub struct Lexer {
    /// The chars of the source
    chars: Vec<char>,
    /// The start position of the token currently being scanned
    start: usize,
    /// The current position of the scan
    current: usize,
    /// The line of the current position
    line: usize,
    /// The index of the current position on the current line
    line_index: usize,
}

impl Lexer {
    /// Returns the next token, or None if at EOF.
    fn next_token(&mut self) -> Option<Token> {
        if let Err(tok) = self.skip_whitespace() {
            return Some(tok);
        }

        self.start = self.current;
        let ch = self.advance()?;

        Some(match ch {
            // Single-char
            '(' => self.make_token(Type::LeftParen),
            ')' => self.make_token(Type::RightParen),
            '[' => self.make_token(Type::LeftBracket),
            ']' => self.make_token(Type::RightBracket),
            '{' => self.make_token(Type::LeftBrace),
            '}' => self.make_token(Type::RightBrace),
            ';' => self.make_token(Type::Semicolon),
            ',' => self.make_token(Type::Comma),
            '.' => self.make_token(Type::Dot),
            '+' => self.make_token(Type::Plus),
            '*' => self.make_token(Type::Star),
            '/' => self.make_token(Type::Slash),

            // Double-char
            '!' => self.check_double_token('=', Type::BangEqual, Type::Bang),
            '=' => self.check_double_token('=', Type::EqualEqual, Type::Equal),
            '<' => self.check_double_token('=', Type::LessEqual, Type::Less),
            '>' => self.check_double_token('=', Type::GreaterEqual, Type::Greater),
            '-' => self.check_double_token('>', Type::Arrow, Type::Minus),

            // Literals
            '"' => self.string(),
            '\'' => self.ch(),
            _ if ch.is_ascii_digit() => self.number(),

            // Identifiers/Keywords
            _ if (ch.is_alphabetic() || ch == '_') => self.identifier(),

            _ => self.error_token("Unexpected symbol."),
        })
    }

    /// Matches the next char to check for double-char tokens. Will emit token based on match.
    fn check_double_token(&mut self, next: char, matched: Type, not_matched: Type) -> Token {
        let token = if self.match_next(next) {
            matched
        } else {
            not_matched
        };
        self.make_token(token)
    }

    /// Creates an identifier or keyword token.
    fn identifier(&mut self) -> Token {
        while self.peek().is_alphanumeric() || self.check('_') {
            self.advance();
        }
        let mut token = self.make_token(Type::Identifier);

        token.t_type = match &token.lexeme[..] {
            "and" => Type::And,
            "break" => Type::Break,
            "class" => Type::Class,
            "else" => Type::Else,
            "enum" => Type::Enum,
            "error" => Type::Error,
            "exfn" => Type::ExFn,
            "ext" => Type::Ext,
            "false" => Type::False,
            "for" => Type::For,
            "from" => Type::From,
            "func" => Type::Func,
            "if" => Type::If,
            "import" => Type::Import,
            "in" => Type::In,
            "None" => Type::None,
            "or" => Type::Or,
            "return" => Type::Return,
            "super" => Type::Super,
            "to" => Type::To,
            "true" => Type::True,
            "val" => Type::Val,
            "var" => Type::Var,
            "when" => Type::When,

            _ => Type::Identifier,
        };

        token
    }

    /// Creates a Int or Float token
    fn number(&mut self) -> Token {
        while self.peek().is_ascii_digit() {
            self.advance();
        }

        if self.check('.') && self.peek_twice().is_ascii_digit() {
            self.advance();
            while self.peek().is_ascii_digit() {
                self.advance();
            }
            self.match_next('f');
            self.make_token(Type::Float)
        } else {
            if self.match_next('i') {
                while self.peek().is_ascii_digit() {
                    self.advance();
                }
            }
            self.make_token(Type::Int)
        }
    }

    /// Creates a string token
    fn string(&mut self) -> Token {
        let start_line = self.line;
        while !self.check('"') && !self.is_at_end() {
            if self.check('\n') {
                self.line += 1;
                self.line_index = 0;
            }
            self.advance();
        }

        if self.is_at_end() {
            let mut token = self.error_token("Unterminated string!");
            token.line = start_line;
            token
        } else {
            // Ensure the quotes are not included in the literal
            self.start += 1;
            let token = self.make_token(Type::String);
            self.advance();
            token
        }
    }

    /// Creates a char token
    fn ch(&mut self) -> Token {
        self.advance();
        if self.match_next('\'') {
            self.make_token(Type::Char)
        } else {
            self.advance();
            self.error_token("Unterminated char literal!")
        }
    }

    /// Creates a token based on the current position of self.start and self.current
    fn make_token(&mut self, t_type: Type) -> Token {
        Token {
            t_type,
            lexeme: Rc::new(self.chars[(self.start)..(self.current)].iter().collect()),
            index: self.line_index,
            line: self.line,
        }
    }

    /// Creates a ScanError token with the given message at the current location
    fn error_token(&mut self, message: &'static str) -> Token {
        Token {
            t_type: Type::ScanError,
            lexeme: Rc::new(message.to_string()),
            index: self.line_index + message.len(),
            line: self.line,
        }
    }

    /// Skips all whitespace and comments
    fn skip_whitespace(&mut self) -> Result<(), Token> {
        loop {
            match self.peek() {
                ' ' | '\r' | '\t' => {
                    self.advance();
                }

                '\n' => {
                    self.line += 1;
                    self.line_index = 0;
                    self.advance();
                }

                '/' => match self.peek_twice() {
                    '/' => {
                        while !self.check('\n') && !self.is_at_end() {
                            self.advance();
                        }
                    }

                    '*' => {
                        self.advance();
                        let mut nest_level = 1;

                        while nest_level > 0 && !self.is_at_end() {
                            if self.check_two('*', '/') {
                                nest_level -= 1;
                            } else if self.check_two('/', '*') {
                                nest_level += 1;
                            } else if self.check('\n') {
                                self.line += 1;
                            }
                            self.advance();
                        }

                        if self.is_at_end() {
                            return Err(self.error_token("Unterminated comment"));
                        }

                        self.advance();
                    }

                    _ => return Ok(()),
                },

                _ => return Ok(()),
            }
        }
    }

    /// Is the current cursor at the EOF?
    fn is_at_end(&self) -> bool {
        self.check('\0')
    }

    /// Matches the next char and consumes it if it matches. Returns if it matched.
    fn match_next(&mut self, expected: char) -> bool {
        let matches = self.check(expected);
        if matches {
            self.advance();
        }
        matches
    }

    /// Checks if the next char matches.
    fn check(&self, expected: char) -> bool {
        self.peek() == expected
    }

    /// Checks if the next 2 char match.
    fn check_two(&self, expected: char, expected_second: char) -> bool {
        self.peek() == expected && self.peek_twice() == expected_second
    }

    /// Advances the char pointer by 1 and returns the consumed char, or None at EOF.
    fn advance(&mut self) -> Option<char> {
        if self.is_at_end() {
            None
        } else {
            self.current += 1;
            self.line_index += 1;
            Some(self.char_at(self.current - 1))
        }
    }

    /// Returns the current char without consuming it.
    fn peek(&self) -> char {
        self.char_at(self.current)
    }

    /// Returns the next char without consuming it.
    fn peek_twice(&self) -> char {
        self.char_at(self.current + 1)
    }

    /// Returns the char at the given position or \0 for OOB.
    fn char_at(&self, pos: usize) -> char {
        **self.chars.get(pos).get_or_insert(&'\0')
    }

    /// Create a new lexer for scanning the given source.
    pub fn new(source: &str) -> Lexer {
        let chars: Vec<char> = source.chars().collect();
        Lexer {
            chars,
            start: 0,
            current: 0,
            line: 1,
            line_index: 0,
        }
    }
}

impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}
