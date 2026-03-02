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
        SyntaxKind::SceneKw => "'scene'",
        SyntaxKind::ThemeKw => "'theme'",
        SyntaxKind::NodeKw => "'node'",
        SyntaxKind::InterfaceKw => "'interface'",
        SyntaxKind::HasKw => "'has'",
        SyntaxKind::CanKw => "'can'",
        SyntaxKind::FnKw => "'fn'",
        SyntaxKind::SystemKw => "'system'",
        SyntaxKind::ViewKw => "'view'",
        SyntaxKind::MaterialKw => "'material'",
        SyntaxKind::WidgetKw => "'widget'",
        SyntaxKind::AnimKw => "'anim'",
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
        SyntaxKind::RenderKw => "'render'",
        SyntaxKind::PresetKw => "'preset'",
        SyntaxKind::ProfileKw => "'profile'",
        SyntaxKind::OverridesKw => "'overrides'",
        SyntaxKind::GpuKw => "'gpu'",
        SyntaxKind::AssetsKw => "'assets'",
        SyntaxKind::MmoKw => "'mmo'",
        SyntaxKind::AssetSpecKw => "'asset_spec'",
        SyntaxKind::StyleProfileKw => "'style_profile'",
        SyntaxKind::GeneratorProfileKw => "'generator_profile'",
        SyntaxKind::QualityProfileKw => "'quality_profile'",
        SyntaxKind::ProvenancePolicyKw => "'provenance_policy'",
        SyntaxKind::CharacterSpecKw => "'character_spec'",
        SyntaxKind::RigSpecKw => "'rig_spec'",
        SyntaxKind::AnimSetSpecKw => "'anim_set_spec'",
        SyntaxKind::AudioSpecKw => "'audio_spec'",
        SyntaxKind::VfxSpecKw => "'vfx_spec'",
        SyntaxKind::UiSpecKw => "'ui_spec'",
        SyntaxKind::WorldRecipeKw => "'world_recipe'",
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
    fn test_removed_optimize_is_rejected() {
        let text = "optimize balance { x = 1 }";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("removed keyword `optimize`")),
            "{errors:?}"
        );
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
    fn test_render_contract_and_gpu_function_parse_without_errors() {
        let text = "\
render UiLane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags ui, frame
}
gpu fn shade() -> String {
    return \"wgsl\"
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
    fn test_render_contract_attach_to_ast_with_v5_clauses() {
        use ast::{AstNode, Stmt};
        let text = "\
render SpriteLane {
    resources \"ui_assets\"
    temporal history
    quality tier balanced
    budget tags render, \"latency-critical\"
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let render = match root.statements().next().unwrap() {
            Stmt::RenderDef(r) => r,
            _ => panic!("Expected render contract"),
        };
        assert_eq!(render.name().unwrap().text(), "SpriteLane");
        assert_eq!(
            render
                .resources_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().trim_matches('"').to_string()),
            Some("ui_assets".to_string())
        );
        assert_eq!(
            render
                .temporal_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("history".to_string())
        );
        assert_eq!(
            render
                .quality_tier_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("balanced".to_string())
        );
        let budget_tags = render
            .budget_tags_clause()
            .expect("render budget tags clause missing");
        let levels: Vec<_> = budget_tags
            .tags()
            .map(|token| token.text().trim_matches('"').to_string())
            .collect();
        assert_eq!(
            levels,
            vec!["render".to_string(), "latency-critical".to_string()]
        );
    }

    #[test]
    fn test_render_contract_unknown_clause_recovers_without_hanging() {
        let text = "\
render UiLane {
    resources UiAssets
    temporal stable
    quality tier medium
    bogus nope
    budget tags ui
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("expected render v5 clause")),
            "expected render clause recovery diagnostic, got {errors:?}"
        );
    }

    #[test]
    fn test_render_contract_shader_clauses_preserve_source_order_in_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
