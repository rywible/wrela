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
fn start(c: char) -> bool {
    c == '_' || unicode16::contains(unicode16::XID_START, c)
}
fn continuation(c: char) -> bool {
    c == '_' || unicode16::contains(unicode16::XID_CONTINUE, c)
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
fn keyword(bytes: &[u8]) -> TokenKind {
    match bytes {
        b"fn" => Fn,
        b"record" => Record,
        b"enum" => Enum,
        b"where" => Where,
        b"let" => Let,
        b"var" => Var,
        b"mut" => Mut,
        b"return" => Return,
        b"if" => If,
        b"else" => Else,
        b"match" => Match,
        b"for" => For,
        b"every" => Every,
        b"in" => In,
        b"true" => True,
        b"false" => False,
        _ => Ident,
    }
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
    fn scan(&mut self) {
        while self.at < self.bytes.len() {
            let from = self.at;
            let b = self.bytes[from];
            match b {
                b' ' | b'\t' => {
                    while matches!(self.bytes.get(self.at), Some(b' ' | b'\t')) {
                        self.at += 1;
                    }
                    self.piece(from, PieceKind::Trivia(TriviaKind::Whitespace));
                }
                b'\r' | b'\n' => {
                    self.at += 1;
                    if b == b'\r' && self.bytes.get(self.at) == Some(&b'\n') {
                        self.at += 1;
                    }
                    let id = self.piece(from, PieceKind::Trivia(TriviaKind::Newline));
                    self.newline(id, from, self.at);
                }
                b'/' if self.bytes.get(from + 1) == Some(&b'/') => {
                    self.at += 2;
                    while self.at < self.bytes.len()
                        && !matches!(self.bytes[self.at], b'\r' | b'\n')
                    {
                        self.at += 1;
                    }
                    let valid = std::str::from_utf8(&self.bytes[from..self.at]).is_ok();
                    self.piece(
                        from,
                        if valid {
                            PieceKind::Trivia(TriviaKind::LineComment)
                        } else {
                            PieceKind::Invalid
                        },
                    );
                    if !valid {
                        self.error(
                            DiagnosticCode::Lexical,
                            "invalid UTF-8 in comment",
                            from,
                            self.at,
                        );
                    }
                }
                b'/' if self.bytes.get(from + 1) == Some(&b'*') => {
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
                        break;
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
                }
                b'"' => {
                    self.at += 1;
                    let mut valid = true;
                    let mut closed = false;
                    while self.at < self.bytes.len()
                        && !matches!(self.bytes[self.at], b'\r' | b'\n')
                    {
                        match self.bytes[self.at] {
                            b'"' => {
                                self.at += 1;
                                closed = true;
                                break;
                            }
                            b'\\' => {
                                self.at += 1;
                                if self.at == self.bytes.len()
                                    || matches!(self.bytes[self.at], b'\r' | b'\n')
                                {
                                    valid = false;
                                    break;
                                }
                                if !matches!(self.bytes[self.at], b'n' | b'r' | b't' | b'"' | b'\\')
                                {
                                    valid = false;
                                }
                                self.at += 1;
                            }
                            _ => {
                                if let Some((_, n)) = scalar(self.bytes, self.at) {
                                    self.at += n;
                                } else {
                                    valid = false;
                                    self.at += 1;
                                }
                            }
                        }
                    }
                    valid &= closed;
                    self.piece(
                        from,
                        if valid {
                            PieceKind::Token(String)
                        } else {
                            PieceKind::Invalid
                        },
                    );
                    if !valid {
                        self.error(
                            DiagnosticCode::Lexical,
                            "unterminated string, invalid escape, or invalid UTF-8",
                            from,
                            self.at,
                        );
                    }
                }
                b'0'..=b'9' => {
                    self.at += 1;
                    while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                        self.at += 1;
                    }
                    if self.bytes.get(self.at) == Some(&b'.')
                        && self.bytes.get(self.at + 1).is_some_and(u8::is_ascii_digit)
                    {
                        self.at += 1;
                        while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                            self.at += 1;
                        }
                    }
                    let mut valid = true;
                    if matches!(self.bytes.get(self.at), Some(b'e' | b'E')) {
                        self.at += 1;
                        if matches!(self.bytes.get(self.at), Some(b'+' | b'-')) {
                            self.at += 1;
                        }
                        let digits = self.at;
                        while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                            self.at += 1;
                        }
                        valid = self.at > digits;
                    }
                    while let Some((c, n)) = scalar(self.bytes, self.at) {
                        if continuation(c) || ignorable(c) {
                            valid = false;
                            self.at += n;
                        } else {
                            break;
                        }
                    }
                    self.piece(
                        from,
                        if valid {
                            PieceKind::Token(Number)
                        } else {
                            PieceKind::Invalid
                        },
                    );
                    if !valid {
                        self.error(
                            DiagnosticCode::Lexical,
                            "invalid decimal number spelling",
                            from,
                            self.at,
                        );
                    }
                }
                _ => {
                    if let Some((c, n)) = scalar(self.bytes, from) {
                        if start(c) || ignorable(c) {
                            let mut valid = start(c) && !ignorable(c);
                            let mut forbidden =
                                ignorable(c).then_some(ByteRange::new(self.at, self.at + n));
                            self.at += n;
                            while let Some((c, n)) = scalar(self.bytes, self.at) {
                                if continuation(c) || ignorable(c) {
                                    valid &= !ignorable(c);
                                    if ignorable(c) && forbidden.is_none() {
                                        forbidden = Some(ByteRange::new(self.at, self.at + n));
                                    }
                                    self.at += n;
                                } else {
                                    break;
                                }
                            }
                            let kind = if valid {
                                PieceKind::Token(keyword(&self.bytes[from..self.at]))
                            } else {
                                PieceKind::Invalid
                            };
                            self.piece(from, kind);
                            if let Some(range) = forbidden {
                                self.error(
                                    DiagnosticCode::Lexical,
                                    "default-ignorable character forbidden in identifier",
                                    range.start,
                                    range.end,
                                );
                            }
                        } else if let Some((kind, width)) = punctuation(&self.bytes[from..]) {
                            if matches!(kind, LParen | LBracket | LBrace) {
                                if self.delimiters.len() >= self.limits.max_depth {
                                    self.error(
                                        DiagnosticCode::Limit,
                                        "delimiter depth limit exceeded",
                                        from,
                                        from,
                                    );
                                    self.remainder();
                                    break;
                                }
                                self.delimiters.push(kind);
                            } else if matches!(kind, RParen | RBracket | RBrace) {
                                let opening = match kind {
                                    RParen => LParen,
                                    RBracket => LBracket,
                                    RBrace => LBrace,
                                    _ => unreachable!(),
                                };
                                if self.delimiters.last() == Some(&opening) {
                                    self.delimiters.pop();
                                }
                            }
                            self.at += width;
                            self.piece(from, PieceKind::Token(kind));
                        } else {
                            self.at += n;
                            self.piece(from, PieceKind::Invalid);
                            self.error(
                                DiagnosticCode::Lexical,
                                "character is not admitted by the lexical policy",
                                from,
                                self.at,
                            );
                        }
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
fn punctuation(bytes: &[u8]) -> Option<(TokenKind, usize)> {
    let two = match bytes.get(..2) {
        Some(b"::") => Some(PathSeparator),
        Some(b"->") => Some(Arrow),
        Some(b"=>") => Some(FatArrow),
        Some(b"==") => Some(EqualEqual),
        Some(b"!=") => Some(BangEqual),
        Some(b"<=") => Some(LessEqual),
        Some(b">=") => Some(GreaterEqual),
        Some(b"&&") => Some(AndAnd),
        Some(b"||") => Some(OrOr),
        _ => None,
    };
    if let Some(k) = two {
        return Some((k, 2));
    }
    let k = match bytes.first()? {
        b'(' => LParen,
        b')' => RParen,
        b'[' => LBracket,
        b']' => RBracket,
        b'{' => LBrace,
        b'}' => RBrace,
        b',' => Comma,
        b':' => Colon,
        b';' => Semicolon,
        b'.' => Dot,
        b'?' => Question,
        b'+' => Plus,
        b'-' => Minus,
        b'*' => Star,
        b'/' => Slash,
        b'%' => Percent,
        b'!' => Bang,
        b'=' => Equal,
        b'<' => Less,
        b'>' => Greater,
        _ => return None,
    };
    Some((k, 1))
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
    let mut tokens = logical_tokens(&mut scanner);
    bound_expression_complexity(&mut scanner, &mut tokens);
    Lexed {
        source: scanner.source,
        tokens,
        diagnostics: scanner.diagnostics,
    }
}

/// A bounded syntax-only recognizer for prospective type lists. No symbol lookup.
struct TypeLookahead<'a> {
    kinds: &'a [TokenKind],
    at: usize,
    visited: usize,
    limit: usize,
    depth_limit: usize,
    angles: Vec<(usize, usize)>,
}
#[derive(Clone, Copy, Debug)]
enum LookaheadFailure {
    NotType,
    Tokens,
    Depth,
}
impl TypeLookahead<'_> {
    fn take(&mut self, k: TokenKind) -> Result<(), LookaheadFailure> {
        if self.visited >= self.limit {
            return Err(LookaheadFailure::Tokens);
        }
        self.visited += 1;
        if self.kinds.get(self.at) != Some(&k) {
            return Err(LookaheadFailure::NotType);
        }
        self.at += 1;
        Ok(())
    }
    fn ty(&mut self, depth: usize) -> Result<(), LookaheadFailure> {
        if depth > self.depth_limit {
            return Err(LookaheadFailure::Depth);
        }
        match self.kinds.get(self.at).copied() {
            Some(Ident) => {
                self.take(Ident)?;
                while self.kinds.get(self.at) == Some(&PathSeparator) {
                    self.take(PathSeparator)?;
                    self.take(Ident)?;
                }
                if self.kinds.get(self.at) == Some(&Less) {
                    self.list(depth + 1)?;
                }
                Ok(())
            }
            Some(LParen) => {
                self.take(LParen)?;
                if self.kinds.get(self.at) != Some(&RParen) {
                    self.ty(depth + 1)?;
                    while self.kinds.get(self.at) == Some(&Comma) {
                        self.take(Comma)?;
                        if self.kinds.get(self.at) == Some(&RParen) {
                            break;
                        }
                        self.ty(depth + 1)?;
                    }
                }
                self.take(RParen)
            }
            Some(LBracket) => {
                self.take(LBracket)?;
                self.ty(depth + 1)?;
                self.take(RBracket)
            }
            _ => Err(LookaheadFailure::NotType),
        }
    }
    fn list(&mut self, depth: usize) -> Result<(), LookaheadFailure> {
        if depth > self.depth_limit {
            return Err(LookaheadFailure::Depth);
        }
        let open = self.at;
        self.take(Less)?;
        self.ty(depth)?;
        while self.kinds.get(self.at) == Some(&Comma) {
            self.take(Comma)?;
            if self.kinds.get(self.at) == Some(&Greater) {
                break;
            }
            self.ty(depth)?;
        }
        let close = self.at;
        self.take(Greater)?;
        self.angles.push((open, close));
        Ok(())
    }
}
fn logical_tokens(scanner: &mut Scanner<'_>) -> Vec<(usize, Tok, usize)> {
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
    let mut suppress = vec![false; raw.len()];
    let mut type_context = false;
    let mut declaration_name = false;
    let mut stop = None;
    let mut s = 0;
    while s < significant.len() {
        let k = kinds[s];
        if s > 0 && !gap_allowed(significant[s - 1], significant[s]) {
            type_context = false;
            declaration_name = false;
        }
        if k == Less
            && s > 0
            && kinds[s - 1] == Ident
            && (type_context || gap_allowed(significant[s - 1], significant[s]))
        {
            let mut look = TypeLookahead {
                kinds: &kinds,
                at: s,
                visited: 0,
                limit: scanner.limits.max_generic_tokens,
                depth_limit: scanner.limits.max_depth,
                angles: Vec::new(),
            };
            match look.list(1) {
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
                Err(LookaheadFailure::NotType) => {}
                Err(problem) => {
                    stop = Some((s, problem));
                    break;
                }
            }
        }
        match k {
            Fn | Record | Enum => {
                declaration_name = true;
                type_context = false;
            }
            Ident if declaration_name => {
                declaration_name = false;
                type_context = true;
            }
            Colon | Arrow | Where => type_context = true,
            Equal | Semicolon | LBrace | RBrace => {
                type_context = false;
                declaration_name = false;
            }
            // A declaration's parameter list starts fresh annotation contexts.
            LParen if s > 0 && matches!(kinds[s - 1], Ident | TypeClose) => type_context = false,
            _ => {}
        }
        s += 1;
    }
    if let Some((s, problem)) = stop {
        let raw_index = significant[s];
        let start = raw[raw_index].0;
        let id = raw[raw_index].1.id();
        let message = match problem {
            LookaheadFailure::Tokens => "prospective generic token limit exceeded",
            LookaheadFailure::Depth => "prospective generic depth limit exceeded",
            LookaheadFailure::NotType => unreachable!(),
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
}
