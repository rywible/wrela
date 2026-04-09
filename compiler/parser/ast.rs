use crate::parser::SyntaxNode;
use crate::parser::SyntaxToken;
use crate::parser::kind::SyntaxKind;

pub(crate) fn is_name_like_label_token(kind: SyntaxKind) -> bool {
    kind.is_name_like_label()
}

pub trait AstNode {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>
    where
        Self: Sized;
    fn syntax(&self) -> &SyntaxNode;
}

pub struct Root(SyntaxNode);
impl AstNode for Root {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Root
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Root(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl Root {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
}

pub enum Stmt {
    Expr(StmtExpr),
    ClassDef(ClassDef),
    ResourceDef(ResourceDef),
    EventDef(EventDef),
    ValueDef(ValueDef),
    EnumDef(EnumDef),
    FuncDef(FuncDef),
    KernelDef(KernelDef),
    SystemDef(SystemDef),
    FieldDecl(FieldDecl),
    RegionDecl(RegionDecl),
    DomainDecl(DomainDecl),
    RenderDecl(RenderDecl),
    RadianceDecl(RadianceDecl),
    VolumeDecl(VolumeDecl),
    MaterialDecl(MaterialDecl),
    ShapeDecl(ShapeDecl),
    StyleProfileDef(StyleProfileDef),
    GeneratorProfileDef(GeneratorProfileDef),
    QualityProfileDef(QualityProfileDef),
    ProvenancePolicyDef(ProvenancePolicyDef),
    VarAssign(VarAssign),
    IfStmt(IfStmt),
    WhileStmt(WhileStmt),
    ForStmt(ForStmt),
    ReturnStmt(ReturnStmt),
    BreakStmt(BreakStmt),
    ContinueStmt(ContinueStmt),
    MatchStmt(MatchStmt),
    UseStmt(UseStmt),
    AssertStmt(AssertStmt),
    DeferStmt(DeferStmt),
    IgnoreResultStmt(IgnoreResultStmt),
    CaptureStmt(CaptureStmt),
    RequireStmt(RequireStmt),
    PrivateBlock(PrivateBlock),
}

impl AstNode for Stmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::StmtExpr
                | SyntaxKind::ClassDef
                | SyntaxKind::ResourceDef
                | SyntaxKind::EventDef
                | SyntaxKind::ValueDef
                | SyntaxKind::EnumDef
                | SyntaxKind::FuncDef
                | SyntaxKind::KernelDef
                | SyntaxKind::SystemDef
                | SyntaxKind::FieldDecl
                | SyntaxKind::RegionDecl
                | SyntaxKind::DomainDecl
                | SyntaxKind::RenderDecl
                | SyntaxKind::RadianceDecl
                | SyntaxKind::VolumeDecl
                | SyntaxKind::MaterialDecl
                | SyntaxKind::ShapeDecl
                | SyntaxKind::StyleProfileDef
                | SyntaxKind::GeneratorProfileDef
                | SyntaxKind::QualityProfileDef
                | SyntaxKind::ProvenancePolicyDef
                | SyntaxKind::VarAssign
                | SyntaxKind::IfStmt
                | SyntaxKind::WhileStmt
                | SyntaxKind::ForStmt
                | SyntaxKind::ReturnStmt
                | SyntaxKind::BreakStmt
                | SyntaxKind::ContinueStmt
                | SyntaxKind::MatchStmt
                | SyntaxKind::UseStmt
                | SyntaxKind::AssertStmt
                | SyntaxKind::DeferStmt
                | SyntaxKind::IgnoreResultStmt
                | SyntaxKind::CaptureStmt
                | SyntaxKind::RequireStmt
                | SyntaxKind::PrivateBlock
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::StmtExpr => StmtExpr::cast(node).map(Stmt::Expr),
            SyntaxKind::ClassDef => ClassDef::cast(node).map(Stmt::ClassDef),
            SyntaxKind::ResourceDef => ResourceDef::cast(node).map(Stmt::ResourceDef),
            SyntaxKind::EventDef => EventDef::cast(node).map(Stmt::EventDef),
            SyntaxKind::ValueDef => ValueDef::cast(node).map(Stmt::ValueDef),
            SyntaxKind::EnumDef => EnumDef::cast(node).map(Stmt::EnumDef),
            SyntaxKind::FuncDef => FuncDef::cast(node).map(Stmt::FuncDef),
            SyntaxKind::KernelDef => KernelDef::cast(node).map(Stmt::KernelDef),
            SyntaxKind::SystemDef => SystemDef::cast(node).map(Stmt::SystemDef),
            SyntaxKind::FieldDecl => FieldDecl::cast(node).map(Stmt::FieldDecl),
            SyntaxKind::RegionDecl => RegionDecl::cast(node).map(Stmt::RegionDecl),
            SyntaxKind::DomainDecl => DomainDecl::cast(node).map(Stmt::DomainDecl),
            SyntaxKind::RenderDecl => RenderDecl::cast(node).map(Stmt::RenderDecl),
            SyntaxKind::RadianceDecl => RadianceDecl::cast(node).map(Stmt::RadianceDecl),
            SyntaxKind::VolumeDecl => VolumeDecl::cast(node).map(Stmt::VolumeDecl),
            SyntaxKind::MaterialDecl => MaterialDecl::cast(node).map(Stmt::MaterialDecl),
            SyntaxKind::ShapeDecl => ShapeDecl::cast(node).map(Stmt::ShapeDecl),
            SyntaxKind::StyleProfileDef => StyleProfileDef::cast(node).map(Stmt::StyleProfileDef),
            SyntaxKind::GeneratorProfileDef => {
                GeneratorProfileDef::cast(node).map(Stmt::GeneratorProfileDef)
            }
            SyntaxKind::QualityProfileDef => {
                QualityProfileDef::cast(node).map(Stmt::QualityProfileDef)
            }
            SyntaxKind::ProvenancePolicyDef => {
                ProvenancePolicyDef::cast(node).map(Stmt::ProvenancePolicyDef)
            }
            SyntaxKind::VarAssign => VarAssign::cast(node).map(Stmt::VarAssign),
            SyntaxKind::IfStmt => IfStmt::cast(node).map(Stmt::IfStmt),
            SyntaxKind::WhileStmt => WhileStmt::cast(node).map(Stmt::WhileStmt),
            SyntaxKind::ForStmt => ForStmt::cast(node).map(Stmt::ForStmt),
            SyntaxKind::ReturnStmt => ReturnStmt::cast(node).map(Stmt::ReturnStmt),
            SyntaxKind::BreakStmt => BreakStmt::cast(node).map(Stmt::BreakStmt),
            SyntaxKind::ContinueStmt => ContinueStmt::cast(node).map(Stmt::ContinueStmt),
            SyntaxKind::MatchStmt => MatchStmt::cast(node).map(Stmt::MatchStmt),
            SyntaxKind::UseStmt => UseStmt::cast(node).map(Stmt::UseStmt),
            SyntaxKind::AssertStmt => AssertStmt::cast(node).map(Stmt::AssertStmt),
            SyntaxKind::DeferStmt => DeferStmt::cast(node).map(Stmt::DeferStmt),
            SyntaxKind::IgnoreResultStmt => {
                IgnoreResultStmt::cast(node).map(Stmt::IgnoreResultStmt)
            }
            SyntaxKind::CaptureStmt => CaptureStmt::cast(node).map(Stmt::CaptureStmt),
            SyntaxKind::RequireStmt => RequireStmt::cast(node).map(Stmt::RequireStmt),
            SyntaxKind::PrivateBlock => PrivateBlock::cast(node).map(Stmt::PrivateBlock),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Stmt::Expr(it) => it.syntax(),
            Stmt::ClassDef(it) => it.syntax(),
            Stmt::ResourceDef(it) => it.syntax(),
            Stmt::EventDef(it) => it.syntax(),
            Stmt::ValueDef(it) => it.syntax(),
            Stmt::EnumDef(it) => it.syntax(),
            Stmt::FuncDef(it) => it.syntax(),
            Stmt::KernelDef(it) => it.syntax(),
            Stmt::SystemDef(it) => it.syntax(),
            Stmt::FieldDecl(it) => it.syntax(),
            Stmt::RegionDecl(it) => it.syntax(),
            Stmt::DomainDecl(it) => it.syntax(),
            Stmt::RenderDecl(it) => it.syntax(),
            Stmt::RadianceDecl(it) => it.syntax(),
            Stmt::VolumeDecl(it) => it.syntax(),
            Stmt::MaterialDecl(it) => it.syntax(),
            Stmt::ShapeDecl(it) => it.syntax(),
            Stmt::StyleProfileDef(it) => it.syntax(),
            Stmt::GeneratorProfileDef(it) => it.syntax(),
            Stmt::QualityProfileDef(it) => it.syntax(),
            Stmt::ProvenancePolicyDef(it) => it.syntax(),
            Stmt::VarAssign(it) => it.syntax(),
            Stmt::IfStmt(it) => it.syntax(),
            Stmt::WhileStmt(it) => it.syntax(),
            Stmt::ForStmt(it) => it.syntax(),
            Stmt::ReturnStmt(it) => it.syntax(),
            Stmt::BreakStmt(it) => it.syntax(),
            Stmt::ContinueStmt(it) => it.syntax(),
            Stmt::MatchStmt(it) => it.syntax(),
            Stmt::UseStmt(it) => it.syntax(),
            Stmt::AssertStmt(it) => it.syntax(),
            Stmt::DeferStmt(it) => it.syntax(),
            Stmt::IgnoreResultStmt(it) => it.syntax(),
            Stmt::CaptureStmt(it) => it.syntax(),
            Stmt::RequireStmt(it) => it.syntax(),
            Stmt::PrivateBlock(it) => it.syntax(),
        }
    }
}

pub enum AssertMode {
    Value,
    Identity,
    Approx,
}

