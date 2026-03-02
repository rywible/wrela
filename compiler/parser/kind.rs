use crate::lexer::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // Keywords
    ClassKw,            // class
    ComponentKw,        // component
    ResourceKw,         // resource
    EventKw,            // event
    SceneKw,            // scene
    ThemeKw,            // theme
    NodeKw,             // node
    InterfaceKw,        // interface
    HasKw,              // has
    CanKw,              // can
    MustKw,             // must
    DerivesKw,          // derives
    FnKw,               // fn
    SystemKw,           // system
    ViewKw,             // view
    MaterialKw,         // material
    WidgetKw,           // widget
    AnimKw,             // anim
    IfKw,               // if
    ElseKw,             // else
    WhileKw,            // while
    ForKw,              // for
    InKw,               // in
    ReturnKw,           // return
    BreakKw,            // break
    ContinueKw,         // continue
    MatchKw,            // match
    DefaultKw,          // default
    ErrKw,              // error
    CrashKw,            // crash
    TrueKw,             // true
    FalseKw,            // false
    NothingKw,          // nothing
    AndKw,              // and
    OrKw,               // or
    NotKw,              // not
    AwaitKw,            // await
    DetachKw,           // detach
    SpawnKw,            // spawn
    FireKw,             // fire
    AssertKw,           // assert
    UseKw,              // use
    FromKw,             // from
    PrivateKw,          // private
    SelfKw,             // self
    MutableKw,          // mutable
    IsKw,               // is
    EnumKw,             // enum
    DeferKw,            // defer
    IgnoreKw,           // ignore
    CaptureKw,          // capture
    CheckKw,            // check
    ChecksKw,           // checks
    GivenKw,            // given
    RequireKw,          // require
    RenderKw,           // render
    PresetKw,           // preset
    ProfileKw,          // profile
    OverridesKw,        // overrides
    GpuKw,              // gpu
    AssetsKw,           // assets
    MmoKw,              // mmo
    AssetSpecKw,        // asset_spec
    StyleProfileKw,     // style_profile
    GeneratorProfileKw, // generator_profile
    QualityProfileKw,   // quality_profile
    ProvenancePolicyKw, // provenance_policy
    CharacterSpecKw,    // character_spec
    RigSpecKw,          // rig_spec
    AnimSetSpecKw,      // anim_set_spec
    AudioSpecKw,        // audio_spec
    VfxSpecKw,          // vfx_spec
    UiSpecKw,           // ui_spec
    WorldRecipeKw,      // world_recipe
    ShaderKw,           // shader

    // Literals
    Ident,
    StringLiteral,
    StringStart,
    StringPart,
    StringEnd,
    IntNumber,
    FloatNumber,

    // Symbols
    Colon,            // :
    LParen,           // (
    RParen,           // )
    LBracket,         // [
    RBracket,         // ]
    LBrace,           // {
    RBrace,           // }
    Dot,              // .
    Range,            // ...
    Comma,            // ,
    At,               // @
    QuestionQuestion, // ??
    Question,         // ?
    Arrow,            // ->

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
    Newline,
    Whitespace,
    Comment,
    DocComment,
    InvalidLiteral,
    Eof,

    // --- Nodes (Composite elements) ---
    Root,
    ClassDef,
    NodeDef,
    NodeProfileClause,
    ComponentDef,
    ResourceDef,
    EventDef,
    SceneDef,
    ThemeDef,
    EnumDef,
    EnumVariant,
    FuncDef,
    SystemDef,
    ViewDef,
    MaterialDef,
    MaterialSurfaceModelClause,
    MaterialPresetClause,
    MaterialTexturesClause,
    MaterialParamsClause,
    MaterialFeaturesClause,
    MaterialSemanticsClause,
    MaterialRenderAlphaClause,
    MaterialRenderDoubleSidedClause,
    MaterialRenderReceivesDecalsClause,
    WidgetDef,
    AnimDef,
    GpuFuncDef,
    RenderDef,
    RenderResourcesClause,
    RenderTemporalClause,
    RenderQualityTierClause,
    RenderBudgetTagsClause,
    RenderPresetClause,
    RenderProfileClause,
    RenderTargetClause,
    RenderShaderClause,
    RenderOverridesClause,
    RenderOverrideTier0,
    RenderOverrideTier1,
    RenderOverrideTier2,
    AssetsDef,
    AssetsManifestClause,
    AssetsStreamingClause,
    MmoDef,
    MmoGatewayClause,
    MmoZoneClause,
    MmoWorldClause,
    AssetSpecDef,
    AssetSpecIdClause,
    AssetSpecProfileClause,
    StyleProfileDef,
    StyleProfileIdClause,
    StyleProfileProfileClause,
    GeneratorProfileDef,
    GeneratorProfileIdClause,
    GeneratorProfileProfileClause,
    QualityProfileDef,
    QualityProfileIdClause,
    QualityProfileProfileClause,
    ProvenancePolicyDef,
    ProvenancePolicyIdClause,
    ProvenancePolicyProfileClause,
    CharacterSpecDef,
    CharacterSpecIdClause,
    CharacterSpecProfileClause,
    RigSpecDef,
    RigSpecIdClause,
    RigSpecProfileClause,
    AnimSetSpecDef,
    AnimSetSpecIdClause,
    AnimSetSpecProfileClause,
    AudioSpecDef,
    AudioSpecIdClause,
    AudioSpecProfileClause,
    VfxSpecDef,
    VfxSpecIdClause,
    VfxSpecProfileClause,
    UiSpecDef,
    UiSpecIdClause,
    UiSpecProfileClause,
    WorldRecipeDef,
    WorldRecipeIdClause,
    WorldRecipeProfileClause,
    ShaderDef,
    MethodDef,
    MustMethodDef,
    FieldDef,
    ParamList,
    Param,
    TypeParamList,
    TypeRef,
    TypeArgList,
    IsAClause,
    Attribute,
    Block,

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
    AssertStmt,
    DeferStmt,
    IgnoreResultStmt,
    CaptureStmt,
    RequireStmt,
    PrivateBlock,

    // Expressions
    BinExpr,
    PrefixExpr,
    TryExpr,
    CallExpr,
    TypeApplyExpr,
    IndexExpr,
    MemberExpr,
    LiteralExpr,
    IdentExpr,
    ParenExpr,
    ListExpr,
    MapExpr,
    StringInterp,
    NamedArg,
    CrashExpr,
    Pattern,
    PatternArgList,
    PatternFieldList,
    PatternField,

    Error,
}

