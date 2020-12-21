use super::{event::Event, source::Source};
use rowan::{GreenNode, GreenNodeBuilder, Language, SmolStr};
use syntax::{kind::SyntaxKind, language::GelixLang};

pub(crate) struct Sink<'s> {
    builder: GreenNodeBuilder<'static>,
    source: Source<'s>,
    events: Vec<Event>,
}

impl<'s> Sink<'s> {
    pub fn new(source: Source<'s>, events: Vec<Event>) -> Self {
        Self {
            builder: GreenNodeBuilder::new(),
            source,
            events,
        }
    }

    pub fn finish(mut self) -> GreenNode {
        let mut reordered_events = self.events.clone();

        for (idx, event) in self.events.iter().enumerate() {
            if let Event::StartNodeAt { kind, checkpoint } = event {
                reordered_events.remove(idx);
                reordered_events.insert(*checkpoint, Event::StartNode(*kind));
            }
        }

        for event in reordered_events {
            match event {
                Event::StartNode(kind) => self.builder.start_node(GelixLang::kind_to_raw(kind)),
                Event::StartNodeAt { .. } => unreachable!(),
                Event::AddToken { kind, lexeme } => self.token(kind, lexeme),
                Event::FinishNode => self.builder.finish_node(),
            }

            self.eat_whitespace();
        }

        self.builder.finish()
    }

    fn token(&mut self, kind: SyntaxKind, text: SmolStr) {
        self.builder.token(GelixLang::kind_to_raw(kind), text);
        self.source.next();
    }

    fn eat_whitespace(&mut self) {
        while let Some(lexeme) = self.source.get_current() {
            if !lexeme.kind.should_skip() {
                break;
            }
            self.token(lexeme.kind, lexeme.lexeme.into());
        }
    }
}
