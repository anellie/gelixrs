/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 8/30/19 6:20 PM.
 * This file is under the GPL3 license. See LICENSE in the root directory of this repository for details.
 */

use std::rc::Rc;

/// A token in the gelix language. These are produced by a [Lexer].
/// Cloning a Token is cheap, since the lexemes are refcounted.
#[derive(Debug, Clone)]
pub struct Token {
    /// The type of the token.
    pub t_type: Type,
    /// The lexeme of the token. Does not include escape chars (ex. String lexeme is <i>I'm a string!</i>)
    pub lexeme: Rc<String>,
    /// The index of the last char on the line of the token inside the source.
    /// This is used for error reporting.
    pub index: usize,
    /// The line of the token.
    pub line: usize,
}

impl Token {
    pub fn eof_token(line: usize) -> Token {
        Token {
            t_type: Type::EndOfFile,
            lexeme: Rc::new("\0".to_string()),
            index: 1,
            line,
        }
    }

    pub fn generic_identifier(lexeme: String) -> Token {
        let index = lexeme.len();
        Token {
            t_type: Type::Identifier,
            lexeme: Rc::new(lexeme),
            index,
            line: 0,
        }
    }

    pub fn generic_token(token: Type) -> Token {
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
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum Type {
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
    Slash,
    Star,
    Arrow,
    Newline,

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
    In,
    None,
    Or,
    Return,
    Super,
    To,
    True,
    Var,
    Val,
    When,
    While,

    ScanError,
    EndOfFile,
}
