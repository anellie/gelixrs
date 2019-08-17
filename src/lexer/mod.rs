pub mod token;

use token::{Token, Type};

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
}

impl Lexer {
    /// Returns the next token, or None if at EOF.
    fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
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

            // Other
            _ if (ch.is_alphabetic() || ch == '_') => self.identifier(),
            _ if ch.is_ascii_digit() => self.number(),

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
        self.make_token(self.identifier_type())
    }

    /// Returns the correct token type for the current token. Can be a keyword or identifier.
    /// TODO: This is really ugly and hard to read.
    /// Just using a map or similar should be much easier to read with little performance impact.
    fn identifier_type(&self) -> Type {
        match self.char_at(self.start) {
            'a' => self.check_identifier_keyword(1, &['n', 'd'], Type::And),
            'n' => self.check_identifier_keyword(1, &['o', 'n', 'e'], Type::None),
            'o' => self.check_identifier_keyword(1, &['r'], Type::Or),
            'r' => self.check_identifier_keyword(1, &['e', 't', 'u', 'r', 'n'], Type::Return),
            's' => self.check_identifier_keyword(1, &['u', 'p', 'e', 'r'], Type::Super),
            'w' => self.check_identifier_keyword(1, &['h', 'e', 'n'], Type::When),
            'c' => self.check_identifier_keyword(1, &['l', 'a', 's', 's'], Type::Class),

            'i' => match self.char_at(self.start + 1) {
                'f' => self.check_identifier_keyword(2, &[], Type::If),
                'n' => self.check_identifier_keyword(2, &[], Type::In),
                _ => Type::Identifier,
            },

            'e' => match self.char_at(self.start + 1) {
                'l' => self.check_identifier_keyword(2, &['s', 'e'], Type::Else),
                'n' => self.check_identifier_keyword(2, &['u', 'm'], Type::Enum),
                'r' => self.check_identifier_keyword(2, &['r', 'o', 'r'], Type::Error),
                'x' => match self.char_at(self.start + 2) {
                    'f' => self.check_identifier_keyword(3, &['n'], Type::ExFn),
                    't' => self.check_identifier_keyword(3, &[], Type::Ext),
                    _ => Type::Identifier,
                },
                _ => Type::Identifier,
            },

            'v' => {
                if self.char_at(self.start + 1) == 'a' {
                    match self.char_at(self.start + 2) {
                        'r' => self.check_identifier_keyword(3, &[], Type::Var),
                        'l' => self.check_identifier_keyword(3, &[], Type::Val),
                        _ => Type::Identifier,
                    }
                } else {
                    Type::Identifier
                }
            }

            'f' => match self.char_at(self.start + 1) {
                'a' => self.check_identifier_keyword(2, &['l', 's', 'e'], Type::False),
                'o' => self.check_identifier_keyword(2, &['r'], Type::For),
                'u' => self.check_identifier_keyword(2, &['n', 'c'], Type::Func),
                _ => Type::Identifier,
            },

            't' => match self.char_at(self.start + 1) {
                'a' => self.check_identifier_keyword(2, &['k', 'e'], Type::Take),
                'r' => self.check_identifier_keyword(2, &['u', 'e'], Type::True),
                _ => Type::Identifier,
            },

            _ => Type::Identifier,
        }
    }

    /// Helper function for [identifier_type], checks if rest of keyword matches
    fn check_identifier_keyword(&self, start: usize, pattern: &[char], t_type: Type) -> Type {
        // Loop all chars in the pattern; if one does not match it is NOT the keyword
        for ch in 0..pattern.len() {
            if self.char_at(self.start + start + ch) != pattern[ch] {
                return Type::Identifier;
            }
        }

        // If the next char is alphabetic, it is an identifier that starts with a keyword ('superb')
        // If it is not (other token, whitespace, etc.) it is the keyword type
        if self
            .char_at(self.start + start + pattern.len())
            .is_alphabetic()
        {
            Type::Identifier
        } else {
            t_type
        }
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
            self.make_token(Type::Float)
        } else {
            self.make_token(Type::Int)
        }
    }

    /// Creates a string token
    fn string(&mut self) -> Token {
        let start_line = self.line;
        while !self.check('"') && !self.is_at_end() {
            if self.check('\n') {
                self.line += 1;
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
            lexeme: self.chars[(self.start)..(self.current)]
                .into_iter()
                .collect(),
            line: self.line,
        }
    }

    /// Creates a ScanError token with the given message at the current location
    fn error_token(&mut self, message: &'static str) -> Token {
        Token {
            t_type: Type::ScanError,
            lexeme: message.to_string(),
            line: self.line,
        }
    }

    /// Skips all whitespace and comments
    fn skip_whitespace(&mut self) {
        loop {
            match self.peek() {
                ' ' | '\r' | '\t' => {
                    self.advance();
                }

                '\n' => {
                    self.line += 1;
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

                        while nest_level > 0 {
                            if self.check_two('*', '/') {
                                nest_level -= 1;
                            } else if self.check_two('/', '*') {
                                nest_level += 1;
                            } else if self.check('\n') {
                                self.line += 1;
                            }
                            self.advance();
                        }

                        self.advance();
                    }

                    _ => return,
                },

                _ => return,
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
        }
    }
}

impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}
