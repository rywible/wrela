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

    fn peek_n_bytes(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.byte_index + n).copied()
    }

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
            return self.next_token(); // bounded recursion; you can turn this into a while loop later
        }

        // 6) “Real” tokenization: placeholder.
        // Replace this with identifiers/numbers/operators/etc.
        let b = self.next_byte().unwrap();
        Ok(Some(Token::OtherByte(b)))
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
