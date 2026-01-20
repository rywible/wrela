use strum_macros::AsRefStr;

#[derive(AsRefStr, Debug)]
pub enum Token {
    // class declaration
    #[strum(serialize = "A")]
    ClassA,

    #[strum(serialize = "Indent")]
    Indent,

    #[strum(serialize = "Dedent")]
    Dedent,

    #[strum(serialize = "End of file")]
    Eof,

    #[strum(serialize = "Newline")]
    Newline,

    #[strum(serialize = "Other Byte")]
    OtherByte(u8),
}
