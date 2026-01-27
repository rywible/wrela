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
    FuncDef(FuncDef),
    VarAssign(VarAssign),
    IfStmt(IfStmt),
    WhileStmt(WhileStmt),
    ForStmt(ForStmt),
    ReturnStmt(ReturnStmt),
    BreakStmt(BreakStmt),
    ContinueStmt(ContinueStmt),
    MatchStmt(MatchStmt),
    UseStmt(UseStmt),
    OptimizeStmt(OptimizeStmt),
}

impl AstNode for Stmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::StmtExpr
                | SyntaxKind::ClassDef
                | SyntaxKind::FuncDef
                | SyntaxKind::VarAssign
                | SyntaxKind::IfStmt
                | SyntaxKind::WhileStmt
                | SyntaxKind::ForStmt
                | SyntaxKind::ReturnStmt
                | SyntaxKind::BreakStmt
                | SyntaxKind::ContinueStmt
                | SyntaxKind::MatchStmt
                | SyntaxKind::UseStmt
                | SyntaxKind::OptimizeStmt
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::StmtExpr => StmtExpr::cast(node).map(Stmt::Expr),
            SyntaxKind::ClassDef => ClassDef::cast(node).map(Stmt::ClassDef),
            SyntaxKind::FuncDef => FuncDef::cast(node).map(Stmt::FuncDef),
            SyntaxKind::VarAssign => VarAssign::cast(node).map(Stmt::VarAssign),
            SyntaxKind::IfStmt => IfStmt::cast(node).map(Stmt::IfStmt),
            SyntaxKind::WhileStmt => WhileStmt::cast(node).map(Stmt::WhileStmt),
            SyntaxKind::ForStmt => ForStmt::cast(node).map(Stmt::ForStmt),
            SyntaxKind::ReturnStmt => ReturnStmt::cast(node).map(Stmt::ReturnStmt),
            SyntaxKind::BreakStmt => BreakStmt::cast(node).map(Stmt::BreakStmt),
            SyntaxKind::ContinueStmt => ContinueStmt::cast(node).map(Stmt::ContinueStmt),
            SyntaxKind::MatchStmt => MatchStmt::cast(node).map(Stmt::MatchStmt),
            SyntaxKind::UseStmt => UseStmt::cast(node).map(Stmt::UseStmt),
            SyntaxKind::OptimizeStmt => OptimizeStmt::cast(node).map(Stmt::OptimizeStmt),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Stmt::Expr(it) => it.syntax(),
            Stmt::ClassDef(it) => it.syntax(),
            Stmt::FuncDef(it) => it.syntax(),
            Stmt::VarAssign(it) => it.syntax(),
            Stmt::IfStmt(it) => it.syntax(),
            Stmt::WhileStmt(it) => it.syntax(),
            Stmt::ForStmt(it) => it.syntax(),
            Stmt::ReturnStmt(it) => it.syntax(),
            Stmt::BreakStmt(it) => it.syntax(),
            Stmt::ContinueStmt(it) => it.syntax(),
            Stmt::MatchStmt(it) => it.syntax(),
            Stmt::UseStmt(it) => it.syntax(),
            Stmt::OptimizeStmt(it) => it.syntax(),
        }
    }
}

pub struct OptimizeStmt(SyntaxNode);
impl AstNode for OptimizeStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::OptimizeStmt
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(OptimizeStmt(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl OptimizeStmt {
    pub fn objective(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }

    pub fn block(&self) -> Option<Block> {
        self.0.children().find_map(Block::cast)
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

    pub fn has_blocks(&self) -> impl Iterator<Item = HasBlock> {
        self.0.children().filter_map(HasBlock::cast)
    }

    pub fn methods(&self) -> impl Iterator<Item = MethodDef> {
        self.0.children().filter_map(MethodDef::cast)
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
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
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
    pub fn labels(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }

    pub fn block(&self) -> Option<Block> {
        self.0.children().filter_map(Block::cast).next()
    }

    pub fn statement(&self) -> Option<Stmt> {
        self.0.children().filter_map(Stmt::cast).next()
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
}

pub enum Expr {
    Literal(LiteralExpr),
    Ident(IdentExpr),
    Prefix(PrefixExpr),
    Bin(BinExpr),
    Call(CallExpr),
    Member(MemberExpr),
    Paren(ParenExpr),
    List(ListExpr),
    Map(MapExpr),
    StringInterp(StringInterp),
    Crash(CrashExpr),
    Its(ItsExpr),
    It(ItExpr),
}

impl AstNode for Expr {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::LiteralExpr
                | SyntaxKind::IdentExpr
                | SyntaxKind::PrefixExpr
                | SyntaxKind::BinExpr
                | SyntaxKind::CallExpr
                | SyntaxKind::MemberExpr
                | SyntaxKind::ParenExpr
                | SyntaxKind::ListExpr
                | SyntaxKind::MapExpr
                | SyntaxKind::StringInterp
                | SyntaxKind::CrashExpr
                | SyntaxKind::ItsExpr
                | SyntaxKind::ItExpr
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::LiteralExpr => LiteralExpr::cast(node).map(Expr::Literal),
            SyntaxKind::IdentExpr => IdentExpr::cast(node).map(Expr::Ident),
            SyntaxKind::PrefixExpr => PrefixExpr::cast(node).map(Expr::Prefix),
            SyntaxKind::BinExpr => BinExpr::cast(node).map(Expr::Bin),
            SyntaxKind::CallExpr => CallExpr::cast(node).map(Expr::Call),
            SyntaxKind::MemberExpr => MemberExpr::cast(node).map(Expr::Member),
            SyntaxKind::ParenExpr => ParenExpr::cast(node).map(Expr::Paren),
            SyntaxKind::ListExpr => ListExpr::cast(node).map(Expr::List),
            SyntaxKind::MapExpr => MapExpr::cast(node).map(Expr::Map),
            SyntaxKind::StringInterp => StringInterp::cast(node).map(Expr::StringInterp),
            SyntaxKind::CrashExpr => CrashExpr::cast(node).map(Expr::Crash),
            SyntaxKind::ItsExpr => ItsExpr::cast(node).map(Expr::Its),
            SyntaxKind::ItExpr => ItExpr::cast(node).map(Expr::It),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Literal(it) => it.syntax(),
            Expr::Ident(it) => it.syntax(),
            Expr::Prefix(it) => it.syntax(),
            Expr::Bin(it) => it.syntax(),
            Expr::Call(it) => it.syntax(),
            Expr::Member(it) => it.syntax(),
            Expr::Paren(it) => it.syntax(),
            Expr::List(it) => it.syntax(),
            Expr::Map(it) => it.syntax(),
            Expr::StringInterp(it) => it.syntax(),
            Expr::Crash(it) => it.syntax(),
            Expr::Its(it) => it.syntax(),
            Expr::It(it) => it.syntax(),
        }
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
            .find(|it| it.kind() == SyntaxKind::Ident)
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

pub struct ItsExpr(SyntaxNode);
impl AstNode for ItsExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ItsExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ItsExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl ItExpr {
    pub fn token(&self) -> SyntaxToken {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::ItKw)
            .unwrap()
    }
}

pub struct ItExpr(SyntaxNode);
impl AstNode for ItExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ItExpr
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(ItExpr(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
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
}

pub struct HasBlock(SyntaxNode);
impl AstNode for HasBlock {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::HasBlock
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

impl HasBlock {
    pub fn fields(&self) -> impl Iterator<Item = FieldDef> {
        self.0.children().filter_map(FieldDef::cast)
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
}
