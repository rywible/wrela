//! Declarative byte-level spelling of ordinary Wrela tokens.
//!
//! Unicode property classes come from the pinned Unicode 16 data. Longer matches
//! win; priority distinguishes valid spellings from their malformed envelopes.
//! Stateful policies (nested comments, invalid strings, limits and byte errors)
//! live in `lexer` and never serve as a fallback ordinary-token scanner.
use crate::{
    source::{PieceKind, TriviaKind},
    token::TokenKind,
};
use logos::Logos;
// The macro accepts generated literal classes because Logos attributes require
// literals. Only Unicode data is generated; token rules are authored here.
macro_rules! lexical_rules {
    ($xid_start:literal, $xid_continue:literal, $ignorable:literal,
     $admitted_start:literal, $admitted_continue:literal $(,)?) => {
        #[derive(Logos, Debug, Clone, Copy, PartialEq)]
        #[logos(utf8 = false)]
        #[logos(subpattern xid_start = $xid_start)]
        #[logos(subpattern xid_continue = $xid_continue)]
        #[logos(subpattern ignorable = $ignorable)]
        #[logos(subpattern admitted_start = $admitted_start)]
        #[logos(subpattern admitted_continue = $admitted_continue)]
        pub(crate) enum Lexeme {
            #[regex(r###"[ \t]+"###)]
            Whitespace,
            #[regex(r###"\r\n|\r|\n"###)]
            Newline,
            #[regex(r###"(?-u://[^\r\n]*)"###, allow_greedy = true)]
            LineComment,
            #[regex(r###"[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?"###, priority = 3)]
            Number,
            #[regex(r"(?&admitted_start)(?&admitted_continue)*", priority = 3)]
            Ident,
            #[regex(r###"(?-u:"([^"\\\r\n]|\\[nrt"\\])*")"###)]
            String,
            #[regex(
                r"((?&xid_start)|(?&ignorable))((?&xid_continue)|(?&ignorable))*",
                priority = 1
            )]
            InvalidIdentifier,
            #[regex(
                r"[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]*)?((?&xid_continue)|(?&ignorable))*",
                priority = 1
            )]
            InvalidNumber,
            #[token("fn", priority = 4)]
            Fn,
            #[token("record", priority = 4)]
            Record,
            #[token("enum", priority = 4)]
            Enum,
            #[token("where", priority = 4)]
            Where,
            #[token("let", priority = 4)]
            Let,
            #[token("var", priority = 4)]
            Var,
            #[token("mut", priority = 4)]
            Mut,
            #[token("return", priority = 4)]
            Return,
            #[token("if", priority = 4)]
            If,
            #[token("else", priority = 4)]
            Else,
            #[token("match", priority = 4)]
            Match,
            #[token("for", priority = 4)]
            For,
            #[token("every", priority = 4)]
            Every,
            #[token("in", priority = 4)]
            In,
            #[token("true", priority = 4)]
            True,
            #[token("false", priority = 4)]
            False,
            #[token("(")]
            LParen,
            #[token(")")]
            RParen,
            #[token("[")]
            LBracket,
            #[token("]")]
            RBracket,
            #[token("{")]
            LBrace,
            #[token("}")]
            RBrace,
            #[token(",")]
            Comma,
            #[token(":")]
            Colon,
            #[token(";")]
            Semicolon,
            #[token(".")]
            Dot,
            #[token("::")]
            PathSeparator,
            #[token("->")]
            Arrow,
            #[token("=>")]
            FatArrow,
            #[token("?")]
            Question,
            #[token("+")]
            Plus,
            #[token("-")]
            Minus,
            #[token("*")]
            Star,
            #[token("/")]
            Slash,
            #[token("%")]
            Percent,
            #[token("!")]
            Bang,
            #[token("=")]
            Equal,
            #[token("==")]
            EqualEqual,
            #[token("!=")]
            BangEqual,
            #[token("<")]
            Less,
            #[token("<=")]
            LessEqual,
            #[token(">")]
            Greater,
            #[token(">=")]
            GreaterEqual,
            #[token("&&")]
            AndAnd,
            #[token("||")]
            OrOr,
            #[token("/*")]
            BlockCommentStart,
            #[token("\"")]
            StringStart,
        }
    };
}
include!("lexical_unicode.rs");

