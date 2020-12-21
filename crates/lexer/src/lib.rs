mod token;

pub use logos::{Logos, Span};
pub use token::Token;

pub struct Lexer<'l> {
    logos: logos::Lexer<'l, Token>,
}

impl<'l> Lexer<'l> {
    pub fn span(&self) -> Span {
        self.logos.span()
    }

    pub fn new(input: &'l str) -> Self {
        Self {
            logos: Token::lexer(input),
        }
    }
}

impl<'l> Iterator for Lexer<'l> {
    type Item = (Token, &'l str);

    fn next(&mut self) -> Option<Self::Item> {
        let kind = self.logos.next()?;
        let text = self.logos.slice();
        Some((kind, text))
    }
}
