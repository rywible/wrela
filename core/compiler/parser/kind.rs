use crate::lexer::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // Keywords
    ClassKw,     // A
    AnKw,        // An
    HasKw,       // has
    CanKw,       // can
    MustKw,      // must
    DerivesKw,   // derives
    ToKw,        // to
    IfKw,        // if
    ButKw,       // but
    WhileKw,     // while
    ForKw,       // for
    InKw,        // in
    ReturnKw,    // return
    BreakKw,     // break
    ContinueKw,  // continue
    MatchKw,     // match
    OtherwiseKw, // otherwise
    ErrKw,       // error
    CrashKw,     // crash
    TrueKw,      // true
    FalseKw,     // false
    NothingKw,   // nothing
    AndKw,       // and
    OrKw,        // or
    NotKw,       // not
    AwaitKw,     // await
    DetachKw,    // detach
    SpawnKw,     // spawn
    FireKw,      // fire
    OptimizeKw,  // optimize
    AssertKw,    // assert
    UseKw,       // use
    FromKw,      // from
    PrivateKw,   // private
    ItsKw,       // its
    ItKw,        // it
    MutableKw,  // mutable
    IsKw,       // is
    EitherKw,   // either
    DeferKw,    // defer
    IgnoreKw,   // ignore
    CaptureKw,  // capture
    CheckKw,    // check
    ChecksKw,   // checks
    GivenKw,    // given
    RequireKw,  // require
    UnsafeKw,   // unsafe
    ExternKw,   // extern

    // Literals
    Ident,
    StringLiteral,
    StringStart,
    StringPart,
    StringEnd,
    IntNumber,
    FloatNumber,

    // Symbols
    Colon,    // :
    LParen,   // (
    RParen,   // )
    LBracket, // [
    RBracket, // ]
    LBrace,   // {
    RBrace,   // }
    Dot,      // .
    Range,    // ...
    Comma,    // ,
    At,       // @
    Arrow,    // ->

    // Operators
    Equals,    // =
    EqEq,      // ==
    BangEq,    // !=
    Less,      // <
    LessEq,    // <=
    Greater,   // >
    GreaterEq, // >=
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %

    // Augmented Assignment
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,

    // Bitwise
    Ampersand,
    Pipe,
    Caret,
    BitwiseNot,
    ShiftLeft,
    ShiftRight,

    // Structural / Trivia
    Indent,
    Dedent,
    Newline,
    Whitespace,
    Comment,
    DocComment,
    InvalidLiteral,
    Eof,

    // --- Nodes (Composite elements) ---
    Root,
    ClassDef,
    EnumDef,
    EnumVariant,
    FuncDef,
    CheckDef,
    ExternFuncDef,
    MethodDef,
    CheckMethodDef,
    MustMethodDef,
    DeriveDef,
    FieldDef,
    ParamList,
    Param,
    TypeParamList,
    TypeRef,
    TypeArgList,
    IsAClause,
    Block,
    HasBlock,
    LayoutClause,

    // Statements
    StmtExpr,
    VarAssign,
    IfStmt,
    WhileStmt,
    ForStmt,
    ReturnStmt,
    BreakStmt,
    ContinueStmt,
    MatchStmt,
    MatchCase,
    OtherwiseCase,
    UseStmt,
    OptimizeStmt,
    AssertStmt,
    DeferStmt,
    IgnoreResultStmt,
    CaptureStmt,
    RequireStmt,
    UnsafeStmt,
    PrivateBlock,

    // Expressions
    BinExpr,
    PrefixExpr,
    CallExpr,
    TypeApplyExpr,
    MemberExpr,
    LiteralExpr,
    IdentExpr,
    ParenExpr,
    ListExpr,
    MapExpr,
    StringInterp,
    ItsExpr,
    ItExpr,
    GivenExpr,
    NamedArg,
    CrashExpr,
    Pattern,
    PatternArgList,

    Error,
}

