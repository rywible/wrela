//! The lexer: source text to tokens, with lexical diagnostics.
//!
//! This is S0's skeleton: identifiers, numbers, operators and punctuation, `//` comments and
//! newlines, enough to report real diagnostics. Keywords, number validation, newline significance
//! and the tier-1 tokens (strings, `?`) arrive with the parser.

use wrela_diag::codes::{BLOCK_COMMENT, UNEXPECTED_CHARACTER};
use wrela_diag::{Diagnostic, Edit, FileId, Span};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TokenKind {
    /// `[A-Za-z_][A-Za-z0-9_]*`. Keywords are identifiers until the parser distinguishes them.
    Ident,
    /// A digit followed by letters, digits, `_`, one fraction (`.` then a digit) and signed
    /// exponents (`1e-9`). The parser validates the contents.
    Number,
    /// An operator or delimiter; the token's text says which. Longest match wins.
    Punct,
    /// `// ...` up to, not including, the line break.
    LineComment,
    /// `/// ...` (but not `////`).
    DocComment,
    /// A `\n`. Whether it ends a statement is the parser's business.
    Newline,
    /// A run of characters that can't start a token; already reported as `E0001`.
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// The tokens of one file, and the lexical diagnostics.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Operators and delimiters, longest first within each length so the first match is the longest.
const PUNCTS: &[&str] = &[
    "**=", "<<=", ">>=", "..=", //
    "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "^=", "&=",
    "|=", "<<", ">>", "**", "..", //
    "(", ")", "[", "]", "{", "}", ",", ";", ":", ".", "@", "=", "+", "-", "*", "/", "%", "^", "&",
    "|", "!", "<", ">",
];

/// Lexes `text`, which is the contents of `file`.
pub fn lex(file: FileId, text: &str) -> Lexed {
    let mut lexer = Lexer {
        file,
        text,
        pos: 0,
        out: Lexed::default(),
    };
    lexer.run();
    lexer.out
}

struct Lexer<'a> {
    file: FileId,
    text: &'a str,
    pos: usize,
    out: Lexed,
}

