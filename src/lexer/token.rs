/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 11/26/19 6:54 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use std::rc::Rc;

/// A token in the gelix language. These are produced by a lexer.
/// Cloning a Token is cheap, since the lexemes are refcounted.
#[derive(Debug, Clone)]
pub struct Token {
    /// The type of the token.
    pub t_type: TType,
    /// The lexeme of the token. Does not include escape chars (ex. String lexeme is <i>I'm a string!</i>)
    pub lexeme: Rc<String>,
    /// The index of the last char of the token inside the source.
    /// This is used for error reporting.
    pub index: usize,
    /// The line of the token.
    pub line: usize,
}

impl Token {
    pub fn eof_token(line: usize) -> Token {
        Token {
            t_type: TType::EndOfFile,
            lexeme: Rc::new("\0".to_string()),
            index: 1,
            line,
        }
    }

    pub fn generic_identifier(lexeme: String) -> Token {
        let index = lexeme.len();
        Token {
            t_type: TType::Identifier,
            lexeme: Rc::new(lexeme),
            index,
            line: 0,
        }
    }

    pub fn generic_token(token: TType) -> Token {
        Token {
            t_type: token,
            lexeme: Rc::new("".to_string()),
            index: 0,
            line: 0,
        }
    }
}

/// All types of tokens available. Most are keywords or special chars.
/// The ScanError token is a special token signifying a syntax error.
/// Its lexeme is an error message to be displayed to the user.
#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum TType {
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
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
    Class,
    Else,
    Enum,
    Error,
    ExFn,
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
    None,
    Or,
    Return,
    To,
    True,
    Var,
    Val,
    When,
    While,

    ScanError,
    EndOfFile,
}
