pub mod ast;
pub mod event;
pub mod grammar;
pub mod kind;
pub mod sink;
pub mod source;
pub mod validate;

use crate::diag::catalog::ParseDiagKind;
use crate::lexer::LexError;
use event::Event;
use kind::SyntaxKind;
use miette::SourceSpan;
use source::TokenSource;
use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WrelaLanguage {}

impl rowan::Language for WrelaLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        Self::Kind::from(raw.0)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind.into())
    }
}

pub type SyntaxNode = rowan::SyntaxNode<WrelaLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<WrelaLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<WrelaLanguage>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<WrelaLanguage>;
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<WrelaLanguage>;

pub fn parse(text: &str) -> SyntaxNode {
    parse_with_errors(text).0
}

pub fn parse_with_errors(text: &str) -> (SyntaxNode, Vec<ParseError>) {
    let (tokens, lex_errors) = TokenSource::lex_with_errors(text);
    let source = TokenSource::from_tokens(text, tokens.clone());
    let mut parser = Parser::new(source);
    grammar::root(&mut parser);
    let (events, mut errors) = parser.finish();
    let sink = sink::TreeSink::from_tokens(text, tokens, events);
    errors.extend(lex_errors.into_iter().map(parse_error_from_lex_error));
    (SyntaxNode::new_root(sink.finish()), errors)
}

fn parse_error_from_lex_error(error: LexError) -> ParseError {
    match error {
        LexError::UnexpectedTabCharacter { span } => ParseError {
            kind: ParseDiagKind::Lexical,
            message: "unexpected tab (\\t) character".to_string(),
            span,
            code: Some("lang::lex::unexpected_tab".to_string()),
            help: Some("Use spaces instead of tab characters.".to_string()),
        },
        LexError::UnexpectedCharacter { span, char } => ParseError {
            kind: ParseDiagKind::Lexical,
            message: format!("unexpected character '{char}'"),
            span,
            code: Some("lang::lex::unexpected_char".to_string()),
            help: Some("This character is not valid in this position.".to_string()),
        },
        LexError::UnterminatedString { span } => ParseError {
            kind: ParseDiagKind::Lexical,
            message: "unterminated string literal".to_string(),
            span,
            code: Some("lang::lex::unterminated_string".to_string()),
            help: Some("Add a closing quote (\") to the end of the string.".to_string()),
        },
        LexError::InvalidEscapeSequence { span, char } => ParseError {
            kind: ParseDiagKind::Lexical,
            message: format!("invalid escape sequence '\\{char}'"),
            span,
            code: Some("lang::lex::invalid_escape".to_string()),
            help: Some("Supported escapes are \\n, \\r, \\t, \\\", and \\\\".to_string()),
        },
        LexError::LegacySoComment { span } => ParseError {
            kind: ParseDiagKind::Lexical,
            message: "legacy `so:` comments are not supported".to_string(),
            span,
            code: Some("lang::lex::legacy_so_comment".to_string()),
            help: Some("Use `// ...` or `/* ... */` comments instead.".to_string()),
        },
        LexError::InvalidLiteral { span } => ParseError {
            kind: ParseDiagKind::Lexical,
            message: "invalid numeric literal".to_string(),
            span,
            code: Some("lang::lex::invalid_literal".to_string()),
            help: Some("Check the format of this numeric literal.".to_string()),
        },
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub kind: ParseDiagKind,
    pub message: String,
    pub span: SourceSpan,
    pub code: Option<String>,
    pub help: Option<String>,
}

pub struct Parser<'a> {
    source: TokenSource<'a>,
    events: Vec<Event>,
    errors: Vec<ParseError>,
}

impl<'a> Parser<'a> {
    pub fn new(source: TokenSource<'a>) -> Self {
        Self {
            source,
            events: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::Placeholder);
        Marker::new(pos)
    }

    pub fn peek(&self) -> SyntaxKind {
        let mut n = 0;
        loop {
            let kind = self.source.peek_at(n);
            if !kind.is_trivia() {
                return kind;
            }
            n += 1;
        }
    }

    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.peek() == kind
    }

    pub fn at_ident_text(&self, text: &str) -> bool {
        self.peek() == SyntaxKind::Ident && self.peek_text() == text
    }

    pub fn peek_text(&self) -> &str {
        let mut n = 0;
        loop {
            let kind = self.source.peek_at(n);
            if !kind.is_trivia() {
                return self.source.text_at(n);
            }
            n += 1;
        }
    }

    pub fn peek_nth_non_trivia(&self, target: usize) -> SyntaxKind {
        let mut raw = 0;
        let mut seen = 0;
        loop {
            let kind = self.source.peek_at(raw);
            if !kind.is_trivia() {
                if seen == target {
                    return kind;
                }
                seen += 1;
            }
            raw += 1;
        }
    }

    pub fn bump(&mut self) {
        loop {
            let kind = self.source.peek();
            if !kind.is_trivia() {
                self.events.push(Event::AddToken);
                self.source.bump();
                break;
            }
            self.source.bump();
        }
    }

    pub fn expect(&mut self, kind: SyntaxKind) {
        if self.at(kind) {
            self.bump();
        } else {
            self.error_expected(kind);
        }
    }

    pub fn error(&mut self) {
        self.error_unexpected(true);
    }

    pub fn error_no_bump(&mut self) {
        self.error_unexpected(false);
    }

    pub fn expect_stmt_boundary(&mut self) {
        if self.at_stmt_boundary() {
            self.consume_stmt_separators();
        } else {
            self.error_expected_stmt_boundary();
        }
    }

    pub fn error_expected_stmt_boundary(&mut self) {
        self.error_with_kind(
            ParseDiagKind::ExpectedStatementBoundary,
            "expected end of line",
            false,
        );
    }

    pub fn error_with_message_no_bump(&mut self, message: &str) {
        self.error_with_kind(ParseDiagKind::SyntaxError, message, false);
    }

    pub fn expect_with_message(&mut self, kind: SyntaxKind, message: &str) {
        if self.at(kind) {
            self.bump();
        } else {
            self.error_with_kind(ParseDiagKind::ExpectedToken, message, true);
        }
    }

    pub fn peek_nontrivia_at(&self, n: usize) -> SyntaxKind {
        let mut seen = 0;
        let mut idx = 0;
        loop {
            let kind = self.source.peek_at(idx);
            if !kind.is_trivia() {
                if seen == n {
                    return kind;
                }
                seen += 1;
            }
            idx += 1;
        }
    }

    pub fn recover_until(&mut self, sync: &[SyntaxKind]) {
        while !self.is_at_eof() {
            let kind = self.source.peek();
            if sync.contains(&kind) {
                break;
            }
            self.bump_any();
        }
    }

    pub fn finish(mut self) -> (Vec<Event>, Vec<ParseError>) {
        self.repair_forward_parents();
        (self.events, self.errors)
    }

    pub fn is_at_eof(&self) -> bool {
        let mut idx = 0;
        loop {
            let kind = self.source.peek_at(idx);
            if kind == SyntaxKind::Eof {
                return true;
            }
            if !kind.is_trivia() {
                return false;
            }
            idx += 1;
        }
    }

    pub fn cursor_pos(&self) -> usize {
        self.source.cursor()
    }

    pub fn consume_trivia(&mut self) {
        while self.peek().is_trivia() {
            self.bump_any();
        }
    }

    fn error_with_message(&mut self, message: &str, should_bump: bool) {
        self.error_with_kind(ParseDiagKind::SyntaxError, message, should_bump);
    }

    fn error_with_kind(&mut self, kind: ParseDiagKind, message: &str, should_bump: bool) {
        let desc = crate::diag::catalog::parse_descriptor(kind);
        let span = self.source.peek_span();
        self.errors.push(ParseError {
            kind,
            message: message.to_string(),
            span,
            code: Some(desc.code.to_string()),
            help: Some(desc.help_template.to_string()),
        });
        let m = self.start();
        if should_bump && !self.is_at_eof() {
            self.bump();
        }
        m.complete(self, SyntaxKind::Error);
    }

    fn error_unexpected(&mut self, should_bump: bool) {
        let found_kind = self.peek();
        let found = format_found(found_kind, self.source.text());
        let message = format!("unexpected {}", found);
        self.error_with_kind(ParseDiagKind::UnexpectedToken, &message, should_bump);
    }

    fn error_expected(&mut self, expected: SyntaxKind) {
        let expected_label = kind_label(expected);
        let found_kind = self.peek();
        let found = format_found(found_kind, self.source.text());
        let message = format!("expected {}, found {}", expected_label, found);
        self.error_with_kind(ParseDiagKind::ExpectedToken, &message, true);
    }

    fn bump_any(&mut self) {
        let kind = self.source.peek();
        if !kind.is_trivia() {
            self.events.push(Event::AddToken);
        }
        self.source.bump();
    }

    pub(crate) fn at_stmt_boundary(&self) -> bool {
        let mut n = 0;
        loop {
            let kind = self.source.peek_at(n);
            if kind == SyntaxKind::Eof || kind == SyntaxKind::RBrace {
                return true;
            }
            if kind == SyntaxKind::Newline {
                return true;
            }
            if kind.is_trivia() {
                n += 1;
                continue;
            }
            return false;
        }
    }

    pub(crate) fn has_newline_before_next_token(&self) -> bool {
        let mut n = 0;
        loop {
            let kind = self.source.peek_at(n);
            if kind == SyntaxKind::Newline {
                return true;
            }
            if !kind.is_trivia() || kind == SyntaxKind::Eof {
                return false;
            }
            n += 1;
        }
    }

    fn consume_stmt_separators(&mut self) {
        while let SyntaxKind::Newline
        | SyntaxKind::Whitespace
        | SyntaxKind::Comment
        | SyntaxKind::DocComment = self.source.peek()
        {
            self.source.bump();
        }
    }

    fn repair_forward_parents(&mut self) {
        let len = self.events.len();
        let mut claimed_targets = std::collections::HashSet::new();
        for i in 0..len {
            let Event::StartNode {
                forward_parent: Some(distance),
                ..
            } = self.events[i]
            else {
                continue;
            };
            let idx = i + distance;
            let is_valid = idx < len
                && matches!(self.events[idx], Event::StartNode { .. })
                && claimed_targets.insert(idx);
            if !is_valid
                && let Event::StartNode {
                    ref mut forward_parent,
                    ..
                } = self.events[i]
            {
                *forward_parent = None;
            }
        }
    }
}

