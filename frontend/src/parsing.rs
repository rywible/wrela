//! Generated recognition and bounded diagnostic conversion.
use crate::{
    diagnostic::{Diagnostic, DiagnosticCode, push_bounded},
    source::{ByteRange, PieceKind, Source},
    syntax::{Declaration, Document, ErrorSyntax, IncompleteFunction, Node},
    token::{Tok, TokenId},
};
use lalrpop_util::{ErrorRecovery, ParseError};
lalrpop_util::lalrpop_mod!(
    #[allow(clippy::all)]
    grammar
);
pub type RecognitionError = ParseError<usize, Tok, &'static str>;
#[derive(Clone, Debug)]
pub struct Parsed {
    pub document: Document,
    pub diagnostics: Vec<Diagnostic>,
    pub complete: bool,
}
/// Collect diagnostics as recognition proceeds, so rejected input cannot create
/// an unbounded error backlog before the public frontend applies its budget.
pub struct RecoveryDiagnostics {
    diagnostics: Vec<Diagnostic>,
    maximum: usize,
    last_eof: Option<RecognitionError>,
}
impl RecoveryDiagnostics {
    fn new(maximum: usize) -> Self {
        Self {
            diagnostics: Vec::new(),
            maximum,
            last_eof: None,
        }
    }
    pub fn can_record(&self) -> bool {
        self.diagnostics.len() < self.maximum.max(1)
    }
    pub fn record(&mut self, source: &Source, error: RecognitionError) {
        push_bounded(
            &mut self.diagnostics,
            diagnostic(source, &error),
            self.maximum,
        );
        if matches!(error, ParseError::UnrecognizedEof { .. }) {
            self.last_eof = Some(error);
        }
    }
}
pub fn parse(source: &Source, tokens: &[(usize, Tok, usize)]) -> Parsed {
    parse_with_limit(source, tokens, 64)
}
pub fn parse_with_limit(source: &Source, tokens: &[(usize, Tok, usize)], maximum: usize) -> Parsed {
    let mut errors = RecoveryDiagnostics::new(maximum);
    let result =
        grammar::DocumentParser::new().parse(source, &mut errors, tokens.iter().copied().map(Ok));
    match result {
        Ok(document) => {
            if let Some(error) = errors.last_eof.take() {
                let document = incomplete_document(source, tokens, &error, errors.maximum);
                Parsed {
                    document,
                    diagnostics: errors.diagnostics,
                    complete: false,
                }
            } else {
                Parsed {
                    complete: !document.has_errors(),
                    document,
                    diagnostics: errors.diagnostics,
                }
            }
        }
        Err(error) => {
            errors.record(source, error.clone());
            let document = incomplete_document(source, tokens, &error, errors.maximum);
            Parsed {
                document,
                diagnostics: errors.diagnostics,
                complete: false,
            }
        }
    }
}
pub fn expected(error: &RecognitionError) -> Vec<String> {
    match error {
        ParseError::UnrecognizedEof { expected, .. }
        | ParseError::UnrecognizedToken { expected, .. } => expected.clone(),
        _ => vec![],
    }
}
pub fn error_syntax(
    source: &Source,
    start: usize,
    end: usize,
    error: &ErrorRecovery<usize, Tok, &'static str>,
    retain_expected: bool,
) -> ErrorSyntax {
    // LALRPOP's dropped_tokens omits tokens already shifted and later popped
    // during recovery. The recognized error range includes that prefix.
    let first = source
        .pieces
        .partition_point(|piece| piece.range.end <= start);
    let mut tokens: Vec<_> = source.pieces[first..]
        .iter()
        .enumerate()
        .take_while(|(_, piece)| piece.range.start < end)
        .filter(|(_, piece)| {
            matches!(
                piece.kind,
                PieceKind::Token(_) | PieceKind::Invalid | PieceKind::Unparsed
            )
        })
        .map(|(index, _)| TokenId(first + index))
        .collect();
    tokens.extend(error.dropped_tokens.iter().map(|t| t.1.id()));
    tokens.sort_by_key(|id| id.0);
    tokens.dedup();
    ErrorSyntax {
        tokens,
        expected: if retain_expected {
            expected(&error.error)
        } else {
            vec![]
        },
    }
}
pub fn diagnostic(source: &Source, error: &RecognitionError) -> Diagnostic {
    let (range, message) = match error {
        ParseError::UnrecognizedEof { expected, .. } => (
            ByteRange::empty(source.len()),
            format!("incomplete syntax; expected {}", expected.join(", ")),
        ),
        ParseError::UnrecognizedToken {
            token: (l, t, r),
            expected,
        } => (
            ByteRange::new(*l, *r),
            format!("unexpected {t}; expected {}", expected.join(", ")),
        ),
        ParseError::ExtraToken { token: (l, t, r) } => {
            (ByteRange::new(*l, *r), format!("unexpected {t}"))
        }
        ParseError::InvalidToken { location } => (
            ByteRange::empty(*location),
            "invalid parser token".to_owned(),
        ),
        ParseError::User { error } => (ByteRange::empty(source.len()), (*error).to_owned()),
    };
    Diagnostic::new(DiagnosticCode::Syntax, message, range)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Delimiter {
    Brace,
    Parenthesis,
    Bracket,
}
fn track_delimiter(stack: &mut Vec<Delimiter>, token: Tok) {
    match token {
        Tok::LBrace(_) => stack.push(Delimiter::Brace),
        Tok::LParen(_) => stack.push(Delimiter::Parenthesis),
        Tok::LBracket(_) => stack.push(Delimiter::Bracket),
        Tok::RBrace(_) if stack.last() == Some(&Delimiter::Brace) => {
            stack.pop();
        }
        Tok::RParen(_) if stack.last() == Some(&Delimiter::Parenthesis) => {
            stack.pop();
        }
        Tok::RBracket(_) if stack.last() == Some(&Delimiter::Bracket) => {
            stack.pop();
        }
        _ => {}
    }
}

/// On terminal EOF failure, preserve prior complete declarations and a generated,
/// typed function header. Recovery recognizes complete statement islands only.
/// The primary document parse owns diagnostics; reparsing islands reconstructs
/// structure without reporting the same failures or slice-local EOFs again.
fn incomplete_document(
    source: &Source,
    tokens: &[(usize, Tok, usize)],
    error: &RecognitionError,
    diagnostic_limit: usize,
) -> Document {
    let mut stack = Vec::new();
    let mut starts = Vec::new();
    for (i, (_, tok, _)) in tokens.iter().enumerate() {
        if stack.is_empty() && matches!(tok, Tok::Fn(_) | Tok::Record(_) | Tok::Enum(_)) {
            starts.push(i);
        }
        track_delimiter(&mut stack, *tok);
    }
    let start = starts.last().copied().unwrap_or(0);
    let mut recovered = RecoveryDiagnostics::new(diagnostic_limit);
    let mut document = grammar::DocumentParser::new()
        .parse(
            source,
            &mut recovered,
            tokens[..start].iter().copied().map(Ok),
        )
        .unwrap_or(Document {
            range: ByteRange::new(0, source.len()),
            syntax_range: ByteRange::empty(source.len()),
            declarations: vec![],
            separators: vec![],
        });
    let mut prefix = None;
    if matches!(tokens.get(start).map(|t| t.1), Some(Tok::Fn(_)))
        && let Some(open) = tokens[start..]
            .iter()
            .position(|t| matches!(t.1, Tok::LBrace(_)))
    {
        let end = start + open + 1;
        let mut scratch = RecoveryDiagnostics::new(diagnostic_limit);
        if let Ok(header) = grammar::FunctionPrefixParser::new().parse(
            source,
            &mut scratch,
            tokens[start..end].iter().copied().map(Ok),
        ) {
            prefix = Some((header, end));
        }
    }
    let declaration = if let Some((header, body_start)) = prefix {
        let mut statements = Vec::new();
        let mut separators = Vec::new();
        let mut begin = body_start;
        let mut depth = Vec::new();
        for i in body_start..tokens.len() {
            let tok = tokens[i].1;
            if depth.is_empty() && matches!(tok, Tok::Newline(_) | Tok::Semicolon(_)) {
                if begin < i {
                    let mut scratch = RecoveryDiagnostics::new(diagnostic_limit);
                    if let Ok(statement) = grammar::StatementParser::new().parse(
                        source,
                        &mut scratch,
                        tokens[begin..i].iter().copied().map(Ok),
                    ) {
                        statements.push(statement);
                    }
                }
                separators.push(tok.id());
                begin = i + 1;
            } else {
                track_delimiter(&mut depth, tok);
            }
        }
        let remainder = ErrorSyntax {
            tokens: tokens[begin..].iter().map(|t| t.1.id()).collect(),
            expected: expected(error),
        };
        Node::new(
            header.range.start,
            source.len(),
            Declaration::IncompleteFunction(IncompleteFunction {
                prefix: header,
                statements,
                separators,
                remainder,
                missing_close: ByteRange::empty(source.len()),
            }),
        )
    } else {
        Node::new(
            tokens.get(start).map_or(source.len(), |t| t.0),
            source.len(),
            Declaration::Error(ErrorSyntax {
                tokens: tokens[start..].iter().map(|t| t.1.id()).collect(),
                expected: expected(error),
            }),
        )
    };
    document.declarations.push(declaration);
    document.range = ByteRange::new(0, source.len());
    document.syntax_range = ByteRange::new(
        document.declarations.first().unwrap().range.start,
        source.len(),
    );
    document
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lexer::{Limits, lex},
        syntax::StatementKind,
    };
    #[test]
    fn eof_recovery_retains_prior_declarations_typed_header_and_completed_statements() {
        let text=b"fn ready() -> Unit { return }\nfn broken(mut x: Array<T>) -> Unit {\n let y = 1\n // trailing\r\n  ";
        let lexed = lex(text, Limits::default());
        let parsed = parse(&lexed.source, &lexed.tokens);
        assert!(!parsed.complete);
        assert!(matches!(
            parsed.document.declarations[0].kind,
            Declaration::Function(_)
        ));
        let Declaration::IncompleteFunction(f) = &parsed.document.declarations[1].kind else {
            panic!("typed prefix was lost: {:?}", parsed.document)
        };
        assert_eq!(lexed.source.token_text(f.prefix.kind.name), Some("broken"));
        assert!(
            f.prefix.kind.parameters.contents.items[0]
                .kind
                .permission
                .is_some()
        );
        assert_eq!(f.missing_close, ByteRange::empty(text.len()));
        assert!(matches!(f.statements[0].kind, StatementKind::Local { .. }));
        assert_eq!(parsed.diagnostics[0].range, ByteRange::empty(text.len()));
    }
    #[test]
    fn generated_recovery_keeps_statement_tokens_as_references() {
        let text = b"fn f() -> Unit { let a = ; let b = ; let tail = 3; return }";
        let lexed = lex(text, Limits::default());
        let parsed = parse(&lexed.source, &lexed.tokens);
        assert_eq!(parsed.diagnostics.len(), 2);
        let Declaration::Function(f) = &parsed.document.declarations[0].kind else {
            panic!("later body lost")
        };
        assert!(matches!(f.body.statements[0].kind, StatementKind::Error(_)));
        assert!(matches!(f.body.statements[1].kind, StatementKind::Error(_)));
        assert!(matches!(
            f.body.statements[2].kind,
            StatementKind::Local { .. }
        ));
        assert!(matches!(
            f.body.statements[3].kind,
            StatementKind::Return { .. }
        ));
    }
    #[test]
    fn recovery_budget_is_applied_during_recognition() {
        let mut text = String::from("fn f() -> Unit {\n");
        for _ in 0..100 {
            text.push_str("let a = ;\n");
        }
        text.push('}');
        let lexed = lex(text.as_bytes(), Limits::default());
        let parsed = parse_with_limit(&lexed.source, &lexed.tokens, 3);
        assert_eq!(parsed.diagnostics.len(), 3);
        assert_eq!(
            parsed.diagnostics.last().unwrap().code,
            DiagnosticCode::Truncated
        );
        let Declaration::Function(f) = &parsed.document.declarations[0].kind else {
            panic!("function lost");
        };
        assert_eq!(f.body.statements.len(), 100);
        let StatementKind::Error(error) = &f.body.statements[99].kind else {
            panic!("error island lost");
        };
        assert!(
            error.expected.is_empty(),
            "repeated expected-token strings should respect the diagnostic budget"
        );
        assert!(
            !error.tokens.is_empty(),
            "source references must survive diagnostic truncation"
        );
    }
    #[test]
    fn incomplete_recovery_reports_primary_errors_once_without_spending_extra_budget() {
        let text = "fn ready() -> Unit { return }\nfn broken() -> Unit {\n    let bad = ;\n    let good = 3\n    // trailing 🚀\r\n  ";
        let lexed = lex(text.as_bytes(), Limits::default());
        assert!(lexed.diagnostics.is_empty());
        let semicolon = text.find(';').unwrap();
        let expected_ranges = [
            ByteRange::new(semicolon, semicolon + 1),
            ByteRange::empty(text.len()),
        ];
        for maximum in [64, 2] {
            let parsed = parse_with_limit(&lexed.source, &lexed.tokens, maximum);
            assert_eq!(
                parsed.diagnostics.len(),
                2,
                "statement-island reparsing must not add diagnostics: {:?}",
                parsed.diagnostics
            );
            assert_eq!(
                parsed
                    .diagnostics
                    .iter()
                    .map(|d| d.range)
                    .collect::<Vec<_>>(),
                expected_ranges
            );
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .all(|d| d.code == DiagnosticCode::Syntax)
            );
            assert_eq!(
                parsed
                    .diagnostics
                    .iter()
                    .filter(|d| d.range == ByteRange::empty(text.len()))
                    .count(),
                1
            );
            assert!(!parsed.complete);
            assert!(parsed.document.has_errors());
            assert_eq!(lexed.source.render(), text.as_bytes());
            assert!(matches!(
                parsed.document.declarations[0].kind,
                Declaration::Function(_)
            ));
            let Declaration::IncompleteFunction(function) = &parsed.document.declarations[1].kind
            else {
                panic!("typed function prefix lost");
            };
            assert_eq!(function.missing_close, ByteRange::empty(text.len()));
            assert_eq!(function.statements.len(), 2);
            assert!(matches!(
                function.statements[0].kind,
                StatementKind::Error(_)
            ));
            let StatementKind::Local { name, .. } = &function.statements[1].kind else {
                panic!("later local lost");
            };
            assert_eq!(lexed.source.token_text(*name), Some("good"));
        }
    }
}
