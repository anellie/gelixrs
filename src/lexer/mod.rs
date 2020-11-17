/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 6:50 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::{convert::TryInto, iter::FromIterator, rc::Rc};

use crate::{
    ast::module::ModulePath,
    error::{Error, Res},
};
use smol_str::SmolStr;
use std::collections::VecDeque;
use token::{TType, Token};

pub mod token;

type LRes = Res<Token>;

/// A lexer is an iterator that turns gelix source code into [Token]s.
/// It returns the next token with [next], and allows temporarily
/// looking at the next token with [peek].
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

    /// The path of the module this lexer is lexing.
    /// Required for error display.
    module_path: Rc<ModulePath>,
}

impl Lexer {
    /// Consume the lexer, producing all tokens.
    pub fn consume(mut self) -> Res<VecDeque<Token>> {
        let mut toks = VecDeque::with_capacity(300);
        while let Some(token) = self.next_token()? {
            toks.push_back(token)
        }
        Ok(toks)
    }

    fn next_token(&mut self) -> Res<Option<Token>> {
        self.skip_whitespace()?;
        self.start = self.current;

        let ch = self.advance();
        ch.map(|ch| {
            Ok(match ch {
                // Single-char
                '(' => self.make_token(TType::LeftParen),
                ')' => self.make_token(TType::RightParen),
                '[' => self.make_token(TType::LeftBracket),
                ']' => self.make_token(TType::RightBracket),
                '{' => self.make_token(TType::LeftBrace),
                '}' => self.make_token(TType::RightBrace),
                ';' => self.make_token(TType::Semicolon),
                ',' => self.make_token(TType::Comma),
                '.' => self.make_token(TType::Dot),
                '+' => self.make_token(TType::Plus),
                '*' => self.make_token(TType::Star),
                '/' => self.make_token(TType::Slash),
                '~' => self.make_token(TType::Tilde),
                '&' => self.make_token(TType::AndSym),

                // Double-char
                '!' => self.check_double_token('=', TType::BangEqual, TType::Bang),
                '=' => self.check_double_token('=', TType::EqualEqual, TType::Equal),
                '<' => self.check_double_token('=', TType::LessEqual, TType::Less),
                '>' => self.check_double_token('=', TType::GreaterEqual, TType::Greater),
                '-' => self.check_double_token('>', TType::Arrow, TType::Minus),
                ':' => self.check_double_token(':', TType::ColonColon, TType::Colon),

                // Literals
                '"' => self.string()?,
                '\'' => self.ch()?,
                _ if ch.is_ascii_digit() => self.number(),

                // Identifiers/Keywords
                _ if (ch.is_alphabetic() || ch == '_') => self.identifier(),

                _ => return Err(self.error("Unexpected symbol.")),
            })
        })
        .transpose()
    }

    /// Matches the next char to check for double-char tokens. Will emit token based on match.
    fn check_double_token(&mut self, next: char, matched: TType, not_matched: TType) -> Token {
        let token = if self.match_next(next) {
            matched
        } else {
            not_matched
        };
        self.make_token(token)
    }

    /// Creates an identifier or keyword token.
    fn identifier(&mut self) -> Token {
        while self.peek_ch().is_alphanumeric() || self.check('_') {
            self.advance();
        }
        let mut token = self.make_token(TType::Identifier);

        token.t_type = match &token.lexeme[..] {
            "and" => TType::And,
            "break" => TType::Break,
            "case" => TType::Case,
            "class" => TType::Class,
            "construct" => TType::Construct,
            "else" => TType::Else,
            "enum" => TType::Enum,
            "error" => TType::Error,
            "export" => TType::Export,
            "ext" => TType::Ext,
            "false" => TType::False,
            "for" => TType::For,
            "from" => TType::From,
            "func" => TType::Func,
            "if" => TType::If,
            "impl" => TType::Impl,
            "import" => TType::Import,
            "in" => TType::In,
            "interface" => TType::Interface,
            "is" => TType::Is,
            "new" => TType::New,
            "or" => TType::Or,
            "return" => TType::Return,
            "strong" => TType::Strong,
            "to" => TType::To,
            "true" => TType::True,
            "val" => TType::Val,
            "var" => TType::Var,
            "when" => TType::When,

            "public" => TType::Public,
            "private" => TType::Private,
            "extern" => TType::Extern,
            "variadic" => TType::Variadic,

            _ => TType::Identifier,
        };

        token
    }

