use logos::Logos;

/// A direct token that implements Logos. Most are keywords or special chars.
/// The `Error` token is a special token signifying a syntax error.
#[derive(Logos, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum Token {
    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token("&")]
    AndSym,
    #[token("~")]
    Tilde,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("-")]
    Minus,
    #[token("+")]
    Plus,
    #[token(";")]
    Semicolon,
    #[token(":")]
    Colon,
    #[token("::")]
    ColonColon,
    #[token("/")]
    Slash,
    #[token("*")]
    Star,
    #[token("->")]
    Arrow,

    #[token("!")]
    Bang,
    #[token("!=")]
    BangEqual,
    #[token("=")]
    Equal,
    #[token("==")]
    EqualEqual,
    #[token(">")]
    Greater,
    #[token(">=")]
    GreaterEqual,
    #[token("<")]
    Less,
    #[token("<=")]
    LessEqual,

    #[regex("[a-zA-Z_][a-zA-Z0-9_]*")]
    Identifier,
    #[regex("\"[^\"]*\"")]
    String,
    #[regex(r"[0-9]+(?:(i|u)(size|8|16|32|64))?")]
    Int,
    #[regex(r"[0-9]+\.[0-9]+(?:(f)(32|64))?")]
    Float,

    #[token("and")]
    And,
    #[token("break")]
    Break,
    #[token("class")]
    Class,
    #[token("construct")]
    Construct,
    #[token("else")]
    Else,
    #[token("enum")]
    Enum,
    #[token("export")]
    Export,
    #[token("false")]
    False,
    #[token("for")]
    For,
    #[token("func")]
    Func,
    #[token("if")]
    If,
    #[token("impl")]
    Impl,
    #[token("import")]
    Import,
    #[token("in")]
    In,
    #[token("interface")]
    Interface,
    #[token("is")]
    Is,
    #[token("new")]
    New,
    #[token("or")]
    Or,
    #[token("return")]
    Return,
    #[token("strong")]
    Strong,
    #[token("true")]
    True,
    #[token("var")]
    Var,
    #[token("val")]
    Val,
    #[token("when")]
    When,

    #[token("public")]
    Public,
    #[token("private")]
    Private,
    #[token("extern")]
    Extern,
    #[token("variadic")]
    Variadic,

    #[regex(r"/\*([^*]|\*+[^*/])*\*?")] // https://github.com/maciejhirsz/logos/issues/180
    #[error]
    Error,

    #[regex(r"//[^\n]*")]
    #[regex(r"/\*([^*]|\**[^*/])*\*+/")]
    Comment,

    #[regex(r"[ \t\n\f]+")]
    Whitespace,

    /// This special token is unused by the lexer itself, but is
    /// here for parsers or others to use as an alternative to using
    /// Option<Token>, as this enum case makes handling EOF much easier.
    EndOfFile,
}