impl From<Token> for SyntaxKind {
    fn from(token: Token) -> Self {
        match token {
            Token::Class => SyntaxKind::ClassKw,
            Token::An => SyntaxKind::AnKw,
            Token::Has => SyntaxKind::HasKw,
            Token::Can => SyntaxKind::CanKw,
            Token::Must => SyntaxKind::MustKw,
            Token::Derives => SyntaxKind::DerivesKw,
            Token::To => SyntaxKind::ToKw,
            Token::If => SyntaxKind::IfKw,
            Token::But => SyntaxKind::ButKw,
            Token::While => SyntaxKind::WhileKw,
            Token::For => SyntaxKind::ForKw,
            Token::In => SyntaxKind::InKw,
            Token::Return => SyntaxKind::ReturnKw,
            Token::Break => SyntaxKind::BreakKw,
            Token::Continue => SyntaxKind::ContinueKw,
            Token::Match => SyntaxKind::MatchKw,
            Token::Otherwise => SyntaxKind::OtherwiseKw,
            Token::Err => SyntaxKind::ErrKw,
            Token::Crash => SyntaxKind::CrashKw,
            Token::True => SyntaxKind::TrueKw,
            Token::False => SyntaxKind::FalseKw,
            Token::Nothing => SyntaxKind::NothingKw,
            Token::And => SyntaxKind::AndKw,
            Token::Or => SyntaxKind::OrKw,
            Token::Not => SyntaxKind::NotKw,
            Token::Await => SyntaxKind::AwaitKw,
            Token::Detach => SyntaxKind::DetachKw,
            Token::Spawn => SyntaxKind::SpawnKw,
            Token::Fire => SyntaxKind::FireKw,
            Token::Optimize => SyntaxKind::OptimizeKw,
            Token::Assert => SyntaxKind::AssertKw,
            Token::Use => SyntaxKind::UseKw,
            Token::From => SyntaxKind::FromKw,
            Token::Private => SyntaxKind::PrivateKw,
            Token::Its => SyntaxKind::ItsKw,
            Token::It => SyntaxKind::ItKw,
            Token::Mutable => SyntaxKind::MutableKw,
            Token::Is => SyntaxKind::IsKw,
            Token::Either => SyntaxKind::EitherKw,
            Token::Defer => SyntaxKind::DeferKw,
            Token::Ignore => SyntaxKind::IgnoreKw,
            Token::Capture => SyntaxKind::CaptureKw,
            Token::Check => SyntaxKind::CheckKw,
            Token::Checks => SyntaxKind::ChecksKw,
            Token::Given => SyntaxKind::GivenKw,
            Token::Require => SyntaxKind::RequireKw,
            Token::Unsafe => SyntaxKind::UnsafeKw,
            Token::Extern => SyntaxKind::ExternKw,
            Token::Identifier(_) => SyntaxKind::Ident,
            Token::StringLiteral(_) => SyntaxKind::StringLiteral,
            Token::StringStart(_) => SyntaxKind::StringStart,
            Token::StringPart(_) => SyntaxKind::StringPart,
            Token::StringEnd(_) => SyntaxKind::StringEnd,
            Token::Integer(_, _) => SyntaxKind::IntNumber,
            Token::Float(_, _) => SyntaxKind::FloatNumber,
            Token::Colon => SyntaxKind::Colon,
            Token::LParen => SyntaxKind::LParen,
            Token::RParen => SyntaxKind::RParen,
            Token::LBracket => SyntaxKind::LBracket,
            Token::RBracket => SyntaxKind::RBracket,
            Token::LBrace => SyntaxKind::LBrace,
            Token::RBrace => SyntaxKind::RBrace,
            Token::Dot => SyntaxKind::Dot,
            Token::Range => SyntaxKind::Range,
            Token::Comma => SyntaxKind::Comma,
            Token::At => SyntaxKind::At,
            Token::Arrow => SyntaxKind::Arrow,
            Token::Equals => SyntaxKind::Equals,
            Token::EqEq => SyntaxKind::EqEq,
            Token::BangEq => SyntaxKind::BangEq,
            Token::Less => SyntaxKind::Less,
            Token::LessEq => SyntaxKind::LessEq,
            Token::Greater => SyntaxKind::Greater,
            Token::GreaterEq => SyntaxKind::GreaterEq,
            Token::Plus => SyntaxKind::Plus,
            Token::Minus => SyntaxKind::Minus,
            Token::Star => SyntaxKind::Star,
            Token::Slash => SyntaxKind::Slash,
            Token::Percent => SyntaxKind::Percent,
            Token::PlusEq => SyntaxKind::PlusEq,
            Token::MinusEq => SyntaxKind::MinusEq,
            Token::StarEq => SyntaxKind::StarEq,
            Token::SlashEq => SyntaxKind::SlashEq,
            Token::Ampersand => SyntaxKind::Ampersand,
            Token::Pipe => SyntaxKind::Pipe,
            Token::Caret => SyntaxKind::Caret,
            Token::BitwiseNot => SyntaxKind::BitwiseNot,
            Token::ShiftLeft => SyntaxKind::ShiftLeft,
            Token::ShiftRight => SyntaxKind::ShiftRight,
            Token::Indent => SyntaxKind::Indent,
            Token::Dedent => SyntaxKind::Dedent,
            Token::Eof => SyntaxKind::Eof,
            Token::Newline(_) => SyntaxKind::Newline,
            Token::Whitespace(_) => SyntaxKind::Whitespace,
            Token::Comment(_) => SyntaxKind::Comment,
            Token::DocComment(_) => SyntaxKind::DocComment,
            Token::InvalidLiteral(_) => SyntaxKind::InvalidLiteral,
        }
    }
}

impl From<u16> for SyntaxKind {
    fn from(d: u16) -> Self {
        assert!(d <= SyntaxKind::Error as u16);
        unsafe { std::mem::transmute::<u16, SyntaxKind>(d) }
    }
}

impl From<SyntaxKind> for u16 {
    fn from(k: SyntaxKind) -> Self {
        k as u16
    }
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::Whitespace
                | SyntaxKind::Comment
                | SyntaxKind::Newline
                | SyntaxKind::DocComment
        )
    }
}
