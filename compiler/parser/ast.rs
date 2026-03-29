use crate::parser::SyntaxNode;
use crate::parser::SyntaxToken;
use crate::parser::kind::SyntaxKind;

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
    EnumDef(EnumDef),
    FuncDef(FuncDef),
    SystemDef(SystemDef),
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
                | SyntaxKind::EnumDef
                | SyntaxKind::FuncDef
                | SyntaxKind::SystemDef
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
            SyntaxKind::EnumDef => EnumDef::cast(node).map(Stmt::EnumDef),
            SyntaxKind::FuncDef => FuncDef::cast(node).map(Stmt::FuncDef),
            SyntaxKind::SystemDef => SystemDef::cast(node).map(Stmt::SystemDef),
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
            Stmt::EnumDef(it) => it.syntax(),
            Stmt::FuncDef(it) => it.syntax(),
            Stmt::SystemDef(it) => it.syntax(),
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
            }
        }
        AssertMode::Value
    }

    pub fn expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
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
        self.0
            .children()
            .filter(|node| node.kind() == SyntaxKind::Block)
            .flat_map(|block| block.children().filter_map(Stmt::cast))
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
            .find(|it| it.kind() == SyntaxKind::Ident)
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
            .find(|it| it.kind() == SyntaxKind::Ident)
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
}