pub struct AssertStmt(SyntaxNode);
impl AstNode for AssertStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::AssertStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(AssertStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl AssertStmt {
    pub fn mode(&self) -> AssertMode {
        for token in self
            .0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
        {
            if token.kind() == SyntaxKind::Ident {
                let text = token.text();
                if text == "value" {
                    return AssertMode::Value;
                }
                if text == "identity" {
                    return AssertMode::Identity;
                }
                if text == "approx" {
                    return AssertMode::Approx;
                }
            }
        }
        AssertMode::Value
    }

    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn rhs_expr(&self) -> Option<Expr> {
        let mut exprs = self.0.children().filter_map(Expr::cast);
        exprs.next()?;
        exprs.next()
    }

    pub fn tolerance_expr(&self) -> Option<Expr> {
        let mut exprs = self.0.children().filter_map(Expr::cast);
        exprs.next()?;
        exprs.next()?;
        exprs.next()
    }
}

pub struct RequireStmt(SyntaxNode);
impl AstNode for RequireStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RequireStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(RequireStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl RequireStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn message(&self) -> Option<Expr> {
        let mut exprs = self.0.children().filter_map(Expr::cast);
        exprs.next();
        exprs.next()
    }
}

pub struct DeferStmt(SyntaxNode);
impl AstNode for DeferStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::DeferStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(DeferStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl DeferStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct IgnoreResultStmt(SyntaxNode);
impl AstNode for IgnoreResultStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IgnoreResultStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(IgnoreResultStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl IgnoreResultStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct CaptureStmt(SyntaxNode);
impl AstNode for CaptureStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CaptureStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(CaptureStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl CaptureStmt {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct PrivateBlock(SyntaxNode);
impl AstNode for PrivateBlock {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PrivateBlock
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(PrivateBlock(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl PrivateBlock {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }
}

pub struct StmtExpr(SyntaxNode);
impl AstNode for StmtExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StmtExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(StmtExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl StmtExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct ClassDef(SyntaxNode);
impl AstNode for ClassDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ClassDef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ClassDef(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ClassDef {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn type_params(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::TypeParamList)
            .flat_map(|node| node.children_with_tokens())
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn is_a(&self) -> Option<SyntaxToken> {
        self.0
            .children()
            .find(|it| it.kind() == SyntaxKind::IsAClause)
            .and_then(|node| {
                node.children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .filter(|it| it.kind() == SyntaxKind::Ident)
                    .last()
            })
    }

    pub fn fields(&self) -> impl Iterator<Item = FieldDef> {
        self.0.children().filter_map(FieldDef::cast)
    }

    pub fn methods(&self) -> impl Iterator<Item = MethodDef> {
        self.0.children().filter_map(MethodDef::cast)
    }

    pub fn must_methods(&self) -> impl Iterator<Item = MustMethodDef> {
        self.0.children().filter_map(MustMethodDef::cast)
    }
}

macro_rules! impl_class_like_def {
    ($name:ident, $kind:expr) => {
        pub struct $name(SyntaxNode);
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some(Self(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $name {
            pub fn name(&self) -> Option<SyntaxToken> {
                self.0
                    .children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .find(|it| it.kind() == SyntaxKind::Ident)
            }

            pub fn type_params(&self) -> impl Iterator<Item = SyntaxToken> {
                self.0
                    .children()
                    .filter(|it| it.kind() == SyntaxKind::TypeParamList)
                    .flat_map(|node| node.children_with_tokens())
                    .filter_map(|it| it.into_token())
                    .filter(|it| it.kind() == SyntaxKind::Ident)
            }

            pub fn is_a(&self) -> Option<SyntaxToken> {
                self.0
                    .children()
                    .find(|it| it.kind() == SyntaxKind::IsAClause)
                    .and_then(|node| {
                        node.children_with_tokens()
                            .filter_map(|it| it.into_token())
                            .filter(|it| it.kind() == SyntaxKind::Ident)
                            .last()
                    })
            }

            pub fn fields(&self) -> impl Iterator<Item = FieldDef> {
                self.0.children().filter_map(FieldDef::cast)
            }

            pub fn methods(&self) -> impl Iterator<Item = MethodDef> {
                self.0.children().filter_map(MethodDef::cast)
            }

            pub fn must_methods(&self) -> impl Iterator<Item = MustMethodDef> {
                self.0.children().filter_map(MustMethodDef::cast)
            }
        }
    };
}

impl_class_like_def!(ResourceDef, SyntaxKind::ResourceDef);
impl_class_like_def!(EventDef, SyntaxKind::EventDef);

pub struct ValueDef(SyntaxNode);
impl AstNode for ValueDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ValueDef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ValueDef {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .nth(1)
    }

    pub fn type_params(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::TypeParamList)
            .flat_map(|node| node.children_with_tokens())
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn is_a(&self) -> Option<SyntaxToken> {
        self.0
            .children()
            .find(|it| it.kind() == SyntaxKind::IsAClause)
            .and_then(|node| {
                node.children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .filter(|it| it.kind() == SyntaxKind::Ident)
                    .last()
            })
    }

    pub fn fields(&self) -> impl Iterator<Item = FieldDef> {
        self.0.children().filter_map(FieldDef::cast)
    }

    pub fn methods(&self) -> impl Iterator<Item = MethodDef> {
        self.0.children().filter_map(MethodDef::cast)
    }

    pub fn must_methods(&self) -> impl Iterator<Item = MustMethodDef> {
        self.0.children().filter_map(MustMethodDef::cast)
    }
}

pub struct EnumDef(SyntaxNode);
impl AstNode for EnumDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::EnumDef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(EnumDef(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl EnumDef {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn type_params(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::TypeParamList)
            .flat_map(|node| node.children_with_tokens())
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn variants(&self) -> impl Iterator<Item = EnumVariant> {
        self.0.children().filter_map(EnumVariant::cast)
    }
}

pub struct EnumVariant(SyntaxNode);
impl AstNode for EnumVariant {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::EnumVariant
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(EnumVariant(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl EnumVariant {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::ParamList)
            .flat_map(|node| node.children())
            .filter_map(Param::cast)
    }
}

pub struct Attribute(SyntaxNode);
impl AstNode for Attribute {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Attribute
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Attribute(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

#[derive(Clone)]
pub struct AttributeArg {
    key: SyntaxToken,
    value: SyntaxToken,
}

impl AttributeArg {
    pub fn key(&self) -> SyntaxToken {
        self.key.clone()
    }

    pub fn value(&self) -> SyntaxToken {
        self.value.clone()
    }
}

impl Attribute {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::Ident)
    }

    pub fn args(&self) -> impl Iterator<Item = AttributeArg> {
        let tokens = self
            .0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|token| !token.kind().is_trivia())
            .collect::<Vec<_>>();
        let mut out = Vec::new();
        let mut idx = 0usize;
        while idx + 2 < tokens.len() {
            if tokens[idx].kind() == SyntaxKind::Ident
                && tokens[idx + 1].kind() == SyntaxKind::Equals
                && is_attribute_arg_value_kind(tokens[idx + 2].kind())
            {
                out.push(AttributeArg {
                    key: tokens[idx].clone(),
                    value: tokens[idx + 2].clone(),
                });
                idx += 3;
                continue;
            }
            idx += 1;
        }
        out.into_iter()
    }
}

pub struct FuncDef(SyntaxNode);
impl AstNode for FuncDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FuncDef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FuncDef(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FuncDef {
    pub fn attributes(&self) -> impl Iterator<Item = Attribute> {
        self.0.children().filter_map(Attribute::cast)
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        function_like_name_token(&self.0)
    }

    pub fn is_pure(&self) -> bool {
        let tokens = function_like_tokens(&self.0);
        matches!(
            tokens.as_slice(),
            [first, second, ..]
                if first.kind() == SyntaxKind::Ident
                    && first.text() == "pure"
                    && second.kind() == SyntaxKind::FnKw
        )
    }

    pub fn type_params(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::TypeParamList)
            .flat_map(|node| {
                node.children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .filter(|it| it.kind() == SyntaxKind::Ident)
                    .collect::<Vec<_>>()
            })
    }

    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::ParamList)
            .flat_map(|node| node.children())
            .filter_map(Param::cast)
    }

    pub fn ret_type(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }

    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(Stmt::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
    }

    pub fn implicit_return_expr(&self) -> Option<StmtExpr> {
        let mut last = None;
        for stmt in self.statements() {
            last = Some(stmt);
        }
        match last {
            Some(Stmt::Expr(expr)) => Some(expr),
            _ => None,
        }
    }
}

fn function_like_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect()
}

fn function_like_name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    let tokens = function_like_tokens(node);
    let mut idx = 0usize;
    if tokens
        .first()
        .is_some_and(|token| token.kind() == SyntaxKind::Ident && token.text() == "pure")
    {
        idx += 1;
    }
    match tokens.get(idx)?.kind() {
        SyntaxKind::FnKw | SyntaxKind::SystemKw => {}
        SyntaxKind::KernelKw => {
            if tokens.get(idx + 1)?.kind() != SyntaxKind::FnKw {
                return None;
            }
            idx += 1;
        }
        _ => return None,
    }
    tokens
        .iter()
        .skip(idx + 1)
        .find(|token| token.kind() == SyntaxKind::Ident)
        .cloned()
}

macro_rules! impl_function_like_def {
    ($name:ident, $kind:expr) => {
        pub struct $name(SyntaxNode);
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some(Self(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $name {
            pub fn attributes(&self) -> impl Iterator<Item = Attribute> {
                self.0.children().filter_map(Attribute::cast)
            }

            pub fn name(&self) -> Option<SyntaxToken> {
                self.0
                    .children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .find(|it| it.kind() == SyntaxKind::Ident)
            }

            pub fn type_params(&self) -> impl Iterator<Item = SyntaxToken> {
                self.0
                    .children()
                    .filter(|it| it.kind() == SyntaxKind::TypeParamList)
                    .flat_map(|node| {
                        node.children_with_tokens()
                            .filter_map(|it| it.into_token())
                            .filter(|it| it.kind() == SyntaxKind::Ident)
                            .collect::<Vec<_>>()
                    })
            }

            pub fn params(&self) -> impl Iterator<Item = Param> {
                self.0
                    .children()
                    .filter(|it| it.kind() == SyntaxKind::ParamList)
                    .flat_map(|node| node.children())
                    .filter_map(Param::cast)
            }

            pub fn ret_type(&self) -> Option<TypeRef> {
                self.0.children().filter_map(TypeRef::cast).next()
            }

            pub fn statements(&self) -> impl Iterator<Item = Stmt> {
                self.0
                    .children()
                    .filter_map(Block::cast)
                    .next()
                    .into_iter()
                    .flat_map(|b| {
                        b.0.children()
                            .filter_map(Stmt::cast)
                            .collect::<Vec<_>>()
                            .into_iter()
                    })
            }

            pub fn implicit_return_expr(&self) -> Option<StmtExpr> {
                let mut last = None;
                for stmt in self.statements() {
                    last = Some(stmt);
                }
                match last {
                    Some(Stmt::Expr(expr)) => Some(expr),
                    _ => None,
                }
            }
        }
    };
}

impl_function_like_def!(SystemDef, SyntaxKind::SystemDef);
impl_function_like_def!(KernelDef, SyntaxKind::KernelDef);

pub enum FieldClass {
    Exact,
    Conservative,
}

pub enum FieldKind {
    Distance,
}

pub enum FieldExpr {
    Use(FieldUseExpr),
    Primitive(FieldPrimitiveExpr),
    Union(FieldUnionExpr),
    Intersection(FieldIntersectionExpr),
    Subtract(FieldSubtractExpr),
    SmoothUnion(FieldSmoothUnionExpr),
    SmoothIntersection(FieldSmoothIntersectionExpr),
    SmoothSubtract(FieldSmoothSubtractExpr),
    Translate(FieldTranslateExpr),
    Rotate(FieldRotateExpr),
    UniformScale(FieldUniformScaleExpr),
    AffineTransform(FieldAffineTransformExpr),
    Warp(FieldWarpExpr),
    RepeatLinear(FieldRepeatLinearExpr),
    RepeatGrid(FieldRepeatGridExpr),
    RadialRepeat(FieldRadialRepeatExpr),
    MirrorArray(FieldMirrorArrayExpr),
    InstanceArray(FieldInstanceArrayExpr),
    Bend(FieldBendExpr),
    Twist(FieldTwistExpr),
    Taper(FieldTaperExpr),
    Displace(FieldDisplaceExpr),
    Extrude(FieldExtrudeExpr),
    Revolve(FieldRevolveExpr),
    Sweep(FieldSweepExpr),
    Loft(FieldLoftExpr),
}

impl AstNode for FieldExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::FieldUseExpr
                | SyntaxKind::FieldPrimitiveExpr
                | SyntaxKind::FieldUnionExpr
                | SyntaxKind::FieldIntersectionExpr
                | SyntaxKind::FieldSubtractExpr
                | SyntaxKind::FieldSmoothUnionExpr
                | SyntaxKind::FieldSmoothIntersectionExpr
                | SyntaxKind::FieldSmoothSubtractExpr
                | SyntaxKind::FieldTranslateExpr
                | SyntaxKind::FieldRotateExpr
                | SyntaxKind::FieldUniformScaleExpr
                | SyntaxKind::FieldAffineTransformExpr
                | SyntaxKind::FieldWarpExpr
                | SyntaxKind::FieldRepeatLinearExpr
                | SyntaxKind::FieldRepeatGridExpr
                | SyntaxKind::FieldRadialRepeatExpr
                | SyntaxKind::FieldMirrorArrayExpr
                | SyntaxKind::FieldInstanceArrayExpr
                | SyntaxKind::FieldBendExpr
                | SyntaxKind::FieldTwistExpr
                | SyntaxKind::FieldTaperExpr
                | SyntaxKind::FieldDisplaceExpr
                | SyntaxKind::FieldExtrudeExpr
                | SyntaxKind::FieldRevolveExpr
                | SyntaxKind::FieldSweepExpr
                | SyntaxKind::FieldLoftExpr
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::FieldUseExpr => FieldUseExpr::cast(node).map(FieldExpr::Use),
            SyntaxKind::FieldPrimitiveExpr => {
                FieldPrimitiveExpr::cast(node).map(FieldExpr::Primitive)
            }
            SyntaxKind::FieldUnionExpr => FieldUnionExpr::cast(node).map(FieldExpr::Union),
            SyntaxKind::FieldIntersectionExpr => {
                FieldIntersectionExpr::cast(node).map(FieldExpr::Intersection)
            }
            SyntaxKind::FieldSubtractExpr => FieldSubtractExpr::cast(node).map(FieldExpr::Subtract),
            SyntaxKind::FieldSmoothUnionExpr => {
                FieldSmoothUnionExpr::cast(node).map(FieldExpr::SmoothUnion)
            }
            SyntaxKind::FieldSmoothIntersectionExpr => {
                FieldSmoothIntersectionExpr::cast(node).map(FieldExpr::SmoothIntersection)
            }
            SyntaxKind::FieldSmoothSubtractExpr => {
                FieldSmoothSubtractExpr::cast(node).map(FieldExpr::SmoothSubtract)
            }
            SyntaxKind::FieldTranslateExpr => {
                FieldTranslateExpr::cast(node).map(FieldExpr::Translate)
            }
            SyntaxKind::FieldRotateExpr => FieldRotateExpr::cast(node).map(FieldExpr::Rotate),
            SyntaxKind::FieldUniformScaleExpr => {
                FieldUniformScaleExpr::cast(node).map(FieldExpr::UniformScale)
            }
            SyntaxKind::FieldAffineTransformExpr => {
                FieldAffineTransformExpr::cast(node).map(FieldExpr::AffineTransform)
            }
            SyntaxKind::FieldWarpExpr => FieldWarpExpr::cast(node).map(FieldExpr::Warp),
            SyntaxKind::FieldRepeatLinearExpr => {
                FieldRepeatLinearExpr::cast(node).map(FieldExpr::RepeatLinear)
            }
            SyntaxKind::FieldRepeatGridExpr => {
                FieldRepeatGridExpr::cast(node).map(FieldExpr::RepeatGrid)
            }
            SyntaxKind::FieldRadialRepeatExpr => {
                FieldRadialRepeatExpr::cast(node).map(FieldExpr::RadialRepeat)
            }
            SyntaxKind::FieldMirrorArrayExpr => {
                FieldMirrorArrayExpr::cast(node).map(FieldExpr::MirrorArray)
            }
            SyntaxKind::FieldInstanceArrayExpr => {
                FieldInstanceArrayExpr::cast(node).map(FieldExpr::InstanceArray)
            }
            SyntaxKind::FieldBendExpr => FieldBendExpr::cast(node).map(FieldExpr::Bend),
            SyntaxKind::FieldTwistExpr => FieldTwistExpr::cast(node).map(FieldExpr::Twist),
            SyntaxKind::FieldTaperExpr => FieldTaperExpr::cast(node).map(FieldExpr::Taper),
            SyntaxKind::FieldDisplaceExpr => FieldDisplaceExpr::cast(node).map(FieldExpr::Displace),
            SyntaxKind::FieldExtrudeExpr => FieldExtrudeExpr::cast(node).map(FieldExpr::Extrude),
            SyntaxKind::FieldRevolveExpr => FieldRevolveExpr::cast(node).map(FieldExpr::Revolve),
            SyntaxKind::FieldSweepExpr => FieldSweepExpr::cast(node).map(FieldExpr::Sweep),
            SyntaxKind::FieldLoftExpr => FieldLoftExpr::cast(node).map(FieldExpr::Loft),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            FieldExpr::Use(it) => it.syntax(),
            FieldExpr::Primitive(it) => it.syntax(),
            FieldExpr::Union(it) => it.syntax(),
            FieldExpr::Intersection(it) => it.syntax(),
            FieldExpr::Subtract(it) => it.syntax(),
            FieldExpr::SmoothUnion(it) => it.syntax(),
            FieldExpr::SmoothIntersection(it) => it.syntax(),
            FieldExpr::SmoothSubtract(it) => it.syntax(),
            FieldExpr::Translate(it) => it.syntax(),
            FieldExpr::Rotate(it) => it.syntax(),
            FieldExpr::UniformScale(it) => it.syntax(),
            FieldExpr::AffineTransform(it) => it.syntax(),
            FieldExpr::Warp(it) => it.syntax(),
            FieldExpr::RepeatLinear(it) => it.syntax(),
            FieldExpr::RepeatGrid(it) => it.syntax(),
            FieldExpr::RadialRepeat(it) => it.syntax(),
            FieldExpr::MirrorArray(it) => it.syntax(),
            FieldExpr::InstanceArray(it) => it.syntax(),
            FieldExpr::Bend(it) => it.syntax(),
            FieldExpr::Twist(it) => it.syntax(),
            FieldExpr::Taper(it) => it.syntax(),
            FieldExpr::Displace(it) => it.syntax(),
            FieldExpr::Extrude(it) => it.syntax(),
            FieldExpr::Revolve(it) => it.syntax(),
            FieldExpr::Sweep(it) => it.syntax(),
            FieldExpr::Loft(it) => it.syntax(),
        }
    }
}

pub enum ProfileExpr {
    Primitive(FieldPrimitiveExpr),
}

impl AstNode for ProfileExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldPrimitiveExpr
    }

    fn cast(node: SyntaxNode) -> Option<Self> {
        let primitive = FieldPrimitiveExpr::cast(node)?;
        let name = primitive.name()?.text().to_string();
        if is_profile_primitive_name(&name) {
            Some(ProfileExpr::Primitive(primitive))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            ProfileExpr::Primitive(it) => it.syntax(),
        }
    }
}

impl ProfileExpr {
    pub fn primitive(&self) -> &FieldPrimitiveExpr {
        match self {
            ProfileExpr::Primitive(primitive) => primitive,
        }
    }
}

fn is_profile_primitive_name(name: &str) -> bool {
    matches!(
        name,
        "circle2" | "rect2" | "rounded_rect2" | "capsule2" | "segment2" | "polygon2" | "polyline2"
    )
}

pub struct FieldDecl(SyntaxNode);
impl AstNode for FieldDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldDecl
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldDecl(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldDecl {
    fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn support_clause(&self) -> Option<FieldSupportClause> {
        self.block().and_then(|block| {
            block
                .0
                .children()
                .filter_map(FieldSupportClause::cast)
                .next()
        })
    }

    pub fn bounds_clause(&self) -> Option<FieldBoundsClause> {
        self.block().and_then(|block| {
            block
                .0
                .children()
                .filter_map(FieldBoundsClause::cast)
                .next()
        })
    }

    pub fn field_class(&self) -> Option<FieldClass> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find_map(|token| match token.text() {
                "exact" => Some(FieldClass::Exact),
                "conservative" => Some(FieldClass::Conservative),
                _ => None,
            })
    }

    pub fn field_kind(&self) -> Option<FieldKind> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find_map(|token| match token.text() {
                "distance" => Some(FieldKind::Distance),
                _ => None,
            })
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .nth(3)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::ParamList)
            .flat_map(|node| node.children())
            .filter_map(Param::cast)
    }

    pub fn ret_type(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }

    pub fn semantic_expr(&self) -> Option<FieldExpr> {
        self.block()
            .and_then(|block| block.0.children().filter_map(FieldExpr::cast).next())
    }

    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.block().into_iter().flat_map(|b| {
            b.0.children()
                .filter_map(Stmt::cast)
                .collect::<Vec<_>>()
                .into_iter()
        })
    }

    pub fn implicit_return_expr(&self) -> Option<StmtExpr> {
        let mut last = None;
        for stmt in self.statements() {
            last = Some(stmt);
        }
        match last {
            Some(Stmt::Expr(expr)) => Some(expr),
            _ => None,
        }
    }
}

macro_rules! impl_world_decl {
    ($name:ident, $kind:expr) => {
        pub struct $name(SyntaxNode);
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some($name(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $name {
            pub fn name(&self) -> Option<SyntaxToken> {
                self.0
                    .children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .filter(|it| it.kind() == SyntaxKind::Ident)
                    .nth(1)
            }

            pub fn params(&self) -> impl Iterator<Item = Param> {
                self.0
                    .children()
                    .filter(|it| it.kind() == SyntaxKind::ParamList)
                    .flat_map(|node| node.children())
                    .filter_map(Param::cast)
            }

            fn block(&self) -> Option<Block> {
                self.0.children().filter_map(Block::cast).next()
            }

            pub fn statements(&self) -> impl Iterator<Item = Stmt> {
                self.block().into_iter().flat_map(|b| {
                    b.0.children()
                        .filter_map(Stmt::cast)
                        .collect::<Vec<_>>()
                        .into_iter()
                })
            }
        }
    };
}

impl_world_decl!(RegionDecl, SyntaxKind::RegionDecl);
impl_world_decl!(DomainDecl, SyntaxKind::DomainDecl);
impl_world_decl!(RenderDecl, SyntaxKind::RenderDecl);

impl RegionDecl {
    pub fn items(&self) -> impl Iterator<Item = RegionItem> {
        self.block().into_iter().flat_map(|b| {
            b.0.children()
                .filter_map(RegionItem::cast)
                .collect::<Vec<_>>()
                .into_iter()
        })
    }
}

pub enum RegionItem {
    Place(RegionPlaceStmt),
    Overlay(RegionOverlayStmt),
    Replace(RegionReplaceStmt),
    Scatter(RegionScatterStmt),
    If(IfStmt),
}

impl AstNode for RegionItem {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::RegionPlaceStmt
                | SyntaxKind::RegionOverlayStmt
                | SyntaxKind::RegionReplaceStmt
                | SyntaxKind::RegionScatterStmt
                | SyntaxKind::IfStmt
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::RegionPlaceStmt => RegionPlaceStmt::cast(node).map(RegionItem::Place),
            SyntaxKind::RegionOverlayStmt => RegionOverlayStmt::cast(node).map(RegionItem::Overlay),
            SyntaxKind::RegionReplaceStmt => RegionReplaceStmt::cast(node).map(RegionItem::Replace),
            SyntaxKind::RegionScatterStmt => RegionScatterStmt::cast(node).map(RegionItem::Scatter),
            SyntaxKind::IfStmt => IfStmt::cast(node).map(RegionItem::If),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            RegionItem::Place(it) => it.syntax(),
            RegionItem::Overlay(it) => it.syntax(),
            RegionItem::Replace(it) => it.syntax(),
            RegionItem::Scatter(it) => it.syntax(),
            RegionItem::If(it) => it.syntax(),
        }
    }
}

macro_rules! impl_region_named_assignment_stmt {
    ($name:ident, $kind:expr) => {
        pub struct $name(SyntaxNode);
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some($name(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $name {
            pub fn name(&self) -> Option<SyntaxToken> {
                self.0
                    .children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .filter(|it| it.kind() == SyntaxKind::Ident)
                    .nth(1)
            }

            pub fn value(&self) -> Option<Expr> {
                self.0.children().filter_map(Expr::cast).next()
            }
        }
    };
}

pub struct RegionScatterStmt(SyntaxNode);
impl AstNode for RegionScatterStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RegionScatterStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(RegionScatterStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl RegionScatterStmt {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .nth(1)
    }

    fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn items(&self) -> impl Iterator<Item = RegionItem> {
        self.block().into_iter().flat_map(|b| {
            b.0.children()
                .filter_map(RegionItem::cast)
                .collect::<Vec<_>>()
                .into_iter()
        })
    }
}

impl_region_named_assignment_stmt!(RegionPlaceStmt, SyntaxKind::RegionPlaceStmt);
impl_region_named_assignment_stmt!(RegionOverlayStmt, SyntaxKind::RegionOverlayStmt);
impl_region_named_assignment_stmt!(RegionReplaceStmt, SyntaxKind::RegionReplaceStmt);

pub struct RadianceDecl(SyntaxNode);
impl AstNode for RadianceDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RadianceDecl
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(RadianceDecl(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl RadianceDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .nth(2)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::ParamList)
            .flat_map(|node| node.children())
            .filter_map(Param::cast)
    }

    pub fn ret_type(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }

    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(Stmt::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
    }

    pub fn implicit_return_expr(&self) -> Option<StmtExpr> {
        let mut last = None;
        for stmt in self.statements() {
            last = Some(stmt);
        }
        match last {
            Some(Stmt::Expr(expr)) => Some(expr),
            _ => None,
        }
    }
}

pub struct FieldSupportClause(SyntaxNode);
impl AstNode for FieldSupportClause {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldSupportClause
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldSupportClause(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldSupportClause {
    pub fn value(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct FieldBoundsClause(SyntaxNode);
impl AstNode for FieldBoundsClause {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldBoundsClause
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldBoundsClause(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldBoundsClause {
    pub fn value(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct FieldUseExpr(SyntaxNode);
impl AstNode for FieldUseExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldUseExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldUseExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldUseExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }
}

pub struct FieldPrimitiveExpr(SyntaxNode);
impl AstNode for FieldPrimitiveExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldPrimitiveExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldPrimitiveExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldPrimitiveExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn args(&self) -> impl Iterator<Item = Arg> {
        let mut seen_name = false;
        self.0.children().filter_map(move |node| {
            if !seen_name && node.kind() == SyntaxKind::IdentExpr {
                seen_name = true;
                return None;
            }
            if let Some(named) = NamedArg::cast(node.clone()) {
                return Some(Arg::Named(named));
            }
            Expr::cast(node).map(Arg::Positional)
        })
    }
}

pub struct FieldUnionExpr(SyntaxNode);
impl AstNode for FieldUnionExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldUnionExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldUnionExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldUnionExpr {
    pub fn provenance_policy_clause(&self) -> Option<FieldProvenancePolicyClause> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(FieldProvenancePolicyClause::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
            .next()
    }

    pub fn provenance_policy(&self) -> Option<SyntaxToken> {
        self.provenance_policy_clause()
            .and_then(|clause| clause.value())
    }

    pub fn items(&self) -> impl Iterator<Item = FieldExpr> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(FieldExpr::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
    }
}

pub struct FieldIntersectionExpr(SyntaxNode);
impl AstNode for FieldIntersectionExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldIntersectionExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldIntersectionExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldIntersectionExpr {
    pub fn provenance_policy_clause(&self) -> Option<FieldProvenancePolicyClause> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(FieldProvenancePolicyClause::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
            .next()
    }

    pub fn provenance_policy(&self) -> Option<SyntaxToken> {
        self.provenance_policy_clause()
            .and_then(|clause| clause.value())
    }

    pub fn items(&self) -> impl Iterator<Item = FieldExpr> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(FieldExpr::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
    }
}

pub struct FieldSubtractExpr(SyntaxNode);
impl AstNode for FieldSubtractExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldSubtractExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldSubtractExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldSubtractExpr {
    pub fn provenance_policy_clause(&self) -> Option<FieldProvenancePolicyClause> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(FieldProvenancePolicyClause::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
            .next()
    }

    pub fn provenance_policy(&self) -> Option<SyntaxToken> {
        self.provenance_policy_clause()
            .and_then(|clause| clause.value())
    }

    pub fn lhs(&self) -> Option<FieldExpr> {
        self.items().next()
    }

    pub fn rhs(&self) -> Option<FieldExpr> {
        self.items().nth(1)
    }

    pub fn items(&self) -> impl Iterator<Item = FieldExpr> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(FieldExpr::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
    }
}

pub struct FieldProvenancePolicyClause(SyntaxNode);
impl AstNode for FieldProvenancePolicyClause {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldProvenancePolicyClause
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldProvenancePolicyClause(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldProvenancePolicyClause {
    pub fn value(&self) -> Option<SyntaxToken> {
        self.0.children_with_tokens().find_map(|element| {
            let token = element.into_token()?;
            if token.kind() == SyntaxKind::Ident {
                Some(token)
            } else {
                None
            }
        })
    }
}

pub struct FieldSmoothingClause(SyntaxNode);
impl AstNode for FieldSmoothingClause {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldSmoothingClause
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldSmoothingClause(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldSmoothingClause {
    pub fn value(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

macro_rules! impl_field_wrapped_expr {
    ($name:ident, $kind:expr, $keyword_method:ident) => {
        pub struct $name(SyntaxNode);
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some($name(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $name {
            pub fn $keyword_method(&self) -> Option<Expr> {
                self.0.children().filter_map(Expr::cast).next()
            }

            pub fn body(&self) -> Option<FieldExpr> {
                self.0
                    .children()
                    .filter_map(Block::cast)
                    .next()
                    .and_then(|block| block.0.children().filter_map(FieldExpr::cast).next())
            }
        }
    };
}

macro_rules! impl_field_smooth_expr {
    ($name:ident, $kind:expr) => {
        pub struct $name(SyntaxNode);
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some($name(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $name {
            fn block(&self) -> Option<Block> {
                self.0.children().filter_map(Block::cast).next()
            }

            pub fn smoothing_clause(&self) -> Option<FieldSmoothingClause> {
                self.block().and_then(|block| {
                    block
                        .0
                        .children()
                        .filter_map(FieldSmoothingClause::cast)
                        .next()
                })
            }

            pub fn smoothing(&self) -> Option<Expr> {
                self.smoothing_clause().and_then(|clause| clause.value())
            }

            pub fn items(&self) -> impl Iterator<Item = FieldExpr> {
                self.block().into_iter().flat_map(|b| {
                    b.0.children()
                        .filter_map(FieldExpr::cast)
                        .collect::<Vec<_>>()
                        .into_iter()
                })
            }
        }
    };
}

impl_field_wrapped_expr!(
    FieldTranslateExpr,
    SyntaxKind::FieldTranslateExpr,
    translate
);
impl_field_wrapped_expr!(FieldRotateExpr, SyntaxKind::FieldRotateExpr, rotate);
impl_field_wrapped_expr!(
    FieldUniformScaleExpr,
    SyntaxKind::FieldUniformScaleExpr,
    uniform_scale
);
impl_field_wrapped_expr!(
    FieldAffineTransformExpr,
    SyntaxKind::FieldAffineTransformExpr,
    affine_transform
);
impl_field_wrapped_expr!(FieldWarpExpr, SyntaxKind::FieldWarpExpr, warp);
impl_field_wrapped_expr!(
    FieldRepeatLinearExpr,
    SyntaxKind::FieldRepeatLinearExpr,
    repeat_linear
);
impl_field_wrapped_expr!(
    FieldRepeatGridExpr,
    SyntaxKind::FieldRepeatGridExpr,
    repeat_grid
);
impl_field_wrapped_expr!(
    FieldRadialRepeatExpr,
    SyntaxKind::FieldRadialRepeatExpr,
    radial_repeat
);
impl_field_wrapped_expr!(
    FieldMirrorArrayExpr,
    SyntaxKind::FieldMirrorArrayExpr,
    mirror_array
);
impl_field_wrapped_expr!(
    FieldInstanceArrayExpr,
    SyntaxKind::FieldInstanceArrayExpr,
    instance_array
);
impl_field_wrapped_expr!(FieldBendExpr, SyntaxKind::FieldBendExpr, bend);
impl_field_wrapped_expr!(FieldTwistExpr, SyntaxKind::FieldTwistExpr, twist);
impl_field_wrapped_expr!(FieldTaperExpr, SyntaxKind::FieldTaperExpr, taper);
impl_field_wrapped_expr!(FieldDisplaceExpr, SyntaxKind::FieldDisplaceExpr, displace);
impl_field_wrapped_expr!(FieldExtrudeExpr, SyntaxKind::FieldExtrudeExpr, height);
impl_field_wrapped_expr!(FieldSweepExpr, SyntaxKind::FieldSweepExpr, path);
impl FieldExtrudeExpr {
    pub fn profile(&self) -> Option<ProfileExpr> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .and_then(|block| block.0.children().filter_map(ProfileExpr::cast).next())
    }
}

impl FieldSweepExpr {
    pub fn profile(&self) -> Option<ProfileExpr> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .and_then(|block| block.0.children().filter_map(ProfileExpr::cast).next())
    }
}

pub struct FieldRevolveExpr(SyntaxNode);
impl AstNode for FieldRevolveExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldRevolveExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldRevolveExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldRevolveExpr {
    pub fn profile(&self) -> Option<ProfileExpr> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .and_then(|block| block.0.children().filter_map(ProfileExpr::cast).next())
    }
}

pub struct FieldLoftExpr(SyntaxNode);
impl AstNode for FieldLoftExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldLoftExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldLoftExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldLoftExpr {
    pub fn height(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn from_profile(&self) -> Option<ProfileExpr> {
        self.block()
            .and_then(|block| block.0.children().filter_map(ProfileExpr::cast).next())
    }

    pub fn to_profile(&self) -> Option<ProfileExpr> {
        self.block()
            .and_then(|block| block.0.children().filter_map(ProfileExpr::cast).nth(1))
    }
}
impl_field_smooth_expr!(FieldSmoothUnionExpr, SyntaxKind::FieldSmoothUnionExpr);
impl_field_smooth_expr!(
    FieldSmoothIntersectionExpr,
    SyntaxKind::FieldSmoothIntersectionExpr
);

pub struct FieldSmoothSubtractExpr(SyntaxNode);
impl AstNode for FieldSmoothSubtractExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldSmoothSubtractExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(FieldSmoothSubtractExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldSmoothSubtractExpr {
    fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn smoothing_clause(&self) -> Option<FieldSmoothingClause> {
        self.block().and_then(|block| {
            block
                .0
                .children()
                .filter_map(FieldSmoothingClause::cast)
                .next()
        })
    }

    pub fn smoothing(&self) -> Option<Expr> {
        self.smoothing_clause().and_then(|clause| clause.value())
    }

    pub fn lhs(&self) -> Option<FieldExpr> {
        self.items().next()
    }

    pub fn rhs(&self) -> Option<FieldExpr> {
        self.items().nth(1)
    }

    pub fn items(&self) -> impl Iterator<Item = FieldExpr> {
        self.block().into_iter().flat_map(|b| {
            b.0.children()
                .filter_map(FieldExpr::cast)
                .collect::<Vec<_>>()
                .into_iter()
        })
    }
}

pub struct MaterialDecl(SyntaxNode);
impl AstNode for MaterialDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MaterialDecl
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(MaterialDecl(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl MaterialDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .nth(1)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::ParamList)
            .flat_map(|node| node.children())
            .filter_map(Param::cast)
    }

    pub fn ret_type(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }

    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(Stmt::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
    }

    pub fn implicit_return_expr(&self) -> Option<StmtExpr> {
        let mut last = None;
        for stmt in self.statements() {
            last = Some(stmt);
        }
        match last {
            Some(Stmt::Expr(expr)) => Some(expr),
            _ => None,
        }
    }
}

pub struct VolumeDecl(SyntaxNode);
impl AstNode for VolumeDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::VolumeDecl
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(VolumeDecl(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl VolumeDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .nth(2)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::ParamList)
            .flat_map(|node| node.children())
            .filter_map(Param::cast)
    }

    pub fn ret_type(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }

    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(Stmt::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
    }

    pub fn implicit_return_expr(&self) -> Option<StmtExpr> {
        let mut last = None;
        for stmt in self.statements() {
            last = Some(stmt);
        }
        match last {
            Some(Stmt::Expr(expr)) => Some(expr),
            _ => None,
        }
    }
}

pub enum ShapeExpr {
    Use(ShapeUseExpr),
    Union(ShapeUnionExpr),
    Intersection(ShapeIntersectionExpr),
    Subtract(ShapeSubtractExpr),
    Leaf(ShapeLeafExpr),
}

impl AstNode for ShapeExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::ShapeUseExpr
                | SyntaxKind::ShapeUnionExpr
                | SyntaxKind::ShapeIntersectionExpr
                | SyntaxKind::ShapeSubtractExpr
                | SyntaxKind::ShapeLeafExpr
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::ShapeUseExpr => ShapeUseExpr::cast(node).map(ShapeExpr::Use),
            SyntaxKind::ShapeUnionExpr => ShapeUnionExpr::cast(node).map(ShapeExpr::Union),
            SyntaxKind::ShapeIntersectionExpr => {
                ShapeIntersectionExpr::cast(node).map(ShapeExpr::Intersection)
            }
            SyntaxKind::ShapeSubtractExpr => ShapeSubtractExpr::cast(node).map(ShapeExpr::Subtract),
            SyntaxKind::ShapeLeafExpr => ShapeLeafExpr::cast(node).map(ShapeExpr::Leaf),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            ShapeExpr::Use(it) => it.syntax(),
            ShapeExpr::Union(it) => it.syntax(),
            ShapeExpr::Intersection(it) => it.syntax(),
            ShapeExpr::Subtract(it) => it.syntax(),
            ShapeExpr::Leaf(it) => it.syntax(),
        }
    }
}

pub struct ShapeDecl(SyntaxNode);
impl AstNode for ShapeDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ShapeDecl
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ShapeDecl(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ShapeDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .nth(1)
    }

    pub fn semantic_expr(&self) -> Option<ShapeExpr> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .and_then(|block| block.0.children().filter_map(ShapeExpr::cast).next())
    }
}

pub struct ShapeUseExpr(SyntaxNode);
impl AstNode for ShapeUseExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ShapeUseExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ShapeUseExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ShapeUseExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }
}

pub struct ShapeUnionExpr(SyntaxNode);
impl AstNode for ShapeUnionExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ShapeUnionExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ShapeUnionExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ShapeUnionExpr {
    fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn provenance_policy_clause(&self) -> Option<ShapeProvenancePolicyClause> {
        self.block().and_then(|block| {
            block
                .0
                .children()
                .filter_map(ShapeProvenancePolicyClause::cast)
                .next()
        })
    }

    pub fn provenance_policy(&self) -> Option<SyntaxToken> {
        self.provenance_policy_clause()
            .and_then(|clause| clause.value())
    }

    pub fn items(&self) -> impl Iterator<Item = ShapeExpr> {
        self.block().into_iter().flat_map(|b| {
            b.0.children()
                .filter_map(ShapeExpr::cast)
                .collect::<Vec<_>>()
                .into_iter()
        })
    }
}

pub struct ShapeIntersectionExpr(SyntaxNode);
impl AstNode for ShapeIntersectionExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ShapeIntersectionExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ShapeIntersectionExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ShapeIntersectionExpr {
    fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn provenance_policy_clause(&self) -> Option<ShapeProvenancePolicyClause> {
        self.block().and_then(|block| {
            block
                .0
                .children()
                .filter_map(ShapeProvenancePolicyClause::cast)
                .next()
        })
    }

    pub fn provenance_policy(&self) -> Option<SyntaxToken> {
        self.provenance_policy_clause()
            .and_then(|clause| clause.value())
    }

    pub fn items(&self) -> impl Iterator<Item = ShapeExpr> {
        self.block().into_iter().flat_map(|b| {
            b.0.children()
                .filter_map(ShapeExpr::cast)
                .collect::<Vec<_>>()
                .into_iter()
        })
    }
}

pub struct ShapeSubtractExpr(SyntaxNode);
impl AstNode for ShapeSubtractExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ShapeSubtractExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ShapeSubtractExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ShapeSubtractExpr {
    fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn provenance_policy_clause(&self) -> Option<ShapeProvenancePolicyClause> {
        self.block().and_then(|block| {
            block
                .0
                .children()
                .filter_map(ShapeProvenancePolicyClause::cast)
                .next()
        })
    }

    pub fn provenance_policy(&self) -> Option<SyntaxToken> {
        self.provenance_policy_clause()
            .and_then(|clause| clause.value())
    }

    pub fn lhs(&self) -> Option<ShapeExpr> {
        self.items().next()
    }

    pub fn rhs(&self) -> Option<ShapeExpr> {
        self.items().nth(1)
    }

    pub fn items(&self) -> impl Iterator<Item = ShapeExpr> {
        self.block().into_iter().flat_map(|b| {
            b.0.children()
                .filter_map(ShapeExpr::cast)
                .collect::<Vec<_>>()
                .into_iter()
        })
    }
}

pub struct ShapeProvenancePolicyClause(SyntaxNode);
impl AstNode for ShapeProvenancePolicyClause {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ShapeProvenancePolicyClause
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ShapeProvenancePolicyClause(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ShapeProvenancePolicyClause {
    pub fn value(&self) -> Option<SyntaxToken> {
        declaration_clause_value_token(&self.0)
    }
}

pub struct ShapeLeafExpr(SyntaxNode);
impl AstNode for ShapeLeafExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ShapeLeafExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ShapeLeafExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ShapeLeafExpr {
    fn bindings(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn field(&self) -> Option<ShapeFieldBinding> {
        self.bindings().and_then(|block| {
            block
                .0
                .children()
                .filter_map(ShapeFieldBinding::cast)
                .next()
        })
    }

    pub fn material(&self) -> Option<ShapeMaterialBinding> {
        self.bindings().and_then(|block| {
            block
                .0
                .children()
                .filter_map(ShapeMaterialBinding::cast)
                .next()
        })
    }

    pub fn radiance(&self) -> Option<ShapeRadianceBinding> {
        self.bindings().and_then(|block| {
            block
                .0
                .children()
                .filter_map(ShapeRadianceBinding::cast)
                .next()
        })
    }

    pub fn volume(&self) -> Option<ShapeVolumeBinding> {
        self.bindings().and_then(|block| {
            block
                .0
                .children()
                .filter_map(ShapeVolumeBinding::cast)
                .next()
        })
    }

    pub fn payload(&self) -> Option<ShapePayloadBinding> {
        self.bindings().and_then(|block| {
            block
                .0
                .children()
                .filter_map(ShapePayloadBinding::cast)
                .next()
        })
    }
}

macro_rules! impl_shape_binding {
    ($name:ident, $kind:expr) => {
        pub struct $name(SyntaxNode);
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some($name(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $name {
            pub fn value(&self) -> Option<Expr> {
                self.0.children().filter_map(Expr::cast).next()
            }
        }
    };
}

impl_shape_binding!(ShapeFieldBinding, SyntaxKind::ShapeFieldBinding);
impl_shape_binding!(ShapeMaterialBinding, SyntaxKind::ShapeMaterialBinding);
impl_shape_binding!(ShapeRadianceBinding, SyntaxKind::ShapeRadianceBinding);
impl_shape_binding!(ShapeVolumeBinding, SyntaxKind::ShapeVolumeBinding);
impl_shape_binding!(ShapePayloadBinding, SyntaxKind::ShapePayloadBinding);

macro_rules! impl_profiled_spec_def {
    (
        $def_name:ident,
        $def_kind:expr,
        $id_clause_name:ident,
        $id_clause_kind:expr,
        $profile_clause_name:ident,
        $profile_clause_kind:expr
    ) => {
        pub struct $def_name(SyntaxNode);
        impl AstNode for $def_name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $def_kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some($def_name(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $def_name {
            pub fn name(&self) -> Option<SyntaxToken> {
                self.0
                    .children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .find(|it| it.kind() == SyntaxKind::Ident)
            }

            pub fn id_clause(&self) -> Option<$id_clause_name> {
                self.0.children().filter_map($id_clause_name::cast).next()
            }

            pub fn profile_clause(&self) -> Option<$profile_clause_name> {
                self.0
                    .children()
                    .filter_map($profile_clause_name::cast)
                    .next()
            }
        }

        pub struct $id_clause_name(SyntaxNode);
        impl AstNode for $id_clause_name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $id_clause_kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some($id_clause_name(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $id_clause_name {
            pub fn value(&self) -> Option<SyntaxToken> {
                declaration_clause_value_token(&self.0)
            }
        }

        pub struct $profile_clause_name(SyntaxNode);
        impl AstNode for $profile_clause_name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $profile_clause_kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some($profile_clause_name(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }

        impl $profile_clause_name {
            pub fn value(&self) -> Option<SyntaxToken> {
                declaration_clause_value_token(&self.0)
            }
        }
    };
}

impl_profiled_spec_def!(
    StyleProfileDef,
    SyntaxKind::StyleProfileDef,
    StyleProfileIdClause,
    SyntaxKind::StyleProfileIdClause,
    StyleProfileProfileClause,
    SyntaxKind::StyleProfileProfileClause
);
impl_profiled_spec_def!(
    GeneratorProfileDef,
    SyntaxKind::GeneratorProfileDef,
    GeneratorProfileIdClause,
    SyntaxKind::GeneratorProfileIdClause,
    GeneratorProfileProfileClause,
    SyntaxKind::GeneratorProfileProfileClause
);
impl_profiled_spec_def!(
    QualityProfileDef,
    SyntaxKind::QualityProfileDef,
    QualityProfileIdClause,
    SyntaxKind::QualityProfileIdClause,
    QualityProfileProfileClause,
    SyntaxKind::QualityProfileProfileClause
);
impl_profiled_spec_def!(
    ProvenancePolicyDef,
    SyntaxKind::ProvenancePolicyDef,
    ProvenancePolicyIdClause,
    SyntaxKind::ProvenancePolicyIdClause,
    ProvenancePolicyProfileClause,
    SyntaxKind::ProvenancePolicyProfileClause
);

fn declaration_clause_value_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    let mut tokens = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .filter(|token| !token.kind().is_trivia());
    let _ = tokens.next();
    tokens.find(|token| is_declaration_clause_value_kind(token.kind()))
}

fn is_attribute_arg_value_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Ident
            | SyntaxKind::StringLiteral
            | SyntaxKind::IntNumber
            | SyntaxKind::FloatNumber
            | SyntaxKind::TrueKw
            | SyntaxKind::FalseKw
            | SyntaxKind::PresetKw
            | SyntaxKind::ProfileKw
            | SyntaxKind::OverridesKw
    )
}

fn is_declaration_clause_value_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Ident
            | SyntaxKind::StringLiteral
            | SyntaxKind::IntNumber
            | SyntaxKind::FloatNumber
            | SyntaxKind::TrueKw
            | SyntaxKind::FalseKw
            | SyntaxKind::NothingKw
            | SyntaxKind::PresetKw
            | SyntaxKind::ProfileKw
            | SyntaxKind::OverridesKw
    )
}

pub struct VarAssign(SyntaxNode);
impl AstNode for VarAssign {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::VarAssign
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(VarAssign(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl VarAssign {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn value(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct IfStmt(SyntaxNode);
impl AstNode for IfStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IfStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(IfStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl IfStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn then_block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn else_block(&self) -> Option<Block> {
        let mut blocks = self.0.children().filter_map(Block::cast);
        blocks.next();
        blocks.next()
    }

    pub fn else_if(&self) -> Option<IfStmt> {
        self.0.children().filter_map(IfStmt::cast).next()
    }
}

pub struct WhileStmt(SyntaxNode);
impl AstNode for WhileStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WhileStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(WhileStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl WhileStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn body(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }
}

pub struct ForStmt(SyntaxNode);
impl AstNode for ForStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ForStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ForStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ForStmt {
    fn all_tokens(&self) -> Vec<SyntaxToken> {
        self.0
            .descendants_with_tokens()
            .filter_map(|it| it.into_token())
            .collect()
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        self.value_name()
    }

    pub fn value_name(&self) -> Option<SyntaxToken> {
        let idents = self.binding_idents();
        if self.is_map_binding_form() {
            idents.get(1).cloned()
        } else {
            idents.first().cloned()
        }
    }

    pub fn key_name(&self) -> Option<SyntaxToken> {
        if !self.is_map_binding_form() {
            return None;
        }
        self.binding_idents().into_iter().next()
    }

    pub fn index_name(&self) -> Option<SyntaxToken> {
        let mut saw_with = false;
        let mut saw_index = false;
        for token in self.all_tokens() {
            if token.kind() != SyntaxKind::Ident {
                continue;
            }
            if saw_with && saw_index {
                return Some(token);
            }
            if saw_with && token.text() == "index" {
                saw_index = true;
                continue;
            }
            if token.text() == "with" {
                saw_with = true;
                saw_index = false;
            }
        }
        None
    }

    fn is_map_binding_form(&self) -> bool {
        self.header_tokens_before_in()
            .into_iter()
            .any(|it| it.kind() == SyntaxKind::Comma)
    }

    fn header_tokens_before_in(&self) -> Vec<SyntaxToken> {
        let mut out = Vec::new();
        for token in self.all_tokens() {
            if token.kind() == SyntaxKind::InKw {
                break;
            }
            out.push(token);
        }
        out
    }

    fn binding_idents(&self) -> Vec<SyntaxToken> {
        self.header_tokens_before_in()
            .into_iter()
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .collect()
    }

    pub fn iterable(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn body(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }
}

pub struct ReturnStmt(SyntaxNode);
impl AstNode for ReturnStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ReturnStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ReturnStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ReturnStmt {
    pub fn value(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct BreakStmt(SyntaxNode);
impl AstNode for BreakStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BreakStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(BreakStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

pub struct ContinueStmt(SyntaxNode);
impl AstNode for ContinueStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ContinueStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ContinueStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

pub struct MatchStmt(SyntaxNode);
impl AstNode for MatchStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MatchStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(MatchStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl MatchStmt {
    pub fn subject(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn cases(&self) -> impl Iterator<Item = MatchCaseItem> {
        self.0.children().filter_map(MatchCaseItem::cast)
    }
}

pub enum MatchCaseItem {
    Case(MatchCase),
    Otherwise(OtherwiseCase),
}

impl AstNode for MatchCaseItem {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::MatchCase | SyntaxKind::OtherwiseCase)
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::MatchCase => MatchCase::cast(node).map(MatchCaseItem::Case),
            SyntaxKind::OtherwiseCase => OtherwiseCase::cast(node).map(MatchCaseItem::Otherwise),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            MatchCaseItem::Case(it) => it.syntax(),
            MatchCaseItem::Otherwise(it) => it.syntax(),
        }
    }
}

pub struct MatchCase(SyntaxNode);
impl AstNode for MatchCase {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MatchCase
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(MatchCase(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl MatchCase {
    pub fn labels(&self) -> impl Iterator<Item = Pattern> {
        self.0.children().filter_map(Pattern::cast)
    }

    pub fn guard(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn statement(&self) -> Option<Stmt> {
        self.0.children().filter_map(Stmt::cast).next()
    }
}

pub struct Pattern(SyntaxNode);
impl AstNode for Pattern {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Pattern
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Pattern(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl Pattern {
    pub fn name_tokens(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn literals(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| {
                matches!(
                    it.kind(),
                    SyntaxKind::StringLiteral
                        | SyntaxKind::IntNumber
                        | SyntaxKind::FloatNumber
                        | SyntaxKind::TrueKw
                        | SyntaxKind::FalseKw
                        | SyntaxKind::NothingKw
                )
            })
    }

    pub fn args(&self) -> impl Iterator<Item = Pattern> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::PatternArgList)
            .flat_map(|node| node.children())
            .filter_map(Pattern::cast)
    }

    pub fn fields(&self) -> impl Iterator<Item = PatternField> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::PatternFieldList)
            .flat_map(|node| node.children())
            .filter_map(PatternField::cast)
    }
}

pub struct PatternField(SyntaxNode);
impl AstNode for PatternField {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PatternField
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(PatternField(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl PatternField {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn pattern(&self) -> Option<Pattern> {
        self.0.children().filter_map(Pattern::cast).next()
    }
}

pub struct OtherwiseCase(SyntaxNode);
impl AstNode for OtherwiseCase {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::OtherwiseCase
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(OtherwiseCase(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl OtherwiseCase {
    pub fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn statement(&self) -> Option<Stmt> {
        self.0.children().filter_map(Stmt::cast).next()
    }
}

pub struct UseStmt(SyntaxNode);
impl AstNode for UseStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::UseStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(UseStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl UseStmt {
    pub fn names(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
    }
}

pub struct Block(SyntaxNode);
impl AstNode for Block {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Block
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Block(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl Block {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }

    pub fn region_items(&self) -> impl Iterator<Item = RegionItem> {
        self.0.children().filter_map(RegionItem::cast)
    }

    pub fn trailing_stmt_expr(&self) -> Option<StmtExpr> {
        let mut last = None;
        for stmt in self.statements() {
            last = Some(stmt);
        }
        match last {
            Some(Stmt::Expr(expr)) => Some(expr),
            _ => None,
        }
    }
}

pub enum Expr {
    Literal(LiteralExpr),
    Ident(IdentExpr),
    Capture(CaptureExpr),
    TypeApply(TypeApplyExpr),
    Index(IndexExpr),
    Prefix(PrefixExpr),
    Try(TryExpr),
    Bin(BinExpr),
    Call(CallExpr),
    Member(MemberExpr),
    Paren(ParenExpr),
    List(ListExpr),
    Map(MapExpr),
    StringInterp(StringInterp),
    Crash(CrashExpr),
}

impl AstNode for Expr {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::LiteralExpr
                | SyntaxKind::IdentExpr
                | SyntaxKind::CaptureExpr
                | SyntaxKind::TypeApplyExpr
                | SyntaxKind::IndexExpr
                | SyntaxKind::PrefixExpr
                | SyntaxKind::TryExpr
                | SyntaxKind::BinExpr
                | SyntaxKind::CallExpr
                | SyntaxKind::MemberExpr
                | SyntaxKind::ParenExpr
                | SyntaxKind::ListExpr
                | SyntaxKind::MapExpr
                | SyntaxKind::StringInterp
                | SyntaxKind::CrashExpr
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::LiteralExpr => LiteralExpr::cast(node).map(Expr::Literal),
            SyntaxKind::IdentExpr => IdentExpr::cast(node).map(Expr::Ident),
            SyntaxKind::CaptureExpr => CaptureExpr::cast(node).map(Expr::Capture),
            SyntaxKind::TypeApplyExpr => TypeApplyExpr::cast(node).map(Expr::TypeApply),
            SyntaxKind::IndexExpr => IndexExpr::cast(node).map(Expr::Index),
            SyntaxKind::PrefixExpr => PrefixExpr::cast(node).map(Expr::Prefix),
            SyntaxKind::TryExpr => TryExpr::cast(node).map(Expr::Try),
            SyntaxKind::BinExpr => BinExpr::cast(node).map(Expr::Bin),
            SyntaxKind::CallExpr => CallExpr::cast(node).map(Expr::Call),
            SyntaxKind::MemberExpr => MemberExpr::cast(node).map(Expr::Member),
            SyntaxKind::ParenExpr => ParenExpr::cast(node).map(Expr::Paren),
            SyntaxKind::ListExpr => ListExpr::cast(node).map(Expr::List),
            SyntaxKind::MapExpr => MapExpr::cast(node).map(Expr::Map),
            SyntaxKind::StringInterp => StringInterp::cast(node).map(Expr::StringInterp),
            SyntaxKind::CrashExpr => CrashExpr::cast(node).map(Expr::Crash),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Literal(it) => it.syntax(),
            Expr::Ident(it) => it.syntax(),
            Expr::Capture(it) => it.syntax(),
            Expr::TypeApply(it) => it.syntax(),
            Expr::Index(it) => it.syntax(),
            Expr::Prefix(it) => it.syntax(),
            Expr::Try(it) => it.syntax(),
            Expr::Bin(it) => it.syntax(),
            Expr::Call(it) => it.syntax(),
            Expr::Member(it) => it.syntax(),
            Expr::Paren(it) => it.syntax(),
            Expr::List(it) => it.syntax(),
            Expr::Map(it) => it.syntax(),
            Expr::StringInterp(it) => it.syntax(),
            Expr::Crash(it) => it.syntax(),
        }
    }
}

impl Expr {
    pub fn is_self(&self) -> bool {
        matches!(self, Expr::Ident(ident) if ident.is_self())
    }
}

pub struct LiteralExpr(SyntaxNode);
impl AstNode for LiteralExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(LiteralExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

pub struct IdentExpr(SyntaxNode);
impl AstNode for IdentExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IdentExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(IdentExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl IdentExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| matches!(it.kind(), SyntaxKind::Ident | SyntaxKind::SelfKw))
    }

    pub fn is_self(&self) -> bool {
        self.name().is_some_and(|it| it.text() == "self")
    }
}

pub struct CaptureExpr(SyntaxNode);
impl AstNode for CaptureExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CaptureExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(CaptureExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl CaptureExpr {
    pub fn target(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct TypeApplyExpr(SyntaxNode);
impl AstNode for TypeApplyExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TypeApplyExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(TypeApplyExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl TypeApplyExpr {
    pub fn callee(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn args(&self) -> Vec<TypeRef> {
        self.0
            .children()
            .filter_map(TypeArgList::cast)
            .flat_map(|list| list.args())
            .collect()
    }
}

pub struct IndexExpr(SyntaxNode);
impl AstNode for IndexExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IndexExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(IndexExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl IndexExpr {
    pub fn object(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn index(&self) -> Option<Expr> {
        let mut exprs = self.0.children().filter_map(Expr::cast);
        exprs.next();
        exprs.next()
    }
}

pub struct PrefixExpr(SyntaxNode);
impl AstNode for PrefixExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PrefixExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(PrefixExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl PrefixExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct TryExpr(SyntaxNode);
impl AstNode for TryExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TryExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(TryExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl TryExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct BinExpr(SyntaxNode);
impl AstNode for BinExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BinExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(BinExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        let mut exprs = self.0.children().filter_map(Expr::cast);
        exprs.next()
    }

    pub fn rhs(&self) -> Option<Expr> {
        let mut exprs = self.0.children().filter_map(Expr::cast);
        exprs.next();
        exprs.next()
    }
}

pub struct CallExpr(SyntaxNode);
impl AstNode for CallExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CallExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(CallExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl CallExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        match self.callee()? {
            Expr::Ident(ident) => ident.name(),
            _ => None,
        }
    }

    pub fn callee(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn args(&self) -> impl Iterator<Item = Arg> {
        let mut seen_callee = false;
        self.0.children().filter_map(move |node| {
            if !seen_callee && Expr::can_cast(node.kind()) {
                seen_callee = true;
                return None;
            }
            if let Some(named) = NamedArg::cast(node.clone()) {
                return Some(Arg::Named(named));
            }
            Expr::cast(node).map(Arg::Positional)
        })
    }
}

pub enum Arg {
    Positional(Expr),
    Named(NamedArg),
}

pub struct NamedArg(SyntaxNode);
impl AstNode for NamedArg {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NamedArg
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(NamedArg(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl NamedArg {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| is_name_like_label_token(it.kind()))
    }

    pub fn value(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct MemberExpr(SyntaxNode);
impl AstNode for MemberExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MemberExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(MemberExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl MemberExpr {
    pub fn object(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .last()
    }
}

pub struct ParenExpr(SyntaxNode);
impl AstNode for ParenExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ParenExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ParenExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

pub struct ListExpr(SyntaxNode);
impl AstNode for ListExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ListExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ListExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ListExpr {
    pub fn items(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

pub struct MapExpr(SyntaxNode);
impl AstNode for MapExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MapExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(MapExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl MapExpr {
    pub fn items(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

pub struct StringInterp(SyntaxNode);
impl AstNode for StringInterp {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StringInterp
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(StringInterp(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

pub struct CrashExpr(SyntaxNode);
impl AstNode for CrashExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CrashExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(CrashExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl CrashExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }
}

pub struct TypeRef(SyntaxNode);
impl AstNode for TypeRef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TypeRef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl TypeRef {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| matches!(it.kind(), SyntaxKind::Ident | SyntaxKind::IntNumber))
    }

    pub fn args(&self) -> Vec<TypeRef> {
        self.0
            .children()
            .filter_map(TypeArgList::cast)
            .flat_map(|list| list.args())
            .collect()
    }
}

pub struct TypeArgList(SyntaxNode);
impl AstNode for TypeArgList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TypeArgList
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl TypeArgList {
    pub fn args(&self) -> Vec<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).collect()
    }
}

pub struct Param(SyntaxNode);
impl AstNode for Param {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Param
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl Param {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn ty(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }
}

pub struct ParamList(SyntaxNode);
impl AstNode for ParamList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ParamList
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ParamList {
    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0.children().filter_map(Param::cast)
    }
}

pub struct TypeParamList(SyntaxNode);
impl AstNode for TypeParamList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TypeParamList
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(TypeParamList(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl TypeParamList {
    pub fn params(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
    }
}

pub struct FieldDef(SyntaxNode);
impl AstNode for FieldDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldDef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl FieldDef {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn ty(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }

    pub fn default_expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn is_mutable(&self) -> bool {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .any(|it| it.kind() == SyntaxKind::MutableKw)
    }
}

pub struct MethodDef(SyntaxNode);
impl AstNode for MethodDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MethodDef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl MethodDef {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::ParamList)
            .flat_map(|node| node.children())
            .filter_map(Param::cast)
    }

    pub fn ret_type(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }

    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0
            .children()
            .filter_map(Block::cast)
            .next()
            .into_iter()
            .flat_map(|b| {
                b.0.children()
                    .filter_map(Stmt::cast)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
    }

    pub fn implicit_return_expr(&self) -> Option<StmtExpr> {
        let mut last = None;
        for stmt in self.statements() {
            last = Some(stmt);
        }
        match last {
            Some(Stmt::Expr(expr)) => Some(expr),
            _ => None,
        }
    }
}

pub struct MustMethodDef(SyntaxNode);
impl AstNode for MustMethodDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MustMethodDef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(MustMethodDef(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl MustMethodDef {
    pub fn is_check(&self) -> bool {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .any(|it| it.kind() == SyntaxKind::CheckKw)
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.0
            .children()
            .filter(|it| it.kind() == SyntaxKind::ParamList)
            .flat_map(|node| node.children())
            .filter_map(Param::cast)
    }

    pub fn ret_type(&self) -> Option<TypeRef> {
        self.0.children().filter_map(TypeRef::cast).next()
    }
}

pub struct IsAClause(SyntaxNode);
impl AstNode for IsAClause {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IsAClause
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(IsAClause(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl IsAClause {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|it| it.kind() == SyntaxKind::Ident)
            .last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    #[test]
    fn root_statements_capture_top_level_functions() {
        let source = r#"
fn run() -> Integer {
    return 1
}

fn gate() -> Boolean {
    return true
}

value = 7
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let mut kinds = Vec::new();
        let mut names = Vec::new();
        for stmt in root.statements() {
            match stmt {
                Stmt::FuncDef(func) => {
                    kinds.push("func");
                    names.push(func.name().expect("func name").text().to_string());
                }
                _ => {}
            }
        }
        assert_eq!(kinds, vec!["func", "func"]);
        assert_eq!(names, vec!["run".to_string(), "gate".to_string()]);
    }

    #[test]
    fn private_block_exposes_nested_statements() {
        let source = r#"
private {
    fn helper() -> Integer {
        return 1
    }
    fn helper_gate() -> Boolean {
        return true
    }
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let private = root
            .statements()
            .find_map(|stmt| match stmt {
                Stmt::PrivateBlock(block) => Some(block),
                _ => None,
            })
            .expect("private block");
        let mut names = Vec::new();
        for stmt in private.statements() {
            match stmt {
                Stmt::FuncDef(func) => {
                    names.push(func.name().expect("func name").text().to_string())
                }
                _ => {}
            }
        }
        assert_eq!(names, vec!["helper".to_string(), "helper_gate".to_string()]);
    }

    #[test]
    fn root_statements_capture_top_level_kernel_definitions() {
        let source = r#"
kernel fn shade() -> Nothing {
    return nothing
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let kernel = root
            .statements()
            .find_map(|stmt| match stmt {
                Stmt::KernelDef(kernel) => Some(kernel),
                _ => None,
            })
            .expect("kernel definition");
        assert_eq!(kernel.name().expect("kernel name").text(), "shade");
    }

    #[test]
    fn private_block_exposes_nested_kernel_statements() {
        let source = r#"
private {
    kernel fn shade() -> Nothing {
        return nothing
    }
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let private = root
            .statements()
            .find_map(|stmt| match stmt {
                Stmt::PrivateBlock(block) => Some(block),
                _ => None,
            })
            .expect("private block");
        let kernel = private
            .statements()
            .find_map(|stmt| match stmt {
                Stmt::KernelDef(kernel) => Some(kernel),
                _ => None,
            })
            .expect("kernel definition");
        assert_eq!(kernel.name().expect("kernel name").text(), "shade");
    }

    #[test]
    fn capture_expr_parses_as_expression() {
        let source = r#"
fn run() -> Nothing {
    scene = capture world
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let func = root
            .statements()
            .find_map(|stmt| match stmt {
                Stmt::FuncDef(func) => Some(func),
                _ => None,
            })
            .expect("function");
        let assign = func
            .statements()
            .find_map(|stmt| match stmt {
                Stmt::VarAssign(assign) => Some(assign),
                _ => None,
            })
            .expect("assignment");
        let value = assign.value().expect("assignment value");
        let capture = match value {
            Expr::Capture(capture) => capture,
            _ => panic!("expected capture expression"),
        };
        let op = capture
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::CaptureKw)
            .expect("capture keyword");
        assert_eq!(op.text(), "capture");
        let inner = capture.target().expect("capture target");
        assert!(matches!(inner, Expr::Ident(_)));
    }

    #[test]
    fn implicit_return_and_self_expr_contract() {
        let source = r#"
fn read(self_value: Integer) -> Integer {
    self
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let func = match root.statements().next().expect("statement") {
            Stmt::FuncDef(func) => func,
            _ => panic!("expected function definition"),
        };

        let trailing = func
            .implicit_return_expr()
            .expect("expected implicit return candidate");
        let expr = trailing.expr().expect("expected trailing expression");
        assert!(expr.is_self(), "expected trailing expression to be self");
    }

    #[test]
    fn field_primitive_expr_accessors_expose_name_and_args() {
        let source = r#"
field exact distance sphere(p: Vec3) -> F32 {
    sphere(center = p, radius = 1)
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let field = match root.statements().next().expect("statement") {
            Stmt::FieldDecl(field) => field,
            _ => panic!("expected field declaration"),
        };
        let expr = field.semantic_expr().expect("expected semantic field expr");
        let FieldExpr::Primitive(primitive) = expr else {
            panic!("expected primitive field expression");
        };
        assert_eq!(primitive.name().expect("primitive name").text(), "sphere");
        let args: Vec<_> = primitive.args().collect();
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn field_decl_accessors_expose_support_and_bounds_clauses() {
        let source = r#"
field conservative distance shell(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-2.0, -2.0, -2.0),
        max=vec3(2.0, 2.0, 2.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    sphere(radius=1.0)
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let field = match root.statements().next().expect("statement") {
            Stmt::FieldDecl(field) => field,
            _ => panic!("expected field declaration"),
        };
        assert!(field.support_clause().is_some(), "expected support clause");
        assert!(field.bounds_clause().is_some(), "expected bounds clause");
        assert!(
            field.semantic_expr().is_some(),
            "expected semantic field expression"
        );
    }

    #[test]
    fn field_support_and_bounds_clauses_are_exposed() {
        let source = r#"
field conservative distance scene(p: Vec3) -> F32 {
    support = Support3(bounds = Bounds3(min = vec3(-1.0, -1.0, -1.0), max = vec3(1.0, 1.0, 1.0)))
    bounds = Bounds3(min = vec3(-2.0, -2.0, -2.0), max = vec3(2.0, 2.0, 2.0))
    sphere(radius = 1)
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let field = match root.statements().next().expect("statement") {
            Stmt::FieldDecl(field) => field,
            _ => panic!("expected field declaration"),
        };
        assert!(field.support_clause().is_some(), "expected support clause");
        assert!(field.bounds_clause().is_some(), "expected bounds clause");
        assert_eq!(
            field
                .semantic_expr()
                .and_then(|expr| match expr {
                    FieldExpr::Primitive(primitive) =>
                        primitive.name().map(|tok| tok.text().to_string()),
                    _ => None,
                })
                .as_deref(),
            Some("sphere")
        );
    }

    #[test]
    fn shape_decl_accessors_expose_leaf_and_union() {
        let source = r#"
shape cube_shape {
    field = cube
    material = cube_surface
    payload = Payload(id = 1)
}

shape scene_shape {
    union {
        use cube_shape
        use ground_shape
    }
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let mut statements = root.statements();

        let leaf = match statements.next().expect("first statement") {
            Stmt::ShapeDecl(shape) => shape,
            _ => panic!("expected shape declaration"),
        };
        assert_eq!(leaf.name().expect("shape name").text(), "cube_shape");
        let leaf_expr = match leaf.semantic_expr().expect("leaf expr") {
            ShapeExpr::Leaf(leaf) => leaf,
            _ => panic!("expected leaf shape expression"),
        };
        assert!(leaf_expr.field().is_some(), "expected field binding");
        assert!(leaf_expr.material().is_some(), "expected material binding");
        assert!(leaf_expr.payload().is_some(), "expected payload binding");

        let union = match statements.next().expect("second statement") {
            Stmt::ShapeDecl(shape) => shape,
            _ => panic!("expected shape declaration"),
        };
        assert_eq!(union.name().expect("shape name").text(), "scene_shape");
        let union_expr = match union.semantic_expr().expect("union expr") {
            ShapeExpr::Union(union) => union,
            _ => panic!("expected union shape expression"),
        };
        assert_eq!(union_expr.items().count(), 2);
    }

    #[test]
    fn radiance_and_volume_decls_and_shape_leaf_bindings_are_first_class() {
        let source = r#"
radiance field emit_sky(direction: Vec3, time: F32) -> Vec3 {
    return direction
}

volume field accumulate_fog(p: Vec3, density: F32) -> Medium {
    return Medium(density = density, emission = vec3(0.0, 0.0, 0.0), anisotropy = 0.0)
}

shape scene_shape {
    field = cube
    material = cube_surface
    radiance = emit_sky
    volume = accumulate_fog
    payload = Payload(id = 1)
}
"#;
        let syntax = parser::parse(source);
        let root = Root::cast(syntax).expect("root");
        let mut statements = root.statements();

        let radiance = match statements.next().expect("first statement") {
            Stmt::RadianceDecl(radiance) => radiance,
            _ => panic!("expected radiance declaration"),
        };
        assert_eq!(radiance.name().expect("radiance name").text(), "emit_sky");
        assert!(
            radiance.ret_type().is_some(),
            "expected radiance return type"
        );

        let volume = match statements.next().expect("second statement") {
            Stmt::VolumeDecl(volume) => volume,
            _ => panic!("expected volume declaration"),
        };
        assert_eq!(volume.name().expect("volume name").text(), "accumulate_fog");
        assert!(volume.ret_type().is_some(), "expected volume return type");

        let shape = match statements.next().expect("third statement") {
            Stmt::ShapeDecl(shape) => shape,
            _ => panic!("expected shape declaration"),
        };
        let leaf_expr = match shape.semantic_expr().expect("shape expr") {
            ShapeExpr::Leaf(leaf) => leaf,
            _ => panic!("expected leaf shape expression"),
        };
        assert!(leaf_expr.radiance().is_some(), "expected radiance binding");
        assert!(leaf_expr.volume().is_some(), "expected volume binding");
    }
}
