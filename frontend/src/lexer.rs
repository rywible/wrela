use crate::diagnostic::{Diagnostic, DiagnosticCode, push_bounded};
use crate::source::{ByteRange, Piece, PieceKind, Source, TriviaKind};
use crate::token::{Tok, TokenId, TokenKind};
use crate::unicode16;
use TokenKind::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_source_bytes: usize,
    pub max_depth: usize,
    pub max_generic_tokens: usize,
    pub max_diagnostics: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_source_bytes: 16 * 1024 * 1024,
            max_depth: 256,
            max_generic_tokens: 4096,
            max_diagnostics: 64,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Lexed {
    pub source: Source,
    pub tokens: Vec<(usize, Tok, usize)>,
    pub diagnostics: Vec<Diagnostic>,
}
fn ignorable(c: char) -> bool {
    unicode16::contains(unicode16::DEFAULT_IGNORABLE_CODE_POINT, c)
}
fn scalar(bytes: &[u8], at: usize) -> Option<(char, usize)> {
    let b = *bytes.get(at)?;
    let n = match b {
        0..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    let text = std::str::from_utf8(bytes.get(at..at + n)?).ok()?;
    Some((text.chars().next()?, n))
}
struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
    source: Source,
    raw: Vec<(usize, Tok, usize)>,
    diagnostics: Vec<Diagnostic>,
    limits: Limits,
    delimiters: Vec<TokenKind>,
}
impl Scanner<'_> {
    fn piece(&mut self, from: usize, kind: PieceKind) -> TokenId {
        let id = TokenId(self.source.pieces.len());
        self.source.pieces.push(Piece {
            kind,
            range: ByteRange::new(from, self.at),
            bytes: self.bytes[from..self.at].to_vec(),
        });
        if let PieceKind::Token(k) = kind {
            self.raw.push((from, Tok::new(k, id), self.at));
        }
        id
    }
    fn error(&mut self, code: DiagnosticCode, message: &str, from: usize, to: usize) {
        push_bounded(
            &mut self.diagnostics,
            Diagnostic::new(code, message, ByteRange::new(from, to)),
            self.limits.max_diagnostics,
        );
    }
    fn remainder(&mut self) {
        let from = self.at;
        self.at = self.bytes.len();
        if from < self.at {
            self.piece(from, PieceKind::Unparsed);
        }
    }
    fn newline(&mut self, id: TokenId, from: usize, to: usize) {
        self.raw.push((from, Tok::Newline(id), to));
    }
    fn emit_with_delimiter_limit(&mut self, from: usize, width: usize, kind: PieceKind) -> bool {
        if let PieceKind::Token(token) = kind {
            if matches!(token, LParen | LBracket | LBrace) {
                if self.delimiters.len() >= self.limits.max_depth {
                    self.error(
                        DiagnosticCode::Limit,
                        "delimiter depth limit exceeded",
                        from,
                        from,
                    );
                    self.remainder();
                    return false;
                }
                self.delimiters.push(token);
            } else if matches!(token, RParen | RBracket | RBrace) {
                let open = match token {
                    RParen => LParen,
                    RBracket => LBracket,
                    RBrace => LBrace,
                    _ => unreachable!(),
                };
                if self.delimiters.last() == Some(&open) {
                    self.delimiters.pop();
                }
            }
        }
        self.at += width;
        let id = self.piece(from, kind);
        if kind == PieceKind::Trivia(TriviaKind::Newline) {
            self.newline(id, from, self.at);
        }
        true
    }
    /// Retain an invalid string through its closing quote or physical newline.
    fn malformed_string(&mut self, from: usize) {
        self.at += 1;
        while self.at < self.bytes.len() && !matches!(self.bytes[self.at], b'\r' | b'\n') {
            match self.bytes[self.at] {
                b'"' => {
                    self.at += 1;
                    break;
                }
                b'\\' => {
                    self.at += 1;
                    if self.at == self.bytes.len() || matches!(self.bytes[self.at], b'\r' | b'\n') {
                        break;
                    }
                    self.at += 1;
                }
                _ => self.at += 1,
            }
        }
        self.piece(from, PieceKind::Invalid);
        self.error(
            DiagnosticCode::Lexical,
            "unterminated string, invalid escape, or invalid UTF-8",
            from,
            self.at,
        );
    }
    /// Consume nested comments with bounded depth and retain their first newline.
    fn nested_comment(&mut self, from: usize) -> bool {
        self.at += 2;
        let mut depth = 1usize;
        let mut newline = None;
        let mut exceeded = depth > self.limits.max_depth;
        while self.at < self.bytes.len() && depth > 0 && !exceeded {
            if self.bytes.get(self.at..self.at + 2) == Some(b"/*") {
                depth += 1;
                if depth > self.limits.max_depth {
                    exceeded = true;
                    break;
                }
                self.at += 2;
            } else if self.bytes.get(self.at..self.at + 2) == Some(b"*/") {
                depth -= 1;
                self.at += 2;
            } else {
                if newline.is_none() && matches!(self.bytes[self.at], b'\r' | b'\n') {
                    newline = Some(self.at);
                }
                self.at += 1;
            }
        }
        if exceeded {
            self.error(
                DiagnosticCode::Limit,
                "nested comment depth limit exceeded",
                self.at,
                self.at,
            );
            self.at = from;
            self.remainder();
            return false;
        }
        let valid = std::str::from_utf8(&self.bytes[from..self.at]).is_ok();
        let id = self.piece(
            from,
            if depth == 0 && valid {
                PieceKind::Trivia(TriviaKind::BlockComment)
            } else {
                PieceKind::Invalid
            },
        );
        if depth != 0 {
            self.error(
                DiagnosticCode::Lexical,
                "unterminated block comment",
                self.bytes.len(),
                self.bytes.len(),
            );
        } else if !valid {
            self.error(
                DiagnosticCode::Lexical,
                "invalid UTF-8 in comment",
                from,
                self.at,
            );
        }
        if let Some(n) = newline {
            let end = n + if self.bytes.get(n..n + 2) == Some(b"\r\n") {
                2
            } else {
                1
            };
            self.newline(id, n, end);
        }
        true
    }
    /// Preserve one rejected scalar, or a maximal group of invalid UTF-8 bytes.
    fn rejected_bytes(&mut self, from: usize) {
        if let Some((_, width)) = scalar(self.bytes, from) {
            self.at += width;
            self.piece(from, PieceKind::Invalid);
            self.error(
                DiagnosticCode::Lexical,
                "character is not admitted by the lexical policy",
                from,
                self.at,
            );
        } else {
            self.at += 1;
            while self.at < self.bytes.len() && scalar(self.bytes, self.at).is_none() {
                self.at += 1;
            }
            self.piece(from, PieceKind::Invalid);
            self.error(
                DiagnosticCode::Lexical,
                "invalid UTF-8 bytes",
                from,
                self.at,
            );
        }
    }
    fn scan(&mut self) {
        use crate::lexical::Lexeme;
        while self.at < self.bytes.len() {
            let from = self.at;
            match crate::lexical::regular(&self.bytes[from..]) {
                Some((Lexeme::BlockCommentStart, _)) => {
                    if !self.nested_comment(from) {
                        break;
                    }
                }
                Some((Lexeme::StringStart, _)) => self.malformed_string(from),
                Some((Lexeme::InvalidIdentifier, width)) => {
                    self.emit_with_delimiter_limit(from, width, PieceKind::Invalid);
                    let text = std::str::from_utf8(&self.bytes[from..self.at]).unwrap();
                    let (offset, c) = text
                        .char_indices()
                        .find(|(_, c)| ignorable(*c))
                        .expect("generated invalid identifier must contain a forbidden scalar");
                    self.error(
                        DiagnosticCode::Lexical,
                        "default-ignorable character forbidden in identifier",
                        from + offset,
                        from + offset + c.len_utf8(),
                    );
                }
                Some((Lexeme::InvalidNumber, width)) => {
                    self.emit_with_delimiter_limit(from, width, PieceKind::Invalid);
                    self.error(
                        DiagnosticCode::Lexical,
                        "invalid decimal number spelling",
                        from,
                        self.at,
                    );
                }
                Some((lexeme, width)) => {
                    let valid = !matches!(lexeme, Lexeme::String | Lexeme::LineComment)
                        || std::str::from_utf8(&self.bytes[from..from + width]).is_ok();
                    if !self.emit_with_delimiter_limit(
                        from,
                        width,
                        if valid {
                            lexeme.piece().unwrap()
                        } else {
                            PieceKind::Invalid
                        },
                    ) {
                        break;
                    }
                    if !valid {
                        let message = if lexeme == Lexeme::LineComment {
                            "invalid UTF-8 in comment"
                        } else {
                            "unterminated string, invalid escape, or invalid UTF-8"
                        };
                        self.error(DiagnosticCode::Lexical, message, from, self.at);
                    }
                }
                None => self.rejected_bytes(from),
            }
            if self
                .diagnostics
                .last()
                .is_some_and(|d| d.code == DiagnosticCode::Truncated)
            {
                self.remainder();
                break;
            }
        }
    }
}
fn trailing(k: TokenKind) -> bool {
    matches!(
        k,
        Comma
            | Colon
            | Arrow
            | FatArrow
            | Plus
            | Minus
            | Star
            | Slash
            | Percent
            | Bang
            | Equal
            | EqualEqual
            | BangEqual
            | Less
            | LessEqual
            | Greater
            | GreaterEqual
            | AndAnd
            | OrOr
            | PathSeparator
    )
}

