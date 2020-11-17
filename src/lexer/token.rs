/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/27/19 1:32 AM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use smol_str::SmolStr;

/// A token in the gelix language. These are produced by a lexer.
/// Cloning a Token is cheap, since the lexemes use smol_str.
#[derive(Debug, Clone)]
pub struct Token {
    /// The type of the token.
    pub t_type: TType,
    /// The lexeme of the token. Does not include escape chars (ex. String lexeme is <i>I'm a string!</i>)
    pub lexeme: SmolStr,
    /// The index of the last char of the token inside the source.
    /// This is used for error reporting.
    pub index: usize,
    /// The line of the token.
    pub line: usize,
    /// Length of the token, separate since some tokens are modified.
    pub len: usize,
}

impl Token {
    pub fn eof_token(line: usize) -> Token {
        Token {
            t_type: TType::EndOfFile,
            lexeme: SmolStr::new_inline("\0"),
            index: 1,
            line,
            len: 0,
        }
    }

    // Warning: `lexeme` must not be over 22 bytes!
    // See `SmolStr::new_inline`.
    pub fn generic_identifier(lexeme: &str) -> Token {
        let index = lexeme.len();
        Token {
            t_type: TType::Identifier,
            lexeme: SmolStr::new_inline(lexeme),
            index: index + 1,
            line: 1,
            len: index,
        }
    }

    pub fn generic_token(token: TType) -> Token {
        Token {
            t_type: token,
            lexeme: SmolStr::new_inline(""),
            index: 1,
            line: 1,
            len: 0,
        }
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.lexeme == other.lexeme
    }
}

/// All types of tokens available. Most are keywords or special chars.
/// The `ScanError` token is a special token signifying a syntax error.
/// Its lexeme is an error message to be displayed to the user.
#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum TType {
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    AndSym,
    Tilde,
    Comma,
    Dot,
    Minus,
    Plus,
    Semicolon,
    Colon,
    ColonColon,
    Slash,
    Star,
    Arrow,

    Bang,
    BangEqual,
    Equal,
    EqualEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,

    Identifier,
    String,
    Int,
    Float,
    Char,

    And,
    Break,
    Case,
    Class,
    Construct,
    Else,
    Enum,
    Error,
    Export,
    Ext,
    False,
    For,
    From,
    Func,
    If,
    Impl,
    Import,
    In,
    Interface,
    Is,
    New,
    Or,
    Return,
    Strong,
    To,
    True,
    Var,
    Val,
    When,
    While,

    Public,
    Private,
    Extern,
    Variadic,

    EndOfFile,
}
