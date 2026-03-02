use super::cursor::{Cursor, EOF_CHAR};
use super::errors::LexError;
use super::strings::{StringSegment, lex_string_segment};
use super::tokens::{SpannedToken, Token};
use miette::{Result, SourceSpan};
use smol_str::SmolStr;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LexerMode {
    Normal,
    String { interpolated: bool },
}

pub struct Lexer<'a> {
    src: &'a str,
    cursor: Cursor<'a>,
    at_bol: bool,
    nesting_depth: usize,
    pending: VecDeque<SpannedToken>,
    eof_emitted: bool,
    errors: Vec<LexError>,
    line_number: usize,
    no_data_consumed: bool,
    mode_stack: Vec<LexerMode>,
    last_comment: Option<(SmolStr, SourceSpan)>,
    pub(crate) keywords: HashMap<SmolStr, Token>,
    pub(crate) static_tokens: Vec<(SmolStr, Token)>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        use strum::IntoEnumIterator;

        let mut keywords = HashMap::new();
        let mut static_tokens = Vec::new();

        for token in Token::iter() {
            if token.is_keyword() {
                keywords.insert(SmolStr::new(token.as_ref()), token);
            } else if token.is_symbol() {
                static_tokens.push((SmolStr::new(token.as_ref()), token));
            }
        }

        static_tokens.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        Self {
            src,
            cursor: Cursor::new(src),
            at_bol: true,
            nesting_depth: 0,
            pending: VecDeque::new(),
            eof_emitted: false,
            errors: Vec::new(),
            line_number: 1,
            no_data_consumed: true,
            mode_stack: vec![LexerMode::Normal],
            last_comment: None,
            keywords,
            static_tokens,
        }
    }

    pub fn is_incomplete(&self) -> bool {
        self.mode_stack.len() > 1
    }

    pub fn lex(&mut self) -> (Vec<SpannedToken>, Vec<LexError>) {
        let mut tokens = Vec::new();
        while let Ok(Some(token)) = self.next_token() {
            tokens.push(token);
        }
        (tokens, self.errors.clone())
    }

    pub fn next_token(&mut self) -> Result<Option<SpannedToken>> {
        if let Some(token) = self.pending.pop_front() {
            return Ok(Some(token));
        }

        if self.cursor.is_eof() {
            return self.handle_eof();
        }

        match self.mode_stack.last().cloned().unwrap_or(LexerMode::Normal) {
            LexerMode::Normal => self.next_normal_token(),
            LexerMode::String { interpolated } => self.next_string_token(interpolated),
        }
    }

    fn next_normal_token(&mut self) -> Result<Option<SpannedToken>> {
        if self.at_bol {
            self.handle_indentation();
            if let Some(token) = self.pending.pop_front() {
                return Ok(Some(token));
            }
        }

        if let Some(token) = self.lex_whitespace() {
            return Ok(Some(token));
        }

        if self.cursor.is_eof() {
            return self.next_token();
        }

        let start_pos = self.current_pos();

        // Newline Handling
        if let Some(tok) = self.try_lex_newline() {
            return Ok(Some(tok));
        }

        let c = self.cursor.first();
        let token = match c {
            'a'..='z' | 'A'..='Z' | '_' | '\u{0080}'..='\u{10FFFF}' if !c.is_whitespace() => {
                self.lex_identifier_keyword_or_comment()
            }
            '0'..='9' => Some(self.lex_number()),
            '.' if self.cursor.second().is_ascii_digit() => Some(self.lex_number()),
            '/' if self.cursor.second() == '/' => Some(self.lex_line_comment()),
            '/' if self.cursor.second() == '*' => Some(self.lex_block_comment()),
            '"' => {
                self.cursor.bump(); // consume "
                self.mode_stack.push(LexerMode::String {
                    interpolated: false,
                });
                return self.next_token();
            }
            _ => self.lex_symbol_or_operator(),
        };

        if let Some(tok) = token {
            let end_pos = self.current_pos();
            let span = SourceSpan::new(start_pos.into(), end_pos - start_pos);

            // Handle nesting and interpolation exit
            match &tok {
                Token::LParen | Token::LBracket | Token::LBrace => {
                    self.nesting_depth += 1;
                }
                Token::RParen | Token::RBracket | Token::RBrace => {
                    self.nesting_depth = self.nesting_depth.saturating_sub(1);
                    // If we just matched a '}' and we were in an interpolation, pop back to string mode.
                    if matches!(tok, Token::RBrace)
                        && self.mode_stack.len() > 1
                        && matches!(
                            self.mode_stack[self.mode_stack.len() - 2],
                            LexerMode::String { .. }
                        )
                    {
                        self.mode_stack.pop();
                    }
                }
                _ => {}
            }

            self.no_data_consumed = false;

            if matches!(tok, Token::InvalidLiteral(_)) {
                self.errors.push(LexError::InvalidLiteral { span });
            }

            Ok(Some((tok, span)))
        } else {
            self.next_token()
        }
    }

    fn next_string_token(&mut self, interpolated: bool) -> Result<Option<SpannedToken>> {
        let start_pos = self.current_pos();
        let (segment, mut errors) = lex_string_segment(&mut self.cursor, start_pos);
        self.errors.append(&mut errors);

        let end_pos = self.current_pos();
        let span = SourceSpan::new(start_pos.into(), end_pos - start_pos);

        match segment {
            StringSegment::End(content) => {
                self.mode_stack.pop();
                if interpolated {
                    Ok(Some((Token::StringEnd(content), span)))
                } else {
                    Ok(Some((Token::StringLiteral(content), span)))
                }
            }
            StringSegment::Interpolate(content) => {
                let token = if interpolated {
                    Token::StringPart(content)
                } else {
                    // Update current mode to show we've now interpolated
                    if let Some(LexerMode::String { interpolated: i }) = self.mode_stack.last_mut()
                    {
                        *i = true;
                    }
                    Token::StringStart(content)
                };

                // Push LBrace for the '{' we just hit
                let brace_span = SourceSpan::new((self.current_pos() - 1).into(), 1usize);
                self.pending.push_back((Token::LBrace, brace_span));
                self.nesting_depth += 1;
                self.mode_stack.push(LexerMode::Normal);

                Ok(Some((token, span)))
            }
            StringSegment::Unterminated => {
                self.errors.push(LexError::UnterminatedString { span });
                self.mode_stack.pop();
                self.next_token()
            }
        }
    }

    fn current_pos(&self) -> usize {
        self.src.len() - self.cursor.len_remaining()
    }

    fn handle_eof(&mut self) -> Result<Option<SpannedToken>> {
        if self.eof_emitted {
            return Ok(None);
        }

        let pos = self.current_pos();
        let span = SourceSpan::new(pos.into(), 0usize);
        self.pending.push_back((Token::Eof, span));
        self.eof_emitted = true;
        Ok(self.pending.pop_front())
    }

    fn handle_indentation(&mut self) {
        let start_pos = self.current_pos();
        let mut width = 0;

        while self.cursor.first() == ' ' {
            self.cursor.bump();
            width += 1;
        }
        while self.cursor.first() == '\t' {
            self.cursor.bump();
            width += 1;
        }

        if width > 0 {
            let span = SourceSpan::new(start_pos.into(), width);
            let text = SmolStr::new(&self.src[start_pos..start_pos + width]);
            self.pending.push_back((Token::Whitespace(text), span));
        }

        let next = self.cursor.first();
        // Blank lines do not affect tokenization.
        if next == '\n' || next == '\r' || next == EOF_CHAR {
            self.at_bol = true;
            return;
        }

        self.at_bol = false;
    }

    fn lex_whitespace(&mut self) -> Option<SpannedToken> {
        let start_pos = self.current_pos();
        let mut spaces = 0;
        while self.cursor.first() == ' ' {
            self.cursor.bump();
            spaces += 1;
        }
        if spaces > 0 {
            let span = SourceSpan::new(start_pos.into(), spaces);
            let text = SmolStr::new(&self.src[start_pos..start_pos + spaces]);
            Some((Token::Whitespace(text), span))
        } else {
            None
        }
    }

    fn try_lex_newline(&mut self) -> Option<SpannedToken> {
        let start_pos = self.current_pos();
        let c = self.cursor.first();
        if c == '\n' || c == '\r' {
            if c == '\r' && self.cursor.second() == '\n' {
                self.cursor.bump();
            }
            self.cursor.bump();
            self.line_number += 1;

            let end_pos = self.current_pos();
            let text = SmolStr::new(&self.src[start_pos..end_pos]);
            let span = SourceSpan::new(start_pos.into(), end_pos - start_pos);

            if self.nesting_depth > 0 {
                let start_of_spaces = self.current_pos();
                let mut spaces = 0;
                while self.cursor.first() == ' ' {
                    self.cursor.bump();
                    spaces += 1;
                }
                if spaces > 0 {
                    let s_span = SourceSpan::new(start_of_spaces.into(), spaces);
                    let s_text = SmolStr::new(&self.src[start_of_spaces..start_of_spaces + spaces]);
                    self.pending.push_back((Token::Whitespace(s_text), s_span));
                }
                return Some((Token::Newline(text), span));
            }

            self.at_bol = true;
            return Some((Token::Newline(text), span));
        }
        None
    }

    fn lex_identifier_keyword_or_comment(&mut self) -> Option<Token> {
        let start_pos = self.current_pos();
        let text = self
            .cursor
            .take_while(|c| c.is_alphanumeric() || c == '_' || (c > '\x7f' && !c.is_whitespace()));

        if text == "so" && self.cursor.first() == ':' {
            self.cursor.bump(); // :
            while self.cursor.first() != '\n'
                && self.cursor.first() != '\r'
                && self.cursor.first() != EOF_CHAR
            {
                self.cursor.bump();
            }
            let span = SourceSpan::new(start_pos.into(), self.current_pos() - start_pos);
            self.errors.push(LexError::LegacySoComment { span });
            return None;
        }

        if let Some(token) = self.keywords.get(text) {
            // Check for DocComment promotion
            if matches!(token, Token::Class | Token::Fn | Token::Can | Token::Has)
                && let Some((_comment, _span)) = self.last_comment.take()
            {
                // Promotion logic can go here if needed later
            }
            return Some(token.clone());
        }

        self.last_comment = None;
        Some(Token::Identifier(SmolStr::new(text)))
    }

    fn lex_line_comment(&mut self) -> Token {
        self.cursor.bump(); // /
        self.cursor.bump(); // /
        let start = self.current_pos();
        while self.cursor.first() != '\n'
            && self.cursor.first() != '\r'
            && self.cursor.first() != EOF_CHAR
        {
            self.cursor.bump();
        }
        let end = self.current_pos();
        Token::Comment(SmolStr::new(&self.src[start..end]))
    }

    fn lex_block_comment(&mut self) -> Token {
        self.cursor.bump(); // /
        self.cursor.bump(); // *
        let start = self.current_pos();
        while !self.cursor.is_eof() {
            if self.cursor.first() == '*' && self.cursor.second() == '/' {
                let end = self.current_pos();
                self.cursor.bump();
                self.cursor.bump();
                return Token::Comment(SmolStr::new(&self.src[start..end]));
            }
            if self.cursor.first() == '\n' || self.cursor.first() == '\r' {
                self.line_number += 1;
            }
            self.cursor.bump();
        }
        let end = self.current_pos();
        self.errors.push(LexError::UnterminatedString {
            span: SourceSpan::new(start.saturating_sub(2).into(), end - start + 2),
        });
        Token::Comment(SmolStr::new(&self.src[start..end]))
    }

    fn lex_number(&mut self) -> Token {
        use super::literals::consume_number;
        consume_number(&mut self.cursor)
    }

    fn lex_symbol_or_operator(&mut self) -> Option<Token> {
        let current_src = self.cursor.as_str();
        for (pattern, token) in &self.static_tokens {
            if current_src.starts_with(pattern.as_str()) {
                for _ in 0..pattern.len() {
                    self.cursor.bump();
                }
                return Some(token.clone());
            }
        }

        let c = self.cursor.first();
        if c != EOF_CHAR {
            let span = SourceSpan::new(self.current_pos().into(), 1usize);
            self.errors
                .push(LexError::UnexpectedCharacter { span, char: c });
            self.cursor.bump();
        }
        None
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = SpannedToken;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token().ok().flatten()
    }
}