pub fn lex(bytes: &[u8], limits: Limits) -> Lexed {
    let mut scanner = Scanner {
        bytes,
        at: 0,
        source: Source::default(),
        raw: Vec::new(),
        diagnostics: Vec::new(),
        limits,
        delimiters: Vec::new(),
    };
    if bytes.len() > limits.max_source_bytes {
        scanner.error(
            DiagnosticCode::Limit,
            "source byte limit exceeded",
            0,
            bytes.len(),
        );
        scanner.remainder();
    } else {
        scanner.scan();
    }
    let mut tokens = adapt_newlines_and_generics(&mut scanner);
    bound_expression_complexity(&mut scanner, &mut tokens);
    Lexed {
        source: scanner.source,
        tokens,
        diagnostics: scanner.diagnostics,
    }
}

fn adapt_newlines_and_generics(scanner: &mut Scanner<'_>) -> Vec<(usize, Tok, usize)> {
    let raw = &scanner.raw;
    let significant: Vec<usize> = raw
        .iter()
        .enumerate()
        .filter_map(|(i, (_, t, _))| (t.kind() != Newline).then_some(i))
        .collect();
    let mut kinds: Vec<TokenKind> = significant.iter().map(|&i| raw[i].1.kind()).collect();
    // Continuation contexts are scoped by every brace. A lambda body inside a call
    // therefore starts statements even though an enclosing '(' is still open.
    let mut frames = vec![0usize];
    let mut continued = vec![false; raw.len()];
    let mut grouped = vec![false; raw.len()];
    let mut previous = None;
    let mut last_piece = 0;
    for (i, (_, t, _)) in raw.iter().enumerate() {
        if scanner.source.pieces[last_piece..t.id().0]
            .iter()
            .any(|p| p.kind == PieceKind::Invalid)
        {
            previous = None;
        }
        last_piece = t.id().0 + 1;
        let k = t.kind();
        if k == Newline {
            grouped[i] = *frames.last().unwrap() > 0;
            continued[i] = grouped[i] || previous.is_some_and(trailing);
            continue;
        }
        match k {
            LBrace => frames.push(0),
            RBrace => {
                if frames.len() > 1 {
                    frames.pop();
                }
            }
            LParen | LBracket => *frames.last_mut().unwrap() += 1,
            RParen | RBracket => {
                let n = frames.last_mut().unwrap();
                *n = n.saturating_sub(1);
            }
            _ => {}
        }
        previous = Some(k);
    }
    let gap_allowed = |left: usize, right: usize| {
        (left + 1..right).all(|i| raw[i].1.kind() != Newline || continued[i])
    };
    let mut regions = crate::lexical_regions::LexicalRegions::new();
    let mut suppress = vec![false; raw.len()];
    let mut stop = None;
    let mut s = 0;
    while s < significant.len() {
        let k = kinds[s];
        if s > 0 && !gap_allowed(significant[s - 1], significant[s]) {
            regions.boundary();
        }
        let type_context = regions.expects_type_angles();
        if k == Less
            && s > 0
            && kinds[s - 1] == Ident
            && (type_context || gap_allowed(significant[s - 1], significant[s]))
        {
            let mut look = crate::generic_selection::ProspectiveTypes {
                kinds: &kinds,
                at: s,
                visited: 0,
                limit: scanner.limits.max_generic_tokens,
                depth_limit: scanner.limits.max_depth,
                angles: Vec::new(),
            };
            match look.recognize() {
                Ok(()) => {
                    let end = look.at;
                    let call = end < kinds.len()
                        && kinds[end] == LParen
                        && (significant[end - 1] + 1..significant[end])
                            .all(|i| raw[i].1.kind() != Newline || grouped[i]);
                    if type_context || call {
                        let angles = look.angles;
                        for (open, close) in angles {
                            kinds[open] = if open == s && !type_context {
                                GenericOpen
                            } else {
                                TypeOpen
                            };
                            kinds[close] = if open == s && !type_context {
                                GenericClose
                            } else {
                                TypeClose
                            };
                        }
                        for item in &mut suppress[significant[s]..=significant[end - 1]] {
                            *item = true;
                        }
                        s = end;
                        continue;
                    }
                }
                Err(crate::generic_selection::Failure::NotType) => {}
                Err(problem) => {
                    stop = Some((s, problem));
                    break;
                }
            }
        }
        regions.observe(k, s.checked_sub(1).map(|previous| kinds[previous]));
        s += 1;
    }
    if let Some((s, problem)) = stop {
        let raw_index = significant[s];
        let start = raw[raw_index].0;
        let id = raw[raw_index].1.id();
        let message = match problem {
            crate::generic_selection::Failure::Tokens => "prospective generic token limit exceeded",
            crate::generic_selection::Failure::Depth => "prospective generic depth limit exceeded",
            crate::generic_selection::Failure::NotType => unreachable!(),
        };
        scanner.error(DiagnosticCode::Limit, message, start, start);
        scanner.source.pieces.truncate(id.0);
        scanner.source.pieces.push(Piece {
            kind: PieceKind::Unparsed,
            range: ByteRange::new(start, scanner.bytes.len()),
            bytes: scanner.bytes[start..].to_vec(),
        });
        scanner.raw.truncate(raw_index);
    }
    // Change tape classifications together with the parser terminals; byte ownership stays put.
    for (s, &r) in significant.iter().enumerate() {
        if r >= scanner.raw.len() {
            break;
        }
        let id = scanner.raw[r].1.id();
        scanner.raw[r].1 = Tok::new(kinds[s], id);
        scanner.source.pieces[id.0].kind = PieceKind::Token(kinds[s]);
    }
    let mut result = Vec::new();
    let mut frames = vec![0usize];
    let mut previous = None;
    let mut last_piece = 0;
    for (i, &item) in scanner.raw.iter().enumerate() {
        if scanner.source.pieces[last_piece..item.1.id().0]
            .iter()
            .any(|p| p.kind == PieceKind::Invalid)
        {
            previous = None;
        }
        last_piece = item.1.id().0 + 1;
        let k = item.1.kind();
        if k == Newline {
            if !suppress[i] && *frames.last().unwrap() == 0 && !previous.is_some_and(trailing) {
                result.push(item);
            }
            continue;
        }
        match k {
            LBrace => frames.push(0),
            RBrace => {
                if frames.len() > 1 {
                    frames.pop();
                }
            }
            LParen | LBracket => *frames.last_mut().unwrap() += 1,
            RParen | RBracket => {
                let n = frames.last_mut().unwrap();
                *n = n.saturating_sub(1);
            }
            _ => {}
        }
        previous = Some(k);
        result.push(item);
    }
    result
}

