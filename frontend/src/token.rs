use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenKind {
    Ident,
    Number,
    String,
    Fn,
    Record,
    Enum,
    Where,
    Let,
    Var,
    Mut,
    Return,
    If,
    Else,
    Match,
    For,
    Every,
    In,
    True,
    False,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semicolon,
    Dot,
    PathSeparator,
    Arrow,
    FatArrow,
    Question,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    Equal,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    GenericOpen,
    GenericClose,
    TypeOpen,
    TypeClose,
    Newline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tok {
    Ident(TokenId),
    Number(TokenId),
    String(TokenId),
    Fn(TokenId),
    Record(TokenId),
    Enum(TokenId),
    Where(TokenId),
    Let(TokenId),
    Var(TokenId),
    Mut(TokenId),
    Return(TokenId),
    If(TokenId),
    Else(TokenId),
    Match(TokenId),
    For(TokenId),
    Every(TokenId),
    In(TokenId),
    True(TokenId),
    False(TokenId),
    LParen(TokenId),
    RParen(TokenId),
    LBracket(TokenId),
    RBracket(TokenId),
    LBrace(TokenId),
    RBrace(TokenId),
    Comma(TokenId),
    Colon(TokenId),
    Semicolon(TokenId),
    Dot(TokenId),
    PathSeparator(TokenId),
    Arrow(TokenId),
    FatArrow(TokenId),
    Question(TokenId),
    Plus(TokenId),
    Minus(TokenId),
    Star(TokenId),
    Slash(TokenId),
    Percent(TokenId),
    Bang(TokenId),
    Equal(TokenId),
    EqualEqual(TokenId),
    BangEqual(TokenId),
    Less(TokenId),
    LessEqual(TokenId),
    Greater(TokenId),
    GreaterEqual(TokenId),
    AndAnd(TokenId),
    OrOr(TokenId),
    GenericOpen(TokenId),
    GenericClose(TokenId),
    TypeOpen(TokenId),
    TypeClose(TokenId),
    Newline(TokenId),
}
impl Tok {
    pub fn id(self) -> TokenId {
        match self {
            Self::Ident(id)
            | Self::Number(id)
            | Self::String(id)
            | Self::Fn(id)
            | Self::Record(id)
            | Self::Enum(id)
            | Self::Where(id)
            | Self::Let(id)
            | Self::Var(id)
            | Self::Mut(id)
            | Self::Return(id)
            | Self::If(id)
            | Self::Else(id)
            | Self::Match(id)
            | Self::For(id)
            | Self::Every(id)
            | Self::In(id)
            | Self::True(id)
            | Self::False(id)
            | Self::LParen(id)
            | Self::RParen(id)
            | Self::LBracket(id)
            | Self::RBracket(id)
            | Self::LBrace(id)
            | Self::RBrace(id)
            | Self::Comma(id)
            | Self::Colon(id)
            | Self::Semicolon(id)
            | Self::Dot(id)
            | Self::PathSeparator(id)
            | Self::Arrow(id)
            | Self::FatArrow(id)
            | Self::Question(id)
            | Self::Plus(id)
            | Self::Minus(id)
            | Self::Star(id)
            | Self::Slash(id)
            | Self::Percent(id)
            | Self::Bang(id)
            | Self::Equal(id)
            | Self::EqualEqual(id)
            | Self::BangEqual(id)
            | Self::Less(id)
            | Self::LessEqual(id)
            | Self::Greater(id)
            | Self::GreaterEqual(id)
            | Self::AndAnd(id)
            | Self::OrOr(id)
            | Self::GenericOpen(id)
            | Self::GenericClose(id)
            | Self::TypeOpen(id)
            | Self::TypeClose(id)
            | Self::Newline(id) => id,
        }
    }
    pub fn kind(self) -> TokenKind {
        match self {
            Self::Ident(_) => TokenKind::Ident,
            Self::Number(_) => TokenKind::Number,
            Self::String(_) => TokenKind::String,
            Self::Fn(_) => TokenKind::Fn,
            Self::Record(_) => TokenKind::Record,
            Self::Enum(_) => TokenKind::Enum,
            Self::Where(_) => TokenKind::Where,
            Self::Let(_) => TokenKind::Let,
            Self::Var(_) => TokenKind::Var,
            Self::Mut(_) => TokenKind::Mut,
            Self::Return(_) => TokenKind::Return,
            Self::If(_) => TokenKind::If,
            Self::Else(_) => TokenKind::Else,
            Self::Match(_) => TokenKind::Match,
            Self::For(_) => TokenKind::For,
            Self::Every(_) => TokenKind::Every,
            Self::In(_) => TokenKind::In,
            Self::True(_) => TokenKind::True,
            Self::False(_) => TokenKind::False,
            Self::LParen(_) => TokenKind::LParen,
            Self::RParen(_) => TokenKind::RParen,
            Self::LBracket(_) => TokenKind::LBracket,
            Self::RBracket(_) => TokenKind::RBracket,
            Self::LBrace(_) => TokenKind::LBrace,
            Self::RBrace(_) => TokenKind::RBrace,
            Self::Comma(_) => TokenKind::Comma,
            Self::Colon(_) => TokenKind::Colon,
            Self::Semicolon(_) => TokenKind::Semicolon,
            Self::Dot(_) => TokenKind::Dot,
            Self::PathSeparator(_) => TokenKind::PathSeparator,
            Self::Arrow(_) => TokenKind::Arrow,
            Self::FatArrow(_) => TokenKind::FatArrow,
            Self::Question(_) => TokenKind::Question,
            Self::Plus(_) => TokenKind::Plus,
            Self::Minus(_) => TokenKind::Minus,
            Self::Star(_) => TokenKind::Star,
            Self::Slash(_) => TokenKind::Slash,
            Self::Percent(_) => TokenKind::Percent,
            Self::Bang(_) => TokenKind::Bang,
            Self::Equal(_) => TokenKind::Equal,
            Self::EqualEqual(_) => TokenKind::EqualEqual,
            Self::BangEqual(_) => TokenKind::BangEqual,
            Self::Less(_) => TokenKind::Less,
            Self::LessEqual(_) => TokenKind::LessEqual,
            Self::Greater(_) => TokenKind::Greater,
            Self::GreaterEqual(_) => TokenKind::GreaterEqual,
            Self::AndAnd(_) => TokenKind::AndAnd,
            Self::OrOr(_) => TokenKind::OrOr,
            Self::GenericOpen(_) => TokenKind::GenericOpen,
            Self::GenericClose(_) => TokenKind::GenericClose,
            Self::TypeOpen(_) => TokenKind::TypeOpen,
            Self::TypeClose(_) => TokenKind::TypeClose,
            Self::Newline(_) => TokenKind::Newline,
        }
    }
    pub fn new(kind: TokenKind, id: TokenId) -> Self {
        match kind {
            TokenKind::Ident => Self::Ident(id),
            TokenKind::Number => Self::Number(id),
            TokenKind::String => Self::String(id),
            TokenKind::Fn => Self::Fn(id),
            TokenKind::Record => Self::Record(id),
            TokenKind::Enum => Self::Enum(id),
            TokenKind::Where => Self::Where(id),
            TokenKind::Let => Self::Let(id),
            TokenKind::Var => Self::Var(id),
            TokenKind::Mut => Self::Mut(id),
            TokenKind::Return => Self::Return(id),
            TokenKind::If => Self::If(id),
            TokenKind::Else => Self::Else(id),
            TokenKind::Match => Self::Match(id),
            TokenKind::For => Self::For(id),
            TokenKind::Every => Self::Every(id),
            TokenKind::In => Self::In(id),
            TokenKind::True => Self::True(id),
            TokenKind::False => Self::False(id),
            TokenKind::LParen => Self::LParen(id),
            TokenKind::RParen => Self::RParen(id),
            TokenKind::LBracket => Self::LBracket(id),
            TokenKind::RBracket => Self::RBracket(id),
            TokenKind::LBrace => Self::LBrace(id),
            TokenKind::RBrace => Self::RBrace(id),
            TokenKind::Comma => Self::Comma(id),
            TokenKind::Colon => Self::Colon(id),
            TokenKind::Semicolon => Self::Semicolon(id),
            TokenKind::Dot => Self::Dot(id),
            TokenKind::PathSeparator => Self::PathSeparator(id),
            TokenKind::Arrow => Self::Arrow(id),
            TokenKind::FatArrow => Self::FatArrow(id),
            TokenKind::Question => Self::Question(id),
            TokenKind::Plus => Self::Plus(id),
            TokenKind::Minus => Self::Minus(id),
            TokenKind::Star => Self::Star(id),
            TokenKind::Slash => Self::Slash(id),
            TokenKind::Percent => Self::Percent(id),
            TokenKind::Bang => Self::Bang(id),
            TokenKind::Equal => Self::Equal(id),
            TokenKind::EqualEqual => Self::EqualEqual(id),
            TokenKind::BangEqual => Self::BangEqual(id),
            TokenKind::Less => Self::Less(id),
            TokenKind::LessEqual => Self::LessEqual(id),
            TokenKind::Greater => Self::Greater(id),
            TokenKind::GreaterEqual => Self::GreaterEqual(id),
            TokenKind::AndAnd => Self::AndAnd(id),
            TokenKind::OrOr => Self::OrOr(id),
            TokenKind::GenericOpen => Self::GenericOpen(id),
            TokenKind::GenericClose => Self::GenericClose(id),
            TokenKind::TypeOpen => Self::TypeOpen(id),
            TokenKind::TypeClose => Self::TypeClose(id),
            TokenKind::Newline => Self::Newline(id),
        }
    }
}
impl std::fmt::Display for Tok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind())
    }
}