impl From<Token> for SyntaxKind {
    fn from(token: Token) -> Self {
        match token {
            Token::Class => SyntaxKind::ClassKw,
            Token::Component => SyntaxKind::ComponentKw,
            Token::Resource => SyntaxKind::ResourceKw,
            Token::Event => SyntaxKind::EventKw,
            Token::Scene => SyntaxKind::SceneKw,
            Token::Theme => SyntaxKind::ThemeKw,
            Token::Node => SyntaxKind::NodeKw,
            Token::Interface => SyntaxKind::InterfaceKw,
            Token::Has => SyntaxKind::HasKw,
            Token::Can => SyntaxKind::CanKw,
            Token::Must => SyntaxKind::MustKw,
            Token::Derives => SyntaxKind::DerivesKw,
            Token::Fn => SyntaxKind::FnKw,
            Token::System => SyntaxKind::SystemKw,
            Token::View => SyntaxKind::ViewKw,
            Token::Material => SyntaxKind::MaterialKw,
            Token::Widget => SyntaxKind::WidgetKw,
            Token::Anim => SyntaxKind::AnimKw,
            Token::If => SyntaxKind::IfKw,
            Token::Else => SyntaxKind::ElseKw,
            Token::While => SyntaxKind::WhileKw,
            Token::For => SyntaxKind::ForKw,
            Token::In => SyntaxKind::InKw,
            Token::Return => SyntaxKind::ReturnKw,
            Token::Break => SyntaxKind::BreakKw,
            Token::Continue => SyntaxKind::ContinueKw,
            Token::Match => SyntaxKind::MatchKw,
            Token::Default => SyntaxKind::DefaultKw,
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
            Token::Assert => SyntaxKind::AssertKw,
            Token::Use => SyntaxKind::UseKw,
            Token::From => SyntaxKind::FromKw,
            Token::Private => SyntaxKind::PrivateKw,
            Token::SelfKw => SyntaxKind::SelfKw,
            Token::Mutable => SyntaxKind::MutableKw,
            Token::Is => SyntaxKind::IsKw,
            Token::Enum => SyntaxKind::EnumKw,
            Token::Defer => SyntaxKind::DeferKw,
            Token::Ignore => SyntaxKind::IgnoreKw,
            Token::Capture => SyntaxKind::CaptureKw,
            Token::Check => SyntaxKind::CheckKw,
            Token::Checks => SyntaxKind::ChecksKw,
            Token::Given => SyntaxKind::GivenKw,
            Token::Require => SyntaxKind::RequireKw,
            Token::Render => SyntaxKind::RenderKw,
            Token::Preset => SyntaxKind::PresetKw,
            Token::Profile => SyntaxKind::ProfileKw,
            Token::Overrides => SyntaxKind::OverridesKw,
            Token::Gpu => SyntaxKind::GpuKw,
            Token::Assets => SyntaxKind::AssetsKw,
            Token::Mmo => SyntaxKind::MmoKw,
            Token::AssetSpec => SyntaxKind::AssetSpecKw,
            Token::StyleProfile => SyntaxKind::StyleProfileKw,
            Token::GeneratorProfile => SyntaxKind::GeneratorProfileKw,
            Token::QualityProfile => SyntaxKind::QualityProfileKw,
            Token::ProvenancePolicy => SyntaxKind::ProvenancePolicyKw,
            Token::CharacterSpec => SyntaxKind::CharacterSpecKw,
            Token::RigSpec => SyntaxKind::RigSpecKw,
            Token::AnimSetSpec => SyntaxKind::AnimSetSpecKw,
            Token::AudioSpec => SyntaxKind::AudioSpecKw,
            Token::VfxSpec => SyntaxKind::VfxSpecKw,
            Token::UiSpec => SyntaxKind::UiSpecKw,
            Token::WorldRecipe => SyntaxKind::WorldRecipeKw,
            Token::Shader => SyntaxKind::ShaderKw,
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
            Token::QuestionQuestion => SyntaxKind::QuestionQuestion,
            Token::Question => SyntaxKind::Question,
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
