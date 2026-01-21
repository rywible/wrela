use miette::SourceSpan;
use strum_macros::AsRefStr;

pub type SpannedToken = (Token, SourceSpan);

#[derive(AsRefStr, Debug, Clone, PartialEq)]
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
    #[strum(serialize = "changing")]
    Changing,

    // Literals
    #[strum(serialize = "Identifier")]
    Identifier(String),
    #[strum(serialize = "String")]
    StringLiteral(String),
    #[strum(serialize = "StringStart")]
    StringStart(String),
    #[strum(serialize = "StringPart")]
    StringPart(String),
    #[strum(serialize = "StringEnd")]
    StringEnd(String),
    #[strum(serialize = "Integer")]
    Integer(i64),
    #[strum(serialize = "Float")]
    Float(f64),

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

    // Structural
    #[strum(serialize = "Indent")]
    Indent,
    #[strum(serialize = "Dedent")]
    Dedent,
    #[strum(serialize = "End of file")]
    Eof,
    #[strum(serialize = "Newline")]
    Newline,
}
