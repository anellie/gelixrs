use crate::Lexeme;

pub(crate) struct Source<'s> {
    lexemes: &'s [Lexeme<'s>],
    current: usize,
    saved: usize,
}

impl<'s> Source<'s> {
    pub fn get_current(&self) -> Option<Lexeme<'s>> {
        self.lexemes.get(self.current).copied()
    }

    pub fn get_last(&self) -> Lexeme<'s> {
        self.lexemes.get(self.current - 1).copied().unwrap()
    }

    pub fn position(&self) -> usize {
        self.current
    }

    pub fn next(&mut self) {
        self.current += 1;
    }

    pub fn save(&mut self) {
        self.saved = self.current;
    }

    pub fn restore(&mut self) {
        self.current = self.saved;
    }

    pub fn new(lexemes: &'s [Lexeme<'s>]) -> Self {
        Self {
            lexemes,
            current: 0,
            saved: 0,
        }
    }
}