fn kind_label(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::ClassKw => "'class'",
        SyntaxKind::ComponentKw => "'component'",
        SyntaxKind::ResourceKw => "'resource'",
        SyntaxKind::EventKw => "'event'",
        SyntaxKind::InterfaceKw => "'interface'",
        SyntaxKind::HasKw => "'has'",
        SyntaxKind::CanKw => "'can'",
        SyntaxKind::FnKw => "'fn'",
        SyntaxKind::SystemKw => "'system'",
        SyntaxKind::WidgetKw => "'widget'",
        SyntaxKind::IfKw => "'if'",
        SyntaxKind::ElseKw => "'else'",
        SyntaxKind::WhileKw => "'while'",
        SyntaxKind::ForKw => "'for'",
        SyntaxKind::InKw => "'in'",
        SyntaxKind::ReturnKw => "'return'",
        SyntaxKind::BreakKw => "'break'",
        SyntaxKind::ContinueKw => "'continue'",
        SyntaxKind::MatchKw => "'match'",
        SyntaxKind::DefaultKw => "'default'",
        SyntaxKind::ErrKw => "'error'",
        SyntaxKind::CrashKw => "'crash'",
        SyntaxKind::TrueKw => "'true'",
        SyntaxKind::FalseKw => "'false'",
        SyntaxKind::NothingKw => "'nothing'",
        SyntaxKind::AndKw => "'and'",
        SyntaxKind::OrKw => "'or'",
        SyntaxKind::NotKw => "'not'",
        SyntaxKind::AwaitKw => "'await'",
        SyntaxKind::DetachKw => "'detach'",
        SyntaxKind::SpawnKw => "'spawn'",
        SyntaxKind::FireKw => "'fire'",
        SyntaxKind::IgnoreKw => "'ignore'",
        SyntaxKind::CaptureKw => "'capture'",
        SyntaxKind::UseKw => "'use'",
        SyntaxKind::FromKw => "'from'",
        SyntaxKind::PrivateKw => "'private'",
        SyntaxKind::SelfKw => "'self'",
        SyntaxKind::MutableKw => "'mutable'",
        SyntaxKind::IsKw => "'is'",
        SyntaxKind::EnumKw => "'enum'",
        SyntaxKind::CheckKw => "'check'",
        SyntaxKind::ChecksKw => "'checks'",
        SyntaxKind::GivenKw => "'given'",
        SyntaxKind::RequireKw => "'require'",
        SyntaxKind::PresetKw => "'preset'",
        SyntaxKind::ProfileKw => "'profile'",
        SyntaxKind::OverridesKw => "'overrides'",
        SyntaxKind::StyleProfileKw => "'style_profile'",
        SyntaxKind::GeneratorProfileKw => "'generator_profile'",
        SyntaxKind::QualityProfileKw => "'quality_profile'",
        SyntaxKind::ProvenancePolicyKw => "'provenance_policy'",
        SyntaxKind::Ident => "identifier",
        SyntaxKind::StringLiteral
        | SyntaxKind::StringStart
        | SyntaxKind::StringPart
        | SyntaxKind::StringEnd => "string",
        SyntaxKind::IntNumber => "integer",
        SyntaxKind::FloatNumber => "float",
        SyntaxKind::Colon => "':'",
        SyntaxKind::LParen => "'('",
        SyntaxKind::RParen => "')'",
        SyntaxKind::LBracket => "'['",
        SyntaxKind::RBracket => "']'",
        SyntaxKind::LBrace => "'{'",
        SyntaxKind::RBrace => "'}'",
        SyntaxKind::Dot => "'.'",
        SyntaxKind::Range => "'...'",
        SyntaxKind::Comma => "','",
        SyntaxKind::At => "'@'",
        SyntaxKind::QuestionQuestion => "'??'",
        SyntaxKind::Question => "'?'",
        SyntaxKind::Arrow => "'->'",
        SyntaxKind::Equals => "'='",
        SyntaxKind::EqEq => "'=='",
        SyntaxKind::BangEq => "'!='",
        SyntaxKind::Less => "'<'",
        SyntaxKind::LessEq => "'<='",
        SyntaxKind::Greater => "'>'",
        SyntaxKind::GreaterEq => "'>='",
        SyntaxKind::Plus => "'+'",
        SyntaxKind::Minus => "'-'",
        SyntaxKind::Star => "'*'",
        SyntaxKind::Slash => "'/'",
        SyntaxKind::Percent => "'%'",
        SyntaxKind::PlusEq => "'+='",
        SyntaxKind::MinusEq => "'-='",
        SyntaxKind::StarEq => "'*='",
        SyntaxKind::SlashEq => "'/='",
        SyntaxKind::Ampersand => "'&'",
        SyntaxKind::Pipe => "'|'",
        SyntaxKind::Caret => "'^'",
        SyntaxKind::BitwiseNot => "'~'",
        SyntaxKind::ShiftLeft => "'<<'",
        SyntaxKind::ShiftRight => "'>>'",
        SyntaxKind::Newline => "end of line",
        SyntaxKind::Eof => "end of file",
        _ => "token",
    }
}