impl Lexeme {
    pub(crate) fn piece(self) -> Option<PieceKind> {
        Some(match self {
            Self::Whitespace => PieceKind::Trivia(TriviaKind::Whitespace),
            Self::Newline => PieceKind::Trivia(TriviaKind::Newline),
            Self::LineComment => PieceKind::Trivia(TriviaKind::LineComment),
            Self::InvalidIdentifier | Self::InvalidNumber => PieceKind::Invalid,
            Self::BlockCommentStart | Self::StringStart => return None,
            Self::Number => PieceKind::Token(TokenKind::Number),
            Self::Ident => PieceKind::Token(TokenKind::Ident),
            Self::String => PieceKind::Token(TokenKind::String),
            Self::Fn => PieceKind::Token(TokenKind::Fn),
            Self::Record => PieceKind::Token(TokenKind::Record),
            Self::Enum => PieceKind::Token(TokenKind::Enum),
            Self::Where => PieceKind::Token(TokenKind::Where),
            Self::Let => PieceKind::Token(TokenKind::Let),
            Self::Var => PieceKind::Token(TokenKind::Var),
            Self::Mut => PieceKind::Token(TokenKind::Mut),
            Self::Return => PieceKind::Token(TokenKind::Return),
            Self::If => PieceKind::Token(TokenKind::If),
            Self::Else => PieceKind::Token(TokenKind::Else),
            Self::Match => PieceKind::Token(TokenKind::Match),
            Self::For => PieceKind::Token(TokenKind::For),
            Self::Every => PieceKind::Token(TokenKind::Every),
            Self::In => PieceKind::Token(TokenKind::In),
            Self::True => PieceKind::Token(TokenKind::True),
            Self::False => PieceKind::Token(TokenKind::False),
            Self::LParen => PieceKind::Token(TokenKind::LParen),
            Self::RParen => PieceKind::Token(TokenKind::RParen),
            Self::LBracket => PieceKind::Token(TokenKind::LBracket),
            Self::RBracket => PieceKind::Token(TokenKind::RBracket),
            Self::LBrace => PieceKind::Token(TokenKind::LBrace),
            Self::RBrace => PieceKind::Token(TokenKind::RBrace),
            Self::Comma => PieceKind::Token(TokenKind::Comma),
            Self::Colon => PieceKind::Token(TokenKind::Colon),
            Self::Semicolon => PieceKind::Token(TokenKind::Semicolon),
            Self::Dot => PieceKind::Token(TokenKind::Dot),
            Self::PathSeparator => PieceKind::Token(TokenKind::PathSeparator),
            Self::Arrow => PieceKind::Token(TokenKind::Arrow),
            Self::FatArrow => PieceKind::Token(TokenKind::FatArrow),
            Self::Question => PieceKind::Token(TokenKind::Question),
            Self::Plus => PieceKind::Token(TokenKind::Plus),
            Self::Minus => PieceKind::Token(TokenKind::Minus),
            Self::Star => PieceKind::Token(TokenKind::Star),
            Self::Slash => PieceKind::Token(TokenKind::Slash),
            Self::Percent => PieceKind::Token(TokenKind::Percent),
            Self::Bang => PieceKind::Token(TokenKind::Bang),
            Self::Equal => PieceKind::Token(TokenKind::Equal),
            Self::EqualEqual => PieceKind::Token(TokenKind::EqualEqual),
            Self::BangEqual => PieceKind::Token(TokenKind::BangEqual),
            Self::Less => PieceKind::Token(TokenKind::Less),
            Self::LessEqual => PieceKind::Token(TokenKind::LessEqual),
            Self::Greater => PieceKind::Token(TokenKind::Greater),
            Self::GreaterEqual => PieceKind::Token(TokenKind::GreaterEqual),
            Self::AndAnd => PieceKind::Token(TokenKind::AndAnd),
            Self::OrOr => PieceKind::Token(TokenKind::OrOr),
        })
    }
}
pub(crate) fn regular(bytes: &[u8]) -> Option<(Lexeme, usize)> {
    let mut lexer = Lexeme::lexer(bytes);
    let lexeme = lexer.next()?.ok()?;
    let width = lexer.span().end;
    Some((lexeme, width))
}
