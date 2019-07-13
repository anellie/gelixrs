#[derive(Debug, Clone, Copy)]
pub struct Token<'t> {
    pub t_type: Type,
    pub lexeme: &'t str,
    pub line: usize,
}

plain_enum_mod! {this, Type {
    LeftParen, RightParen,
    LeftBracket, RightBracket,
    LeftBrace, RightBrace,
    Comma, Dot, Minus, Plus,
    Semicolon, Slash, Star,
    Newline,

    Bang, BangEqual,
    Equal, EqualEqual,
    Greater, GreaterEqual,
    Less, LessEqual,

    Identifier, String, Int, Float, Char,

    And, Class, Else, Error, Ext, False,
    For, Func, If, In, Null, Or,
    Print, Return, Super, Take, This,
    True, Var, Val, While,

    ScanError, EOF,
}}
