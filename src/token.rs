/// A token in the gelix language. These are produced by a [Tokenizer].
#[derive(Debug, Clone)]
pub struct Token<'t> {
    /// The type of the token.
    pub t_type: Type,
    /// The lexeme of the token. Includes escape chars (ex. String lexeme is <i>"I'm a string!"</i>)
    pub lexeme: &'t str,
    /// The line the token is on. For multiline tokens, this is the last line.
    pub line: usize,
}

// All types of tokens available. Most are keywords or special chars.

// The ScanError token is a special token signifying a syntax error.
// Its lexeme is an error message to be displayed to the user.
plain_enum_mod! {this,
Type {
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

    And, Class, Else, Enum, Error, Ext, False,
    For, Func, If, In, Null, Or,
    Return, Super, Take, This,
    True, Var, Val, When, While,

    ScanError, EndOfFile,
}}