fn format_found(kind: SyntaxKind, text: &str) -> String {
    match kind {
        SyntaxKind::Eof => "end of file".to_string(),
        SyntaxKind::Newline => "end of line".to_string(),
        SyntaxKind::Whitespace => kind_label(kind).into(),
        SyntaxKind::Comment | SyntaxKind::DocComment => "comment".to_string(),
        _ => {
            let mut slice = text.replace('\n', "\\n").replace('\r', "\\r");
            if slice.len() > 20 {
                slice.truncate(20);
                slice.push('…');
            }
            if slice.is_empty() {
                kind_label(kind).into()
            } else {
                format!("{} ({})", kind_label(kind), slice)
            }
        }
    }
}

pub struct Marker {
    pos: usize,
    completed: Cell<bool>,
}

impl Marker {
    pub fn new(pos: usize) -> Self {
        Self {
            pos,
            completed: Cell::new(false),
        }
    }

    pub fn complete(self, parser: &mut Parser, kind: SyntaxKind) -> CompletedMarker {
        self.completed.set(true);
        let event = &mut parser.events[self.pos];
        assert_eq!(*event, Event::Placeholder);
        *event = Event::StartNode {
            kind,
            forward_parent: None,
        };
        parser.events.push(Event::FinishNode);
        CompletedMarker::new(self.pos, kind)
    }
}

pub struct CompletedMarker {
    pos: usize,
}

impl CompletedMarker {
    pub fn new(pos: usize, _kind: SyntaxKind) -> Self {
        Self { pos }
    }

