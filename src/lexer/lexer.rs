use super::errors::LexError;
use super::tokens::{SpannedToken, Token};
use miette::{Result, SourceSpan};
use std::collections::VecDeque;

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    line_number: usize,
    byte_index: usize,
    at_beginning_of_line: bool,
    no_data_consumed: bool,
    indent_stack: Vec<usize>,
    pending: VecDeque<SpannedToken>,
    eof_emitted: bool,
    nesting: usize, // Tracks depth of ( [ {
    errors: Vec<LexError>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            line_number: 1,
            byte_index: 0,
            at_beginning_of_line: true,
            no_data_consumed: true,
            indent_stack: vec![0],
            pending: VecDeque::new(),
            eof_emitted: false,
            nesting: 0,
            errors: Vec::new(),
        }
    }

    fn is_eof(&self) -> bool {
        self.byte_index >= self.bytes.len()
    }

    fn current_byte(&self) -> Option<u8> {
        self.bytes.get(self.byte_index).copied()
    }

    fn peek_next_byte(&self) -> Option<u8> {
        self.bytes.get(self.byte_index + 1).copied()
    }

    fn peek_n_bytes(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.byte_index + n).copied()
    }

    #[allow(dead_code)]
    fn next_byte(&mut self) -> Option<u8> {
        let b = self.current_byte()?;
        self.byte_index += 1;
        Some(b)
    }

    // Helper to push error
    fn push_error(&mut self, error: LexError) {
        self.errors.push(error);
    }

    // Returns true if a newline token should be emitted or indentation checked.
    // Returns false if the newline was consumed as "whitespace" (inside nesting).
    fn consume_newline(&mut self) -> bool {
        let start_index = self.byte_index;

        // Check if we are at a newline sequence
        let is_newline = match (self.current_byte(), self.peek_next_byte()) {
            (Some(b'\r'), Some(b'\n')) => {
                self.byte_index += 2;
                self.line_number += 1;
                true
            }
            (Some(b'\n'), _) => {
                self.byte_index += 1;
                self.line_number += 1;
                true
            }
            (Some(b'\r'), _) => {
                self.byte_index += 1;
                self.line_number += 1;
                true
            }
            _ => false,
        };

        if !is_newline {
            return false; // Not a newline, caller continues
        }

        // If we are NOT nested, normal behavior: emit Newline token, check indent later.
        if self.nesting == 0 {
            return true;
        }

        // We ARE nested (inside `(`, `[`, `{`).
        // Rule: Content must be indented deeper than the enclosing block.
        // Closer `)`, `]`, `}` must align with enclosing block.

        // 1. Peek ahead to count spaces
        let mut temp_index = self.byte_index;
        let mut spaces = 0;
        loop {
            match self.bytes.get(temp_index).copied() {
                Some(b' ') => {
                    spaces += 1;
                    temp_index += 1;
                }
                Some(b'\t') => {
                    let span = SourceSpan::new(temp_index.into(), 1usize.into());
                    self.push_error(LexError::UnexpectedTabCharacter { span });
                    temp_index += 1;
                }
                Some(b'\n') | Some(b'\r') => {
                    // Another blank line inside grouping. Just skip it?
                    // We treat it as consumed.
                    break;
                }
                None => break,    // EOF
                Some(_) => break, // Content found
            }
        }

        // Check if line is blank/comment (effectively blank for indent rules)
        // Peek at the char after spaces
        let next_char = self.bytes.get(temp_index).copied();
        if matches!(next_char, None | Some(b'\n') | Some(b'\r') | Some(b'#')) {
            // Blank line. Consume spaces and return false (swallowed).
            self.byte_index = temp_index;
            return false;
        }

        // It is a content line. Check indentation.
        let current_block_indent = *self.indent_stack.last().unwrap_or(&0);

        // Is the next char a closer?
        let is_closer = matches!(next_char, Some(b')') | Some(b']') | Some(b'}'));

        if is_closer {
            // Closer should align with block
            if spaces < current_block_indent {
                let span = SourceSpan::new(start_index.into(), (temp_index - start_index).into());
                self.push_error(LexError::InvalidMultilineIndent { span });
            }
        } else {
            // Content. Must be > current_block_indent.
            if spaces <= current_block_indent {
                let span = SourceSpan::new(start_index.into(), (temp_index - start_index).into());
                self.push_error(LexError::InvalidMultilineIndent { span });
            }
        }

        // 4-space multiple check (optional but consistent)
        if spaces % 4 != 0 {
            let span = SourceSpan::new(start_index.into(), (temp_index - start_index).into());
            self.push_error(LexError::IndentNotMultipleOfFour { span });
        }

        // Valid (or error recorded)! Consume the spaces.
        self.byte_index = temp_index;

        // Return false: We swallowed the newline and indentation.
        false
    }

    fn consume_comment_line(&mut self) {
        // Eat characters until newline or EOF
        while let Some(b) = self.current_byte() {
            if b == b'\n' || b == b'\r' {
                break;
            }
            self.byte_index += 1;
        }
    }

    fn consume_so_comment_block(&mut self) {
        // We just consumed "so:".
        // 1. Consume the rest of the current line.
        self.consume_comment_line();

        // At this point, we are at a newline or EOF.
        // If EOF, we are done.
        if self.is_eof() {
            return;
        }

        // We need to capture the current scope's indent level.
        // Any subsequent line with indent > current_scope is part of the comment.
        let current_scope_indent = *self.indent_stack.last().unwrap();

        // Loop to consume indented lines
        loop {
            // We expect to be at a newline here.
            match (self.current_byte(), self.peek_next_byte()) {
                (Some(b'\r'), Some(b'\n')) => self.byte_index += 2,
                (Some(b'\n'), _) => self.byte_index += 1,
                (Some(b'\r'), _) => self.byte_index += 1,
                _ => {} // Not a newline, maybe EOF or something else?
            }

            if self.is_eof() {
                break;
            }

            // Now we are at the start of a new line.
            let mut temp_index = self.byte_index;
            let mut spaces = 0;
            let mut is_blank = false;

            loop {
                match self.bytes.get(temp_index).copied() {
                    Some(b' ') => {
                        spaces += 1;
                        temp_index += 1;
                    }
                    Some(b'\t') => {
                        break;
                    } // Treat as non-space
                    Some(b'\n') | Some(b'\r') => {
                        is_blank = true;
                        break;
                    }
                    None => {
                        // EOF
                        is_blank = true;
                        break;
                    }
                    Some(_) => {
                        break;
                    }
                }
            }

            // 1. Blank lines are part of the comment block (or at least skipped)
            if is_blank {
                // Update real state to consume this blank line
                self.byte_index = temp_index;
                // Continue loop to check next line
                continue;
            }

            // 2. Check indentation depth
            if spaces > current_scope_indent {
                // It is part of the block. Consume this line.
                self.byte_index = temp_index;
                self.consume_comment_line();
            } else {
                // Not indented deeper. The comment block ends here.
                // We do NOT consume this line.
                // But we DID consume the newline above.
                // We need to ensure `at_beginning_of_line` is set so normal indent logic runs.
                self.at_beginning_of_line = true;
                break;
            }
        }
    }

    pub fn next_token(&mut self) -> Result<Option<SpannedToken>> {
        // 1) If we already have queued tokens, return those first.
        if let Some(tok) = self.pending.pop_front() {
            return Ok(Some(tok));
        }

        // 2) EOF handling: emit DEDENTs, then EOF, then end.
        if self.is_eof() {
            if !self.eof_emitted {
                // Dedent back to 0
                while self.indent_stack.len() > 1 {
                    self.indent_stack.pop();
                    let span = SourceSpan::new(self.byte_index.into(), 0usize.into());
                    self.pending.push_back((Token::Dedent, span));
                }
                let span = SourceSpan::new(self.byte_index.into(), 0usize.into());
                self.pending.push_back((Token::Eof, span));
                self.eof_emitted = true;

                return Ok(self.pending.pop_front());
            }
            return Ok(None);
        }

        // 3) If at beginning of line, process indentation.
        if self.at_beginning_of_line {
            self.handle_bol_indentation();
            if let Some(tok) = self.pending.pop_front() {
                return Ok(Some(tok));
            }
        }

        // Capture start for newline or next token
        let start_index = self.byte_index;

        // 4) Newlines produce a token and put us back at BOL.
        // consume_newline handles nesting logic internally.
        if self.consume_newline() {
            // Normal newline
            self.at_beginning_of_line = true;
            let len = self.byte_index - start_index;
            let span = SourceSpan::new(start_index.into(), len.into());
            return Ok(Some((Token::Newline, span)));
        } else if self.byte_index > start_index {
            // Newline WAS consumed but swallowed (hidden inside nesting).
            // We must loop to get the next real token.
            return self.next_token();
        }

        // 5) Skip mid-line spaces (indentation rules only apply at BOL).
        if matches!(self.current_byte(), Some(b' ')) {
            self.byte_index += 1;
            return self.next_token();
        }

        // 6) Real tokenization
        // We reset start_index here because we might have skipped spaces above
        let start_index = self.byte_index;

        let c = self.current_byte().unwrap(); // Safe because is_eof checked above

        let token = match c {
            // Identifiers and Keywords (start with letter or unicode)
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | 0x80..=0xff => self.consume_identifier_or_keyword(),

            // Numbers (start with digit)
            b'0'..=b'9' => self.consume_number(),

            // Strings (start with quote)
            b'"' => self.consume_string(),

            // Simple Symbols
            b':' => {
                self.byte_index += 1;
                Some(Token::Colon)
            }
            b'(' => {
                self.byte_index += 1;
                Some(Token::LParen)
            }
            b')' => {
                self.byte_index += 1;
                Some(Token::RParen)
            }
            b'[' => {
                self.byte_index += 1;
                Some(Token::LBracket)
            }
            b']' => {
                self.byte_index += 1;
                Some(Token::RBracket)
            }
            b'{' => {
                self.byte_index += 1;
                Some(Token::LBrace)
            }
            b'}' => {
                self.byte_index += 1;
                Some(Token::RBrace)
            }
            b',' => {
                self.byte_index += 1;
                Some(Token::Comma)
            }
            b'@' => {
                self.byte_index += 1;
                Some(Token::At)
            }

            // Dot and Range
            b'.' => {
                // Check for ...
                if self.peek_n_bytes(1) == Some(b'.') && self.peek_n_bytes(2) == Some(b'.') {
                    self.byte_index += 3;
                    Some(Token::Range)
                } else {
                    self.byte_index += 1;
                    Some(Token::Dot)
                }
            }

            // Math & Augmented Assignment
            b'+' => {
                if matches!(self.peek_next_byte(), Some(b'=')) {
                    self.byte_index += 2;
                    Some(Token::PlusEq)
                } else {
                    self.byte_index += 1;
                    Some(Token::Plus)
                }
            }
            b'-' => {
                if matches!(self.peek_next_byte(), Some(b'=')) {
                    self.byte_index += 2;
                    Some(Token::MinusEq)
                } else {
                    self.byte_index += 1;
                    Some(Token::Minus)
                }
            }
            b'*' => {
                if matches!(self.peek_next_byte(), Some(b'=')) {
                    self.byte_index += 2;
                    Some(Token::StarEq)
                } else {
                    self.byte_index += 1;
                    Some(Token::Star)
                }
            }
            b'/' => {
                if matches!(self.peek_next_byte(), Some(b'=')) {
                    self.byte_index += 2;
                    Some(Token::SlashEq)
                } else {
                    self.byte_index += 1;
                    Some(Token::Slash)
                }
            }
            b'%' => {
                self.byte_index += 1;
                Some(Token::Percent)
            }

            // Bitwise
            b'&' => {
                self.byte_index += 1;
                Some(Token::Ampersand)
            }
            b'|' => {
                self.byte_index += 1;
                Some(Token::Pipe)
            }
            b'^' => {
                self.byte_index += 1;
                Some(Token::Caret)
            }
            b'~' => {
                self.byte_index += 1;
                Some(Token::BitwiseNot)
            }

            // Comparisons & Shift
            b'=' => {
                if matches!(self.peek_next_byte(), Some(b'=')) {
                    self.byte_index += 2;
                    Some(Token::EqEq)
                } else {
                    self.byte_index += 1;
                    Some(Token::Equals)
                }
            }
            b'!' => {
                if matches!(self.peek_next_byte(), Some(b'=')) {
                    self.byte_index += 2;
                    Some(Token::BangEq)
                } else {
                    let span = SourceSpan::new(self.byte_index.into(), 1usize.into());
                    self.push_error(LexError::UnexpectedCharacter { span, char: '!' });
                    self.byte_index += 1; // Consume bad char to recover
                    None
                }
            }
            b'<' => {
                let next = self.peek_next_byte();
                if next == Some(b'=') {
                    self.byte_index += 2;
                    Some(Token::LessEq)
                } else if next == Some(b'<') {
                    self.byte_index += 2;
                    Some(Token::ShiftLeft)
                } else {
                    self.byte_index += 1;
                    Some(Token::Less)
                }
            }
            b'>' => {
                let next = self.peek_next_byte();
                if next == Some(b'=') {
                    self.byte_index += 2;
                    Some(Token::GreaterEq)
                } else if next == Some(b'>') {
                    self.byte_index += 2;
                    Some(Token::ShiftRight)
                } else {
                    self.byte_index += 1;
                    Some(Token::Greater)
                }
            }

            // Unknown
            _ => {
                let span = SourceSpan::new(self.byte_index.into(), 1usize.into());
                self.push_error(LexError::UnexpectedCharacter {
                    span,
                    char: c as char,
                });
                self.byte_index += 1; // Consume bad char to recover
                None
            }
        };

        if let Some(tok) = token {
            // Update Nesting
            match tok {
                Token::LParen | Token::LBracket | Token::LBrace => {
                    self.nesting += 1;
                }
                Token::RParen | Token::RBracket | Token::RBrace => {
                    if self.nesting > 0 {
                        self.nesting -= 1;
                    }
                }
                _ => {}
            }

            let len = self.byte_index - start_index;
            let span = SourceSpan::new(start_index.into(), len.into());
            Ok(Some((tok, span)))
        } else {
            // Recurse to get the next real token (e.g. after comment block or error recovery)
            self.next_token()
        }
    }

    fn consume_identifier_or_keyword(&mut self) -> Option<Token> {
        let start = self.byte_index;

        while self.byte_index < self.src.len() {
            let remainder = &self.src[self.byte_index..];
            let c = remainder.chars().next().unwrap();

            if c.is_alphanumeric() || c == '_' || (c > '\x7f' && !c.is_whitespace()) {
                self.byte_index += c.len_utf8();
            } else {
                break;
            }
        }

        let text = &self.src[start..self.byte_index];

        // Check keywords
        match text {
            "A" => Some(Token::Class),
            "An" => Some(Token::An),
            "has" => Some(Token::Has),
            "can" => Some(Token::Can),
            "to" => Some(Token::To),
            "if" => Some(Token::If),
            "else" => Some(Token::Else),
            "while" => Some(Token::While),
            "for" => Some(Token::For),
            "in" => Some(Token::In),
            "return" => Some(Token::Return),
            "break" => Some(Token::Break),
            "continue" => Some(Token::Continue),
            "match" => Some(Token::Match),
            "otherwise" => Some(Token::Otherwise),
            "true" => Some(Token::True),
            "false" => Some(Token::False),
            "nothing" => Some(Token::Nothing),
            "and" => Some(Token::And),
            "or" => Some(Token::Or),
            "not" => Some(Token::Not),
            "await" => Some(Token::Await),
            "spawn" => Some(Token::Spawn),
            "use" => Some(Token::Use),
            "from" => Some(Token::From),
            "public" => Some(Token::Public),
            "private" => Some(Token::Private),
            "its" => Some(Token::Its),
            "changing" => Some(Token::Changing),
            "so" => {
                if self.current_byte() == Some(b':') {
                    self.byte_index += 1; // Consume ':'
                    self.consume_so_comment_block();
                    None // Return None to signal "no token here, keep looking"
                } else {
                    Some(Token::Identifier(text.to_string()))
                }
            }
            _ => Some(Token::Identifier(text.to_string())),
        }
    }

    fn consume_string(&mut self) -> Option<Token> {
        self.byte_index += 1; // Skip opening quote
        let start = self.byte_index;
        let mut string_content = String::new();

        loop {
            // Loop for current segment
            while let Some(b) = self.current_byte() {
                match b {
                    b'"' => {
                        // End of string.
                        self.byte_index += 1; // Skip closing quote
                        return Some(Token::StringLiteral(string_content));
                    }
                    b'{' => {
                        // Interpolation start!
                        // 1. Emit what we have so far as StringStart.
                        let first_token = Token::StringStart(string_content);

                        // Push StringStart
                        let len_so_far = self.byte_index - (start - 1); // include opening quote
                        let span = SourceSpan::new((start - 1).into(), len_so_far.into());
                        self.pending.push_back((first_token, span));

                        self.byte_index += 1; // Consume `{`
                        self.nesting += 1; // Increment nesting!

                        let brace_span =
                            SourceSpan::new((self.byte_index - 1).into(), 1usize.into());
                        self.pending.push_back((Token::LBrace, brace_span));

                        // Now we are inside `{...}`.
                        self.consume_interpolation_content();

                        // Continue to parse rest of string
                        return self.continue_parsing_interpolated_string();
                    }
                    b'\\' => {
                        self.byte_index += 1;
                        if let Some(next_byte) = self.current_byte() {
                            match next_byte {
                                b'n' => string_content.push('\n'),
                                b'r' => string_content.push('\r'),
                                b't' => string_content.push('\t'),
                                b'"' => string_content.push('"'),
                                b'\\' => string_content.push('\\'),
                                b'{' => string_content.push('{'), // Escaped brace
                                b'}' => string_content.push('}'),
                                _ => {
                                    let span = SourceSpan::new(
                                        (self.byte_index - 1).into(),
                                        2usize.into(),
                                    );
                                    self.push_error(LexError::InvalidEscapeSequence {
                                        span,
                                        char: next_byte as char,
                                    });
                                    // Treat as char
                                    string_content.push(next_byte as char);
                                }
                            }
                            self.byte_index += 1;
                        } else {
                            let span =
                                SourceSpan::new(start.into(), (self.byte_index - start).into());
                            self.push_error(LexError::UnterminatedString { span });
                            return None;
                        }
                    }
                    _ => {
                        string_content.push(b as char);
                        self.byte_index += 1;
                    }
                }
            }

            // EOF hit
            let span = SourceSpan::new(start.into(), (self.byte_index - start).into());
            self.push_error(LexError::UnterminatedString { span });
            return None;
        }
    }

    // Helper to continue parsing string parts after the first `{...}`
    fn continue_parsing_interpolated_string(&mut self) -> Option<Token> {
        let mut string_content = String::new();
        let start_index = self.byte_index; // Start of this segment

        loop {
            match self.current_byte() {
                Some(b'"') => {
                    self.byte_index += 1; // consume quote
                    // Push StringEnd
                    let len = self.byte_index - start_index;
                    let span = SourceSpan::new(start_index.into(), len.into());
                    self.pending
                        .push_back((Token::StringEnd(string_content), span));
                    return None; // Return None so next_token pops from pending
                }
                Some(b'{') => {
                    // Another interpolation
                    // Push StringPart
                    let len = self.byte_index - start_index;
                    let span = SourceSpan::new(start_index.into(), len.into());
                    self.pending
                        .push_back((Token::StringPart(string_content), span));

                    self.byte_index += 1; // consume `{`
                    self.nesting += 1; // Increment nesting!

                    let brace_span = SourceSpan::new((self.byte_index - 1).into(), 1usize.into());
                    self.pending.push_back((Token::LBrace, brace_span));

                    self.consume_interpolation_content();

                    // Recurse / Loop
                    return self.continue_parsing_interpolated_string();
                }
                Some(b'\\') => {
                    self.byte_index += 1;
                    if let Some(next) = self.current_byte() {
                        match next {
                            b'n' => string_content.push('\n'),
                            b'r' => string_content.push('\r'),
                            b't' => string_content.push('\t'),
                            b'"' => string_content.push('"'),
                            b'\\' => string_content.push('\\'),
                            b'{' => string_content.push('{'),
                            b'}' => string_content.push('}'),
                            _ => {
                                let span =
                                    SourceSpan::new((self.byte_index - 1).into(), 2usize.into());
                                self.push_error(LexError::InvalidEscapeSequence {
                                    span,
                                    char: next as char,
                                });
                                string_content.push(next as char);
                            }
                        }
                        self.byte_index += 1;
                    } else {
                        self.push_error(LexError::UnterminatedString {
                            span: SourceSpan::new(start_index.into(), 1usize.into()),
                        });
                        return None;
                    }
                }
                Some(b) => {
                    string_content.push(b as char);
                    self.byte_index += 1;
                }
                None => {
                    self.push_error(LexError::UnterminatedString {
                        span: SourceSpan::new(
                            start_index.into(),
                            (self.byte_index - start_index).into(),
                        ),
                    });
                    return None;
                }
            }
        }
    }

    fn consume_interpolation_content(&mut self) {
        // We are inside `{`. We need to parse until `}`.
        // Allowed: spaces, identifiers, dots.

        loop {
            // Skip spaces
            while matches!(self.current_byte(), Some(b' ')) {
                self.byte_index += 1;
            }

            match self.current_byte() {
                Some(b'}') => {
                    self.byte_index += 1;
                    if self.nesting > 0 {
                        self.nesting -= 1;
                    } // Decrement nesting!

                    let span = SourceSpan::new((self.byte_index - 1).into(), 1usize.into());
                    self.pending.push_back((Token::RBrace, span));
                    return;
                }
                Some(b'.') => {
                    let start = self.byte_index;
                    self.byte_index += 1;
                    let span = SourceSpan::new(start.into(), 1usize.into());
                    self.pending.push_back((Token::Dot, span));
                }
                Some(c) if c.is_ascii_alphanumeric() || c == b'_' => {
                    let start = self.byte_index;
                    while let Some(b) = self.current_byte() {
                        if b.is_ascii_alphanumeric() || b == b'_' {
                            self.byte_index += 1;
                        } else {
                            break;
                        }
                    }
                    let text = &self.src[start..self.byte_index];
                    let span = SourceSpan::new(start.into(), (self.byte_index - start).into());
                    self.pending
                        .push_back((Token::Identifier(text.to_string()), span));
                }
                Some(c) => {
                    let span = SourceSpan::new(self.byte_index.into(), 1usize.into());
                    self.push_error(LexError::UnexpectedCharacter {
                        span,
                        char: c as char,
                    });
                    self.byte_index += 1; // Consume bad char
                }
                None => {
                    let span = SourceSpan::new(self.byte_index.into(), 0usize.into());
                    self.push_error(LexError::UnterminatedString { span });
                    return;
                }
            }
        }
    }

    fn consume_number(&mut self) -> Option<Token> {
        let start = self.byte_index;

        // Check for Hex/Bin prefixes (0x, 0b, 0o)
        if matches!(self.current_byte(), Some(b'0')) {
            match self.peek_next_byte() {
                Some(b'x') | Some(b'X') => {
                    self.byte_index += 2; // skip 0x
                    let num_start = self.byte_index;
                    // Consume hex digits
                    while let Some(b) = self.current_byte() {
                        if b.is_ascii_hexdigit() {
                            self.byte_index += 1;
                        } else if b == b'_' {
                            self.byte_index += 1; // Skip underscore
                        } else {
                            break;
                        }
                    }
                    let text = &self.src[num_start..self.byte_index];
                    let cleaned = text.replace('_', "");
                    if let Ok(num) = i64::from_str_radix(&cleaned, 16) {
                        return Some(Token::Integer(num));
                    } else {
                        // Overflow or invalid?
                        // Fallback to error or just return what we have (parser error later)
                    }
                }
                Some(b'b') | Some(b'B') => {
                    self.byte_index += 2; // skip 0b
                    let num_start = self.byte_index;
                    while let Some(b) = self.current_byte() {
                        if b == b'0' || b == b'1' {
                            self.byte_index += 1;
                        } else if b == b'_' {
                            self.byte_index += 1;
                        } else {
                            break;
                        }
                    }
                    let text = &self.src[num_start..self.byte_index];
                    let cleaned = text.replace('_', "");
                    if let Ok(num) = i64::from_str_radix(&cleaned, 2) {
                        return Some(Token::Integer(num));
                    }
                }
                Some(b'o') | Some(b'O') => {
                    self.byte_index += 2; // skip 0o
                    let num_start = self.byte_index;
                    while let Some(b) = self.current_byte() {
                        if matches!(b, b'0'..=b'7') {
                            self.byte_index += 1;
                        } else if b == b'_' {
                            self.byte_index += 1;
                        } else {
                            break;
                        }
                    }
                    let text = &self.src[num_start..self.byte_index];
                    let cleaned = text.replace('_', "");
                    if let Ok(num) = i64::from_str_radix(&cleaned, 8) {
                        return Some(Token::Integer(num));
                    }
                }
                _ => {} // Fall through to decimal
            }
        }

        // Decimal loop
        let mut is_float = false;
        while let Some(b) = self.current_byte() {
            if b.is_ascii_digit() {
                self.byte_index += 1;
            } else if b == b'_' {
                self.byte_index += 1;
            } else {
                break;
            }
        }

        // Check for fractional part
        if matches!(self.current_byte(), Some(b'.')) {
            // Need to peek ahead to ensure it's a number, not a method call or range.
            if let Some(next) = self.peek_next_byte() {
                if next.is_ascii_digit() {
                    is_float = true;
                    self.byte_index += 1; // Consume dot

                    // Consume fractional digits
                    while let Some(b) = self.current_byte() {
                        if b.is_ascii_digit() {
                            self.byte_index += 1;
                        } else if b == b'_' {
                            self.byte_index += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        // Check for exponent (e.g., 1e10, 1.5e-5)
        if let Some(b) = self.current_byte() {
            if b == b'e' || b == b'E' {
                // Look ahead. Must be digit or + or -
                let offset = 1;
                let next = self.peek_n_bytes(offset);
                let is_exp = if matches!(next, Some(b'+') | Some(b'-')) {
                    matches!(self.peek_n_bytes(offset+1), Some(d) if d.is_ascii_digit())
                } else {
                    matches!(next, Some(d) if d.is_ascii_digit())
                };

                if is_exp {
                    is_float = true;
                    self.byte_index += 1; // Consume 'e'
                    if matches!(self.current_byte(), Some(b'+') | Some(b'-')) {
                        self.byte_index += 1;
                    }
                    while let Some(b) = self.current_byte() {
                        if b.is_ascii_digit() {
                            self.byte_index += 1;
                        } else if b == b'_' {
                            self.byte_index += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        let text = &self.src[start..self.byte_index];
        let cleaned = text.replace('_', "");

        if is_float {
            let num: f64 = cleaned.parse().unwrap_or(0.0);
            Some(Token::Float(num))
        } else {
            let num: i64 = cleaned.parse().unwrap_or(0);
            Some(Token::Integer(num))
        }
    }

    fn handle_bol_indentation(&mut self) {
        // We are at the logical start of a line.
        let indent_start = self.byte_index;
        let mut spaces = 0usize;

        // Count indentation (spaces only; tabs error)
        while let Some(b) = self.current_byte() {
            match b {
                b' ' => {
                    spaces += 1;
                    self.byte_index += 1;
                }
                b'\t' => {
                    let span = SourceSpan::new(self.byte_index.into(), 1usize.into());
                    self.push_error(LexError::UnexpectedTabCharacter { span });
                    self.byte_index += 1; // Consume bad tab
                }
                _ => break,
            }
        }

        // Blank line or comment-only line: indentation doesn't count.
        match self.current_byte() {
            None | Some(b'\n') | Some(b'\r') => {
                self.at_beginning_of_line = true;
                return;
            }
            _ => {}
        }

        // First non-blank line must be unindented
        if self.no_data_consumed && spaces > 0 {
            let span = SourceSpan::new(indent_start.into(), spaces.into());
            self.push_error(LexError::UnexpectedTopLevelIndent { span });
        }

        // Multiple-of-4 rule
        if (spaces % 4) != 0 {
            let span = SourceSpan::new(indent_start.into(), spaces.into());
            self.push_error(LexError::IndentNotMultipleOfFour { span });
        }

        // Emit INDENT/DEDENT tokens based on indent stack
        let current = *self.indent_stack.last().unwrap();

        // Helper to make span for indentation
        let make_span =
            |len: usize| -> SourceSpan { SourceSpan::new(indent_start.into(), len.into()) };

        if spaces > current {
            self.indent_stack.push(spaces);
            // Indent span covers all spaces
            self.pending.push_back((Token::Indent, make_span(spaces)));
        } else if spaces < current {
            while let Some(&top) = self.indent_stack.last() {
                if top == spaces {
                    break;
                }
                self.indent_stack.pop();
                // Dedent span is technically zero-width at current point,
                // OR we could say it "matches" the indentation of this line.
                // Let's use the whitespace on this line for the span.
                self.pending.push_back((Token::Dedent, make_span(spaces)));
            }
            if *self.indent_stack.last().unwrap() != spaces {
                let span = SourceSpan::new(indent_start.into(), spaces.into());
                self.push_error(LexError::InconsistentIndent { span });
            }
        }

        self.no_data_consumed = false;
        self.at_beginning_of_line = false;
    }

    pub fn lex(&mut self) -> (Vec<SpannedToken>, Vec<LexError>) {
        let mut out = Vec::new();
        while let Ok(Some(tok)) = self.next_token() {
            out.push(tok);
        }
        (out, self.errors.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::Token;

    // Updated helper to discard errors for existing tests
    fn strip_spans(tokens: Vec<SpannedToken>) -> Vec<Token> {
        tokens.into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn test_basic_lexing() {
        // Modified input to use simple identifier in interpolation
        let input = r#"A Whale:
    has:
        name: String

    can swim(distance: Number):
        print("Hi! My name is {name} and I can swim {distance}")

to make_moby_swim():
    moby = Whale(name="moby")
    moby.swim(500)

make_moby_swim()
"#;
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty(), "Lexing failed with errors: {:?}", errors);
        let tokens = strip_spans(tokens);

        // simple sanity check of the first few tokens
        assert_eq!(tokens[0], Token::Class); // A
        assert_eq!(tokens[1], Token::Identifier("Whale".to_string()));
        assert_eq!(tokens[2], Token::Colon);
        assert_eq!(tokens[3], Token::Newline);
        assert_eq!(tokens[4], Token::Indent);
        assert_eq!(tokens[5], Token::Has);
    }

    #[test]
    fn test_extended_features() {
        let input = r#"
so: This is a comment
if x == 3.14:
    return nothing
else:
    val = true and false
"#;
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);

        let valid_tokens: Vec<Token> = tokens
            .into_iter()
            .filter(|t| {
                !matches!(
                    t,
                    Token::Newline | Token::Indent | Token::Dedent | Token::Eof
                )
            })
            .collect();

        assert_eq!(valid_tokens[0], Token::If);
        assert_eq!(valid_tokens[1], Token::Identifier("x".to_string()));
        assert_eq!(valid_tokens[2], Token::EqEq);
        assert_eq!(valid_tokens[3], Token::Float(3.14)); // Changed from Number(3.14)
        assert_eq!(valid_tokens[4], Token::Colon);
        assert_eq!(valid_tokens[5], Token::Return);
        assert_eq!(valid_tokens[6], Token::Nothing);
        assert_eq!(valid_tokens[7], Token::Else);
        assert_eq!(valid_tokens[8], Token::Colon);
        assert_eq!(valid_tokens[9], Token::Identifier("val".to_string()));
        assert_eq!(valid_tokens[10], Token::Equals);
        assert_eq!(valid_tokens[11], Token::True);
        assert_eq!(valid_tokens[12], Token::And);
        assert_eq!(valid_tokens[13], Token::False);
    }

    #[test]
    fn test_async_and_comments() {
        let input = r#"
to async_ops():
    await some_task()
    so: This is an inline comment block
    x = 1
    so:
        This is a multiline comment block
        It spans multiple lines
        
        And has blank lines
    
    return x
"#;
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);

        let valid_tokens: Vec<Token> = tokens
            .into_iter()
            .filter(|t| {
                !matches!(
                    t,
                    Token::Newline | Token::Indent | Token::Dedent | Token::Eof
                )
            })
            .collect();

        assert_eq!(valid_tokens[0], Token::To);
        assert_eq!(valid_tokens[1], Token::Identifier("async_ops".to_string()));
        assert_eq!(valid_tokens[2], Token::LParen);
        assert_eq!(valid_tokens[3], Token::RParen);
        assert_eq!(valid_tokens[4], Token::Colon);
        assert_eq!(valid_tokens[5], Token::Await);
        assert_eq!(valid_tokens[6], Token::Identifier("some_task".to_string()));
        assert_eq!(valid_tokens[7], Token::LParen);
        assert_eq!(valid_tokens[8], Token::RParen);

        // "so: inline" should be gone.
        // x = 1
        assert_eq!(valid_tokens[9], Token::Identifier("x".to_string()));
        assert_eq!(valid_tokens[10], Token::Equals);
        assert_eq!(valid_tokens[11], Token::Integer(1)); // Changed from Number(1.0)

        // "so: block" should be gone.
        // return x
        assert_eq!(valid_tokens[12], Token::Return);
        assert_eq!(valid_tokens[13], Token::Identifier("x".to_string()));
    }

    #[test]
    fn test_spans() {
        let input = "x = 42";
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());

        // 0: Identifier(x) at 0..1
        assert_eq!(tokens[0].0, Token::Identifier("x".to_string()));
        assert_eq!(tokens[0].1.offset(), 0);
        assert_eq!(tokens[0].1.len(), 1);

        // 1: Equals at 2..3 (skipped space at 1)
        assert_eq!(tokens[1].0, Token::Equals);
        assert_eq!(tokens[1].1.offset(), 2);
        assert_eq!(tokens[1].1.len(), 1);

        // 2: Integer(42) at 4..6
        assert_eq!(tokens[2].0, Token::Integer(42)); // Changed from Number(42.0)
        assert_eq!(tokens[2].1.offset(), 4);
        assert_eq!(tokens[2].1.len(), 2);
    }

    #[test]
    fn test_string_escapes() {
        let input = r#"print("Line1\nLine2\t\"Quoted\"\\")"#;
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);

        // 0: Identifier(print)
        assert_eq!(tokens[0], Token::Identifier("print".to_string()));

        // 1: LParen
        assert_eq!(tokens[1], Token::LParen);

        // 2: StringLiteral
        // Expected value: Line1
        //                 Line2	"Quoted"\
        let expected = "Line1\nLine2\t\"Quoted\"\\";
        assert_eq!(tokens[2], Token::StringLiteral(expected.to_string()));
    }

    #[test]
    fn test_new_operators() {
        let input = r#"
x += 1
y -= 2
z = a & b | c ^ d
val = i << 2 >> 1
for i in 0...10:
    pass
"#;
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);
        let valid_tokens: Vec<Token> = tokens
            .into_iter()
            .filter(|t| {
                !matches!(
                    t,
                    Token::Newline | Token::Indent | Token::Dedent | Token::Eof
                )
            })
            .collect();

        // x += 1
        assert_eq!(valid_tokens[0], Token::Identifier("x".to_string()));
        assert_eq!(valid_tokens[1], Token::PlusEq);
        assert_eq!(valid_tokens[2], Token::Integer(1)); // Changed

        // y -= 2
        assert_eq!(valid_tokens[3], Token::Identifier("y".to_string()));
        assert_eq!(valid_tokens[4], Token::MinusEq);
        assert_eq!(valid_tokens[5], Token::Integer(2)); // Changed

        // z = a & b | c ^ d
        assert_eq!(valid_tokens[6], Token::Identifier("z".to_string()));
        assert_eq!(valid_tokens[7], Token::Equals);
        assert_eq!(valid_tokens[8], Token::Identifier("a".to_string()));
        assert_eq!(valid_tokens[9], Token::Ampersand);
        assert_eq!(valid_tokens[10], Token::Identifier("b".to_string()));
        assert_eq!(valid_tokens[11], Token::Pipe);
        assert_eq!(valid_tokens[12], Token::Identifier("c".to_string()));
        assert_eq!(valid_tokens[13], Token::Caret);
        assert_eq!(valid_tokens[14], Token::Identifier("d".to_string()));

        // val = i << 2 >> 1
        // Skip valid_tokens[15]..[17] (val = i)
        assert_eq!(valid_tokens[18], Token::ShiftLeft);
        // ...
        assert_eq!(valid_tokens[20], Token::ShiftRight);

        // for i in 0...10
        // ...
        // Index is getting tricky, let's just find the Range token
        assert!(valid_tokens.contains(&Token::Range));
        assert!(valid_tokens.contains(&Token::In));
        assert!(valid_tokens.contains(&Token::For));
    }

    #[test]
    fn test_interpolation() {
        let input = r#"spawn "Hello {name} and {obj.prop}!""#;
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);

        assert_eq!(tokens[0], Token::Spawn);
        assert_eq!(tokens[1], Token::StringStart("Hello ".to_string()));
        assert_eq!(tokens[2], Token::LBrace);
        assert_eq!(tokens[3], Token::Identifier("name".to_string()));
        assert_eq!(tokens[4], Token::RBrace);
        assert_eq!(tokens[5], Token::StringPart(" and ".to_string()));
        assert_eq!(tokens[6], Token::LBrace);
        assert_eq!(tokens[7], Token::Identifier("obj".to_string()));
        assert_eq!(tokens[8], Token::Dot);
        assert_eq!(tokens[9], Token::Identifier("prop".to_string()));
        assert_eq!(tokens[10], Token::RBrace);
        assert_eq!(tokens[11], Token::StringEnd("!".to_string()));
    }

    #[test]
    fn test_multiline_structures() {
        let input = r#"
func(
    arg1,
    arg2
)
list = [
    1,
    2,
    3
]
"#;
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);

        // Filter whitespace for easier checking
        let valid_tokens: Vec<Token> = tokens
            .into_iter()
            .filter(|t| {
                !matches!(
                    t,
                    Token::Newline | Token::Indent | Token::Dedent | Token::Eof
                )
            })
            .collect();

        // func(arg1, arg2)
        assert_eq!(valid_tokens[0], Token::Identifier("func".to_string()));
        assert_eq!(valid_tokens[1], Token::LParen);
        assert_eq!(valid_tokens[2], Token::Identifier("arg1".to_string()));
        assert_eq!(valid_tokens[3], Token::Comma);
        assert_eq!(valid_tokens[4], Token::Identifier("arg2".to_string()));
        assert_eq!(valid_tokens[5], Token::RParen);

        // list = [1, 2, 3]
        assert_eq!(valid_tokens[6], Token::Identifier("list".to_string()));
        assert_eq!(valid_tokens[7], Token::Equals);
        assert_eq!(valid_tokens[8], Token::LBracket);
        assert_eq!(valid_tokens[9], Token::Integer(1)); // Changed
        assert_eq!(valid_tokens[10], Token::Comma);
        assert_eq!(valid_tokens[11], Token::Integer(2));
        assert_eq!(valid_tokens[12], Token::Comma);
        assert_eq!(valid_tokens[13], Token::Integer(3));
        assert_eq!(valid_tokens[14], Token::RBracket);
    }

    #[test]
    fn test_multiline_invalid_indent() {
        let input = r#"
func(
arg1
)
"#;
        let mut lexer = Lexer::new(input);
        let (_, errors) = lexer.lex();
        assert!(!errors.is_empty()); // Expect errors
    }

    #[test]
    fn test_numeric_literals() {
        let input = "123 0xFF 0b1010 3.14 1_000 1.5e-10";
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);

        assert_eq!(tokens[0], Token::Integer(123));
        assert_eq!(tokens[1], Token::Integer(255)); // 0xFF
        assert_eq!(tokens[2], Token::Integer(10)); // 0b1010
        assert_eq!(tokens[3], Token::Float(3.14));
        assert_eq!(tokens[4], Token::Integer(1000));
        assert_eq!(tokens[5], Token::Float(1.5e-10));
    }

    #[test]
    fn test_modularity() {
        let input = "use std from core @test";
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);

        assert_eq!(tokens[0], Token::Use);
        assert_eq!(tokens[1], Token::Identifier("std".to_string()));
        assert_eq!(tokens[2], Token::From);
        assert_eq!(tokens[3], Token::Identifier("core".to_string()));
        assert_eq!(tokens[4], Token::At);
        assert_eq!(tokens[5], Token::Identifier("test".to_string()));
    }

    #[test]
    fn test_error_recovery() {
        // Valid x = 1; Invalid $; Valid y = 2
        let input = "x = 1 $ y = 2";
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();

        // Should have 1 error
        assert_eq!(errors.len(), 1);

        // Should still produce valid tokens for x and y
        let tokens = strip_spans(tokens);
        // x = 1
        assert_eq!(tokens[0], Token::Identifier("x".to_string()));
        assert_eq!(tokens[1], Token::Equals);
        assert_eq!(tokens[2], Token::Integer(1));

        // y = 2 (skipping $)
        assert_eq!(tokens[3], Token::Identifier("y".to_string()));
        assert_eq!(tokens[4], Token::Equals);
        assert_eq!(tokens[5], Token::Integer(2));
    }

    #[test]
    fn test_final_keywords() {
        let input = r#"
public changing x = 1
match x:
    1: break
    otherwise: continue
its.name
An Apple
"#;
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty());
        let tokens = strip_spans(tokens);
        let valid_tokens: Vec<Token> = tokens
            .into_iter()
            .filter(|t| {
                !matches!(
                    t,
                    Token::Newline | Token::Indent | Token::Dedent | Token::Eof
                )
            })
            .collect();

        // public changing x = 1
        assert_eq!(valid_tokens[0], Token::Public);
        assert_eq!(valid_tokens[1], Token::Changing);
        assert_eq!(valid_tokens[2], Token::Identifier("x".to_string()));
        assert_eq!(valid_tokens[3], Token::Equals);
        assert_eq!(valid_tokens[4], Token::Integer(1));

        // match x:
        assert_eq!(valid_tokens[5], Token::Match);
        assert_eq!(valid_tokens[6], Token::Identifier("x".to_string()));
        assert_eq!(valid_tokens[7], Token::Colon);

        // 1: break
        assert_eq!(valid_tokens[8], Token::Integer(1));
        assert_eq!(valid_tokens[9], Token::Colon);
        assert_eq!(valid_tokens[10], Token::Break);

        // otherwise: continue
        assert_eq!(valid_tokens[11], Token::Otherwise);
        assert_eq!(valid_tokens[12], Token::Colon);
        assert_eq!(valid_tokens[13], Token::Continue);

        // its.name
        assert_eq!(valid_tokens[14], Token::Its);
        assert_eq!(valid_tokens[15], Token::Dot);
        assert_eq!(valid_tokens[16], Token::Identifier("name".to_string()));

        // An Apple
        assert_eq!(valid_tokens[17], Token::An);
        // "Apple" is an identifier. It is not the keyword "A" (Class).
        assert_eq!(valid_tokens[18], Token::Identifier("Apple".to_string()));
    }

    #[test]
    fn test_unicode_and_bitwise() {
        let input = "🚀 = ~0b1010";
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty(), "Lexing failed: {:?}", errors);
        let tokens = strip_spans(tokens);

        // 0: Identifier("🚀")
        assert_eq!(tokens[0], Token::Identifier("🚀".to_string()));
        // 1: Equals
        assert_eq!(tokens[1], Token::Equals);
        // 2: BitwiseNot
        assert_eq!(tokens[2], Token::BitwiseNot);
        // 3: Integer(10)
        assert_eq!(tokens[3], Token::Integer(10));
    }
}
