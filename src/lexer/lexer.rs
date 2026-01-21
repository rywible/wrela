use super::errors::LexError;
use super::tokens::Token;
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
    pending: VecDeque<Token>,
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

    // #[allow(dead_code)]
    // fn peek_n_bytes(&self, n: usize) -> Option<u8> {
    //     self.bytes.get(self.byte_index + n).copied()
    // }

    // #[allow(dead_code)]
    // fn next_byte(&mut self) -> Option<u8> {
    //     let b = self.current_byte()?;
    //     self.byte_index += 1;
    //     Some(b)
    // }

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

    pub fn next_token(&mut self) -> Result<Option<Token>> {
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
                    self.pending.push_back(Token::Dedent);
                }
                self.pending.push_back(Token::Eof);
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

        // 4) Newlines produce a token and put us back at BOL.
        if self.consume_newline() {
            self.at_beginning_of_line = true;
            return Ok(Some(Token::Newline));
        }

        // 5) Skip mid-line spaces (indentation rules only apply at BOL).
        if matches!(self.current_byte(), Some(b' ')) {
            self.byte_index += 1;
            return self.next_token();
        }

        // 6) Real tokenization
        let c = self.current_byte().unwrap(); // Safe because is_eof checked above

        match c {
            // Identifiers and Keywords (start with letter)
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.consume_identifier_or_keyword(),

            // Numbers (start with digit)
            b'0'..=b'9' => self.consume_number(),

            // Strings (start with quote)
            b'"' => self.consume_string(),

            // Simple Symbols
            b':' => {
                self.byte_index += 1;
                Ok(Some(Token::Colon))
            }
            b'(' => {
                self.byte_index += 1;
                Ok(Some(Token::LParen))
            }
            b')' => {
                self.byte_index += 1;
                Ok(Some(Token::RParen))
            }
            b'.' => {
                self.byte_index += 1;
                Ok(Some(Token::Dot))
            }
            b'=' => {
                self.byte_index += 1;
                Ok(Some(Token::Equals))
            }
            b',' => {
                self.byte_index += 1;
                Ok(Some(Token::Comma))
            }

            // Unknown
            _ => {
                let span = SourceSpan::new(self.byte_index.into(), 1usize.into());
                bail!(LexError::UnexpectedCharacter {
                    span,
                    char: c as char
                });
            }
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
        let token = match text {
            "A" => Token::Class,
            "has" => Token::Has,
            "can" => Token::Can,
            "to" => Token::To,
            _ => Token::Identifier(text.to_string()),
        };

        Ok(Some(token))
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

        while let Some(b) = self.current_byte() {
            if b.is_ascii_digit() {
                self.byte_index += 1;
            } else {
                break;
            }
        }

        // Parse the slice
        let text = &self.src[start..self.byte_index];
        // For now, we only handle integers, but storing as f64 per your previous token def
        // If we want to support decimals, we'd need to peek for '.' and more digits.
        // Assuming integer-only for simplicity in this first pass unless you want full float support.
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

        // Blank line (possibly with spaces): indentation doesn't count, and it's not “data”.
        if matches!(self.current_byte(), None | Some(b'\n') | Some(b'\r')) {
            // Stay at BOL; newline consumer will handle the newline next.
            self.at_beginning_of_line = true;
            return Ok(());
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
        if spaces > current {
            self.indent_stack.push(spaces);
            self.pending.push_back(Token::Indent);
        } else if spaces < current {
            while let Some(&top) = self.indent_stack.last() {
                if top == spaces {
                    break;
                }
                self.indent_stack.pop();
                self.pending.push_back(Token::Dedent);
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

    pub fn lex(&mut self) -> Result<Vec<Token>> {
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
        let tokens = lexer.lex().unwrap();

        // simple sanity check of the first few tokens
        assert_eq!(tokens[0], Token::Class); // A
        assert_eq!(tokens[1], Token::Identifier("Whale".to_string()));
        assert_eq!(tokens[2], Token::Colon);
        assert_eq!(tokens[3], Token::Newline);
        assert_eq!(tokens[4], Token::Indent);
        assert_eq!(tokens[5], Token::Has);
    }
}
