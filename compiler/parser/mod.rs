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
        let call_stmt = match func.statements().next().unwrap() {
            Stmt::Expr(expr) => expr,
            _ => panic!("Expected expression statement"),
        };
        assert!(matches!(call_stmt.expr(), Some(Expr::Bin(_)) | Some(Expr::Call(_))));
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
            vec!["shader".to_string(), "pipeline".to_string(), "pass".to_string()]
        );
    }
}
