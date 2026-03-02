use std::str::Chars;

pub const EOF_CHAR: char = '\0';

pub struct Cursor<'a> {
    len_remaining: usize,
    chars: Chars<'a>,
}

impl<'a> Cursor<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            len_remaining: input.len(),
            chars: input.chars(),
        }
    }

    pub fn first(&self) -> char {
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    pub fn second(&self) -> char {
        let mut iter = self.chars.clone();
        iter.next();
        iter.next().unwrap_or(EOF_CHAR)
    }

    pub fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    pub fn len_remaining(&self) -> usize {
        self.len_remaining
    }

    pub fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.len_remaining -= c.len_utf8();
        Some(c)
    }

    pub fn take_while(&mut self, mut predicate: impl FnMut(char) -> bool) -> &'a str {
        let start = self.chars.as_str();
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
        let len = start.len() - self.chars.as_str().len();
        &start[..len]
    }

    pub fn eat_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }

    pub fn as_str(&self) -> &'a str {
        self.chars.as_str()
    }
}