render UiLane {
    resources UiAssets
    temporal stable
    quality tier medium
    shader generated
    shader material UiMaterial
    shader gpu ui_lane_shader
    budget tags ui
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let render = match root.statements().next().unwrap() {
            Stmt::RenderDef(r) => r,
            _ => panic!("Expected render contract"),
        };
        let shader_clauses: Vec<(String, Option<String>)> = render
            .shader_clauses()
            .map(|clause| {
                (
                    clause.mode().expect("shader mode token").text().to_string(),
                    clause.symbol().map(|token| token.text().to_string()),
                )
            })
            .collect();
        assert_eq!(
            shader_clauses,
            vec![
                ("generated".to_string(), None),
                ("material".to_string(), Some("UiMaterial".to_string())),
                ("gpu".to_string(), Some("ui_lane_shader".to_string())),
            ]
        );
    }

    #[test]
    fn test_render_contract_invalid_shader_mode_recovery_reports_no_boundary_noise() {
        let text = "\
render UiLane {
    resources UiAssets
    temporal stable
    quality tier medium
    shader unknown_mode
    budget tags ui
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors.iter().any(|error| {
                error
                    .message
                    .contains("expected shader mode (`generated`, `material <Name>`, or `gpu <FunctionName>`)")
            }),
            "expected invalid shader mode diagnostic, got {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| error.message.contains("expected end of line")),
            "invalid shader recovery should not emit statement boundary noise: {errors:?}"
        );
    }

    #[test]
    fn test_render_contract_missing_required_v5_clauses_reports_migration_diagnostics() {
        let text = "\
render UiLane {
    resources UiAssets
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("render v5 contract is missing required `temporal <mode>` clause")
        }));
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("render v5 contract is missing required `quality tier <tier>` clause")
        }));
        assert!(errors.iter().any(|error| {
            error.message.contains(
                "render v5 contract is missing required `budget tags <tag>[, <tag>...]` clause",
            )
        }));
    }

    #[test]
    fn test_render_contract_legacy_clauses_emit_migration_diagnostics() {
        let text = "\
render UiLane {
    preset site_2d_ui
    profile ui
    target UiNode
    shader generated
    overrides legacy
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("legacy render clause `preset` was removed in v5")
        }));
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("legacy render clause `profile` was removed in v5")
        }));
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("legacy render clause `target` was removed in v5")
        }));
        assert!(!errors.iter().any(|error| {
            error
                .message
                .contains("legacy render clause `shader` was removed in v5")
        }));
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("legacy render clause `overrides` was removed in v5")
        }));
    }

    #[test]
    fn test_assets_and_mmo_declarations_parse_without_errors() {
        let text = "\
assets UiAssets {
    manifest web_manifest
    streaming chunked
}
mmo GlobalShard {
    gateway edge_gateway
    zone us_east_zone
    world earth_world
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_assets_and_mmo_declarations_attach_to_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
assets UiAssets {
    manifest \"web_manifest\"
    streaming chunked
}
mmo GlobalShard {
    gateway edge_gateway
    zone us_east_zone
    world earth_world
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let stmts: Vec<_> = root.statements().collect();
        let Stmt::AssetsDef(assets) = &stmts[0] else {
            panic!("expected assets declaration");
        };
        assert_eq!(assets.name().unwrap().text(), "UiAssets");
        assert_eq!(
            assets
                .manifest_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().trim_matches('"').to_string()),
            Some("web_manifest".to_string())
        );
        assert_eq!(
            assets
                .streaming_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("chunked".to_string())
        );

        let Stmt::MmoDef(mmo) = &stmts[1] else {
            panic!("expected mmo declaration");
        };
        assert_eq!(mmo.name().unwrap().text(), "GlobalShard");
        assert_eq!(
            mmo.gateway_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("edge_gateway".to_string())
        );
        assert_eq!(
            mmo.zone_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("us_east_zone".to_string())
        );
        assert_eq!(
            mmo.world_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("earth_world".to_string())
        );
    }

    #[test]
    fn test_assets_and_mmo_missing_required_clauses_report_parse_diagnostics() {
        let text = "\
assets UiAssets {
    streaming chunked
}
mmo GlobalShard {
    gateway edge_gateway
    zone us_east_zone
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors.iter().any(|error| error.message
                == "assets declaration is missing required `manifest <id>` clause"),
            "expected assets missing manifest diagnostic, got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.message
                    == "mmo declaration is missing required `world <id>` clause"),
            "expected mmo missing world diagnostic, got {errors:?}"
        );
    }

    #[test]
    fn test_asset_factory_declarations_parse_without_errors() {
        let text = "\
asset_spec Assets {
    id assets_v1
    profile realtime
}
style_profile Style {
    id style_v2
}
generator_profile Generator {
    id generator_v1
    profile fast
}
quality_profile Quality {
    id quality_v1
}
provenance_policy Provenance {
    id provenance_v1
    profile strict
}
character_spec Character {
    id character_v1
}
rig_spec Rig {
    id rig_v1
}
anim_set_spec AnimSet {
    id anims_v1
    profile gameplay
}
audio_spec Audio {
    id audio_v1
}
vfx_spec Vfx {
    id vfx_v1
}
ui_spec Ui {
    id ui_v1
    profile accessibility
}
world_recipe World {
    id world_v1
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_asset_factory_declarations_attach_to_ast() {
        use ast::{AstNode, Stmt};

        let text = "\
asset_spec Assets { id assets_v1 profile realtime }
style_profile Style { id style_v2 }
generator_profile Generator { id generator_v1 profile fast }
quality_profile Quality { id quality_v1 }
provenance_policy Provenance { id provenance_v1 profile strict }
character_spec Character { id character_v1 }
rig_spec Rig { id rig_v1 }
anim_set_spec AnimSet { id anims_v1 profile gameplay }
audio_spec Audio { id audio_v1 }
vfx_spec Vfx { id vfx_v1 }
ui_spec Ui { id ui_v1 profile accessibility }
world_recipe World { id world_v1 }
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let stmts: Vec<_> = root.statements().collect();

        let Stmt::AssetSpecDef(asset_spec) = &stmts[0] else {
            panic!("expected asset_spec declaration");
        };
        assert_eq!(asset_spec.name().unwrap().text(), "Assets");
        assert_eq!(
            asset_spec
                .id_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("assets_v1".to_string())
        );
        assert_eq!(
            asset_spec
                .profile_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("realtime".to_string())
        );

        let Stmt::StyleProfileDef(style_profile) = &stmts[1] else {
            panic!("expected style_profile declaration");
        };
        assert_eq!(
            style_profile
                .id_clause()
                .and_then(|clause| clause.value())
                .map(|token| token.text().to_string()),
            Some("style_v2".to_string())
        );
        assert!(style_profile.profile_clause().is_none());

        assert!(matches!(stmts[2], Stmt::GeneratorProfileDef(_)));
        assert!(matches!(stmts[3], Stmt::QualityProfileDef(_)));
        assert!(matches!(stmts[4], Stmt::ProvenancePolicyDef(_)));
        assert!(matches!(stmts[5], Stmt::CharacterSpecDef(_)));
        assert!(matches!(stmts[6], Stmt::RigSpecDef(_)));
        assert!(matches!(stmts[7], Stmt::AnimSetSpecDef(_)));
        assert!(matches!(stmts[8], Stmt::AudioSpecDef(_)));
        assert!(matches!(stmts[9], Stmt::VfxSpecDef(_)));
        assert!(matches!(stmts[10], Stmt::UiSpecDef(_)));
        assert!(matches!(stmts[11], Stmt::WorldRecipeDef(_)));
    }

    #[test]
    fn test_asset_factory_missing_id_clauses_report_parse_diagnostics() {
        let text = "\
asset_spec Assets { profile realtime }
style_profile Style { profile detail }
generator_profile Generator { profile fast }
quality_profile Quality { profile high }
provenance_policy Provenance { profile strict }
character_spec Character { profile hero }
rig_spec Rig { profile biped }
anim_set_spec AnimSet { profile gameplay }
audio_spec Audio { profile surround }
vfx_spec Vfx { profile cinematic }
ui_spec Ui { profile accessibility }
world_recipe World { profile open_world }
";
        let (_node, errors) = parse_with_errors(text);
        let expected = [
            "asset_spec declaration is missing required `id <value>` clause",
            "style_profile declaration is missing required `id <value>` clause",
            "generator_profile declaration is missing required `id <value>` clause",
            "quality_profile declaration is missing required `id <value>` clause",
            "provenance_policy declaration is missing required `id <value>` clause",
            "character_spec declaration is missing required `id <value>` clause",
            "rig_spec declaration is missing required `id <value>` clause",
            "anim_set_spec declaration is missing required `id <value>` clause",
            "audio_spec declaration is missing required `id <value>` clause",
            "vfx_spec declaration is missing required `id <value>` clause",
            "ui_spec declaration is missing required `id <value>` clause",
            "world_recipe declaration is missing required `id <value>` clause",
        ];
        for message in expected {
            assert!(
                errors.iter().any(|error| error.message == message),
                "expected diagnostic `{message}`, got {errors:?}"
            );
        }
    }

    #[test]
    fn test_gpu_fn_attach_to_ast() {
        use ast::{AstNode, Stmt};
        let text = "\
gpu fn sprite_shader(v: Integer) -> String {
    return \"wgsl\"
}
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let gpu = match root.statements().next().unwrap() {
            Stmt::GpuFuncDef(f) => f,
            _ => panic!("Expected gpu function definition"),
        };
        assert_eq!(gpu.name().unwrap().text(), "sprite_shader");
        let params: Vec<_> = gpu.params().collect();
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_legacy_render_annotations_are_rejected_with_canonical_diagnostics() {
        let text = "\
@shader(stage=vertex, entry=\"vs_main\")
@pipeline(name=\"sprite-main\", shader=main_shader)
@pass(name=opaque, pipeline=\"sprite-main\")
fn frame() -> Nothing {
    return
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.iter().any(|e| {
            e.message
                == "legacy render annotation `@shader` was removed; use `render <Name> { resources <AssetsDeclaration> temporal <mode> quality tier <tier> budget tags <tag>[, <tag>...] }` and `gpu fn`"
        }));
        assert!(errors.iter().any(|e| {
            e.message
                == "legacy render annotation `@pipeline` was removed; use `render <Name> { resources <AssetsDeclaration> temporal <mode> quality tier <tier> budget tags <tag>[, <tag>...] }` and `gpu fn`"
        }));
        assert!(errors.iter().any(|e| {
            e.message
                == "legacy render annotation `@pass` was removed; use `render <Name> { resources <AssetsDeclaration> temporal <mode> quality tier <tier> budget tags <tag>[, <tag>...] }` and `gpu fn`"
        }));
    }

    #[test]
    fn test_top_level_legacy_check_reports_migration_without_cascade() {
        let text = "\
check ready() -> Boolean
fn ok() -> Integer {
    return 1
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.iter().any(|e| {
            e.message == "removed keyword `check`; declare `fn ... -> Boolean` instead"
        }));
        assert!(
            !errors.iter().any(|e| e.message == "expected end of line"),
            "{errors:?}"
        );
    }

    #[test]
    fn test_legacy_or_else_is_rejected() {
        let text = "\
fn f() -> Integer {
    return source() or_else 0
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(!errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_legacy_resolve_operator_is_rejected() {
        let text = "\
fn f(flag: StoredBoolean) -> Boolean {
    return resolve flag
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(!errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_derives_clause_is_rejected() {
        let text = "\
class User derives Eq {
    id: Integer
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.iter().any(|e| {
            e.message == "derive traits were removed; semantics are structural by default"
        }));
    }

    #[test]
    fn test_top_level_derives_keyword_is_rejected() {
        let text = "\
derives next_age() -> Integer {
    return 1
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.iter().any(|e| {
            e.message
                == "removed keyword `derives`; use `fn` for methods and intrinsic structural semantics"
        }));
    }

    #[test]
    fn test_removed_component_scene_widget_heads_are_rejected() {
        let text = "\
component Position { x: Integer }
scene MainScene { enabled: Boolean }
widget button() -> Nothing { return }
";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("removed keyword `component`")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("removed keyword `scene`")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("removed keyword `widget`")),
            "{errors:?}"
        );
    }

    #[test]
    fn test_node_profile_clause_is_required() {
        let text = "\
node Position {
    x: Integer
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "node declarations require `profile ui|world|canvas`"),
            "{errors:?}"
        );
    }

    #[test]
    fn test_node_profile_value_is_restricted() {
        let text = "node Position profile wrong { x: Integer }\n";
        let (_node, errors) = parse_with_errors(text);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "node profile must be one of `ui`, `world`, or `canvas`"),
            "{errors:?}"
        );
    }

    #[test]
    fn test_lane_declarations_parse_without_errors() {
        let text = "\
node PositionNode profile world {
    x: Integer
}
resource Inventory {
    size: Integer
}
event Spawned {
    id: Integer
}
node MainSceneNode profile canvas {
    enabled: Boolean
}
theme LightTheme {
    contrast: Integer
}
system tick[stage=update, reads=[PositionNode], writes=[PositionNode]](delta: Integer) -> Nothing {
    return
}
view hud() -> Nothing {
    return
}
material button_node {
    surface_model pbr
    preset ui
    textures albedo ui_button_albedo
    params roughness 0.6
    features receive_shadows true
    semantics physics_surface ui
    semantics friction 0.4
    render alpha blend
    render double_sided false
    render receives_decals true
}
anim pulse() -> Nothing {
    return
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_lane_declarations_map_to_dedicated_stmt_variants() {
        use ast::{AstNode, Stmt};

        let text = "\
node PositionNode profile world { x: Integer }
resource Inventory { size: Integer }
event Spawned { id: Integer }
node MainSceneNode profile ui { enabled: Boolean }
theme LightTheme { contrast: Integer }
system tick[stage=update, reads=[PositionNode], writes=[PositionNode]]() -> Nothing { return }
view hud() -> Nothing { return }
material button_node {
    surface_model pbr
    preset ui
}
anim pulse() -> Nothing { return }
";
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let stmts: Vec<_> = root.statements().collect();
        assert!(matches!(stmts[0], Stmt::NodeDef(_)));
        assert!(matches!(stmts[1], Stmt::ResourceDef(_)));
        assert!(matches!(stmts[2], Stmt::EventDef(_)));
        assert!(matches!(stmts[3], Stmt::NodeDef(_)));
        assert!(matches!(stmts[4], Stmt::ThemeDef(_)));
        let Stmt::SystemDef(system) = &stmts[5] else {
            panic!("expected system definition");
        };
        assert!(
            system
                .syntax()
                .children_with_tokens()
                .filter_map(|it| it.into_token())
                .any(|tok| tok.kind() == SyntaxKind::LBracket),
            "expected system metadata bracket",
        );
        assert!(matches!(stmts[6], Stmt::ViewDef(_)));
        assert!(matches!(stmts[7], Stmt::MaterialDef(_)));
        assert!(matches!(stmts[8], Stmt::AnimDef(_)));
    }

    #[test]
    fn test_legacy_material_function_form_reports_migration_error() {
        let text = "\
material button_node() -> Nothing {
    return
}
";
        let (_node, errors) = parse_with_errors(text);
        assert!(errors.iter().any(|e| {
            e.message
                == "legacy material function declarations were removed; use `material <Name> { surface_model <id> ... }`"
        }));
    }

    #[test]
    fn test_material_semantics_clause_parses() {
        use ast::{AstNode, Stmt};

        let text = r#"
material TreeBark {
    surface_model pbr
    semantics physics_surface wood
    semantics friction 0.7
}
"#;
        let node = parse(text);
        let root = ast::Root::cast(node).expect("root");
        let material = root
            .statements()
            .find_map(|stmt| match stmt {
                Stmt::MaterialDef(material) => Some(material),
                _ => None,
            })
            .expect("material definition");
        let semantics: Vec<_> = material.semantics_clauses().collect();
        assert_eq!(semantics.len(), 2);
        assert_eq!(
            semantics[0].name().map(|tok| tok.text().to_string()),
            Some("physics_surface".to_string())
        );
        assert_eq!(
            semantics[0].value().map(|tok| tok.text().to_string()),
            Some("wood".to_string())
        );
        assert_eq!(
            semantics[1].name().map(|tok| tok.text().to_string()),
            Some("friction".to_string())
        );
        assert_eq!(
            semantics[1].value().map(|tok| tok.text().to_string()),
            Some("0.7".to_string())
        );
    }
}
