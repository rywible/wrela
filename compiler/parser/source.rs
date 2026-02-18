use crate::lexer::Lexer;
use crate::lexer::{LexError, SpannedToken, Token};
use crate::parser::kind::SyntaxKind;
use miette::SourceSpan;

pub struct TokenSource<'a> {
    tokens: Vec<SpannedToken>,
    cursor: usize,
    source: &'a str,
}

impl<'a> TokenSource<'a> {
    pub fn new(source: &'a str) -> Self {
        let lexer = Lexer::new(source);
        let tokens: Vec<_> = lexer.collect();
        Self {
            tokens,
            cursor: 0,
            source,
        }
    }

    pub fn from_tokens(source: &'a str, tokens: Vec<SpannedToken>) -> Self {
        Self {
            tokens,
            cursor: 0,
            source,
        }
    }

    pub fn lex_with_errors(source: &'a str) -> (Vec<SpannedToken>, Vec<LexError>) {
        let mut lexer = Lexer::new(source);
        lexer.lex()
    }

    pub fn peek(&self) -> SyntaxKind {
        self.peek_at(0)
    }

    pub fn peek_at(&self, n: usize) -> SyntaxKind {
        self.tokens
            .get(self.cursor + n)
            .map(|(t, _): &(Token, SourceSpan)| SyntaxKind::from(t.clone()))
            .unwrap_or(SyntaxKind::Eof)
    }

    pub fn peek_token(&self) -> Option<&Token> {
        self.tokens.get(self.cursor).map(|(t, _)| t)
    }

    pub fn peek_span(&self) -> SourceSpan {
        self.tokens
            .get(self.cursor)
            .map(|(_, s)| *s)
            .unwrap_or_else(|| SourceSpan::from((self.source.len(), 0)))
    }

    pub fn peek_span_at(&self, n: usize) -> SourceSpan {
        self.tokens
            .get(self.cursor + n)
            .map(|(_, s)| *s)
            .unwrap_or_else(|| SourceSpan::from((self.source.len(), 0)))
    }

    pub fn bump(&mut self) {
        if self.cursor < self.tokens.len() {
            self.cursor += 1;
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_at_eof(&self) -> bool {
        self.peek() == SyntaxKind::Eof
    }

    /// Returns the raw text of the current token
    pub fn text(&self) -> &str {
        let span = self.peek_span();
        &self.source[span.offset()..span.offset() + span.len()]
    }

    pub fn text_at(&self, n: usize) -> &str {
        let span = self.peek_span_at(n);
        &self.source[span.offset()..span.offset() + span.len()]
    }
}
