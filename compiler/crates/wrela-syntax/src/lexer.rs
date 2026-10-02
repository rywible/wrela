//! The lexer: source text to tokens, with lexical diagnostics.
//!
//! This is S0's skeleton: identifiers, numbers, operators and punctuation, `//` comments and
//! newlines, enough to report real diagnostics. Keywords, number validation, newline significance
//! and the tier-1 tokens (strings, `?`) arrive with the parser.
//!
//! Recovery: each mistake is reported once, and the tokens are what the user most likely meant,
//! so the parser doesn't pile further errors on top. A name with a non-ASCII letter (`café`) is
//! one [`TokenKind::Ident`]; a lone `\r` is a [`TokenKind::Newline`]. A byte-order mark at the
//! very start of the file is skipped silently; offsets still count it.

use wrela_diag::codes::{BLOCK_COMMENT, UNEXPECTED_CHARACTER};
use wrela_diag::{Diagnostic, Edit, FileId, Span};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TokenKind {
    /// `[A-Za-z_][A-Za-z0-9_]*`. Keywords are identifiers until the parser distinguishes them.
    /// A run that also has non-ASCII letters or digits is one identifier, already reported as
    /// `E0001`.
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
    /// A `\n`, or a lone `\r` (already reported as `E0001`). Whether it ends a statement is the
    /// parser's business.
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
    // Some editors start a UTF-8 file with a byte-order mark. It isn't part of the program.
    let bom = '\u{FEFF}';
    let pos = if text.starts_with(bom) {
        bom.len_utf8()
    } else {
        0
    };
    let mut lexer = Lexer {
        file,
        text,
        pos,
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
                ' ' | '\t' => self.pos += 1,
                '\r' if self.rest().starts_with("\r\n") => self.pos += 1,
                '\r' => self.lone_carriage_return(),
                '\n' => {
                    self.pos += 1;
                    self.push(TokenKind::Newline, start);
                }
                '/' if self.rest().starts_with("//") => self.line_comment(),
                '/' if self.rest().starts_with("/*") => self.block_comment(),
                '0'..='9' => self.number(),
                c if is_ident_char(c) => {
                    self.eat_while(is_ident_char);
                    self.non_ascii(start);
                    self.push(TokenKind::Ident, start);
                }
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
        // A `\r` ends the comment too: the `\r` of a `\r\n` isn't part of it, and an editor that
        // shows a lone `\r` as a line break would show the text after it as code, so it's lexed
        // as code (after the `\r` is reported).
        self.eat_while(|c| c != '\n' && c != '\r');
        let text = &self.text[start..self.pos];
        let doc = text.starts_with("///") && !text.starts_with("////");
        let kind = if doc {
            TokenKind::DocComment
        } else {
            TokenKind::LineComment
        };
        self.push(kind, start);
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
                && t.span.end() as usize == start
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
                // A non-ASCII letter in a suffix (`15µm`) stays in the number, like an identifier's.
                _ if !b.is_ascii() => match self.peek() {
                    Some(c) if is_ident_char(c) => self.pos += c.len_utf8(),
                    _ => break,
                },
                _ => break,
            }
        }
        self.non_ascii(start);
        self.push(TokenKind::Number, start);
    }

    /// A `\r` without a `\n` after it. Some editors show it as a line break and others don't,
    /// so it's an error; the lexer reads it as the line break it most likely was.
    fn lone_carriage_return(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let span = self.span(start);
        let diagnostic = Diagnostic::new(UNEXPECTED_CHARACTER, "unexpected character U+000D", span)
            .with_label("a carriage return without a line feed after it")
            .with_note("a line ends with `\\n` or `\\r\\n`")
            .with_fix("replace it with a line break", vec![Edit::new(span, "\n")]);
        self.out.diagnostics.push(diagnostic);
        self.push(TokenKind::Newline, start);
    }

    /// Reports the non-ASCII characters in the identifier or number at `start..self.pos`, once
    /// for the token. The token stays whole, so the parser sees the one name the user wrote.
    fn non_ascii(&mut self, start: usize) {
        let token = &self.text[start..self.pos];
        let mut runs: Vec<(usize, &str)> = Vec::new();
        let mut rest = token;
        while let Some(from) = rest.find(|c: char| !c.is_ascii()) {
            let len = rest[from..]
                .find(|c: char| c.is_ascii())
                .unwrap_or(rest.len() - from);
            let at = token.len() - rest.len() + from;
            runs.push((at, &rest[from..from + len]));
            rest = &rest[from + len..];
        }
        let (Some(&(first, _)), Some(&(last, last_run))) = (runs.first(), runs.last()) else {
            return;
        };
        let span = Span::from_range(self.file, start + first..start + last + last_run.len());
        let count: usize = runs.iter().map(|(_, run)| run.chars().count()).sum();
        let noun = if count == 1 {
            "character"
        } else {
            "characters"
        };
        let described: Vec<String> = runs.iter().map(|(_, run)| describe_run(run)).collect();
        let mut diagnostic = Diagnostic::new(
            UNEXPECTED_CHARACTER,
            format!("unexpected {noun} {}", described.join(" ")),
            span,
        )
        .with_note("identifiers are ASCII: letters, digits and `_`");
        if let [(_, run)] = runs.as_slice()
            && !run.chars().any(wrela_diag::is_visible)
        {
            let it = if count == 1 { "it" } else { "them" };
            diagnostic = diagnostic
                .with_label("invisible in most editors")
                .with_fix(format!("delete {it}"), vec![Edit::new(span, "")]);
        }
        self.out.diagnostics.push(diagnostic);
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
            // A doc comment documents the item below it, so `/** */` after code on its line
            // becomes a plain comment.
            let line_before = self.text[..start].rsplit('\n').next().unwrap_or("");
            let own_line = line_before.trim().is_empty();
            let (prefix, inner) = match inner.strip_prefix('*') {
                Some(doc) if own_line && !doc.trim().is_empty() => ("///", doc),
                Some(doc) => ("//", doc),
                None => ("//", inner),
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
        let noun = if count == 1 {
            "character"
        } else {
            "characters"
        };
        let mut diagnostic = Diagnostic::new(
            UNEXPECTED_CHARACTER,
            format!("unexpected {noun} {}", describe_run(run)),
            span,
        );
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
    is_ident_char(c)
        || matches!(c, ' ' | '\t' | '\r' | '\n')
        || PUNCTS.iter().any(|p| p.starts_with(c))
}

/// Whether `c` continues an identifier: `_` or a letter or digit, ASCII or not. A non-ASCII one is
/// an error, but keeping it in the token keeps the name whole.
fn is_ident_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// A run of characters in a message: in backticks if every one can be, else each by itself.
fn describe_run(run: &str) -> String {
    if run.chars().all(quotable) {
        format!("`{run}`")
    } else {
        run.chars().map(describe).collect::<Vec<_>>().join(" ")
    }
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
