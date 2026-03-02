use crate::hir::arena::{Arena, Idx};
use crate::hir::body::{Body, Literal, UseName};
use miette::SourceSpan;
use rowan::TextRange;
use smol_str::SmolStr;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub functions: Arena<Function>,
    pub classes: Arena<Class>,
    pub enums: Arena<Enum>,
    pub interfaces: Arena<Interface>,
    pub uses: Vec<UseStmt>,
    pub material_declarations: Vec<MaterialDeclarationSurface>,
    pub render_contracts: Vec<RenderContract>,
    pub gpu_functions: Vec<GpuFunctionSurface>,
    pub asset_specs: Vec<AssetSpecIRV2>,
    pub style_profiles: Vec<StyleProfileIRV1>,
    pub generator_plans: Vec<GeneratorPlanIRV1>,
    pub asset_build_graphs: Vec<AssetBuildGraphIRV2>,
    pub provenance_ledgers: Vec<ProvenanceLedgerIRV1>,
    pub quality_gates: Vec<QualityGateIRV2>,
    pub shader_functions: Vec<ShaderFunction>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShaderFunction {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeRef>,
    pub body: Option<Body>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOrigin {
    pub path: PathBuf,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Function,
    Method,
    Derived,
    Check,
    CheckMethod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParam {
    pub name: SmolStr,
    pub bounds: Vec<SmolStr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassRole {
    Class,
    Node,
    Component,
    Resource,
    Event,
    Scene,
    Theme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionRole {
    Function,
    System,
    View,
    Widget,
    Anim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeProfile {
    Ui,
    World,
    Canvas,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemMetadata {
    pub stage: Option<SmolStr>,
    pub reads: Vec<SmolStr>,
    pub writes: Vec<SmolStr>,
    pub before: Vec<SmolStr>,
    pub after: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeArg {
    pub key: SmolStr,
    pub key_span: Option<TextRange>,
    pub value: SmolStr,
    pub value_span: Option<TextRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeAnnotation {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub args: Vec<AttributeArg>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub attributes: Vec<AttributeAnnotation>,
    pub visibility: Visibility,
    pub kind: FunctionKind,
    pub role: FunctionRole,
    pub system_metadata: Option<SystemMetadata>,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeRef>,
    pub body: Option<Body>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialDeclarationSurface {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub surface_model: Option<MaterialClauseValueSurface>,
    pub preset: Option<MaterialClauseValueSurface>,
    pub textures: Vec<MaterialTextureSurface>,
    pub params: Vec<MaterialParamSurface>,
    pub features: Vec<MaterialFeatureSurface>,
    pub semantic_bindings: Vec<MaterialSemanticBindingSurface>,
    pub semantics: MaterialSemanticSurface,
    pub render: MaterialRenderSurface,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialClauseValueSurface {
    pub value: SmolStr,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialTextureSurface {
    pub slot: MaterialClauseValueSurface,
    pub value: MaterialClauseValueSurface,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialParamSurface {
    pub name: MaterialClauseValueSurface,
    pub value: MaterialClauseValueSurface,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialFeatureSurface {
    pub name: MaterialClauseValueSurface,
    pub value: MaterialClauseValueSurface,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialSemanticBindingSurface {
    pub key: MaterialClauseValueSurface,
    pub value: MaterialClauseValueSurface,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MaterialSemanticSurface {
    pub physics_surface: Option<MaterialClauseValueSurface>,
    pub footstep_class: Option<MaterialClauseValueSurface>,
    pub impact_vfx_class: Option<MaterialClauseValueSurface>,
    pub wear_response: Option<MaterialClauseValueSurface>,
    pub friction: Option<MaterialClauseValueSurface>,
    pub restitution: Option<MaterialClauseValueSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MaterialRenderSurface {
    pub alpha: Option<MaterialClauseValueSurface>,
    pub double_sided: Option<MaterialClauseValueSurface>,
    pub receives_decals: Option<MaterialClauseValueSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderContract {
    pub kind: SurfaceDeclarationKind,
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub resources: Option<RenderClauseValueSurface>,
    pub temporal: Option<RenderClauseValueSurface>,
    pub quality_tier: Option<RenderClauseValueSurface>,
    pub budget_tags: Option<RenderBudgetTagsSurface>,
    pub preset: Option<SmolStr>,
    pub profile: Option<SmolStr>,
    pub target: Option<RenderTargetSurface>,
    pub shader_modes: RenderShaderModeSet,
    pub overrides: RenderOverrideTiers,
    pub assets: Option<AssetsContract>,
    pub mmo: Option<MmoContract>,
    pub span: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceDeclarationKind {
    Render,
    Assets,
    Mmo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetFactoryDeclarationKind {
    AssetSpec,
    StyleProfile,
    GeneratorProfile,
    QualityProfile,
    ProvenancePolicy,
    CharacterSpec,
    RigSpec,
    AnimSetSpec,
    AudioSpec,
    VfxSpec,
    UiSpec,
    WorldRecipe,
}

impl AssetFactoryDeclarationKind {
    pub fn keyword(self) -> &'static str {
        match self {
            AssetFactoryDeclarationKind::AssetSpec => "asset_spec",
            AssetFactoryDeclarationKind::StyleProfile => "style_profile",
            AssetFactoryDeclarationKind::GeneratorProfile => "generator_profile",
            AssetFactoryDeclarationKind::QualityProfile => "quality_profile",
            AssetFactoryDeclarationKind::ProvenancePolicy => "provenance_policy",
            AssetFactoryDeclarationKind::CharacterSpec => "character_spec",
            AssetFactoryDeclarationKind::RigSpec => "rig_spec",
            AssetFactoryDeclarationKind::AnimSetSpec => "anim_set_spec",
            AssetFactoryDeclarationKind::AudioSpec => "audio_spec",
            AssetFactoryDeclarationKind::VfxSpec => "vfx_spec",
            AssetFactoryDeclarationKind::UiSpec => "ui_spec",
            AssetFactoryDeclarationKind::WorldRecipe => "world_recipe",
        }
    }
}

impl std::fmt::Display for AssetFactoryDeclarationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.keyword())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFactoryDeclarationSurface {
    pub kind: AssetFactoryDeclarationKind,
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub id: Option<SmolStr>,
    pub id_span: Option<TextRange>,
    pub profile: Option<SmolStr>,
    pub profile_span: Option<TextRange>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSpecIRV2 {
    pub declaration: AssetFactoryDeclarationSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleProfileIRV1 {
    pub declaration: AssetFactoryDeclarationSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorPlanIRV1 {
    pub declaration: AssetFactoryDeclarationSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetBuildGraphIRV2 {
    pub declaration: AssetFactoryDeclarationSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceLedgerIRV1 {
    pub declaration: AssetFactoryDeclarationSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityGateIRV2 {
    pub declaration: AssetFactoryDeclarationSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetsContract {
    pub manifest: Option<SmolStr>,
    pub streaming: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MmoContract {
    pub gateway: Option<SmolStr>,
    pub zone: Option<SmolStr>,
    pub world: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderClauseValueSurface {
    pub value: SmolStr,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderBudgetTagsSurface {
    pub tags: Vec<RenderClauseValueSurface>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderTargetSurface {
    pub node_type: SmolStr,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderShaderSymbolSurface {
    pub symbol: SmolStr,
    pub span: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderShaderModeKind {
    Generated,
    Material,
    Gpu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderShaderModeSurface {
    pub kind: RenderShaderModeKind,
    pub symbol: Option<SmolStr>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderShaderModeSet {
    pub generated: Option<TextRange>,
    pub material: Option<RenderShaderSymbolSurface>,
    pub gpu: Option<RenderShaderSymbolSurface>,
    pub duplicate_modes: Vec<RenderShaderModeSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderOverrideTiers {
    pub tier0: Option<RenderOverrideTier>,
    pub tier1: Option<RenderOverrideTier>,
    pub tier2: Option<RenderOverrideTier>,
    pub duplicate_tiers: Vec<RenderOverrideTier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOverrideTier {
    pub level: RenderOverrideTierLevel,
    pub span: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOverrideTierLevel {
    Tier0,
    Tier1,
    Tier2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpuFunctionSurface {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeRef>,
    pub body: Option<Body>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub role: ClassRole,
    pub node_profile: Option<NodeProfile>,
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<Field>,
    pub methods: Vec<Idx<Function>>,
    pub implements: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub type_params: Vec<TypeParam>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Interface {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub type_params: Vec<TypeParam>,
    pub methods: Vec<InterfaceMethod>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceMethod {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeRef>,
    pub kind: InterfaceMethodKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceMethodKind {
    Method,
    Check,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseStmt {
    pub names: Vec<UseName>,
    pub module: SmolStr,
    pub module_span: Option<TextRange>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub ty: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub ty: Option<TypeRef>,
    pub mutable: bool,
    pub default: Option<FieldDefault>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldDefault {
    Literal(Literal),
    List(Vec<FieldDefault>),
    Map(Vec<(FieldDefault, FieldDefault)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub args: Vec<TypeRef>,
}
