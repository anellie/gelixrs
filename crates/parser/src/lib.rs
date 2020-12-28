mod declaration;
mod expression;
mod nodes;
mod util;

use crate::util::{
    builder::{Checkpoint, NodeBuilder},
    source::Source,
};
use common::bench;
use error::{Error, ErrorSpan, GErr};
use lexer::Lexer;
pub use nodes::*;
use syntax::kind::SyntaxKind;

pub fn parse(input: &str) -> Result<ParseResult, Vec<Error>> {
    let lexer = Lexer::new(input);
    let lexemes = lexer
        .map(|(tok, lexeme)| Lexeme {
            kind: tok.into(),
            lexeme,
        })
        .collect::<Vec<_>>();
    let parser = Parser::new(&lexemes);
    parser.parse()
}

#[derive(Copy, Clone)]
struct Lexeme<'t> {
    kind: SyntaxKind,
    lexeme: &'t str,
}

struct Parser<'p> {
    /// The source that is being parsed.
    source: Source<'p>,

    /// The CST builder.
    builder: NodeBuilder,

    /// A list of all errors encountered during parsing.
    errors: Vec<Error>,
    poisoned: bool,

    /// Stores the modifiers of the current global declaration.
    modifiers: Vec<SyntaxKind>,
}

impl<'p> Parser<'p> {
    fn parse(mut self) -> Result<ParseResult, Vec<Error>> {
        bench!("parser", {
            while self.peek() != SyntaxKind::EndOfFile {
                self.declaration();
                if self.poisoned {
                    self.try_depoison();
                }
            }
        });

        if self.errors.is_empty() {
            Ok(ParseResult {
                green_node: self.builder.finish(),
            })
        } else {
            Err(self.errors)
        }
    }

    /// Checks if the current token is the given kind. If yes, it consumes it.
    fn matches(&mut self, kind: SyntaxKind) -> bool {
        let matches = self.check(kind);
        if matches {
            self.advance();
        }
        matches
    }

    fn consume(&mut self, kind: SyntaxKind, want: &'static str, after: &'static str) {
        if self.advance_checked() != kind {
            self.error_at_current(GErr::E001 { want, after });
        }
    }

    fn consume_either(
        &mut self,
        kind1: SyntaxKind,
        kind2: SyntaxKind,
        want: &'static str,
        after: &'static str,
    ) {
        if self.peek() != kind1 && self.peek() != kind2 {
            self.error_at_current(GErr::E001 { want, after });
        } else {
            self.advance();
        }
    }

    fn error_at_current(&mut self, err: GErr) {
        if self.poisoned {
            self.advance_checked();
            return;
        }

        let err = Error {
            index: ErrorSpan::Token(self.source.position()),
            kind: err,
        };
        self.errors.push(err);
        self.poisoned = true;
    }

    fn try_depoison(&mut self) {
        let recoverable = &[
            SyntaxKind::Enum,
            SyntaxKind::Class,
            SyntaxKind::Func,
            SyntaxKind::Import,
            SyntaxKind::Export,
            SyntaxKind::Impl,
            SyntaxKind::Interface,
            SyntaxKind::EndOfFile,
        ];
        while !recoverable.contains(&self.peek()) {
            self.advance_checked();
        }
        self.poisoned = false;
    }

    /// Is the current token the given kind?
    fn check(&mut self, kind: SyntaxKind) -> bool {
        self.peek() == kind
    }

    /// Same as check, but checks for separators between expressions (`;` or newline)
    fn check_separator(&mut self) -> bool {
        self.check(SyntaxKind::Semicolon) // || self.previous_line != self.current_line() todo newlines
    }

    /// Is the next token the given kind?
    fn check_next(&mut self, kind: SyntaxKind) -> bool {
        self.source.save();
        self.advance();
        let res = self.check(kind);
        self.source.restore();
        res
    }

    fn advance(&mut self) -> Lexeme<'p> {
        self.skip_whitespace();
        self.advance_inner()
    }

    fn advance_inner(&mut self) -> Lexeme<'p> {
        let Lexeme { kind, lexeme } = self.source.get_current().unwrap();
        self.source.next();

        self.builder.token(kind, lexeme.into());
        Lexeme { kind, lexeme }
    }

    fn advance_checked(&mut self) -> SyntaxKind {
        if self.is_at_end() {
            SyntaxKind::EndOfFile
        } else {
            self.advance().kind
        }
    }

    fn peek(&mut self) -> SyntaxKind {
        self.skip_whitespace();
        self.peek_raw().unwrap_or(SyntaxKind::EndOfFile)
    }

    fn peek_next(&mut self) -> SyntaxKind {
        self.source.save();
        self.skip_whitespace();
        self.source.next();
        self.skip_whitespace();
        let ret = self.peek_raw().unwrap_or(SyntaxKind::EndOfFile);
        self.source.restore();
        ret
    }

    fn peek_raw(&self) -> Option<SyntaxKind> {
        self.source.get_current().map(|Lexeme { kind, .. }| kind)
    }

    fn skip_whitespace(&mut self) {
        while self.peek_raw().map(|k| k.should_skip()) == Some(true) {
            self.advance_inner();
        }
    }

    fn is_at_end(&self) -> bool {
        self.source.get_current().is_none()
    }

    fn node_with<T: FnOnce(&mut Self)>(&mut self, kind: SyntaxKind, content: T) {
        self.start_node(kind);
        content(self);
        self.end_node()
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        self.skip_whitespace();
        self.builder.start_node(kind);
    }

    fn start_node_at(&mut self, checkpoint: Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(kind, checkpoint);
        self.skip_whitespace();
    }

    fn end_node(&mut self) {
        self.builder.end_node();
    }

    fn checkpoint(&mut self) -> Checkpoint {
        self.builder.checkpoint()
    }

    fn new(lexemes: &'p [Lexeme<'p>]) -> Self {
        Self {
            source: Source::new(lexemes),
            builder: NodeBuilder::new(),
            errors: vec![],
            poisoned: false,
            modifiers: Vec::with_capacity(4),
        }
    }
}

#[derive(Debug)]
pub struct ParseResult {
    green_node: Node,
}

impl ParseResult {
    pub fn root(self) -> Node {
        self.green_node
    }
}