    pub fn precede(self, parser: &mut Parser) -> Marker {
        let new_m = parser.start();
        if let Event::StartNode {
            ref mut forward_parent,
            ..
        } = parser.events[self.pos]
        {
            *forward_parent = Some(new_m.pos - self.pos);
        } else {
            unreachable!();
        }
        new_m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_wrapper() {
        use ast::AstNode;
        let text = "class Whale { name: String }";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let class = match root.statements().next().unwrap() {
            ast::Stmt::ClassDef(c) => c,
            _ => panic!("Expected class definition"),
        };
        assert_eq!(class.name().unwrap().text(), "Whale");
    }

    #[test]
    fn test_missing_indented_block_error() {
        let text = "class Whale";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "expected '{' after type declaration")
        );
    }

    #[test]
    fn test_statement_separation() {
        let text = "1 2";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.iter().any(|e| e.message == "expected end of line"));

        let text_ok = "1\n2";
        let (_node, errors_ok) = parse_with_errors(text_ok);
        assert!(errors_ok.is_empty());
    }

    #[test]
    fn test_trailing_line_comment_is_ignored() {
        let text = "fn f() -> Nothing { return }\n\n// trailing comment";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_control_flow_and_assignments() {
        let text = "\
if true { x = 1 }
while x < 5 { x = x + 1 }
for i in [1, 2, 3] { x = x + i }
return x
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_brace_blocks_parse_for_function_and_control_flow() {
        let text = "\
fn f(x: Integer) -> Integer {
    if true {
        x = x + 1
    }
    while x < 5 {
        x = x + 1
    }
    for i in [1, 2, 3] {
        x = x + i
    }
    return x
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_expression_features() {
        let text = "\
foo(1, 2, a=3).bar
[1, 2, 3]
{a: 1, b: 2}
\"hi {name}\"
fire foo()
ready?
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_use_detach_self() {
        let text = "\
use {
    std,
    io
}
from core

match x {
    1, 2 { break }
    3 { continue }
    default { return 1 }
}

detach Whale(name=\"moby\") * 1
self.name
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_optimize_is_treated_as_an_identifier() {
        let text = "optimize";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_removed_has_block_is_rejected() {
        let text = "class Whale { has { name: String } }";
        let (_node, errors) = parse_with_errors(text);
        assert!(!errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_removed_it_alias_is_rejected() {
        let text = "fn f() -> Integer { return it }";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors.iter().any(|e| e
                .message
                .contains("removed receiver aliases `it`/`its`; use `self`")),
            "{errors:?}"
        );
    }

    #[test]
    fn test_removed_its_alias_is_rejected() {
        let text = "fn f() -> Integer { return its.value }";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors.iter().any(|e| e
                .message
                .contains("removed receiver aliases `it`/`its`; use `self`")),
            "{errors:?}"
        );
    }

    #[test]
    fn test_use_brace_block_parse() {
        let text = "\
use {
    std,
    io
}
from core
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_use_brace_block_parse_without_colon() {
        let text = "\
use {
    std,
    io
}
from core
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_case_inline_patterns() {
        let text = "\
match status {
    Status.Processing(id) { return id }
    Status.Pending { return 0 }
    default { return 1 }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_case_or_pattern_pipe_parse() {
        let text = "\
match status {
    Status.Pending | Status.Done { return 1 }
    default { return 0 }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_case_guard_parse() {
        let text = "\
match status {
    Status.Processing(id) if id > 0 { return id }
    default { return 0 }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_case_structural_pattern_parse() {
        let text = "\
match status {
    Status.Processing { worker_id } { return worker_id }
    User { id, name: _ } { return id }
    default { return 0 }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_brace_blocks_parse() {
        let text = "\
match status {
    Status.Processing(id) {
        return id
    }
    Status.Pending { return 0 }
    default { return 1 }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_class_has_block() {
        let text = "\
class Whale {
    name: String
    private {
        age: Number
        fn swim(distance: Number) -> Nothing {
            return 1
        }
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_class_brace_blocks_parse() {
        let text = "\
class Whale {
    name: String
    private {
        age: Number
        fn swim(distance: Number) -> Nothing {
            return 1
        }
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_class_private_field_block_braces_without_colon() {
        let text = "\
class Whale {
    private {
        age: Number
    }
    name: String
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_top_level_private_block_braces_without_colon() {
        let text = "\
private {
    fn secret() -> Integer {
        return 1
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_class_has_brace_block_without_colon() {
        let text = "\
class Whale {
    name: String
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_named_arg_ast() {
        use ast::{Arg, AstNode, Expr, Stmt};
        let text = "foo(a=1)";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let stmt = root.statements().next().unwrap();
        let Stmt::Expr(stmt_expr) = stmt else {
            panic!("Expected expression statement");
        };
        let expr = stmt_expr.expr().unwrap();
        let Expr::Call(call) = expr else {
            panic!("Expected call expression");
        };
        let mut args = call.args();
        match args.next().unwrap() {
            Arg::Named(named) => {
                assert_eq!(named.name().unwrap().text(), "a");
            }
            _ => panic!("Expected named argument"),
        }
    }

    #[test]
    fn test_keyword_named_arg_ast() {
        use ast::{Arg, AstNode, Expr, Stmt};
        let text = "dispatch_compute(kernel=run_kernel, default=run_kernel)";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let stmt = root.statements().next().unwrap();
        let Stmt::Expr(stmt_expr) = stmt else {
            panic!("Expected expression statement");
        };
        let expr = stmt_expr.expr().unwrap();
        let Expr::Call(call) = expr else {
            panic!("Expected call expression");
        };
        let labels: Vec<String> = call
            .args()
            .map(|arg| match arg {
                Arg::Named(named) => named.name().unwrap().text().to_string(),
                _ => panic!("Expected named argument"),
            })
            .collect();
        assert_eq!(labels, vec!["kernel".to_string(), "default".to_string()]);
    }

    #[test]
    fn test_reserved_keyword_named_arg_parses_without_special_cases() {
        let text = "foo(if=1, return=2, default=3)";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_try_expr_ast() {
        use ast::{AstNode, Expr, Stmt};
        let text = "source()?";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let stmt = root.statements().next().unwrap();
        let Stmt::Expr(stmt_expr) = stmt else {
            panic!("Expected expression statement");
        };
        let expr = stmt_expr.expr().unwrap();
        let Expr::Try(try_expr) = expr else {
            panic!("Expected try expression");
        };
        let inner = try_expr.expr().expect("missing try operand");
        assert!(matches!(inner, Expr::Call(_)));
    }

    #[test]
    fn test_try_and_or_else_parse() {
        let text = "\
fn f() -> Result[Integer] {
    return source()? ?? 0
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_augmented_assignment() {
        let text = "\
x += 1
y -= 2
z *= 3
w /= 4
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_types_parsing() {
        let text = "\
fn f(x: Integer) -> Boolean {
    return true
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_value_declaration_parses_without_errors() {
        let text = "\
value Point {
    x: F32
    y: F32
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_value_declaration_attaches_to_ast() {
        use ast::{AstNode, Stmt};

        let text = "\
value Point {
    x: F32
    y: F32
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let value_def = match root.statements().next().unwrap() {
            Stmt::ValueDef(def) => def,
            _ => panic!("Expected value definition"),
        };
        assert_eq!(value_def.name().unwrap().text(), "Point");

        let fields: Vec<_> = value_def.fields().collect();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name().unwrap().text(), "x");
        assert_eq!(fields[0].ty().unwrap().name().unwrap().text(), "F32");
        assert_eq!(fields[1].name().unwrap().text(), "y");
        assert_eq!(fields[1].ty().unwrap().name().unwrap().text(), "F32");
    }

    #[test]
    fn test_fixed_array_type_with_numeric_length_parses_without_errors() {
        let text = "\
fn take_values(values: Array[F32, 4]) -> Nothing {
    return nothing
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_fixed_array_type_retains_numeric_length_in_ast() {
        use ast::{AstNode, Stmt};

        let text = "\
fn take_values(values: Array[F32, 4]) -> Nothing {
    return nothing
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let param = func.params().next().expect("missing param");
        let ty = param.ty().expect("missing param type");
        assert_eq!(ty.name().unwrap().text(), "Array");

        let args = ty.args();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].name().unwrap().text(), "F32");
        assert_eq!(args[1].name().unwrap().text(), "4");
    }

    #[test]
    fn test_assert_approx_parses_without_errors() {
        let text = "\
fn test_vec3_normalize() -> Nothing {
    assert approx value.x ~= 0.6 within 0.0001
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_assert_approx_attaches_rhs_and_tolerance_to_ast() {
        use ast::{AssertMode, AstNode, Expr, Stmt};

        let text = "\
fn test_vec3_normalize() -> Nothing {
    assert approx value.x ~= 0.6 within 0.0001
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let stmt = func.statements().next().expect("missing stmt");
        let assert_stmt = match stmt {
            Stmt::AssertStmt(assert_stmt) => assert_stmt,
            _ => panic!("Expected assert statement"),
        };

        assert!(matches!(assert_stmt.mode(), AssertMode::Approx));
        assert!(matches!(assert_stmt.expr().unwrap(), Expr::Member(_)));
        assert!(matches!(assert_stmt.rhs_expr().unwrap(), Expr::Literal(_)));
        assert!(matches!(
            assert_stmt.tolerance_expr().unwrap(),
            Expr::Literal(_)
        ));
    }

    #[test]
    fn test_vec3_constructor_and_field_access_parse() {
        use ast::{AssertMode, AstNode, Expr, Stmt};

        let text = "\
fn f() -> Nothing {
    value = vec3(1.0, 2.0, 3.0)
    assert approx value.x ~= 1.0 within 0.001
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let mut stmts = func.statements();
        let first = stmts.next().expect("missing assignment");
        assert!(matches!(first, Stmt::VarAssign(_)));
        let second = stmts.next().expect("missing assert");
        let assert_stmt = match second {
            Stmt::AssertStmt(assert_stmt) => assert_stmt,
            _ => panic!("Expected assert statement"),
        };
        assert!(matches!(assert_stmt.mode(), AssertMode::Approx));
        assert!(matches!(assert_stmt.expr().unwrap(), Expr::Member(_)));
    }

    #[test]
    fn test_vec2_constructor_and_math_parse() {
        use ast::{AssertMode, AstNode, Expr, Stmt};

        let text = "\
fn f() -> Nothing {
    value = vec2(3.0, 4.0)
    unit = normalize(value)
    assert approx dot(unit, vec2(0.6, 0.8)) ~= 1.0 within 0.001
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let mut stmts = func.statements();
        assert!(matches!(stmts.next().unwrap(), Stmt::VarAssign(_)));
        assert!(matches!(stmts.next().unwrap(), Stmt::VarAssign(_)));
        let assert_stmt = match stmts.next().unwrap() {
            Stmt::AssertStmt(assert_stmt) => assert_stmt,
            _ => panic!("Expected assert statement"),
        };
        assert!(matches!(assert_stmt.mode(), AssertMode::Approx));
        assert!(matches!(assert_stmt.expr().unwrap(), Expr::Call(_)));
    }

    #[test]
    fn test_vec_math_calls_parse() {
        use ast::{AstNode, Expr, Stmt};

        let text = "\
fn f() -> Nothing {
    value = dot(vec3(1.0, 0.0, 0.0), normalize(vec3(0.0, 2.0, 0.0)))
    value = cross(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0))
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        assert!(func.statements().count() >= 2);
        let assign_stmt = match func.statements().next().unwrap() {
            Stmt::VarAssign(assign) => assign,
            _ => panic!("Expected assignment statement"),
        };
        assert!(matches!(
            assign_stmt.value(),
            Some(Expr::Bin(_)) | Some(Expr::Call(_))
        ));
    }

    #[test]
    fn test_mat4_cols_parse() {
        use ast::{AstNode, Expr, Stmt};

        let text = "\
fn f() -> Nothing {
    m = mat4_cols(
        vec4(1.0, 0.0, 0.0, 0.0),
        vec4(0.0, 1.0, 0.0, 0.0),
        vec4(0.0, 0.0, 1.0, 0.0),
        vec4(0.0, 0.0, 0.0, 1.0)
    )
    return nothing
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let stmt = func.statements().next().expect("missing mat4 statement");
        assert!(matches!(stmt, Stmt::VarAssign(_) | Stmt::ReturnStmt(_)));
        if let Stmt::VarAssign(assign) = stmt {
            assert!(matches!(assign.value(), Some(Expr::Call(_))));
        }
    }

    #[test]
    fn test_for_header_extensions_parse() {
        let text = "\
fn f() -> Nothing {
    xs = [1]
    m = {\"k\": 1}
    for value in xs with index idx {
        nothing
    }
    for key, value in m {
        nothing
    }
}
";
        let (node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        use ast::{AstNode, Stmt};
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let stmts: Vec<_> = func.statements().collect();
        let Stmt::ForStmt(first_for) = &stmts[2] else {
            panic!("Expected first for statement");
        };
        assert_eq!(
            first_for.index_name().map(|t| t.text().to_string()),
            Some("idx".to_string())
        );
    }

    #[test]
    fn test_function_block_contains_statements() {
        use ast::{AstNode, Stmt};
        let text = "\
fn f() -> Integer {
    return 1
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let stmts: Vec<_> = func.statements().collect();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_if_block_contains_statements() {
        use ast::{AstNode, Stmt};
        let text = "\
if true {
    1
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let if_stmt = match root.statements().next().unwrap() {
            Stmt::IfStmt(i) => i,
            _ => panic!("Expected if statement"),
        };
        let block = if_stmt.then_block().expect("missing then block");
        let stmts: Vec<_> = block.statements().collect();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_function_attributes_parse_without_errors() {
        let text = "\
@serial
@allows_env_set
fn test_sample() -> Nothing {
    assert value true == true
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_function_attributes_attach_to_function_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
@serial
@allows_fs_escape
fn test_sample() -> Nothing {
    assert value true == true
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let attrs: Vec<String> = func
            .attributes()
            .filter_map(|attr| attr.name())
            .map(|token| token.text().to_string())
            .collect();
        assert_eq!(
            attrs,
            vec!["serial".to_string(), "allows_fs_escape".to_string()]
        );
    }

    #[test]
    fn test_kernel_function_parses_without_errors() {
        let text = "\
kernel fn shade[T](value: Integer) -> Integer {
    return value
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_kernel_function_attaches_to_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
kernel fn shade[T](value: Integer) -> Integer {
    return value
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let kernel = match root.statements().next().unwrap() {
            Stmt::KernelDef(kernel) => kernel,
            _ => panic!("Expected kernel definition"),
        };
        let type_params: Vec<String> = kernel
            .type_params()
            .map(|token| token.text().to_string())
            .collect();
        assert_eq!(kernel.name().unwrap().text(), "shade");
        assert_eq!(type_params, vec!["T".to_string()]);
        assert_eq!(kernel.params().count(), 1);
    }

    #[test]
    fn test_kernel_function_attributes_attach_to_kernel_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
@serial
kernel fn shade() -> Nothing {
    return nothing
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let kernel = match root.statements().next().unwrap() {
            Stmt::KernelDef(kernel) => kernel,
            _ => panic!("Expected kernel definition"),
        };
        let attrs: Vec<String> = kernel
            .attributes()
            .filter_map(|attr| attr.name())
            .map(|token| token.text().to_string())
            .collect();
        assert_eq!(attrs, vec!["serial".to_string()]);
    }

    #[test]
    fn test_pure_function_parses_without_errors() {
        let text = "\
pure fn normalize_step(value: F32) -> F32 {
    return clamp(value, 0.0, 1.0)
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_pure_function_attaches_to_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
pure fn normalize_step[T](value: F32) -> F32 {
    return clamp(value, 0.0, 1.0)
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(func) => func,
            _ => panic!("Expected function definition"),
        };
        let type_params: Vec<String> = func
            .type_params()
            .map(|token| token.text().to_string())
            .collect();
        assert!(func.is_pure());
        assert_eq!(func.name().unwrap().text(), "normalize_step");
        assert_eq!(type_params, vec!["T".to_string()]);
        assert_eq!(func.params().count(), 1);
    }

    #[test]
    fn test_attributed_pure_function_attaches_to_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
@serial
pure fn normalize_step(value: F32) -> F32 {
    return clamp(value, 0.0, 1.0)
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(func) => func,
            _ => panic!("Expected function definition"),
        };
        let attrs: Vec<String> = func
            .attributes()
            .filter_map(|attr| attr.name())
            .map(|token| token.text().to_string())
            .collect();
        assert!(func.is_pure());
        assert_eq!(func.name().unwrap().text(), "normalize_step");
        assert_eq!(attrs, vec!["serial".to_string()]);
    }

    #[test]
    fn test_field_declarations_parse_and_attach_to_ast() {
        use ast::{AstNode, FieldClass, FieldKind, Stmt};
        let text = "\
field exact distance sphere(p: F32) -> F32 {
    return p
}
field conservative distance shell(center: F32) -> F32 {
    return center
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let fields: Vec<_> = root.statements().collect();
        assert_eq!(fields.len(), 2);

        let first = match &fields[0] {
            Stmt::FieldDecl(field) => field,
            _ => panic!("Expected field declaration"),
        };
        assert_eq!(first.name().unwrap().text(), "sphere");
        assert!(matches!(first.field_class(), Some(FieldClass::Exact)));
        assert!(matches!(first.field_kind(), Some(FieldKind::Distance)));
        assert_eq!(first.params().count(), 1);

        let second = match &fields[1] {
            Stmt::FieldDecl(field) => field,
            _ => panic!("Expected field declaration"),
        };
        assert_eq!(second.name().unwrap().text(), "shell");
        assert!(matches!(
            second.field_class(),
            Some(FieldClass::Conservative)
        ));
        assert!(matches!(second.field_kind(), Some(FieldKind::Distance)));
    }

    #[test]
    fn test_field_support_and_bounds_clauses_parse_as_explicit_ast() {
        use ast::{AstNode, FieldExpr, Stmt};
        let text = "\
field conservative distance scene(p: Vec3) -> F32 {
    support = Support3(bounds = Bounds3(min = vec3(-1.0, -1.0, -1.0), max = vec3(1.0, 1.0, 1.0)))
    bounds = Bounds3(min = vec3(-2.0, -2.0, -2.0), max = vec3(2.0, 2.0, 2.0))
    sphere(radius = 1)
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let field = match root.statements().next() {
            Some(Stmt::FieldDecl(field)) => field,
            _ => panic!("Expected field declaration"),
        };
        assert!(field.support_clause().is_some(), "expected support clause");
        assert!(field.bounds_clause().is_some(), "expected bounds clause");
        let expr = field
            .semantic_expr()
            .expect("expected semantic field expression");
        let FieldExpr::Primitive(primitive) = expr else {
            panic!("expected primitive field expression");
        };
        assert_eq!(primitive.name().unwrap().text(), "sphere");
    }

    #[test]
    fn test_field_boolean_provenance_policy_parse_as_explicit_ast() {
        use ast::{AstNode, FieldExpr, Stmt};
        let text = "\
field conservative distance composed(p: Vec3) -> F32 {
    subtract {
        provenance_policy = right
        intersection {
            provenance_policy = nearest
            union {
                provenance_policy = nearest
                use left_x
                use left_y
            }
            use cap_z
        }
        use notch
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let field = match root.statements().next() {
            Some(Stmt::FieldDecl(field)) => field,
            _ => panic!("Expected field declaration"),
        };
        let subtract = match field
            .semantic_expr()
            .expect("expected subtract field expression")
        {
            FieldExpr::Subtract(subtract) => subtract,
            _ => panic!("expected subtract field expression"),
        };
        assert_eq!(
            subtract
                .provenance_policy()
                .expect("expected subtract provenance policy")
                .text(),
            "right"
        );
        let lhs = subtract.lhs().expect("expected subtract lhs");
        let FieldExpr::Intersection(intersection) = lhs else {
            panic!("expected subtract lhs to be intersection");
        };
        assert_eq!(
            intersection
                .provenance_policy()
                .expect("expected intersection provenance policy")
                .text(),
            "nearest"
        );
        let intersection_items: Vec<_> = intersection.items().collect();
        assert_eq!(intersection_items.len(), 2);
        let FieldExpr::Union(union) = &intersection_items[0] else {
            panic!("expected first intersection item to be union");
        };
        assert_eq!(
            union
                .provenance_policy()
                .expect("expected union provenance policy")
                .text(),
            "nearest"
        );
        assert_eq!(union.items().count(), 2);
    }

    #[test]
    fn test_field_new_primitive_catalog_names_parse_as_explicit_ast() {
        use ast::{AstNode, FieldExpr, Stmt};
        let text = "\
field exact distance rounded_box_field(p: Vec3) -> F32 {
    rounded_box(radius = 1.0)
}
field exact distance ellipsoid_field(p: Vec3) -> F32 {
    ellipsoid(radii = vec3(1.0, 2.0, 3.0))
}
field exact distance cone_field(p: Vec3) -> F32 {
    cone(angle = 0.5)
}
field exact distance capped_cone_field(p: Vec3) -> F32 {
    capped_cone(angle = 0.5, cap = 1.0)
}
field exact distance box_frame_field(p: Vec3) -> F32 {
    box_frame(thickness = 0.1)
}
field exact distance slab_field(p: Vec3) -> F32 {
    slab(thickness = 0.25)
}
field exact distance triangle_prism_field(p: Vec3) -> F32 {
    triangle_prism()
}
field exact distance hex_prism_field(p: Vec3) -> F32 {
    hex_prism()
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let fields: Vec<_> = root.statements().collect();
        let expected = [
            "rounded_box",
            "ellipsoid",
            "cone",
            "capped_cone",
            "box_frame",
            "slab",
            "triangle_prism",
            "hex_prism",
        ];
        assert_eq!(fields.len(), expected.len());
        for (stmt, expected_name) in fields.iter().zip(expected.iter()) {
            let field = match stmt {
                Stmt::FieldDecl(field) => field,
                _ => panic!("expected field declaration"),
            };
            let expr = field
                .semantic_expr()
                .expect("expected semantic field expression");
            let FieldExpr::Primitive(primitive) = expr else {
                panic!("expected primitive field expression");
            };
            assert_eq!(primitive.name().unwrap().text(), *expected_name);
        }
    }

    #[test]
    fn test_field_new_operator_families_parse_as_explicit_ast() {
        use ast::{AstNode, FieldExpr, Stmt};
        let text = "\
field exact distance translate_field(p: Vec3) -> F32 {
    translate = vec3(1, 0, 0) {
        use cube
    }
}
field exact distance rotate_field(p: Vec3) -> F32 {
    rotate = vec3(0, 1, 0) {
        use cube
    }
}
field exact distance uniform_scale_field(p: Vec3) -> F32 {
    uniform_scale = 2.0 {
        use cube
    }
}
field exact distance affine_transform_field(p: Vec3) -> F32 {
    affine_transform = vec3(1, 0, 0) {
        use cube
    }
}
field exact distance warp_field(p: Vec3) -> F32 {
    warp = vec3(0, 0, 1) {
        use cube
    }
}
field exact distance repeat_linear_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2, 0, 0) {
        use cube
    }
}
field exact distance repeat_grid_field(p: Vec3) -> F32 {
    repeat_grid = vec3(2, 2, 2) {
        use cube
    }
}
field exact distance radial_repeat_field(p: Vec3) -> F32 {
    radial_repeat = vec3(0, 1, 0) {
        use cube
    }
}
field exact distance mirror_array_field(p: Vec3) -> F32 {
    mirror_array = vec3(1, 0, 0) {
        use cube
    }
}
field exact distance instance_array_field(p: Vec3) -> F32 {
    instance_array = vec3(0, 1, 0) {
        use cube
    }
}
field exact distance bend_field(p: Vec3) -> F32 {
    bend = vec3(0, 0, 1) {
        use cube
    }
}
field exact distance twist_field(p: Vec3) -> F32 {
    twist = vec3(0, 0, 1) {
        use cube
    }
}
field exact distance taper_field(p: Vec3) -> F32 {
    taper = vec3(0, 0, 1) {
        use cube
    }
}
field exact distance displace_field(p: Vec3) -> F32 {
    displace = vec3(0, 0, 1) {
        use cube
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let fields: Vec<_> = root.statements().collect();
        assert_eq!(fields.len(), 14);

        macro_rules! assert_wrapper_field {
            ($stmt:expr, $variant:ident, $accessor:ident) => {{
                let field = match &$stmt {
                    Stmt::FieldDecl(field) => field.clone(),
                    _ => panic!("expected field declaration"),
                };
                let expr = field
                    .semantic_expr()
                    .expect("expected semantic field expression");
                let FieldExpr::$variant(op) = expr else {
                    panic!("expected {} field expression", stringify!($variant));
                };
                assert!(op.$accessor().is_some());
                let inner = op.body().expect("expected nested field body");
                match inner {
                    FieldExpr::Use(use_expr) => {
                        assert_eq!(use_expr.name().unwrap().text(), "cube");
                    }
                    _ => panic!("expected inner use field expression"),
                }
            }};
        }

        assert_wrapper_field!(fields[0], Translate, translate);
        assert_wrapper_field!(fields[1], Rotate, rotate);
        assert_wrapper_field!(fields[2], UniformScale, uniform_scale);
        assert_wrapper_field!(fields[3], AffineTransform, affine_transform);
        assert_wrapper_field!(fields[4], Warp, warp);
        assert_wrapper_field!(fields[5], RepeatLinear, repeat_linear);
        assert_wrapper_field!(fields[6], RepeatGrid, repeat_grid);
        assert_wrapper_field!(fields[7], RadialRepeat, radial_repeat);
        assert_wrapper_field!(fields[8], MirrorArray, mirror_array);
        assert_wrapper_field!(fields[9], InstanceArray, instance_array);
        assert_wrapper_field!(fields[10], Bend, bend);
        assert_wrapper_field!(fields[11], Twist, twist);
        assert_wrapper_field!(fields[12], Taper, taper);
        assert_wrapper_field!(fields[13], Displace, displace);
    }

    #[test]
    fn test_field_construction_operators_parse_as_explicit_ast() {
        use ast::{AstNode, FieldExpr, Stmt};
        let text = "\
field exact distance extruded_field(p: Vec3) -> F32 {
    extrude = 2.0 {
        circle2(radius = 1.0)
    }
}
field exact distance revolved_field(p: Vec3) -> F32 {
    revolve {
        rect2(half = vec2(1.0, 0.5))
    }
}
field exact distance swept_field(p: Vec3) -> F32 {
    sweep = vec3(0.0, 0.0, 1.0) {
        polyline2(vertices = [vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(1.0, 0.0)])
    }
}
field exact distance lofted_field(p: Vec3) -> F32 {
    loft = 1.5 {
        from circle2(radius = 1.0)
        to rounded_rect2(half = vec2(1.0, 0.5), radius = 0.1)
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let fields: Vec<_> = root.statements().collect();
        assert_eq!(fields.len(), 4);

        let extrude = match &fields[0] {
            Stmt::FieldDecl(field) => match field.semantic_expr().expect("extrude expr") {
                FieldExpr::Extrude(expr) => expr,
                _ => panic!("expected extrude field expression"),
            },
            _ => panic!("expected field declaration"),
        };
        assert!(extrude.height().is_some());
        let extrude_profile = extrude.profile().expect("expected extrude profile");
        assert!(matches!(extrude_profile, ast::ProfileExpr::Primitive(_)));

        let revolve = match &fields[1] {
            Stmt::FieldDecl(field) => match field.semantic_expr().expect("revolve expr") {
                FieldExpr::Revolve(expr) => expr,
                _ => panic!("expected revolve field expression"),
            },
            _ => panic!("expected field declaration"),
        };
        let revolve_profile = revolve.profile().expect("expected revolve profile");
        assert!(matches!(revolve_profile, ast::ProfileExpr::Primitive(_)));

        let sweep = match &fields[2] {
            Stmt::FieldDecl(field) => match field.semantic_expr().expect("sweep expr") {
                FieldExpr::Sweep(expr) => expr,
                _ => panic!("expected sweep field expression"),
            },
            _ => panic!("expected field declaration"),
        };
        assert!(sweep.path().is_some());
        let sweep_profile = sweep.profile().expect("expected sweep profile");
        assert!(matches!(sweep_profile, ast::ProfileExpr::Primitive(_)));

        let loft = match &fields[3] {
            Stmt::FieldDecl(field) => match field.semantic_expr().expect("loft expr") {
                FieldExpr::Loft(expr) => expr,
                _ => panic!("expected loft field expression"),
            },
            _ => panic!("expected field declaration"),
        };
        assert!(loft.height().is_some());
        assert!(matches!(
            loft.from_profile().expect("expected loft from profile"),
            ast::ProfileExpr::Primitive(_)
        ));
        assert!(matches!(
            loft.to_profile().expect("expected loft to profile"),
            ast::ProfileExpr::Primitive(_)
        ));
    }

    #[test]
    fn test_field_smooth_boolean_operators_parse_as_explicit_ast() {
        use ast::{AstNode, FieldExpr, Stmt};
        let text = "\
field exact distance smooth_union_field(p: Vec3) -> F32 {
    smooth_union {
        smoothing = 0.25
        use left
        use right
    }
}
field exact distance smooth_intersection_field(p: Vec3) -> F32 {
    smooth_intersection {
        smoothing = 0.5
        use left
        use right
    }
}
field exact distance smooth_subtract_field(p: Vec3) -> F32 {
    smooth_subtract {
        smoothing = 0.75
        use left
        use right
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let fields: Vec<_> = root.statements().collect();
        assert_eq!(fields.len(), 3);

        let smooth_union = match &fields[0] {
            Stmt::FieldDecl(field) => match field.semantic_expr().expect("smooth union expr") {
                FieldExpr::SmoothUnion(expr) => expr,
                _ => panic!("expected smooth union field expression"),
            },
            _ => panic!("expected field declaration"),
        };
        assert!(smooth_union.smoothing().is_some());
        let items: Vec<_> = smooth_union.items().collect();
        assert_eq!(items.len(), 2);
        match &items[0] {
            FieldExpr::Use(use_expr) => assert_eq!(use_expr.name().unwrap().text(), "left"),
            _ => panic!("expected left field use"),
        }
        match &items[1] {
            FieldExpr::Use(use_expr) => assert_eq!(use_expr.name().unwrap().text(), "right"),
            _ => panic!("expected right field use"),
        }

        let smooth_intersection = match &fields[1] {
            Stmt::FieldDecl(field) => {
                match field.semantic_expr().expect("smooth intersection expr") {
                    FieldExpr::SmoothIntersection(expr) => expr,
                    _ => panic!("expected smooth intersection field expression"),
                }
            }
            _ => panic!("expected field declaration"),
        };
        assert!(smooth_intersection.smoothing().is_some());
        assert_eq!(smooth_intersection.items().count(), 2);

        let smooth_subtract = match &fields[2] {
            Stmt::FieldDecl(field) => match field.semantic_expr().expect("smooth subtract expr") {
                FieldExpr::SmoothSubtract(expr) => expr,
                _ => panic!("expected smooth subtract field expression"),
            },
            _ => panic!("expected field declaration"),
        };
        assert!(smooth_subtract.smoothing().is_some());
        assert!(smooth_subtract.lhs().is_some());
        assert!(smooth_subtract.rhs().is_some());
    }

    #[test]
    fn test_old_wrapper_keywords_no_longer_parse_as_semantic_field_expressions() {
        use ast::{AstNode, Stmt};
        let text = "\
field exact distance legacy_transform(p: Vec3) -> F32 {
    transform = vec3(1, 0, 0) {
        use cube
    }
}
field exact distance legacy_mirror(p: Vec3) -> F32 {
    mirror = vec3(0, 1, 0) {
        use cube
    }
}
field exact distance legacy_repeat(p: Vec3) -> F32 {
    repeat = vec3(2, 0, 0) {
        use cube
    }
}
field exact distance legacy_instance(p: Vec3) -> F32 {
    instance = vec3(0, 0, 1) {
        use cube
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        let _ = errors;

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        for stmt in root.statements() {
            let Stmt::FieldDecl(field) = stmt else {
                panic!("expected field declaration");
            };
            assert!(
                field.semantic_expr().is_none(),
                "expected retired wrapper keyword to stay out of semantic field expressions"
            );
        }
    }

    #[test]
    fn test_legacy_field_body_still_parses_as_statement_block() {
        use ast::{AstNode, Stmt};
        let text = "\
field exact distance sphere(p: F32) -> F32 {
    return p
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let field = match root.statements().next() {
            Some(Stmt::FieldDecl(field)) => field,
            _ => panic!("Expected field declaration"),
        };
        assert!(field.semantic_expr().is_none());
        assert_eq!(field.statements().count(), 1);
    }

    #[test]
    fn test_field_primitive_expression_parses_as_explicit_ast() {
        use ast::{Arg, AstNode, Expr, FieldExpr, Stmt};
        let text = "\
field exact distance sphere(p: Vec3) -> F32 {
    sphere(radius = 1)
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let field = match root.statements().next() {
            Some(Stmt::FieldDecl(field)) => field,
            _ => panic!("Expected field declaration"),
        };
        let expr = field
            .semantic_expr()
            .expect("expected primitive field expression");
        let FieldExpr::Primitive(primitive) = expr else {
            panic!("expected primitive field expression");
        };
        assert_eq!(primitive.name().unwrap().text(), "sphere");
        let args: Vec<_> = primitive.args().collect();
        assert_eq!(args.len(), 1);
        match &args[0] {
            Arg::Named(named) => {
                assert_eq!(named.name().unwrap().text(), "radius");
                let value = named.value().expect("named arg value");
                match value {
                    Expr::Literal(_) => {}
                    _ => panic!("expected radius binding to be a literal expression"),
                }
            }
            _ => panic!("expected radius arg to be named"),
        }
    }

    #[test]
    fn test_shape_leaf_and_composition_parse_as_explicit_ast() {
        use ast::{AstNode, Expr, ShapeExpr, Stmt};
        let text = "\
shape cube_shape {
    field = cube
    material = cube_surface
    payload = Payload(id = 1)
}

shape scene_shape {
    union {
        use cube_shape
        use ground_shape
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let mut statements = root.statements();

        let leaf_shape = match statements.next().expect("first statement") {
            Stmt::ShapeDecl(shape) => shape,
            _ => panic!("expected shape declaration"),
        };
        assert_eq!(leaf_shape.name().unwrap().text(), "cube_shape");
        let leaf = match leaf_shape
            .semantic_expr()
            .expect("expected leaf shape expr")
        {
            ShapeExpr::Leaf(leaf) => leaf,
            _ => panic!("expected leaf shape expr"),
        };
        let field = leaf.field().expect("expected field binding");
        match field.value().expect("field binding value") {
            Expr::Ident(expr) => assert_eq!(expr.name().unwrap().text(), "cube"),
            _ => panic!("expected field binding value to be ident"),
        }
        let material = leaf.material().expect("expected material binding");
        match material.value().expect("material binding value") {
            Expr::Ident(expr) => assert_eq!(expr.name().unwrap().text(), "cube_surface"),
            _ => panic!("expected material binding value to be ident"),
        }
        let payload = leaf.payload().expect("expected payload binding");
        match payload.value().expect("payload binding value") {
            Expr::Call(call) => assert_eq!(call.name().unwrap().text(), "Payload"),
            _ => panic!("expected payload binding value to be call"),
        }

        let scene_shape = match statements.next().expect("second statement") {
            Stmt::ShapeDecl(shape) => shape,
            _ => panic!("expected shape declaration"),
        };
        assert_eq!(scene_shape.name().unwrap().text(), "scene_shape");
        let union = match scene_shape
            .semantic_expr()
            .expect("expected union shape expr")
        {
            ShapeExpr::Union(union) => union,
            _ => panic!("expected union shape expr"),
        };
        let items: Vec<_> = union.items().collect();
        assert_eq!(items.len(), 2);
        match &items[0] {
            ShapeExpr::Use(use_expr) => {
                assert_eq!(use_expr.name().unwrap().text(), "cube_shape");
            }
            _ => panic!("expected first union item to be use"),
        }
        match &items[1] {
            ShapeExpr::Use(use_expr) => {
                assert_eq!(use_expr.name().unwrap().text(), "ground_shape");
            }
            _ => panic!("expected second union item to be use"),
        }
    }

    #[test]
    fn test_shape_boolean_provenance_policy_parse_as_explicit_ast() {
        use ast::{AstNode, ShapeExpr, Stmt};
        let text = "\
shape union_shape {
    union {
        provenance_policy = nearest
        use left_shape
        use right_shape
    }
}

shape intersection_shape {
    intersection {
        provenance_policy = ordered
        use left_shape
        use right_shape
    }
}

shape subtract_shape {
    subtract {
        provenance_policy = right
        use left_shape
        use cutter_shape
    }
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let mut statements = root.statements();

        let union_shape = match statements.next().expect("first statement") {
            Stmt::ShapeDecl(shape) => shape,
            _ => panic!("expected union shape declaration"),
        };
        let union = match union_shape
            .semantic_expr()
            .expect("expected union shape expr")
        {
            ShapeExpr::Union(union) => union,
            _ => panic!("expected union shape expr"),
        };
        assert_eq!(
            union
                .provenance_policy()
                .expect("expected union provenance policy")
                .text(),
            "nearest"
        );
        assert_eq!(union.items().count(), 2);

        let intersection_shape = match statements.next().expect("second statement") {
            Stmt::ShapeDecl(shape) => shape,
            _ => panic!("expected intersection shape declaration"),
        };
        let intersection = match intersection_shape
            .semantic_expr()
            .expect("expected intersection shape expr")
        {
            ShapeExpr::Intersection(intersection) => intersection,
            _ => panic!("expected intersection shape expr"),
        };
        assert_eq!(
            intersection
                .provenance_policy()
                .expect("expected intersection provenance policy")
                .text(),
            "ordered"
        );
        assert_eq!(intersection.items().count(), 2);

        let subtract_shape = match statements.next().expect("third statement") {
            Stmt::ShapeDecl(shape) => shape,
            _ => panic!("expected subtract shape declaration"),
        };
        let subtract = match subtract_shape
            .semantic_expr()
            .expect("expected subtract shape expr")
        {
            ShapeExpr::Subtract(subtract) => subtract,
            _ => panic!("expected subtract shape expr"),
        };
        assert_eq!(
            subtract
                .provenance_policy()
                .expect("expected subtract provenance policy")
                .text(),
            "right"
        );
        assert_eq!(subtract.items().count(), 2);
    }

    #[test]
    fn test_region_domain_and_render_declarations_parse_and_attach_to_ast() {
        use ast::{AstNode, RegionItem, Stmt};
        let text = "\
region Highlands(band: I32, seed: U64) {
    place stairs = StairBand(index = band)
    overlay boss = FoldMother(instance = seed)
    replace landing = BossLanding(seed = seed)
    scatter trees {
        place sapling = Oak(seed = seed)
    }
    if band {
        place fallback = Stone()
    }
}
domain Combat(world: Capture[StaircaseWorld]) {
    geometry_detail = coarse
    material = false
    radiance = false
    media = false
}
render StaircaseView(world: Capture[StaircaseWorld], camera: Camera) {
    domain = Presentation(world = world, camera = camera)
    lights = []
    limits = render_limits(max_steps = 128)
}
view PrimaryView(world: Capture[StaircaseWorld], camera: Camera) {
    domain = Presentation(world = world, camera = camera)
    width = 128
    height = 72
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let statements: Vec<_> = root.statements().collect();
        assert_eq!(statements.len(), 4);

        let region = match &statements[0] {
            Stmt::RegionDecl(region) => region,
            _ => panic!("expected region declaration"),
        };
        assert_eq!(region.name().unwrap().text(), "Highlands");
        assert_eq!(region.params().count(), 2);
        let region_items: Vec<_> = region.items().collect();
        assert_eq!(region_items.len(), 5);
        match &region_items[0] {
            RegionItem::Place(place) => {
                assert_eq!(place.name().unwrap().text(), "stairs");
                assert!(place.value().is_some());
            }
            _ => panic!("expected region place item"),
        }
        match &region_items[3] {
            RegionItem::Scatter(scatter) => {
                assert_eq!(scatter.name().unwrap().text(), "trees");
                let nested: Vec<_> = scatter.items().collect();
                assert_eq!(nested.len(), 1);
                match &nested[0] {
                    RegionItem::Place(place) => {
                        assert_eq!(place.name().unwrap().text(), "sapling");
                    }
                    _ => panic!("expected nested scatter place item"),
                }
            }
            _ => panic!("expected region scatter item"),
        }

        let domain = match &statements[1] {
            Stmt::DomainDecl(domain) => domain,
            _ => panic!("expected domain declaration"),
        };
        assert_eq!(domain.name().unwrap().text(), "Combat");
        assert_eq!(domain.params().count(), 1);
        assert_eq!(domain.statements().count(), 4);

        let render = match &statements[2] {
            Stmt::RenderDecl(render) => render,
            _ => panic!("expected render declaration"),
        };
        assert_eq!(render.name().unwrap().text(), "StaircaseView");
        assert_eq!(render.params().count(), 2);
        assert_eq!(render.statements().count(), 3);

        let view = match &statements[3] {
            Stmt::ViewDecl(view) => view,
            _ => panic!("expected view declaration"),
        };
        assert_eq!(view.name().unwrap().text(), "PrimaryView");
        assert_eq!(view.params().count(), 2);
        assert_eq!(view.statements().count(), 3);
    }

    #[test]
    fn test_region_domain_and_render_signature_errors_are_reported_by_the_parser() {
        let region_text = "region (band: I32) {}\n";
        let (_node, region_errors) = parse_with_errors(region_text);
        assert!(
            region_errors
                .iter()
                .any(|error| error.message == "expected region name after 'region'"),
            "expected missing region name parse error, got: {region_errors:?}"
        );

        let domain_text = "domain Combat {}\n";
        let (_node, domain_errors) = parse_with_errors(domain_text);
        assert!(
            domain_errors
                .iter()
                .any(|error| error.message == "expected '(' after domain name"),
            "expected missing domain parameter-list parse error, got: {domain_errors:?}"
        );

        let render_text = "render (world: Capture[StaircaseWorld]) {}\n";
        let (_node, render_errors) = parse_with_errors(render_text);
        assert!(
            render_errors
                .iter()
                .any(|error| error.message == "expected render name after 'render'"),
            "expected missing render name parse error, got: {render_errors:?}"
        );

        let view_text = "view (world: Capture[StaircaseWorld]) {}\n";
        let (_node, view_errors) = parse_with_errors(view_text);
        assert!(
            view_errors
                .iter()
                .any(|error| error.message == "expected view name after 'view'"),
            "expected missing view name parse error, got: {view_errors:?}"
        );
    }

    #[test]
    fn test_material_declarations_parse_and_attach_to_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
material surface(hit: Hit3) -> Surface {
    return hit
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let materials: Vec<_> = root.statements().collect();
        assert_eq!(materials.len(), 1);

        let material = match &materials[0] {
            Stmt::MaterialDecl(material) => material,
            _ => panic!("Expected material declaration"),
        };
        assert_eq!(material.name().unwrap().text(), "surface");
        assert_eq!(material.params().count(), 1);
        assert_eq!(
            material.ret_type().unwrap().name().unwrap().text(),
            "Surface"
        );
    }

    #[test]
    fn test_regular_function_still_attaches_to_func_ast_after_kernel_changes() {
        use ast::{AstNode, Stmt};
        let text = "\
fn helper() -> Integer {
    return 1
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(func) => func,
            _ => panic!("Expected function definition"),
        };
        assert_eq!(func.name().unwrap().text(), "helper");
    }

    #[test]
    fn test_system_definition_still_parses_with_metadata_after_kernel_changes() {
        use ast::{AstNode, Stmt};
        let text = "\
system tick[stage=fixed, reads=[Clock], writes=[FrameClock]]() -> Nothing {
    return nothing
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        match root.statements().next().unwrap() {
            Stmt::SystemDef(system) => assert_eq!(system.name().unwrap().text(), "tick"),
            _ => panic!("Expected system definition"),
        }
    }

    #[test]
    fn test_game_root_and_command_definition_parse() {
        use ast::AstNode;
        use ast::Stmt;
        let text = "\
command MoveForward {
    strength: Integer
}
game TraversalGame {
    fixed_tick = 120
    space = TraversalWorld
    main_view = player_view
    startup = initialize_game
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");

        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let statements = root.statements().collect::<Vec<_>>();
        assert!(
            statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::CommandDef(_)))
        );
        assert!(
            statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::GameDef(_)))
        );
    }

    #[test]
    fn test_shader_related_attributes_parse_as_ordinary_names() {
        use ast::{AstNode, Stmt};
        let text = "\
@shader
@pipeline
@pass
fn test_sample() -> Nothing {
    assert value true == true
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let func = match root.statements().next().unwrap() {
            Stmt::FuncDef(f) => f,
            _ => panic!("Expected function definition"),
        };
        let attrs: Vec<String> = func
            .attributes()
            .filter_map(|attr| attr.name())
            .map(|token| token.text().to_string())
            .collect();
        assert_eq!(
            attrs,
            vec![
                "shader".to_string(),
                "pipeline".to_string(),
                "pass".to_string()
            ]
        );
    }
}
