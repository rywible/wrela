use super::errors::LexError;
use super::tokens::{SpannedToken, Token};
use miette::{Result, SourceSpan, bail};
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

    #[allow(dead_code)]
    fn peek_n_bytes(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.byte_index + n).copied()
    }

    #[allow(dead_code)]
    fn next_byte(&mut self) -> Option<u8> {
        let b = self.current_byte()?;
        self.byte_index += 1;
        Some(b)
    }

    fn consume_newline(&mut self) -> bool {
        match (self.current_byte(), self.peek_next_byte()) {
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
                // optional: accept lone '\r' as newline, or treat as error
                self.byte_index += 1;
                self.line_number += 1;
                true
            }
            _ => false,
        }
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

    fn consume_so_comment_block(&mut self) -> Result<()> {
        // We just consumed "so:".
        // 1. Consume the rest of the current line.
        self.consume_comment_line();

        // At this point, we are at a newline or EOF.
        // If EOF, we are done.
        if self.is_eof() {
            return Ok(());
        }

        // We need to capture the current scope's indent level.
        // Any subsequent line with indent > current_scope is part of the comment.
        let current_scope_indent = *self.indent_stack.last().unwrap();

        // Loop to consume indented lines
        loop {
            // We expect to be at a newline here (from consume_comment_line or loop end)
            // But next_token loop usually handles newlines. Here we must handle them to "peek" the next line's indent.

            // Check if we are actually at a newline
            let newline_consumed = self.consume_newline();
            if !newline_consumed && !self.is_eof() {
                // Should not happen if logic is correct, unless we started mid-line?
                // If we are mid-line (e.g. after 'so:'), consume_comment_line put us at \n.
            }

            if self.is_eof() {
                break;
            }

            // Now we are at the start of a new line.
            // We need to count its indentation without advancing permanently if it's NOT part of the block.
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
                        // For comment scanning, we can be strict or loose.
                        // Let's be strict to match the language rule, but maybe just fail matching?
                        // If we see tab, it's definitely not a valid indent for code, so we can stop consuming block?
                        // Or we bail error? Let's just treat it as non-space char for now.
                        break;
                    }
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
                // We also enforce 4-space rule strictly? The user said "follows the 4 space rule".
                // But since it's a comment, maybe we just care that it IS indented?
                // "make sure ... it still follows the 4 space rule" -> implies we should enforce/expect it?
                // For a comment, enforcing it seems strict, but let's assume valid indentation > current.

                // Advance real byte_index to after indentation
                self.byte_index = temp_index;

                // Consume rest of line
                self.consume_comment_line();
            } else {
                // Not indented deeper. The comment block ends here.
                // We do NOT consume this line.
                // However, we consumed the newline *before* this line at the top of the loop.
                // This is tricky. The main `next_token` loop expects to handle newlines.
                // If we consumed the newline, `at_beginning_of_line` should be set to true.
                self.at_beginning_of_line = true;
                break;
            }
        }

        Ok(())
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
            self.handle_bol_indentation()?;
            if let Some(tok) = self.pending.pop_front() {
                return Ok(Some(tok));
            }
        }

        // Capture start for newline or next token
        let start_index = self.byte_index;

        // 4) Newlines produce a token and put us back at BOL.
        if self.consume_newline() {
            self.at_beginning_of_line = true;
            let len = self.byte_index - start_index;
            let span = SourceSpan::new(start_index.into(), len.into());
            return Ok(Some((Token::Newline, span)));
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
            // Identifiers and Keywords (start with letter)
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.consume_identifier_or_keyword()?,

            // Numbers (start with digit)
            b'0'..=b'9' => self.consume_number()?,

            // Strings (start with quote)
            b'"' => self.consume_string()?,

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
            b'.' => {
                self.byte_index += 1;
                Some(Token::Dot)
            }
            b',' => {
                self.byte_index += 1;
                Some(Token::Comma)
            }

            // Math
            b'+' => {
                self.byte_index += 1;
                Some(Token::Plus)
            }
            b'-' => {
                self.byte_index += 1;
                Some(Token::Minus)
            }
            b'*' => {
                self.byte_index += 1;
                Some(Token::Star)
            }
            b'/' => {
                self.byte_index += 1;
                Some(Token::Slash)
            }
            b'%' => {
                self.byte_index += 1;
                Some(Token::Percent)
            }

            // Multi-char operators
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
                    bail!(LexError::UnexpectedCharacter { span, char: '!' });
                }
            }
            b'<' => {
                if matches!(self.peek_next_byte(), Some(b'=')) {
                    self.byte_index += 2;
                    Some(Token::LessEq)
                } else {
                    self.byte_index += 1;
                    Some(Token::Less)
                }
            }
            b'>' => {
                if matches!(self.peek_next_byte(), Some(b'=')) {
                    self.byte_index += 2;
                    Some(Token::GreaterEq)
                } else {
                    self.byte_index += 1;
                    Some(Token::Greater)
                }
            }

            // Unknown
            _ => {
                let span = SourceSpan::new(self.byte_index.into(), 1usize.into());
                bail!(LexError::UnexpectedCharacter {
                    span,
                    char: c as char
                });
            }
        };

        if let Some(tok) = token {
            let len = self.byte_index - start_index;
            let span = SourceSpan::new(start_index.into(), len.into());
            Ok(Some((tok, span)))
        } else {
            // This happens if consume_identifier_or_keyword consumed a comment block
            // In that case, we recurse to get the next real token
            self.next_token()
        }
    }

    fn consume_identifier_or_keyword(&mut self) -> Result<Option<Token>> {
        let start = self.byte_index;

        // Advance while char is alphanumeric or _
        while let Some(b) = self.current_byte() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.byte_index += 1;
            } else {
                break;
            }
        }

        let text = &self.src[start..self.byte_index];

        // Check keywords
        match text {
            "A" => Ok(Some(Token::Class)),
            "has" => Ok(Some(Token::Has)),
            "can" => Ok(Some(Token::Can)),
            "to" => Ok(Some(Token::To)),
            "if" => Ok(Some(Token::If)),
            "else" => Ok(Some(Token::Else)),
            "while" => Ok(Some(Token::While)),
            "for" => Ok(Some(Token::For)),
            "return" => Ok(Some(Token::Return)),
            "true" => Ok(Some(Token::True)),
            "false" => Ok(Some(Token::False)),
            "nothing" => Ok(Some(Token::Nothing)),
            "and" => Ok(Some(Token::And)),
            "or" => Ok(Some(Token::Or)),
            "not" => Ok(Some(Token::Not)),
            "await" => Ok(Some(Token::Await)),
            "so" => {
                if self.current_byte() == Some(b':') {
                    self.byte_index += 1; // Consume ':'
                    self.consume_so_comment_block()?;
                    Ok(None) // Return None to signal "no token here, keep looking"
                } else {
                    Ok(Some(Token::Identifier(text.to_string())))
                }
            }
            _ => Ok(Some(Token::Identifier(text.to_string()))),
        }
    }

    fn consume_string(&mut self) -> Result<Option<Token>> {
        self.byte_index += 1; // Skip opening quote
        let start = self.byte_index;

        while let Some(b) = self.current_byte() {
            if b == b'"' {
                let text = &self.src[start..self.byte_index];
                self.byte_index += 1; // Skip closing quote
                return Ok(Some(Token::StringLiteral(text.to_string())));
            }
            self.byte_index += 1;
        }

        // If loop finishes, we hit EOF without closing quote
        bail!(LexError::UnterminatedString {
            span: SourceSpan::new(start.into(), (self.byte_index - start).into())
        });
    }

    fn consume_number(&mut self) -> Result<Option<Token>> {
        let start = self.byte_index;

        // Consume integer part
        while let Some(b) = self.current_byte() {
            if b.is_ascii_digit() {
                self.byte_index += 1;
            } else {
                break;
            }
        }

        // Check for fractional part
        if matches!(self.current_byte(), Some(b'.')) {
            // Need to peek ahead to ensure it's a number, not a method call (e.g. 1.toString())
            if let Some(next) = self.peek_next_byte() {
                if next.is_ascii_digit() {
                    self.byte_index += 1; // Consume dot

                    // Consume fractional digits
                    while let Some(b) = self.current_byte() {
                        if b.is_ascii_digit() {
                            self.byte_index += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        let text = &self.src[start..self.byte_index];
        let num: f64 = text.parse().unwrap();
        Ok(Some(Token::Number(num)))
    }

    fn handle_bol_indentation(&mut self) -> Result<()> {
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
                    bail!(LexError::UnexpectedTabCharacter { span });
                }
                _ => break,
            }
        }

        // Blank line or comment-only line: indentation doesn't count.
        // If we see a comment start '#' or newline, we treat it as blank for indent purposes.
        match self.current_byte() {
            None | Some(b'\n') | Some(b'\r') => {
                // Stay at BOL; consumer will handle the newline or comment.
                // NOTE: If it's a comment, `next_token` will consume it and recurse.
                // But we need to ensure we don't treat it as "data consumed" yet.
                self.at_beginning_of_line = true;
                return Ok(());
            }
            _ => {}
        }

        // First non-blank line must be unindented
        if self.no_data_consumed && spaces > 0 {
            let span = SourceSpan::new(indent_start.into(), spaces.into());
            bail!(LexError::UnexpectedTopLevelIndent { span });
        }

        // Multiple-of-4 rule
        if (spaces % 4) != 0 {
            let span = SourceSpan::new(indent_start.into(), spaces.into());
            bail!(LexError::IndentNotMultipleOfFour { span });
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
                bail!(LexError::InconsistentIndent { span });
            }
        }

        self.no_data_consumed = false;
        self.at_beginning_of_line = false;
        Ok(())
    }

    pub fn lex(&mut self) -> Result<Vec<SpannedToken>> {
        let mut out = Vec::new();
        while let Some(tok) = self.next_token()? {
            out.push(tok);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::Token;

    fn strip_spans(tokens: Vec<SpannedToken>) -> Vec<Token> {
        tokens.into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn test_basic_lexing() {
        let input = r#"A Whale:
    has:
        name: String

    can swim(distance: Number):
        print("Hi! My name is {its.name} and I can swim {distance.toString()}")

to make_moby_swim():
    moby = Whale(name="moby")
    moby.swim(500)

make_moby_swim()
"#;
        let mut lexer = Lexer::new(input);
        let tokens = strip_spans(lexer.lex().unwrap());

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
        let tokens = strip_spans(lexer.lex().unwrap());

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
        assert_eq!(valid_tokens[3], Token::Number(3.14));
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
        let tokens = strip_spans(lexer.lex().unwrap());

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
        assert_eq!(valid_tokens[11], Token::Number(1.0));

        // "so: block" should be gone.
        // return x
        assert_eq!(valid_tokens[12], Token::Return);
        assert_eq!(valid_tokens[13], Token::Identifier("x".to_string()));
    }

    #[test]
    fn test_spans() {
        let input = "x = 42";
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex().unwrap();

        // 0: Identifier(x) at 0..1
        assert_eq!(tokens[0].0, Token::Identifier("x".to_string()));
        assert_eq!(tokens[0].1.offset(), 0);
        assert_eq!(tokens[0].1.len(), 1);

        // 1: Equals at 2..3 (skipped space at 1)
        assert_eq!(tokens[1].0, Token::Equals);
        assert_eq!(tokens[1].1.offset(), 2);
        assert_eq!(tokens[1].1.len(), 1);

        // 2: Number(42) at 4..6
        assert_eq!(tokens[2].0, Token::Number(42.0));
        assert_eq!(tokens[2].1.offset(), 4);
        assert_eq!(tokens[2].1.len(), 2);
    }
}
