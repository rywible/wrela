use strum_macros::AsRefStr;

#[derive(AsRefStr, Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    #[strum(serialize = "A")]
    Class,
    #[strum(serialize = "has")]
    Has,
    #[strum(serialize = "can")]
    Can,
    #[strum(serialize = "to")]
    To,

    // Literals
    #[strum(serialize = "Identifier")]
    Identifier(String),
    #[strum(serialize = "String")]
    StringLiteral(String),
    #[strum(serialize = "Number")]
    Number(f64),

    // Symbols
    #[strum(serialize = ":")]
    Colon,
    #[strum(serialize = "(")]
    LParen,
    #[strum(serialize = ")")]
    RParen,
    #[strum(serialize = ".")]
    Dot,
    #[strum(serialize = "=")]
    Equals,
    #[strum(serialize = ",")]
    Comma,

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
