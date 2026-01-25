use miette::SourceSpan;
use smol_str::SmolStr;
use std::fmt;
use strum_macros::{AsRefStr, EnumIter};

pub type SpannedToken = (Token, SourceSpan);

#[derive(AsRefStr, EnumIter, Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    #[strum(serialize = "A")]
    Class,
    #[strum(serialize = "An")]
    An,
    #[strum(serialize = "has")]
    Has,
    #[strum(serialize = "can")]
    Can,
    #[strum(serialize = "to")]
    To,
    #[strum(serialize = "if")]
    If,
    #[strum(serialize = "else")]
    Else,
    #[strum(serialize = "while")]
    While,
    #[strum(serialize = "for")]
    For,
    #[strum(serialize = "in")]
    In,
    #[strum(serialize = "return")]
    Return,
    #[strum(serialize = "break")]
    Break,
    #[strum(serialize = "continue")]
    Continue,
    #[strum(serialize = "match")]
    Match,
    #[strum(serialize = "otherwise")]
    Otherwise,
    #[strum(serialize = "err")]
    Err,
    #[strum(serialize = "crash")]
    Crash,
    #[strum(serialize = "true")]
    True,
    #[strum(serialize = "false")]
    False,
    #[strum(serialize = "nothing")]
    Nothing,
    #[strum(serialize = "and")]
    And,
    #[strum(serialize = "or")]
    Or,
    #[strum(serialize = "not")]
    Not,
    #[strum(serialize = "await")]
    Await,
    #[strum(serialize = "spawn")]
    Spawn,
    #[strum(serialize = "fire")]
    Fire,
    #[strum(serialize = "use")]
    Use,
    #[strum(serialize = "from")]
    From,
    #[strum(serialize = "public")]
    Public,
    #[strum(serialize = "private")]
    Private,
    #[strum(serialize = "its")]
    Its,
    #[strum(serialize = "it")]
    It,
    #[strum(serialize = "changing")]
    Changing,

    // Literals
    #[strum(serialize = "Identifier")]
    Identifier(SmolStr),
    #[strum(serialize = "String")]
    StringLiteral(SmolStr),
    #[strum(serialize = "StringStart")]
    StringStart(SmolStr),
    #[strum(serialize = "StringPart")]
    StringPart(SmolStr),
    #[strum(serialize = "StringEnd")]
    StringEnd(SmolStr),
    #[strum(serialize = "Integer")]
    Integer(i64, SmolStr), // (Value, RawText)
    #[strum(serialize = "Float")]
    Float(f64, SmolStr), // (Value, RawText)

    // Symbols
    #[strum(serialize = ":")]
    Colon,
    #[strum(serialize = "(")]
    LParen,
    #[strum(serialize = ")")]
    RParen,
    #[strum(serialize = "[")]
    LBracket,
    #[strum(serialize = "]")]
    RBracket,
    #[strum(serialize = "{")]
    LBrace,
    #[strum(serialize = "}")]
    RBrace,
    #[strum(serialize = ".")]
    Dot,
    #[strum(serialize = "...")]
    Range,
    #[strum(serialize = ",")]
    Comma,
    #[strum(serialize = "@")]
    At,
    #[strum(serialize = "->")]
    Arrow,

    // Operators
    #[strum(serialize = "=")]
    Equals,
    #[strum(serialize = "==")]
    EqEq,
    #[strum(serialize = "!=")]
    BangEq,
    #[strum(serialize = "<")]
    Less,
    #[strum(serialize = "<=")]
    LessEq,
    #[strum(serialize = ">")]
    Greater,
    #[strum(serialize = ">=")]
    GreaterEq,
    #[strum(serialize = "+")]
    Plus,
    #[strum(serialize = "-")]
    Minus,
    #[strum(serialize = "*")]
    Star,
    #[strum(serialize = "/")]
    Slash,
    #[strum(serialize = "%")]
    Percent,

    // Augmented Assignment
    #[strum(serialize = "+=")]
    PlusEq,
    #[strum(serialize = "-=")]
    MinusEq,
    #[strum(serialize = "*=")]
    StarEq,
    #[strum(serialize = "/=")]
    SlashEq,

    // Bitwise
    #[strum(serialize = "&")]
    Ampersand,
    #[strum(serialize = "|")]
    Pipe,
    #[strum(serialize = "^")]
    Caret,
    #[strum(serialize = "~")]
    BitwiseNot,
    #[strum(serialize = "<<")]
    ShiftLeft,
    #[strum(serialize = ">>")]
    ShiftRight,

    // Structural / Trivia
    #[strum(serialize = "Indent")]
    Indent,
    #[strum(serialize = "Dedent")]
    Dedent,
    #[strum(serialize = "End of file")]
    Eof,
    #[strum(serialize = "Newline")]
    Newline(SmolStr),
    #[strum(serialize = "Whitespace")]
    Whitespace(SmolStr),
    #[strum(serialize = "Comment")]
    Comment(SmolStr),
    #[strum(serialize = "DocComment")]
    DocComment(SmolStr),
    #[strum(serialize = "InvalidLiteral")]
    InvalidLiteral(SmolStr),
}