    /// Creates a Int or Float token
    fn number(&mut self) -> Token {
        while self.peek_ch().is_ascii_digit() {
            self.advance();
        }

        if self.check('.') && self.peek_twice().is_ascii_digit() {
            self.advance();
            while self.peek_ch().is_ascii_digit() {
                self.advance();
            }
            self.match_next('f');
            self.make_token(TType::Float)
        } else {
            if self.match_next('i') || self.match_next('u') {
                while self.peek_ch().is_ascii_alphanumeric() {
                    self.advance();
                }
            }
            self.make_token(TType::Int)
        }
    }

    /// Creates a string token
    fn string(&mut self) -> LRes {
        let start_line = self.line;
        while !self.check('"') && !self.is_at_end() {
            if self.check('\n') {
                self.line += 1;
                self.line_index = 0;
            }
            self.advance();
        }

        if self.is_at_end() {
            Err(self.error("Unterminated string!"))
        } else {
            // Ensure the quotes are not included in the literal
            self.start += 1;
            let token = self.str_escape_seq()?;
            self.advance();
            Ok(token)
        }
    }

    /// Replace all escape sequences inside a string literal with their proper char
    /// and return either an error or the finished string token
    fn str_escape_seq(&mut self) -> LRes {
        for i in self.start..self.current {
            if self.char_at(i) == '\\' {
                self.chars.remove(i);
                self.current -= 1;
                if self.chars.len() == i {
                    return Err(self.error("Unterminated string!"));
                }

                self.chars[i] = match self.char_at(i) {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '0' => '\0',
                    '"' => '"',

                    'u' => {
                        let mut chars = Vec::with_capacity(6);
                        while self.char_at(i + 1).is_ascii_hexdigit() {
                            chars.push(self.chars.remove(i + 1));
                            self.current -= 1;
                        }
                        u32::from_str_radix(&String::from_iter(chars), 16)
                            .unwrap()
                            .try_into()
                            .unwrap()
                    }

                    _ => return Err(self.error("Unknown escape sequence.")),
                }
            }
        }
        Ok(self.make_token(TType::String))
    }

    /// Creates a char token
    fn ch(&mut self) -> LRes {
        self.advance();
        if self.match_next('\'') {
            Ok(self.make_token(TType::Char))
        } else {
            self.advance();
            Err(self.error("Unterminated char literal!"))
        }
    }

    /// Creates a token based on the current position of self.start and self.current
    fn make_token(&mut self, t_type: TType) -> Token {
        Token {
            t_type,
            // TODO Any way to avoid this allocation? probably not without fiddling with smol_str internals
            lexeme: SmolStr::new(
                self.chars[(self.start)..(self.current)]
                    .iter()
                    .collect::<String>(),
            ),
            index: self.line_index,
            line: self.line,
        }
    }

    /// Creates an error with the given message at the current location
    fn error(&mut self, message: &'static str) -> Error {
        Error {
            line: self.line,
            start: self.line_index,
            len: self.current - self.start,
            producer: "Lexer",
            message: message.to_string(),
            module: Rc::clone(&self.module_path),
        }
    }

    /// Skips all whitespace and comments
    fn skip_whitespace(&mut self) -> Result<(), Error> {
        loop {
            match self.peek_ch() {
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
                            return Err(self.error("Unterminated comment"));
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
        self.peek_ch() == expected
    }

    /// Checks if the next 2 char match.
    fn check_two(&self, expected: char, expected_second: char) -> bool {
        self.peek_ch() == expected && self.peek_twice() == expected_second
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
    fn peek_ch(&self) -> char {
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
    pub fn new(source: &Rc<String>, path: &Rc<ModulePath>) -> Lexer {
        let chars: Vec<char> = source.chars().collect();
        Lexer {
            chars,
            start: 0,
            current: 0,
            line: 1,
            line_index: 0,
            module_path: Rc::clone(path),
        }
    }
}
