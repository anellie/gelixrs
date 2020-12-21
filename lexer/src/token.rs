use logos::{Logos, Span};
use smol_str::SmolStr;

/// A (spanned) token in the gelix language. These are produced by a lexer.
/// Cloning a Token is cheap, since the lexemes use smol_str.
#[derive(Debug, Clone)]
pub struct SToken {
    /// The type of the token.
    pub token: Token,
    /// The index of the last char of the token inside the source.
    /// This is used for error reporting.
    pub span: Span,
}

impl SToken {
    pub(crate) fn new(t_type: Token, span: Span) -> SToken {
        SToken {
            token: t_type,
            span,
        }
    }

    pub fn eof_token() -> SToken {
        SToken {
            token: Token::EndOfFile,
            span: 0..1,
        }
    }

    // Warning: `lexeme` must not be over 22 bytes!
    // See `SmolStr::new_inline`.
    pub fn generic_identifier(lexeme: &str) -> SToken {
        SToken {
            token: Token::Identifier(SmolStr::new_inline(lexeme)),
            span: 0..lexeme.len(),
        }
    }

    pub fn generic_token(token: Token) -> SToken {
        SToken {
            token: token,
            span: 0..1,
        }
    }
}

impl PartialEq for SToken {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token
    }
}

/// A direct token that implements Logos. Most are keywords or special chars.
/// The `Error` token is a special token signifying a syntax error.
#[derive(Logos, PartialEq, Eq, Debug, Clone, Hash)]
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

    #[regex("[a-zA-Z][a-zA-Z0-9]+", crate::to_smol_str)]
    Identifier(SmolStr),
    #[regex("\"[^\"]*\"", crate::string)]
    String(SmolStr),
    #[regex(r"[0-9]+(?:(i|u)(size|8|16|32|64))?", crate::to_smol_str)]
    Int(SmolStr),
    #[regex(r"[0-9]+\.[0-9]+(?:(f)(32|64))?", crate::to_smol_str)]
    Float(SmolStr),

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

    #[regex(r"[ \t\n\f]+", logos::skip)]
    #[regex(r"/\*([^*]|\*+[^*/])*\*?")]  // https://github.com/maciejhirsz/logos/issues/180
    #[error]
    Error,

    #[regex(r"//[^\n]*")]
    #[regex(r"/\*([^*]|\**[^*/])*\*+/")]
    Comment,

    EndOfFile,
}
