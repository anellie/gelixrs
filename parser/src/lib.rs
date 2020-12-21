mod declaration;
mod expression;
mod util;

use crate::util::{event::Event, sink::Sink, source::Source};
use lexer::Lexer;
use rowan::{GreenNode, SyntaxNode};
use syntax::{kind::SyntaxKind, language::GelixLang};

pub fn parse(input: &str) -> ParseResult {
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

    /// A list of events that are later translated into a rowan syntax tree.
    events: Vec<Event>,

    /// A list of all errors encountered during parsing.
    errors: Vec<(usize, String)>,

    /// Stores the modifiers of the current global declaration.
    modifiers: Vec<SyntaxKind>,
}

impl<'p> Parser<'p> {
    pub fn parse(mut self) -> ParseResult {
        self.start_node(SyntaxKind::Root);

        while self.source.has_next() {
            self.declaration();
        }

        self.end_node();

        let sink = Sink::new(self.source.clone(), self.events);
        ParseResult {
            green_node: sink.finish(),
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

    fn consume(&mut self, kind: SyntaxKind, msg: &str) {
        if self.peek() != kind {
            self.error_at_current(msg);
        } else {
            self.advance();
        }
    }

    fn consume_either(&mut self, kind1: SyntaxKind, kind2: SyntaxKind, msg: &str) {
        if self.peek() != kind1 && self.peek() != kind2 {
            self.error_at_current(msg);
        } else {
            self.advance();
        }
    }

    fn error_at_current(&mut self, msg: &str) {
        self.errors.push((self.source.position(), msg.to_string()))
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

        let Lexeme { kind, lexeme } = self.source.get_current().unwrap();
        self.source.next();

        self.events.push(Event::AddToken {
            kind,
            lexeme: lexeme.into(),
        });
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
        self.skip_whitespace();
        self.peek_raw().unwrap_or(SyntaxKind::EndOfFile)
    }

    fn peek_raw(&self) -> Option<SyntaxKind> {
        self.source.get_current().map(|Lexeme { kind, .. }| kind)
    }

    fn skip_whitespace(&mut self) {
        while self.peek_raw().map(|k| k.should_skip()) == Some(true) {
            self.source.next();
        }
    }

    fn is_at_end(&self) -> bool {
        !self.source.has_next()
    }

    fn node_with<T: FnOnce(&mut Self)>(&mut self, kind: SyntaxKind, content: T) {
        self.start_node(kind);
        content(self);
        self.end_node()
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        self.events.push(Event::StartNode(kind))
    }

    fn start_node_at(&mut self, checkpoint: usize, kind: SyntaxKind) {
        self.events.push(Event::StartNodeAt { kind, checkpoint });
    }

    fn end_node(&mut self) {
        self.events.push(Event::FinishNode)
    }

    fn checkpoint(&self) -> usize {
        self.events.len()
    }

    pub fn new(lexemes: &'p [Lexeme<'p>]) -> Self {
        Self {
            source: Source::new(lexemes),
            events: Vec::with_capacity(100),
            errors: vec![],
            modifiers: Vec::with_capacity(4),
        }
    }
}

pub struct ParseResult {
    green_node: GreenNode,
}

impl ParseResult {
    pub fn debug_print(&self) {
        let syntax_node = SyntaxNode::<GelixLang>::new_root(self.green_node.clone());
        print!("{:#?}", syntax_node);
    }
}