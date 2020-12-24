use lexer::Token;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, FromPrimitive, ToPrimitive)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // The root node of a parse tree.
    Root,

    // An import or export declaration
    ImportDecl,
    // A top-level function declaration
    FunctionDecl,
    // A top-level ADT declaration
    AdtDecl,
    // A top-level interface implementation declaration
    ImplDecl,

    // An identifier of a declaration, containing type parameters.
    Ident,
    // A type parameter inside Ident, containing a Type and Identifier/name.
    TypeParameter,
    // A modifier on a declaration.
    Modifier,

    // A function signature.
    FunctionSignature,
    // A parameter inside a function signature.
    Parameter,
    // A function body.
    FunctionBody,

    // A method inside of another declaration.
    Method,
    // A constructor
    Constructor,
    // A field inside of an ADT
    AdtMember,
    // The implementor on iface impls
    Implementor,
    // The implemented iface on iface impls
    Implementing,
    // A case inside of an enum declaration
    EnumCase,

    // An expression.
    Expression,

    // A variable declaration like 'var x: String = "hello"'
    Variable,
    // An initializer of a variable or member
    Initializer,
    // A block containing other expressions
    Block,
    // An 'if' expression
    IfExpr,
    // A 'for' expression
    ForExpr,
    // The iterator condition of a for loop.
    // Conditional for loops simply use ExprCondition.
    ForIterCond,
    // A return expression
    ReturnExpr,
    // A break expression
    BreakExpr,
    // A when expression
    WhenExpr,
    // A when branch, containing 1 ExprCondition (missing on else) and 1 ExprBody
    WhenBranch,

    // Condition of if, for and when expressions.
    ExprCondition,
    // The body of if, for and when expressions.
    ExprBody,
    // The body of the else branch of if, for and when expressions.
    ExprElse,
    // A binary expression like '5 + 5'
    BinaryExpr,
    // A prefix expression, currently only '!false'
    PrefixExpr,
    // Operator of a binary or prefix expression.
    Operator,
    // A call expression.
    CallExpr,
    // A get expression ('x.y', 'Callee.Ident')
    GetExpr,
    // A static get expression ('x:y', 'Callee:Ident')
    GetStaticExpr,
    // Callee of a call or get expression
    Callee,
    // Argument of a call expression
    CallArgument,
    // A literal expression, only contains the literal token
    Literal,
    // A closure literal
    ClosureLiteral,
    // A grouping expression, simply '($expr)'
    Grouping,

    // A type literal like "String", "&String", "(u32, u32): u64"
    Type,

    // This special variant is used for SyntaxKind::is_token.
    __TokenStart,

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

    And,
    Break,
    Class,
    Construct,
    Else,
    Enum,
    Export,
    False,
    For,
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
    True,
    Var,
    Val,
    When,

    Public,
    Private,
    Extern,
    Variadic,

    Error,
    Comment,
    Whitespace,
    EndOfFile,
}

impl SyntaxKind {
    pub fn should_skip(&self) -> bool {
        matches!(self, Self::Whitespace | Self::Comment)
    }

    pub fn infix_binding_power(&self) -> Option<(u8, u8)> {
        Some(match self {
            Self::Equal => (8, 7),
            Self::Or => (10, 9),
            Self::And => (12, 11),
            Self::BangEqual | Self::EqualEqual => (14, 13),
            Self::Less | Self::LessEqual | Self::Greater | Self::GreaterEqual => (16, 15),
            Self::Plus | Self::Minus => (16, 15),
            Self::Star | Self::Slash => (18, 17),
            Self::Is => (20, 19),
            _ => return None,
        })
    }

    pub fn prefix_binding_power(&self) -> Option<u8> {
        Some(match self {
            Self::Minus | Self::Bang | Self::New => 30,
            _ => return None,
        })
    }

    pub fn is_token(&self) -> bool {
        (*self as u16) < (SyntaxKind::__TokenStart as u16)
            && (*self as u16) > (SyntaxKind::Error as u16)
    }

    fn from_token(token: Token) -> Self {
        let kind = (SyntaxKind::__TokenStart as u16) + (token as u16) + 1;
        Self::from_u16(kind).unwrap()
    }
}

impl From<Token> for SyntaxKind {
    fn from(token: Token) -> Self {
        // Do a few checks to ensure the types are in sync for easily catching
        // mistakes in possible future code changes
        assert_eq!(SyntaxKind::Error, SyntaxKind::from_token(Token::Error));
        assert_eq!(SyntaxKind::String, SyntaxKind::from_token(Token::String));
        assert_eq!(SyntaxKind::LeftParen, SyntaxKind::from_token(Token::LeftParen));
        Self::from_token(token)
    }
}

impl From<rowan::SyntaxKind> for SyntaxKind {
    fn from(kind: rowan::SyntaxKind) -> Self {
        Self::from_u16(kind.0).unwrap()
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}
