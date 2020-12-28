use smol_str::SmolStr;
use std::{fmt, fmt::Formatter, iter, ops::Range, rc::Rc};
use syntax::kind::SyntaxKind;

#[derive(Clone, Hash, PartialEq, Eq)]
pub enum NodeOrToken {
    Node(Node),
    Token(Token),
}

impl NodeOrToken {
    pub fn into_node(self) -> Option<Node> {
        match self {
            Self::Node(n) => Some(n),
            _ => None,
        }
    }

    pub fn into_token(self) -> Option<Token> {
        match self {
            Self::Token(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_node(&self) -> Option<&Node> {
        match self {
            Self::Node(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_token(&self) -> Option<&Token> {
        match self {
            Self::Token(t) => Some(t),
            _ => None,
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        match self {
            NodeOrToken::Node(n) => n.kind,
            NodeOrToken::Token(t) => t.kind,
        }
    }

    fn debug_fmt(&self, f: &mut Formatter<'_>, indent_size: usize) -> fmt::Result {
        let indent = iter::repeat(' ').take(indent_size).collect::<String>();
        write!(f, "{}{:?}", indent, self.kind())?;
        match self {
            NodeOrToken::Node(n) => {
                writeln!(f, " @ {:?}", n.span)?;
                for child in n.children.iter() {
                    child.debug_fmt(f, indent_size + 2)?;
                }
                Ok(())
            }
            NodeOrToken::Token(_) => writeln!(f, ".."),
        }
    }
}

impl fmt::Debug for NodeOrToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.debug_fmt(f, 0)
    }
}

#[derive(Debug, Clone, Hash, Eq)]
pub struct Node {
    children: Rc<Vec<NodeOrToken>>,
    kind: SyntaxKind,
    span: Range<u32>,
}

impl Node {
    pub fn children(&self) -> impl Iterator<Item = Node> + '_ {
        self.children_with_tokens()
            .filter_map(NodeOrToken::into_node)
    }

    pub fn children_with_tokens(&self) -> impl Iterator<Item = NodeOrToken> + '_ {
        self.children.iter().cloned()
    }

    pub fn first_child(&self) -> Option<Node> {
        self.children().next()
    }

    pub fn first_token(&self) -> Option<Token> {
        self.children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .next()
    }

    pub fn first_token_nest(&self) -> Option<Token> {
        match self.children_with_tokens().next()? {
            NodeOrToken::Token(t) => Some(t),
            NodeOrToken::Node(n) => n.first_token_nest(),
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    pub fn text_range(&self) -> Range<u32> {
        self.span.clone()
    }

    pub fn new(children: Rc<Vec<NodeOrToken>>, kind: SyntaxKind, span: Range<u32>) -> Self {
        Self {
            children,
            kind,
            span,
        }
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.children, &other.children)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Token {
    text: SmolStr,
    kind: SyntaxKind,
}

impl Token {
    pub fn text(&self) -> &SmolStr {
        &self.text
    }

    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    pub fn new(kind: SyntaxKind, text: SmolStr) -> Self {
        Self { text, kind }
    }
}
