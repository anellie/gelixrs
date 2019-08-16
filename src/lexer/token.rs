/// A token in the gelix language. These are produced by a [Lexer].
#[derive(Debug, Clone)]
pub struct Token {
    /// The type of the token.
    pub t_type: Type,
    /// The lexeme of the token. Does not include escape chars (ex. String lexeme is <i>I'm a string!</i>)
    pub lexeme: String,
    /// The line the token is on.
    pub line: usize,
}

impl Token {
    pub fn eof_token(line: usize) -> Token {
        Token {
            t_type: Type::EndOfFile,
            lexeme: "\0".to_string(),
            line,
        }
    }

    pub fn generic_identifier(lexeme: String) -> Token {
        Token {
            t_type: Type::Identifier,
            lexeme,
            line: 0
        }
    }
}

/// All types of tokens available. Most are keywords or special chars.
/// The ScanError token is a special token signifying a syntax error.
/// Its lexeme is an error message to be displayed to the user.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum Type {
    LeftParen, RightParen,
    LeftBracket, RightBracket,
    LeftBrace, RightBrace,
    Comma, Dot, Minus, Plus,
    Semicolon, Slash, Star,
    Arrow,
    Newline,

    Bang, BangEqual,
    Equal, EqualEqual,
    Greater, GreaterEqual,
    Less, LessEqual,

    Identifier, String, Int, Float, Char,

    And, ExFn, Class, Else, Enum, Error, Ext, False,
    For, Func, If, In, None, Or,
    Return, Super, Take, This,
    True, Var, Val, When, While,

    ScanError, EndOfFile,
}