impl Lexer<'_> {
    fn run(&mut self) {
        while let Some(c) = self.peek() {
            let start = self.pos;
            match c {
                ' ' | '\t' | '\r' => self.pos += 1,
                '\n' => {
                    self.pos += 1;
                    self.push(TokenKind::Newline, start);
                }
                '/' if self.rest().starts_with("//") => self.line_comment(),
                '/' if self.rest().starts_with("/*") => self.block_comment(),
                'a'..='z' | 'A'..='Z' | '_' => {
                    self.eat_while(|c| c.is_ascii_alphanumeric() || c == '_');
                    self.push(TokenKind::Ident, start);
                }
                '0'..='9' => self.number(),
                _ => match PUNCTS.iter().find(|p| self.rest().starts_with(**p)) {
                    Some(punct) => {
                        self.pos += punct.len();
                        self.push(TokenKind::Punct, start);
                    }
                    None => self.unexpected(),
                },
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn byte_at(&self, at: usize) -> Option<u8> {
        self.text.as_bytes().get(at).copied()
    }

    fn eat_while(&mut self, mut f: impl FnMut(char) -> bool) {
        let len = self.rest().find(|c| !f(c)).unwrap_or(self.rest().len());
        self.pos += len;
    }

    fn span(&self, start: usize) -> Span {
        Span::from_range(self.file, start..self.pos)
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        let span = self.span(start);
        self.out.tokens.push(Token { kind, span });
    }

    fn line_comment(&mut self) {
        let start = self.pos;
        self.eat_while(|c| c != '\n');
        let text = &self.text[start..self.pos];
        // The `\r` of a `\r\n` line ending isn't part of the comment.
        let end = start + text.strip_suffix('\r').unwrap_or(text).len();
        let doc = text.starts_with("///") && !text.starts_with("////");
        let kind = if doc {
            TokenKind::DocComment
        } else {
            TokenKind::LineComment
        };
        let span = Span::from_range(self.file, start..end);
        self.out.tokens.push(Token { kind, span });
    }

    fn number(&mut self) {
        let start = self.pos;
        let rest = self.rest().as_bytes();
        let decimal = !(rest.len() > 1
            && rest[0] == b'0'
            && matches!(rest[1], b'x' | b'X' | b'b' | b'B' | b'o' | b'O'));
        // After a `.` this is a tuple index (`t.0.1`), so it takes no fraction.
        let after_dot = self.out.tokens.last().is_some_and(|t| {
            t.kind == TokenKind::Punct
                && t.span.end as usize == start
                && &self.text[t.span.range()] == "."
        });
        let mut seen_dot = after_dot || !decimal;
        self.pos += 1;
        while let Some(b) = self.byte_at(self.pos) {
            let next = self.byte_at(self.pos + 1);
            match b {
                b'e' | b'E'
                    if decimal
                        && matches!(next, Some(b'+' | b'-'))
                        && self
                            .byte_at(self.pos + 2)
                            .is_some_and(|b| b.is_ascii_digit()) =>
                {
                    self.pos += 2;
                }
                b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.pos += 1,
                b'.' if !seen_dot && next.is_some_and(|b| b.is_ascii_digit()) => {
                    seen_dot = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }
        self.push(TokenKind::Number, start);
    }

    /// `/* ... */` isn't wrela. Skip to the matching `*/` (nesting, as a Rust author expects) so
    /// one mistake reports one error, and offer a `//` rewrite when it's mechanical.
    fn block_comment(&mut self) {
        let start = self.pos;
        let bytes = self.text.as_bytes();
        let mut depth = 0usize;
        let mut closed = false;
        while self.pos < bytes.len() {
            match (bytes[self.pos], self.byte_at(self.pos + 1)) {
                (b'/', Some(b'*')) => {
                    depth += 1;
                    self.pos += 2;
                }
                (b'*', Some(b'/')) => {
                    depth -= 1;
                    self.pos += 2;
                    if depth == 0 {
                        closed = true;
                        break;
                    }
                }
                // Only ASCII bytes are compared, and UTF-8 continuation bytes are never ASCII, so
                // stepping by byte stays correct; the end is then always a char boundary.
                _ => self.pos += 1,
            }
        }
        self.pos = self.pos.min(bytes.len());
        let message = "`/* */` comments aren't supported";
        let span = self.span(start);

        if !closed {
            let opener = Span::from_range(self.file, start..start + 2);
            let diagnostic = Diagnostic::new(BLOCK_COMMENT, message, opener)
                .with_label("this comment is never closed")
                .with_note("the rest of the file was skipped")
                .with_help("start each comment line with `//`");
            self.out.diagnostics.push(diagnostic);
            return;
        }

        let comment = &self.text[start..self.pos];
        let line_rest = self.rest().split('\n').next().unwrap_or("");
        let diagnostic = Diagnostic::new(BLOCK_COMMENT, message, span);
        let diagnostic = if comment.contains('\n') {
            diagnostic.with_help("start each comment line with `//`")
        } else if !(line_rest.trim().is_empty() || line_rest.trim_start().starts_with("//")) {
            diagnostic.with_help("write it as a `//` comment on its own line or at the line's end")
        } else {
            let inner = &comment[2..comment.len() - 2];
            let (prefix, inner) = match inner.strip_prefix('*') {
                Some(doc) if !doc.trim().is_empty() => ("///", doc),
                _ => ("//", inner),
            };
            let inner = inner.trim();
            let replacement = if inner.is_empty() {
                prefix.to_string()
            } else {
                format!("{prefix} {inner}")
            };
            let help = if prefix == "///" {
                "write it as a `///` doc comment"
            } else {
                "write it as a `//` comment"
            };
            diagnostic.with_fix(help, vec![Edit::new(span, replacement)])
        };
        self.out.diagnostics.push(diagnostic);
    }

    /// Reports a run of characters that can't start a token, as one diagnostic.
    fn unexpected(&mut self) {
        let start = self.pos;
        self.eat_while(|c| !starts_token(c));
        if self.pos == start {
            // Unreachable: `run` only calls this for a character that can't start a token. Step
            // over one character anyway so the loop always makes progress.
            self.pos += self.peek().map_or(1, char::len_utf8);
        }
        let run = &self.text[start..self.pos];
        let span = self.span(start);
        let count = run.chars().count();
        let described = if run.chars().all(quotable) {
            format!("`{run}`")
        } else {
            run.chars().map(describe).collect::<Vec<_>>().join(" ")
        };
        let noun = if count == 1 {
            "character"
        } else {
            "characters"
        };
        let mut diagnostic = Diagnostic::new(
            UNEXPECTED_CHARACTER,
            format!("unexpected {noun} {described}"),
            span,
        );
        if run.chars().any(char::is_alphabetic) {
            diagnostic = diagnostic.with_note("identifiers are ASCII: letters, digits and `_`");
        }
        if !run.chars().any(wrela_diag::is_visible) {
            let it = if count == 1 { "it" } else { "them" };
            let (help, replacement) = if run.chars().any(char::is_whitespace) {
                (format!("replace {it} with a space"), " ")
            } else {
                (format!("delete {it}"), "")
            };
            diagnostic = diagnostic
                .with_label("invisible in most editors")
                .with_fix(help, vec![Edit::new(span, replacement)]);
        }
        self.out.diagnostics.push(diagnostic);
        self.push(TokenKind::Unknown, start);
    }
}

/// Whether `c` can begin a token, a comment or whitespace that the lexer accepts.
fn starts_token(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        || matches!(c, ' ' | '\t' | '\r' | '\n')
        || PUNCTS.iter().any(|p| p.starts_with(c))
}

/// How a character appears in a message: itself in backticks, or its code point if it's
/// invisible or would break the backticks.
fn describe(c: char) -> String {
    if quotable(c) {
        format!("`{c}`")
    } else {
        format!("U+{:04X}", u32::from(c))
    }
}

/// Whether `c` can appear in a message inside backticks: visible, and not a backtick itself.
fn quotable(c: char) -> bool {
    c != '`' && wrela_diag::is_visible(c)
}