impl Token {
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::Class
                | Token::An
                | Token::Has
                | Token::Can
                | Token::To
                | Token::If
                | Token::Else
                | Token::While
                | Token::For
                | Token::In
                | Token::Return
                | Token::Break
                | Token::Continue
                | Token::Match
                | Token::Otherwise
                | Token::Err
                | Token::Crash
                | Token::True
                | Token::False
                | Token::Nothing
                | Token::And
                | Token::Or
                | Token::Not
                | Token::Await
                | Token::Spawn
                | Token::Fire
                | Token::Use
                | Token::From
                | Token::Public
                | Token::Private
                | Token::Its
                | Token::It
                | Token::Changing
        )
    }

    pub fn is_symbol(&self) -> bool {
        matches!(
            self,
            Token::Colon
                | Token::LParen
                | Token::RParen
                | Token::LBracket
                | Token::RBracket
                | Token::LBrace
                | Token::RBrace
                | Token::Dot
                | Token::Range
                | Token::Comma
                | Token::At
                | Token::Arrow
                | Token::Equals
                | Token::EqEq
                | Token::BangEq
                | Token::Less
                | Token::LessEq
                | Token::Greater
                | Token::GreaterEq
                | Token::Plus
                | Token::Minus
                | Token::Star
                | Token::Slash
                | Token::Percent
                | Token::PlusEq
                | Token::MinusEq
                | Token::StarEq
                | Token::SlashEq
                | Token::Ampersand
                | Token::Pipe
                | Token::Caret
                | Token::BitwiseNot
                | Token::ShiftLeft
                | Token::ShiftRight
        )
    }

    pub fn is_trivia(&self) -> bool {
        matches!(
            self,
            Token::Whitespace(_) | Token::Comment(_) | Token::Newline(_) | Token::DocComment(_)
        )
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Identifier(s) => write!(f, "{}", s),
            Token::StringLiteral(s) => write!(f, "\"{}\"", s),
            Token::StringStart(s) => {
                write!(f, "\"")?;
                write!(f, "{}", s)
            }
            Token::StringPart(s) => write!(f, "{}", s),
            Token::StringEnd(s) => {
                write!(f, "{}", s)?;
                write!(f, "\"")
            }
            Token::Integer(_, raw) => write!(f, "{}", raw),
            Token::Float(_, raw) => write!(f, "{}", raw),
            Token::Newline(s) => write!(f, "{}", s),
            Token::Whitespace(s) => write!(f, "{}", s),
            Token::Comment(s) => write!(f, "so:{}", s),
            Token::DocComment(s) => write!(f, "so:{}", s),
            Token::Indent | Token::Dedent | Token::Eof => Ok(()),
            _ => write!(f, "{}", self.as_ref()),
        }
    }
}