/// Conservative structural budget before recursive syntax construction. Within each
/// expression list element, operators and postfixes consume depth alongside active
/// delimiter scopes. Commas and statement boundaries reset only their own scope.
/// This bounds long flat unary/binary/postfix chains as well as nested expressions.
fn bound_expression_complexity(scanner: &mut Scanner<'_>, tokens: &mut Vec<(usize, Tok, usize)>) {
    let mut scopes = vec![0usize];
    let mut delimiter_cost = vec![false];
    let mut stop = None;
    let mut previous = None;
    for (index, &(at, tok, _)) in tokens.iter().enumerate() {
        let kind = tok.kind();
        match kind {
            Plus | Minus | Star | Slash | Percent | Bang | EqualEqual | BangEqual | Less
            | LessEqual | Greater | GreaterEqual | AndAnd | OrOr | Dot | Question | Else => {
                *scopes.last_mut().unwrap() += 1
            }
            LParen | LBracket => {
                let postfix = previous.is_some_and(|k| {
                    matches!(k, Ident | RParen | RBracket | GenericClose | Question)
                });
                if postfix {
                    *scopes.last_mut().unwrap() += 1;
                }
                scopes.push(0);
                delimiter_cost.push(!postfix);
            }
            LBrace => {
                scopes.push(0);
                delimiter_cost.push(true);
            }
            RParen | RBracket | RBrace => {
                if scopes.len() > 1 {
                    scopes.pop();
                    delimiter_cost.pop();
                }
            }
            Comma | Newline | Semicolon | Equal => *scopes.last_mut().unwrap() = 0,
            _ => {}
        }
        let budget =
            scopes.iter().sum::<usize>() + delimiter_cost.iter().filter(|&&cost| cost).count();
        if budget > scanner.limits.max_depth {
            stop = Some((index, at, tok.id()));
            break;
        }
        previous = Some(kind);
    }
    if let Some((index, at, id)) = stop {
        scanner.error(
            DiagnosticCode::Limit,
            "expression structural depth budget exceeded",
            at,
            at,
        );
        scanner.source.pieces.truncate(id.0);
        scanner.source.pieces.push(Piece {
            kind: PieceKind::Unparsed,
            range: ByteRange::new(at, scanner.bytes.len()),
            bytes: scanner.bytes[at..].to_vec(),
        });
        tokens.truncate(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(text.as_bytes(), Limits::default())
            .tokens
            .into_iter()
            .map(|(_, t, _)| t.kind())
            .collect()
    }
    #[test]
    fn ownership_survives_malformed_bytes_and_limits() {
        for bytes in [
            b"/* c */ fn x\r\n\xff\x80@".as_slice(),
            b"\"bad\\q\"\nlet x=0",
            b"/* never closed",
        ] {
            let out = lex(bytes, Limits::default());
            assert_eq!(out.source.render(), bytes);
            assert!(out.source.validate());
            assert!(!out.diagnostics.is_empty());
        }
        let bytes = b"abc def";
        let out = lex(
            bytes,
            Limits {
                max_source_bytes: 6,
                ..Limits::default()
            },
        );
        assert_eq!(out.source.render(), bytes);
        assert!(matches!(out.source.pieces[0].kind, PieceKind::Unparsed));
        assert_eq!(out.diagnostics[0].code, DiagnosticCode::Limit);
    }
    #[test]
    fn unicode_16_and_forbidden_identifiers() {
        // U+1C89 was added in Unicode 16; U+1C8A is its lowercase pair.
        let valid = lex("_π a\u{301} \u{1c89}\u{1c8a}".as_bytes(), Limits::default());
        assert!(valid.diagnostics.is_empty());
        assert_eq!(valid.tokens.len(), 3);
        for c in ['\u{200c}', '\u{200d}', '\u{034f}', '\u{fe0f}', '\u{2060}'] {
            let bytes = format!("ab{c}cd");
            let out = lex(bytes.as_bytes(), Limits::default());
            assert!(out.tokens.is_empty());
            assert_eq!(out.source.pieces.len(), 1);
            assert_eq!(out.source.render(), bytes.as_bytes());
            assert_eq!(out.diagnostics.len(), 1);
            let allowed = format!("\"{c}\" /*{c}*/");
            assert!(
                lex(allowed.as_bytes(), Limits::default())
                    .diagnostics
                    .is_empty()
            );
        }
        // Unicode 17 newly assigned Tolong Siki character must remain unaccepted.
        assert!(
            !lex("\u{11db0}".as_bytes(), Limits::default())
                .diagnostics
                .is_empty()
        );
    }
    #[test]
    fn grouping_and_lambda_blocks_scope_newlines() {
        assert!(!kinds("run(f\n<T>(x))").contains(&Newline));
        assert!(!kinds("(f// c\n<T>(x))").contains(&Newline));
        assert!(!kinds("(f<T>\n(x))").contains(&Newline));
        assert_eq!(
            kinds("run(fn() { return\nx\n})")
                .iter()
                .filter(|&&k| k == Newline)
                .count(),
            2
        );
        assert_eq!(
            kinds("return\nx").iter().filter(|&&k| k == Newline).count(),
            1
        );
        assert!(!kinds("x +\ny").contains(&Newline));
        assert_eq!(
            kinds("x /* c\n */ y")
                .iter()
                .filter(|&&k| k == Newline)
                .count(),
            1
        );
    }
    #[test]
    fn generic_classification_and_newline_gaps() {
        for text in [
            "a<b>(c)",
            "a /*c*/ <b>(c)",
            "(f\n<T>(x))",
            "run(f\n<T>(x))",
            "(f<T>\n(x))",
        ] {
            assert!(kinds(text).contains(&GenericOpen), "{text}");
        }
        for text in ["a<b", "f\n<T>(x)", "f<T>\n(x)", "a < b > c"] {
            assert!(!kinds(text).contains(&GenericOpen), "{text}");
        }
        let nested = kinds("let x: Box<Array<T>>\nlet y=0");
        assert_eq!(nested.iter().filter(|&&k| k == TypeClose).count(), 2);
        assert!(nested.contains(&Newline));
        assert!(kinds("f<\nBox<T>,\n[T], (U,V)\n>(x)").contains(&GenericOpen));
    }
    #[test]
    fn deterministic_depth_and_lookahead_limits() {
        let bytes = b"(((x))) remainder";
        assert!(
            lex(
                bytes,
                Limits {
                    max_depth: 3,
                    ..Limits::default()
                }
            )
            .diagnostics
            .is_empty()
        );
        let out = lex(
            bytes,
            Limits {
                max_depth: 2,
                ..Limits::default()
            },
        );
        assert_eq!(out.source.render(), bytes);
        assert_eq!(out.diagnostics[0].code, DiagnosticCode::Limit);
        assert!(matches!(
            out.source.pieces.last().unwrap().kind,
            PieceKind::Unparsed
        ));
        let bytes = b"f<T>(x) trailing";
        assert!(
            lex(
                bytes,
                Limits {
                    max_generic_tokens: 3,
                    ..Limits::default()
                }
            )
            .diagnostics
            .is_empty()
        );
        let out = lex(
            bytes,
            Limits {
                max_generic_tokens: 2,
                ..Limits::default()
            },
        );
        assert_eq!(out.source.render(), bytes);
        assert_eq!(out.diagnostics[0].code, DiagnosticCode::Limit);
        assert_eq!(out.tokens.len(), 1);
        let out = lex(
            b"@ @ @ @ tail",
            Limits {
                max_diagnostics: 2,
                ..Limits::default()
            },
        );
        assert_eq!(out.diagnostics.len(), 2);
        assert_eq!(out.diagnostics[1].code, DiagnosticCode::Truncated);
        assert_eq!(out.source.render(), b"@ @ @ @ tail");
    }
    #[test]
    fn nested_comment_and_physical_eof() {
        assert!(
            lex(
                b"/* a /* b */ c */",
                Limits {
                    max_depth: 2,
                    ..Limits::default()
                }
            )
            .diagnostics
            .is_empty()
        );
        let out = lex(
            b"/* a /* b */ c */",
            Limits {
                max_depth: 1,
                ..Limits::default()
            },
        );
        assert_eq!(out.diagnostics[0].code, DiagnosticCode::Limit);
        assert_eq!(out.source.render(), b"/* a /* b */ c */");
        let out = lex(b" /* trailing", Limits::default());
        assert_eq!(out.diagnostics[0].range, ByteRange::empty(out.source.len()));
    }
    #[test]
    fn long_operator_and_postfix_chains_are_bounded_before_parsing() {
        for chain in ["---x", "a+a+a+a", "f()()()"] {
            assert!(
                lex(
                    chain.as_bytes(),
                    Limits {
                        max_depth: 3,
                        ..Limits::default()
                    }
                )
                .diagnostics
                .is_empty(),
                "{chain}"
            );
            let out = lex(
                chain.as_bytes(),
                Limits {
                    max_depth: 2,
                    ..Limits::default()
                },
            );
            assert_eq!(out.source.render(), chain.as_bytes());
            assert!(
                out.diagnostics
                    .iter()
                    .any(|d| d.code == DiagnosticCode::Limit),
                "{chain}"
            );
        }
        let huge = format!("{}x", "-".repeat(100_000));
        let out = lex(huge.as_bytes(), Limits::default());
        assert_eq!(out.source.render(), huge.as_bytes());
        assert_eq!(out.diagnostics[0].code, DiagnosticCode::Limit);
        let crossed = "([)".repeat(1000);
        let out = lex(
            crossed.as_bytes(),
            Limits {
                max_depth: 4,
                ..Limits::default()
            },
        );
        assert_eq!(out.source.render(), crossed.as_bytes());
        assert_eq!(out.diagnostics[0].code, DiagnosticCode::Limit);
    }

    #[test]
    fn lexical_errors_terminate_unfinished_statements_and_point_at_forbidden_scalar() {
        let out = lex(b"let x = \"bad\nlet tail=0", Limits::default());
        assert!(out.tokens.iter().any(|(_, t, _)| t.kind() == Newline));
        let out = lex("abc\u{200d}def".as_bytes(), Limits::default());
        assert_eq!(out.diagnostics[0].range, ByteRange::new(3, 6));
        assert_eq!(out.source.pieces.len(), 1);
    }
    #[test]
    fn else_if_chains_are_bounded() {
        let bytes = format!("if x {{}} {} else {{}}", "else if x {} ".repeat(1000));
        let out = lex(
            bytes.as_bytes(),
            Limits {
                max_depth: 10,
                ..Limits::default()
            },
        );
        assert_eq!(out.source.render(), bytes.as_bytes());
        assert_eq!(out.diagnostics[0].code, DiagnosticCode::Limit);
    }
    #[test]
    fn arbitrary_byte_inputs_keep_total_ownership_and_valid_token_references() {
        let mut seed = 0xA4B3C2D1u64;
        for length in 0..512 {
            let mut bytes = Vec::new();
            for _ in 0..length {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                bytes.push(seed as u8);
            }
            let out = lex(
                &bytes,
                Limits {
                    max_depth: 8,
                    max_generic_tokens: 16,
                    max_diagnostics: 4,
                    ..Limits::default()
                },
            );
            assert_eq!(out.source.render(), bytes);
            assert!(out.source.validate());
            for (start, t, end) in out.tokens {
                assert!(start <= end && end <= bytes.len());
                assert!(t.id().0 < out.source.pieces.len());
            }
            for d in out.diagnostics {
                assert!(d.range.start <= d.range.end && d.range.end <= bytes.len());
            }
        }
    }
    #[test]
    fn record_field_values_use_expression_angles_and_declaration_fields_use_type_angles() {
        use crate::syntax::{Declaration, ExprKind, StatementKind, TypeKind};
        for expression in [
            "make<T>(v)",
            "lib::make<T>(v)",
            "(make<T>(v))",
            "a < b",
            "(a < b)",
            "run(fn(v: T) -> T { return make<T>(v) })",
            "Inner { value: make<T>(v) }",
        ] {
            let text = format!("fn f() -> Unit {{ let x = Box {{ value: {expression} }} }}");
            let parsed = crate::parse(text.as_bytes());
            assert!(
                parsed.is_syntax_eligible(),
                "{text}: {:?}",
                parsed.diagnostics
            );
            let lexed = lex(text.as_bytes(), Limits::default());
            if expression.contains("make<T>") {
                assert!(
                    lexed.tokens.iter().any(|(_, t, _)| t.kind() == GenericOpen),
                    "{text}"
                );
            } else {
                assert!(
                    lexed.tokens.iter().any(|(_, t, _)| t.kind() == Less),
                    "{text}"
                );
            }
        }
        let parsed = crate::parse(
            b"fn f() -> Unit { let x = Box { value: make<Array<T>>(v), compare: a < b } }",
        );
        assert!(parsed.is_syntax_eligible(), "{:?}", parsed.diagnostics);
        let Declaration::Function(f) = &parsed.syntax.declarations[0].kind else {
            panic!("function missing")
        };
        let StatementKind::Local { value, .. } = &f.body.statements[0].kind else {
            panic!("local missing")
        };
        let ExprKind::Record { fields, .. } = &value.kind else {
            panic!("record missing")
        };
        let ExprKind::Call {
            type_arguments: Some(arguments),
            ..
        } = &fields.contents.items[0].kind.value.kind
        else {
            panic!("generic call erased")
        };
        let TypeKind::Named {
            arguments: Some(nested),
            ..
        } = &arguments.contents.items[0].kind
        else {
            panic!("nested type argument erased")
        };
        assert_eq!(nested.contents.items.len(), 1);
        assert!(matches!(
            fields.contents.items[1].kind.value.kind,
            ExprKind::Binary {
                operator: crate::syntax::BinaryOperator::Less,
                ..
            }
        ));
        for text in [
            "record Box<T> { value: Array<T>, pair: (Array<T>, [T]) }",
            "enum Value<T> { Some(Array<T>), Pair((Array<T>, [T])) }",
            "fn typed(value: (Array<T>, [T])) -> (Array<T>, [T]) { return value }",
            "fn constrained<T,U>() -> Unit where Array<T>: Bound<T>, Array<U>: Bound<U> { return }",
            "fn f() -> Unit { let x: (Array<T>, [T]) = make<T>(v) }",
            "fn f() -> Unit { if run(Box { value: make<T>(v) }) { return } }",
        ] {
            let parsed = crate::parse(text.as_bytes());
            assert!(
                parsed.is_syntax_eligible(),
                "{text}: {:?}",
                parsed.diagnostics
            );
        }
    }
    #[test]
    fn prospective_generic_budget_also_bounds_comparisons_with_type_shaped_operands() {
        // '< (b,c)' is still a possible generic type-list prefix. The seventh
        // inspected token, '}', establishes that this particular source is a
        // comparison. An exhausted classifier cannot safely choose that fallback.
        let text = b"fn f() -> Unit { a < (b,c) }";
        for budget in [5, 6, 7, 8] {
            let limits = Limits {
                max_generic_tokens: budget,
                ..Limits::default()
            };
            let lexed = lex(text, limits);
            assert_eq!(lexed.source.render(), text);
            assert_eq!(
                lexed
                    .diagnostics
                    .iter()
                    .any(|d| d.code == DiagnosticCode::Limit),
                budget < 7
            );
            assert_eq!(
                crate::parse_with_limits(text, limits).is_syntax_eligible(),
                budget >= 7
            );
            if budget < 7 {
                assert_eq!(
                    lexed.source.pieces.last().unwrap().kind,
                    PieceKind::Unparsed
                );
            }
        }
        for count in [2046, 2047] {
            let names = std::iter::repeat_n("b", count)
                .collect::<Vec<_>>()
                .join(",");
            let text = format!("fn f() -> Unit {{ a < ({names}) }}");
            let first = lex(text.as_bytes(), Limits::default());
            let second = lex(text.as_bytes(), Limits::default());
            assert_eq!(first.source.render(), text.as_bytes());
            assert_eq!(first.diagnostics, second.diagnostics);
            assert_eq!(
                first
                    .diagnostics
                    .iter()
                    .any(|d| d.code == DiagnosticCode::Limit),
                count == 2047
            );
            assert_eq!(
                crate::parse(text.as_bytes()).is_syntax_eligible(),
                count == 2046
            );
            // Grouping the left operand removes the named-callee prefix, so the
            // same broad tuple is recognized directly as a comparison operand.
            let grouped = format!("fn f() -> Unit {{ (a) < ({names}) }}");
            assert!(crate::parse(grouped.as_bytes()).is_syntax_eligible());
        }
        // A non-type kind peek resolves the ambiguity without consuming a
        // candidate terminal check. Only '<' and '(' consume this budget.
        for budget in [1, 2] {
            let parsed = crate::parse_with_limits(
                b"fn f() -> Unit { a < (1,b) }",
                Limits {
                    max_generic_tokens: budget,
                    ..Limits::default()
                },
            );
            assert_eq!(parsed.is_syntax_eligible(), budget == 2);
        }
        // An expression-only scalar resolves the ambiguity before the budget is
        // exhausted, even when the rest of the tuple is substantially broader.
        let names = std::iter::repeat_n("b", 3000).collect::<Vec<_>>().join(",");
        let text = format!("fn f() -> Unit {{ a < (1,{names}) }}");
        assert!(crate::parse(text.as_bytes()).is_syntax_eligible());
    }
}
