use super::errors::LexError;
use super::tokens::Token;
use miette::SourceSpan;

pub struct IndentHandler {
    stack: Vec<(usize, Option<SourceSpan>)>,
    nesting: usize,
    at_bol: bool,
}

impl IndentHandler {
    pub fn new() -> Self {
        Self {
            stack: vec![(0, None)],
            nesting: 0,
            at_bol: true,
        }
    }

    pub fn is_nested(&self) -> bool {
        self.nesting > 0
    }

    pub fn enter_nesting(&mut self) {
        self.nesting += 1;
    }

    pub fn exit_nesting(&mut self) {
        if self.nesting > 0 {
            self.nesting -= 1;
        }
    }

    pub fn current_indent(&self) -> usize {
        self.stack.last().map(|(s, _)| *s).unwrap_or(0)
    }

    pub fn current_span(&self) -> Option<SourceSpan> {
        self.stack.last().and_then(|(_, s)| *s)
    }

    pub fn set_at_bol(&mut self, val: bool) {
        self.at_bol = val;
    }

    pub fn at_bol(&self) -> bool {
        self.at_bol
    }

    pub fn push_indent(&mut self, level: usize, span: SourceSpan) {
        self.stack.push((level, Some(span)));
    }

    pub fn pop_indent(&mut self) -> Option<(usize, Option<SourceSpan>)> {
        if self.stack.len() > 1 {
            self.stack.pop()
        } else {
            None
        }
    }

    pub fn stack_len(&self) -> usize {
        self.stack.len()
    }

    /// Calculates Indent/Dedent tokens based on leading spaces.
    pub fn process_indent(
        &mut self,
        spaces: usize,
        offset: usize,
        is_first_line: bool,
    ) -> (Vec<Token>, Vec<LexError>) {
        let mut tokens = Vec::new();
        let mut errors = Vec::new();
        let span = SourceSpan::new(offset.into(), spaces);

        if is_first_line && spaces > 0 {
            errors.push(LexError::UnexpectedTopLevelIndent { span });
        }

        if !spaces.is_multiple_of(4) {
            errors.push(LexError::IndentNotMultipleOfFour {
                span,
                reference: self.current_span(),
            });
        }

        let current = self.current_indent();
        if spaces > current {
            self.push_indent(spaces, span);
            tokens.push(Token::Indent);
        } else if spaces < current {
            while spaces < self.current_indent() {
                self.pop_indent();
                tokens.push(Token::Dedent);
            }
            if spaces != self.current_indent() {
                errors.push(LexError::InconsistentIndent {
                    span,
                    reference: self.current_span(),
                });
            }
        }

        (tokens, errors)
    }

    /// Validates indentation for multiline expressions (inside parens/brackets/braces).
    pub fn validate_nested_indent(
        &self,
        spaces: usize,
        offset: usize,
        span_len: usize,
        is_closer: bool,
    ) -> Vec<LexError> {
        let mut errors = Vec::new();
        let current_block_indent = self.current_indent();
        let span = SourceSpan::new(offset.into(), span_len);

        if is_closer {
            if spaces < current_block_indent {
                errors.push(LexError::InvalidMultilineIndent { span });
            }
        } else if spaces <= current_block_indent {
            errors.push(LexError::InvalidMultilineIndent { span });
        }

        if !spaces.is_multiple_of(4) {
            errors.push(LexError::IndentNotMultipleOfFour {
                span,
                reference: self.current_span(),
            });
        }
        errors
    }
}
