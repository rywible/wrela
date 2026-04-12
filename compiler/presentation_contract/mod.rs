use crate::artifact_key::{ArtifactPolicyDigestMode, ArtifactReuseKey};
pub use crate::execution_policy::{
    PresentationExecutionPolicy, RayBudgetPolicy, RequiredGuaranteeClass, SelectedMethodClass,
};
use crate::world_identity::WorldSnapshotHandle;
use smol_str::SmolStr;

pub const PRESENTATION_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FrameAttachmentKind {
    PrimaryHit,
    Depth,
    WorldNormal,
    Surface,
    Radiance,
    Medium,
    Motion,
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttachmentLifetime {
    Transient,
    Exported,
    HistorySlot(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttachmentResolutionClass {
    Viewport,
    HalfViewport,
    QuarterViewport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttachmentResolutionScale {
    pub divisor_x: u32,
    pub divisor_y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttachmentClearPolicy {
    Zero,
    SemanticDefault,
    PreservePrevious,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttachmentElementSchema {
    NamedRecord(SmolStr),
    ScalarF32,
    Vec2F32,
    Vec3F32,
    Vec4F32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemporalReuseMode {
    Disabled,
    ReprojectColor,
    ReprojectColorAndMotion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemporalInvalidationPolicy {
    None,
    CameraCut,
    HistoryCompatibilityMismatch,
    CameraCutOrHistoryCompatibilityMismatch,
    CameraCutHistoryMismatchOrDisocclusion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemporalValidationStrictness {
    Relaxed,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemporalHistoryRole {
    ReprojectedColor,
    ContinuationPrimaryHit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RealtimeQualityTier {
    Realtime60,
    Realtime120,
    High,
    Ultra,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RealtimeRadianceMode {
    Full,
    Reduced,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QualityDegradationStep {
    ReduceInternalResolution,
    EnableHitCompaction,
    LowerPrimarySteps,
    DisableMedia,
    LowerRadianceQuality,
    DisableRadiance,
    HalfResolutionParticipants,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RealtimeQualityContract {
    pub tier: RealtimeQualityTier,
    pub target_fps: u32,
    pub internal_resolution_scale: f32,
    pub allow_dynamic_resolution: bool,
    pub primary_max_steps: i32,
    pub allow_radiance: bool,
    pub allow_media: bool,
    pub temporal_mode: TemporalReuseMode,
    pub allow_half_res_participants: bool,
    pub allow_hit_compaction: bool,
    pub degradation_order: Vec<QualityDegradationStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RealtimeQualityState {
    pub tier: RealtimeQualityTier,
    pub target_fps: u32,
    pub internal_resolution_scale: f32,
    pub primary_max_steps: i32,
    pub radiance_mode: RealtimeRadianceMode,
    pub media_enabled: bool,
    pub temporal_mode: TemporalReuseMode,
    pub half_res_participants: bool,
    pub hit_compaction_enabled: bool,
    pub active_degradations: Vec<QualityDegradationStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HistoryCompatibilityKey {
    pub element_schema: AttachmentElementSchema,
    pub resolution: AttachmentResolutionClass,
    pub scale: AttachmentResolutionScale,
    pub projection_input: CanonicalProjectionInput,
    pub ray_space: CanonicalViewRaySpace,
    pub sample_position: ScreenLatticeSamplePosition,
    pub sample_origin: ScreenLatticeOrigin,
    pub samples_per_pixel: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TemporalHistorySlotContract {
    pub slot: u8,
    pub attachment: SmolStr,
    pub role: TemporalHistoryRole,
    pub compatibility: HistoryCompatibilityKey,
    pub max_age_frames: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightingInputBindingSource {
    AuthoredMetadata,
    DefaultCompatibilityRecipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScreenLatticeSamplePosition {
    PixelCenter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScreenLatticeOrigin {
    TopLeft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenLatticeContract {
    pub sample_position: ScreenLatticeSamplePosition,
    pub origin: ScreenLatticeOrigin,
    pub width_source: SmolStr,
    pub height_source: SmolStr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalViewRaySpace {
    World,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthAttachmentSemantics {
    /// Depth is the ray parameter / traveled world distance stored on `Hit3`.
    RayParameterDistance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalViewRayContract {
    pub space: CanonicalViewRaySpace,
    pub normalized_direction: bool,
    pub projection_input: CanonicalProjectionInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalContract {
    pub reuse: TemporalReuseMode,
    pub invalidation: TemporalInvalidationPolicy,
    pub validation: TemporalValidationStrictness,
    pub history_slots: Vec<TemporalHistorySlotContract>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameAttachmentContract {
    pub name: SmolStr,
    pub kind: FrameAttachmentKind,
    pub element_schema: AttachmentElementSchema,
    pub lifetime: AttachmentLifetime,
    pub resolution: AttachmentResolutionClass,
    pub scale: AttachmentResolutionScale,
    pub clear_policy: AttachmentClearPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryHitAttachmentContract {
    pub attachment: SmolStr,
    pub record: SmolStr,
    pub fields: Vec<SmolStr>,
    pub depth_semantics: DepthAttachmentSemantics,
    pub sample_identity: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityProjectionContract {
    /// True while this view is still lowered through the legacy preview
    /// projection path. Canonical projection is represented separately by
    /// `Camera.vertical_fov_degrees`.
    pub legacy_path_active: bool,
    /// Compatibility-only authored `world_up` override. Legacy preview lowering
    /// still supplies a default world-up value when this is false.
    pub authored_world_up_override: bool,
    /// Compatibility-only authored `view_scale` override. Legacy preview
    /// lowering still supplies a default view scale when this is false.
    pub authored_view_scale_override: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalProjectionInput {
    CameraVerticalFovDegrees,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewContract {
    pub canonical_projection: bool,
    pub canonical_projection_input: CanonicalProjectionInput,
    pub screen_lattice: ScreenLatticeContract,
    pub canonical_view_ray: CanonicalViewRayContract,
    pub allows_legacy_projection_override: bool,
    pub compatibility_projection: CompatibilityProjectionContract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightingContract {
    pub key_light: LightingInputContract,
    pub fill_direction: LightingInputContract,
    pub fill_strength: LightingInputContract,
    pub ambient_color: LightingInputContract,
    pub allows_legacy_plural_lights_metadata: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightingInputContract {
    pub binding: SmolStr,
    pub element_schema: AttachmentElementSchema,
    pub source: LightingInputBindingSource,
    pub temporary_compatibility_alias: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationObservabilityProfile {
    pub pass_graph: bool,
    pub materialized_intermediates: bool,
    pub query_dependencies: bool,
    pub backend_dispatch: bool,
    pub future_acceleration_hooks: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameContract {
    pub outputs: Vec<FrameAttachmentContract>,
    pub primary_hit: Option<PrimaryHitAttachmentContract>,
    pub temporal: Option<TemporalContract>,
    pub quality: RealtimeQualityContract,
    pub lighting: LightingContract,
    pub observability: PresentationObservabilityProfile,
}

impl PresentationObservabilityProfile {
    pub const fn preview_compatibility() -> Self {
        Self {
            pass_graph: true,
            materialized_intermediates: true,
            query_dependencies: true,
            backend_dispatch: true,
            future_acceleration_hooks: true,
        }
    }
}

impl AttachmentResolutionScale {
    pub const fn full() -> Self {
        Self {
            divisor_x: 1,
            divisor_y: 1,
        }
    }

    pub const fn half() -> Self {
        Self {
            divisor_x: 2,
            divisor_y: 2,
        }
    }

    pub const fn quarter() -> Self {
        Self {
            divisor_x: 4,
            divisor_y: 4,
        }
    }
}

impl ViewContract {
    pub fn legacy_preview(authored_world_up: bool, authored_view_scale: bool) -> Self {
        Self {
            canonical_projection: true,
            canonical_projection_input: CanonicalProjectionInput::CameraVerticalFovDegrees,
            screen_lattice: ScreenLatticeContract::viewport_pixel_centers(),
            canonical_view_ray: CanonicalViewRayContract::camera_vertical_fov_world(),
            allows_legacy_projection_override: true,
            compatibility_projection: CompatibilityProjectionContract {
                legacy_path_active: true,
                authored_world_up_override: authored_world_up,
                authored_view_scale_override: authored_view_scale,
            },
        }
    }

    pub fn canonical() -> Self {
        Self {
            canonical_projection: true,
            canonical_projection_input: CanonicalProjectionInput::CameraVerticalFovDegrees,
            screen_lattice: ScreenLatticeContract::viewport_pixel_centers(),
            canonical_view_ray: CanonicalViewRayContract::camera_vertical_fov_world(),
            allows_legacy_projection_override: false,
            compatibility_projection: CompatibilityProjectionContract {
                legacy_path_active: false,
                authored_world_up_override: false,
                authored_view_scale_override: false,
            },
        }
    }
}

impl ScreenLatticeContract {
    pub fn viewport_pixel_centers() -> Self {
        Self {
            sample_position: ScreenLatticeSamplePosition::PixelCenter,
            origin: ScreenLatticeOrigin::TopLeft,
            width_source: SmolStr::new("view.width"),
            height_source: SmolStr::new("view.height"),
        }
    }
}

impl CanonicalViewRayContract {
    pub const fn camera_vertical_fov_world() -> Self {
        Self {
            space: CanonicalViewRaySpace::World,
            normalized_direction: true,
            projection_input: CanonicalProjectionInput::CameraVerticalFovDegrees,
        }
    }
}

impl LightingContract {
    pub fn first_color_path(
        key_light_authored: bool,
        key_light_compatibility_alias: bool,
        fill_direction_authored: bool,
        fill_direction_compatibility_alias: bool,
        fill_strength_authored: bool,
        ambient_color_authored: bool,
        allows_legacy_plural_lights_metadata: bool,
    ) -> Self {
        Self {
            key_light: if key_light_authored {
                LightingInputContract::authored(
                    "lighting.key_light",
                    AttachmentElementSchema::NamedRecord(SmolStr::new("Light")),
                    key_light_compatibility_alias,
                )
            } else {
                LightingInputContract::compatibility_default(
                    "lighting.key_light",
                    AttachmentElementSchema::NamedRecord(SmolStr::new("Light")),
                )
            },
            fill_direction: if fill_direction_authored {
                LightingInputContract::authored(
                    "lighting.fill_direction",
                    AttachmentElementSchema::Vec3F32,
                    fill_direction_compatibility_alias,
                )
            } else {
                LightingInputContract::compatibility_default(
                    "lighting.fill_direction",
                    AttachmentElementSchema::Vec3F32,
                )
            },
            fill_strength: if fill_strength_authored {
                LightingInputContract::authored(
                    "lighting.fill_strength",
                    AttachmentElementSchema::ScalarF32,
                    false,
                )
            } else {
                LightingInputContract::compatibility_default(
                    "lighting.fill_strength",
                    AttachmentElementSchema::ScalarF32,
                )
            },
            ambient_color: if ambient_color_authored {
                LightingInputContract::authored(
                    "lighting.ambient_color",
                    AttachmentElementSchema::Vec3F32,
                    false,
                )
            } else {
                LightingInputContract::compatibility_default(
                    "lighting.ambient_color",
                    AttachmentElementSchema::Vec3F32,
                )
            },
            allows_legacy_plural_lights_metadata,
        }
    }

    pub fn legacy_preview(allows_legacy_plural_lights_metadata: bool) -> Self {
        Self::first_color_path(
            true,
            true,
            true,
            true,
            false,
            false,
            allows_legacy_plural_lights_metadata,
        )
    }
}

impl LightingInputContract {
    pub fn authored(
        binding: impl Into<SmolStr>,
        element_schema: AttachmentElementSchema,
        temporary_compatibility_alias: bool,
    ) -> Self {
        Self {
            binding: binding.into(),
            element_schema,
            source: LightingInputBindingSource::AuthoredMetadata,
            temporary_compatibility_alias,
        }
    }

    pub fn compatibility_default(
        binding: impl Into<SmolStr>,
        element_schema: AttachmentElementSchema,
    ) -> Self {
        Self {
            binding: binding.into(),
            element_schema,
            source: LightingInputBindingSource::DefaultCompatibilityRecipe,
            temporary_compatibility_alias: false,
        }
    }
}

impl FrameAttachmentContract {
    pub fn primary_hit(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::PrimaryHit,
            element_schema: AttachmentElementSchema::NamedRecord(SmolStr::new("Hit3")),
            lifetime: AttachmentLifetime::Transient,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::SemanticDefault,
        }
    }

    pub fn depth(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::Depth,
            element_schema: AttachmentElementSchema::ScalarF32,
            lifetime: AttachmentLifetime::Transient,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::SemanticDefault,
        }
    }

    pub fn world_normal(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::WorldNormal,
            element_schema: AttachmentElementSchema::Vec3F32,
            lifetime: AttachmentLifetime::Transient,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::Zero,
        }
    }

    pub fn exported_color(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::Color,
            element_schema: AttachmentElementSchema::Vec3F32,
            lifetime: AttachmentLifetime::Exported,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::Zero,
        }
    }

    pub fn transient_color(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::Color,
            element_schema: AttachmentElementSchema::Vec3F32,
            lifetime: AttachmentLifetime::Transient,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::Zero,
        }
    }

    pub fn surface(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::Surface,
            element_schema: AttachmentElementSchema::NamedRecord(SmolStr::new("Surface")),
            lifetime: AttachmentLifetime::Transient,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::SemanticDefault,
        }
    }

    pub fn radiance(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::Radiance,
            element_schema: AttachmentElementSchema::Vec3F32,
            lifetime: AttachmentLifetime::Transient,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::Zero,
        }
    }

    pub fn medium(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::Medium,
            element_schema: AttachmentElementSchema::NamedRecord(SmolStr::new("Medium")),
            lifetime: AttachmentLifetime::Transient,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::SemanticDefault,
        }
    }

    pub fn motion(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::Motion,
            element_schema: AttachmentElementSchema::NamedRecord(SmolStr::new("MotionVector")),
            lifetime: AttachmentLifetime::Transient,
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::SemanticDefault,
        }
    }

    pub fn history_color(name: impl Into<SmolStr>, slot: u8) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::Color,
            element_schema: AttachmentElementSchema::Vec3F32,
            lifetime: AttachmentLifetime::HistorySlot(slot),
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::PreservePrevious,
        }
    }

    pub fn history_primary_hit(name: impl Into<SmolStr>, slot: u8) -> Self {
        Self {
            name: name.into(),
            kind: FrameAttachmentKind::PrimaryHit,
            element_schema: AttachmentElementSchema::NamedRecord(SmolStr::new("Hit3")),
            lifetime: AttachmentLifetime::HistorySlot(slot),
            resolution: AttachmentResolutionClass::Viewport,
            scale: AttachmentResolutionScale::full(),
            clear_policy: AttachmentClearPolicy::PreservePrevious,
        }
    }
}

impl PrimaryHitAttachmentContract {
    pub fn hit3(attachment: impl Into<SmolStr>) -> Self {
        Self {
            attachment: attachment.into(),
            record: SmolStr::new("Hit3"),
            fields: vec![
                SmolStr::new("hit"),
                SmolStr::new("position"),
                SmolStr::new("normal"),
                SmolStr::new("local_position"),
                SmolStr::new("local_normal"),
                SmolStr::new("shading_frame"),
                SmolStr::new("distance"),
                SmolStr::new("steps"),
                SmolStr::new("root_shape_id"),
                SmolStr::new("feature_id"),
                SmolStr::new("instance_id"),
                SmolStr::new("repeat_id"),
                SmolStr::new("payload"),
            ],
            depth_semantics: DepthAttachmentSemantics::RayParameterDistance,
            sample_identity: SmolStr::new("screen_lattice.row_major_top_left_pixel_center"),
        }
    }
}

impl FrameContract {
    pub fn attachment(&self, name: &str) -> Option<&FrameAttachmentContract> {
        self.outputs
            .iter()
            .find(|attachment| attachment.name == name)
    }
}

impl RealtimeQualityContract {
    pub fn named(tier: RealtimeQualityTier) -> Self {
        match tier {
            RealtimeQualityTier::Realtime60 => Self {
                tier,
                target_fps: 60,
                internal_resolution_scale: 1.0,
                allow_dynamic_resolution: true,
                primary_max_steps: 128,
                allow_radiance: true,
                allow_media: true,
                temporal_mode: TemporalReuseMode::ReprojectColorAndMotion,
                allow_half_res_participants: true,
                allow_hit_compaction: true,
                degradation_order: vec![
                    QualityDegradationStep::ReduceInternalResolution,
                    QualityDegradationStep::EnableHitCompaction,
                    QualityDegradationStep::LowerPrimarySteps,
                    QualityDegradationStep::DisableMedia,
                    QualityDegradationStep::LowerRadianceQuality,
                    QualityDegradationStep::DisableRadiance,
                    QualityDegradationStep::HalfResolutionParticipants,
                ],
            },
            RealtimeQualityTier::Realtime120 => Self {
                tier,
                target_fps: 120,
                internal_resolution_scale: 1.0,
                allow_dynamic_resolution: true,
                primary_max_steps: 96,
                allow_radiance: true,
                allow_media: false,
                temporal_mode: TemporalReuseMode::ReprojectColorAndMotion,
                allow_half_res_participants: true,
                allow_hit_compaction: true,
                degradation_order: vec![
                    QualityDegradationStep::ReduceInternalResolution,
                    QualityDegradationStep::EnableHitCompaction,
                    QualityDegradationStep::LowerPrimarySteps,
                    QualityDegradationStep::LowerRadianceQuality,
                    QualityDegradationStep::DisableRadiance,
                    QualityDegradationStep::HalfResolutionParticipants,
                ],
            },
            RealtimeQualityTier::High => Self {
                tier,
                target_fps: 60,
                internal_resolution_scale: 1.0,
                allow_dynamic_resolution: false,
                primary_max_steps: 160,
                allow_radiance: true,
                allow_media: true,
                temporal_mode: TemporalReuseMode::ReprojectColorAndMotion,
                allow_half_res_participants: true,
                allow_hit_compaction: true,
                degradation_order: vec![
                    QualityDegradationStep::EnableHitCompaction,
                    QualityDegradationStep::LowerPrimarySteps,
                    QualityDegradationStep::DisableMedia,
                    QualityDegradationStep::LowerRadianceQuality,
                    QualityDegradationStep::DisableRadiance,
                    QualityDegradationStep::HalfResolutionParticipants,
                ],
            },
            RealtimeQualityTier::Ultra => Self {
                tier,
                target_fps: 60,
                internal_resolution_scale: 1.0,
                allow_dynamic_resolution: false,
                primary_max_steps: 224,
                allow_radiance: true,
                allow_media: true,
                temporal_mode: TemporalReuseMode::ReprojectColorAndMotion,
                allow_half_res_participants: true,
                allow_hit_compaction: true,
                degradation_order: vec![
                    QualityDegradationStep::EnableHitCompaction,
                    QualityDegradationStep::LowerPrimarySteps,
                    QualityDegradationStep::DisableMedia,
                    QualityDegradationStep::LowerRadianceQuality,
                    QualityDegradationStep::DisableRadiance,
                    QualityDegradationStep::HalfResolutionParticipants,
                ],
            },
            RealtimeQualityTier::Debug => Self {
                tier,
                target_fps: 30,
                internal_resolution_scale: 1.0,
                allow_dynamic_resolution: false,
                primary_max_steps: 256,
                allow_radiance: true,
                allow_media: true,
                temporal_mode: TemporalReuseMode::ReprojectColorAndMotion,
                allow_half_res_participants: false,
                allow_hit_compaction: false,
                degradation_order: vec![
                    QualityDegradationStep::LowerPrimarySteps,
                    QualityDegradationStep::DisableMedia,
                    QualityDegradationStep::DisableRadiance,
                ],
            },
        }
    }

    pub fn with_temporal_mode(mut self, temporal_mode: TemporalReuseMode) -> Self {
        self.temporal_mode = temporal_mode;
        self
    }

    pub fn validate(&self) -> Vec<SmolStr> {
        let mut errors = Vec::new();
        if self.target_fps == 0 {
            errors.push(SmolStr::new(
                "realtime quality contract must target a non-zero FPS",
            ));
        }
        if !(0.25..=1.0).contains(&self.internal_resolution_scale) {
            errors.push(SmolStr::new(format!(
                "internal resolution scale {} must be within [0.25, 1.0]",
                self.internal_resolution_scale
            )));
        }
        if self.internal_resolution_scale != 1.0
            && (self.internal_resolution_scale - 0.5).abs() > f32::EPSILON
            && (self.internal_resolution_scale - 0.25).abs() > f32::EPSILON
        {
            errors.push(SmolStr::new(
                "internal resolution scale must be one of 1.0, 0.5, or 0.25",
            ));
        }
        if !self.allow_dynamic_resolution
            && (self.internal_resolution_scale - 1.0).abs() > f32::EPSILON
        {
            errors.push(SmolStr::new(
                "internal resolution scale below 1.0 requires allow_dynamic_resolution",
            ));
        }
        if self.primary_max_steps <= 0 {
            errors.push(SmolStr::new("primary_max_steps must be greater than zero"));
        }
        if self.degradation_order.is_empty() {
            errors.push(SmolStr::new(
                "realtime quality contract must define an explicit degradation order",
            ));
        }
        errors
    }

    pub fn initial_state(&self) -> RealtimeQualityState {
        RealtimeQualityState {
            tier: self.tier,
            target_fps: self.target_fps,
            internal_resolution_scale: self.internal_resolution_scale,
            primary_max_steps: self.primary_max_steps,
            radiance_mode: if self.allow_radiance {
                RealtimeRadianceMode::Full
            } else {
                RealtimeRadianceMode::Disabled
            },
            media_enabled: self.allow_media,
            temporal_mode: self.temporal_mode,
            half_res_participants: false,
            hit_compaction_enabled: false,
            active_degradations: Vec::new(),
        }
    }
}

impl RealtimeQualityState {
    pub fn radiance_enabled(&self) -> bool {
        self.radiance_mode != RealtimeRadianceMode::Disabled
    }

    pub fn step_down(&mut self, contract: &RealtimeQualityContract) -> bool {
        for step in &contract.degradation_order {
            if self.active_degradations.contains(step) {
                continue;
            }
            if self.apply_step(contract, *step) {
                self.active_degradations.push(*step);
                return true;
            }
        }
        false
    }

    pub fn step_up(&mut self, contract: &RealtimeQualityContract) -> bool {
        if self.active_degradations.pop().is_none() {
            return false;
        }
        let retained = self.active_degradations.clone();
        *self = contract.initial_state();
        for step in retained {
            let _ = self.apply_step(contract, step);
            self.active_degradations.push(step);
        }
        true
    }

    fn apply_step(
        &mut self,
        contract: &RealtimeQualityContract,
        step: QualityDegradationStep,
    ) -> bool {
        match step {
            QualityDegradationStep::ReduceInternalResolution => {
                if !contract.allow_dynamic_resolution {
                    return false;
                }
                let next_scale = if self.internal_resolution_scale > 0.5 {
                    0.5
                } else if self.internal_resolution_scale > 0.25 {
                    0.25
                } else {
                    self.internal_resolution_scale
                };
                if (next_scale - self.internal_resolution_scale).abs() <= f32::EPSILON {
                    false
                } else {
                    self.internal_resolution_scale = next_scale;
                    true
                }
            }
            QualityDegradationStep::EnableHitCompaction => {
                if !contract.allow_hit_compaction || self.hit_compaction_enabled {
                    return false;
                }
                self.hit_compaction_enabled = true;
                true
            }
            QualityDegradationStep::LowerPrimarySteps => {
                let next_steps = (self.primary_max_steps * 3) / 4;
                let next_steps = next_steps.max(16);
                if next_steps >= self.primary_max_steps {
                    false
                } else {
                    self.primary_max_steps = next_steps;
                    true
                }
            }
            QualityDegradationStep::DisableMedia => {
                if !self.media_enabled {
                    return false;
                }
                self.media_enabled = false;
                true
            }
            QualityDegradationStep::LowerRadianceQuality => {
                if self.radiance_mode != RealtimeRadianceMode::Full {
                    return false;
                }
                self.radiance_mode = RealtimeRadianceMode::Reduced;
                true
            }
            QualityDegradationStep::DisableRadiance => {
                if !self.radiance_enabled() {
                    return false;
                }
                self.radiance_mode = RealtimeRadianceMode::Disabled;
                true
            }
            QualityDegradationStep::HalfResolutionParticipants => {
                if !contract.allow_half_res_participants || self.half_res_participants {
                    return false;
                }
                self.half_res_participants = true;
                true
            }
        }
    }
}

impl HistoryCompatibilityKey {
    pub fn from_attachment(
        view: &ViewContract,
        attachment: &FrameAttachmentContract,
        samples_per_pixel: u32,
    ) -> Self {
        Self {
            element_schema: attachment.element_schema.clone(),
            resolution: attachment.resolution,
            scale: attachment.scale,
            projection_input: view.canonical_view_ray.projection_input,
            ray_space: view.canonical_view_ray.space,
            sample_position: view.screen_lattice.sample_position,
            sample_origin: view.screen_lattice.origin,
            samples_per_pixel,
        }
    }

    pub fn compatibility_hash(&self) -> u64 {
        let encoded = format!("{self:?}");
        crate::query_exec::ids::stable_semantic_id(&[encoded.as_bytes()])
    }
}

impl TemporalHistorySlotContract {
    pub fn logical_artifact_schema(&self) -> SmolStr {
        SmolStr::new(format!(
            "presentation-history::{:?}::{}",
            self.role, self.attachment
        ))
    }

    pub fn reuse_key(
        &self,
        snapshot: &WorldSnapshotHandle,
        layout_signature: u64,
    ) -> ArtifactReuseKey {
        let compatibility_hash = self.compatibility.compatibility_hash();
        let combined_hash = crate::query_exec::ids::stable_semantic_id(&[
            &compatibility_hash.to_le_bytes(),
            &layout_signature.to_le_bytes(),
        ]);
        ArtifactReuseKey::new(
            snapshot,
            None,
            self.logical_artifact_schema(),
            combined_hash,
            Some(compatibility_hash),
            ArtifactPolicyDigestMode::CompatibleRange,
        )
    }
}

impl TemporalContract {
    pub fn first_color_path(
        view: &ViewContract,
        history_color: &FrameAttachmentContract,
        history_primary_hit: &FrameAttachmentContract,
        samples_per_pixel: u32,
    ) -> Self {
        Self {
            reuse: TemporalReuseMode::ReprojectColorAndMotion,
            invalidation: TemporalInvalidationPolicy::CameraCutHistoryMismatchOrDisocclusion,
            validation: TemporalValidationStrictness::Strict,
            history_slots: vec![
                TemporalHistorySlotContract {
                    slot: 0,
                    attachment: history_color.name.clone(),
                    role: TemporalHistoryRole::ReprojectedColor,
                    compatibility: HistoryCompatibilityKey::from_attachment(
                        view,
                        history_color,
                        samples_per_pixel,
                    ),
                    max_age_frames: 8,
                },
                TemporalHistorySlotContract {
                    slot: 1,
                    attachment: history_primary_hit.name.clone(),
                    role: TemporalHistoryRole::ContinuationPrimaryHit,
                    compatibility: HistoryCompatibilityKey::from_attachment(
                        view,
                        history_primary_hit,
                        samples_per_pixel,
                    ),
                    max_age_frames: 8,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanonicalCameraInput {
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub vertical_fov_degrees: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanonicalLightInput {
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub intensity: [f32; 3],
    pub range: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresentationLightingInputs {
    pub key_light: CanonicalLightInput,
    pub fill_direction: [f32; 3],
    pub fill_strength: f32,
    pub ambient_color: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegacyCompatibilityProjectionInput {
    pub world_up: [f32; 3],
    pub view_scale: f32,
}

impl PresentationLightingInputs {
    pub fn compatibility_recipe(key_light: CanonicalLightInput) -> Self {
        Self {
            key_light,
            fill_direction: [-0.7, 0.45, 0.2],
            fill_strength: 0.22,
            ambient_color: [0.12, 0.12, 0.12],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalViewportInput {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanonicalRayBudget {
    pub max_distance: f32,
    pub min_step: f32,
    pub hit_epsilon: f32,
    pub max_steps: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanonicalRayQuery {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub max_distance: f32,
    pub min_step: f32,
    pub hit_epsilon: f32,
    pub max_steps: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanonicalScreenSampleQuery {
    pub pixel: [f32; 2],
    pub uv: [f32; 2],
    pub ray: CanonicalRayQuery,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanonicalMotionVector {
    pub delta_pixels: [f32; 2],
    pub previous_sample: [f32; 2],
    pub valid: bool,
    pub disoccluded: bool,
}

pub fn canonical_screen_uv(
    pixel_x: u32,
    pixel_y: u32,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
) -> [f32; 2] {
    let width = viewport.width.max(1) as f32;
    let height = viewport.height.max(1) as f32;
    [
        (pixel_x as f32 + 0.5 + jitter_pixels[0]) / width,
        (pixel_y as f32 + 0.5 + jitter_pixels[1]) / height,
    ]
}

pub fn canonical_view_ray_direction(
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    uv: [f32; 2],
) -> [f32; 3] {
    let forward = normalize_or(camera.forward, [0.0, 0.0, -1.0]);
    let right = normalize_or(cross3(forward, camera.up), [1.0, 0.0, 0.0]);
    let up = normalize_or(cross3(right, forward), [0.0, 1.0, 0.0]);
    let width = viewport.width.max(1) as f32;
    let height = viewport.height.max(1) as f32;
    let aspect = width / height;
    let vertical_scale = (camera.vertical_fov_degrees.to_radians() * 0.5).tan();
    let screen_x = (uv[0] * 2.0 - 1.0) * aspect * vertical_scale;
    let screen_y = (1.0 - uv[1] * 2.0) * vertical_scale;
    normalize_or(
        add3(add3(forward, mul3(right, screen_x)), mul3(up, screen_y)),
        forward,
    )
}

pub fn canonical_screen_sample_query(
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    pixel_x: u32,
    pixel_y: u32,
    jitter_pixels: [f32; 2],
    budget: CanonicalRayBudget,
) -> CanonicalScreenSampleQuery {
    let uv = canonical_screen_uv(pixel_x, pixel_y, viewport, jitter_pixels);
    CanonicalScreenSampleQuery {
        pixel: [pixel_x as f32, pixel_y as f32],
        uv,
        ray: CanonicalRayQuery {
            origin: camera.position,
            direction: canonical_view_ray_direction(camera, viewport, uv),
            max_distance: budget.max_distance,
            min_step: budget.min_step,
            hit_epsilon: budget.hit_epsilon,
            max_steps: budget.max_steps,
        },
    }
}

pub fn legacy_preview_screen_sample_query(
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    pixel_x: u32,
    pixel_y: u32,
    jitter_pixels: [f32; 2],
    budget: CanonicalRayBudget,
    compatibility: LegacyCompatibilityProjectionInput,
) -> CanonicalScreenSampleQuery {
    let uv = canonical_screen_uv(pixel_x, pixel_y, viewport, jitter_pixels);
    let forward = normalize_or(camera.forward, [0.0, 0.0, -1.0]);
    let right = normalize_or(cross3(forward, compatibility.world_up), [1.0, 0.0, 0.0]);
    let up = normalize_or(cross3(right, forward), [0.0, 1.0, 0.0]);
    let width = viewport.width.max(1) as f32;
    let height = viewport.height.max(1) as f32;
    let aspect = width / height;
    let screen_x = (uv[0] * 2.0 - 1.0) * aspect * compatibility.view_scale;
    let screen_y = (1.0 - uv[1] * 2.0) * compatibility.view_scale;
    CanonicalScreenSampleQuery {
        pixel: [pixel_x as f32, pixel_y as f32],
        uv,
        ray: CanonicalRayQuery {
            origin: camera.position,
            direction: normalize_or(
                add3(add3(forward, mul3(right, screen_x)), mul3(up, screen_y)),
                forward,
            ),
            max_distance: budget.max_distance,
            min_step: budget.min_step,
            hit_epsilon: budget.hit_epsilon,
            max_steps: budget.max_steps,
        },
    }
}

fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

fn mul3(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

fn normalize_or(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len_sq = value[0] * value[0] + value[1] * value[1] + value[2] * value[2];
    if len_sq <= f32::EPSILON {
        return fallback;
    }
    let inv_len = len_sq.sqrt().recip();
    [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
}
