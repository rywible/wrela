use crate::acceleration::{self, AccelerationObserver};
use crate::artifact_contract::{
    ArtifactCompatibilityRelation, ArtifactEvidenceCompatibility, ArtifactLogicalField,
    ArtifactLogicalSchema, ArtifactPolicyCompatibility, ArtifactSnapshotRelation,
    ArtifactTransitionRelation, ArtifactUse, ArtifactUseKind, ArtifactUseSource,
    ArtifactValidityPredicate, ArtifactValidityRule, SemanticArtifactContract,
    SemanticArtifactKind,
};
use crate::hir;
use crate::presentation_binding::{PresentationBindingId, PresentationBindingSummary};
use crate::presentation_contract::{
    AttachmentClearPolicy, AttachmentElementSchema, AttachmentLifetime, AttachmentResolutionClass,
    AttachmentResolutionScale, FrameAttachmentContract, FrameAttachmentKind, FrameContract,
    LightingContract, PRESENTATION_CONTRACT_VERSION, PresentationObservabilityProfile,
    PrimaryHitAttachmentContract, QualityDegradationStep, RealtimeQualityContract,
    RealtimeQualityTier, TemporalContract, TemporalHistoryRole, TemporalReuseMode, ViewContract,
};
use crate::query_plan::{DispatchBackend, QueryContractId};
use crate::semantic_evidence::SemanticEvidenceSummary;
use smol_str::SmolStr;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationPlan {
    pub name: SmolStr,
    pub view: ViewContract,
    pub frame: FrameContract,
    pub passes: Vec<PresentationPass>,
    pub frame_artifacts: Vec<FrameArtifactContract>,
    pub bindings: Vec<PresentationBindingSummary>,
    pub observability: PresentationObservability,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationPass {
    pub id: SmolStr,
    pub kind: PresentationPassKind,
    pub consumes: Vec<SmolStr>,
    pub materializes: Vec<SmolStr>,
    pub binding: Option<PresentationBindingId>,
    pub query_dependencies: Vec<QueryContractId>,
    pub future_acceleration_hooks: Vec<PresentationAccelerationHook>,
    pub observability: PresentationObservability,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PresentationPassKind {
    GenerateScreenSamples {
        contract: ScreenSampleGenerationContract,
    },
    PrimaryVisibility {
        contract: PrimaryVisibilityPassContract,
    },
    SurfaceResolve {
        contract: SurfaceResolvePassContract,
    },
    ParticipantsResolve {
        contract: ParticipantsResolvePassContract,
    },
    ShadePrimary {
        contract: ShadePrimaryPassContract,
    },
    CompositeColor {
        contract: CompositeColorPassContract,
    },
    MotionResolve {
        contract: MotionResolvePassContract,
    },
    TemporalResolve {
        contract: TemporalResolvePassContract,
    },
    WorldBatchQuery {
        contract_id: QueryContractId,
    },
    KernelDispatch,
    ExportAttachment {
        attachment: SmolStr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenSampleGenerationContract {
    pub viewport_width_source: SmolStr,
    pub viewport_height_source: SmolStr,
    pub samples_per_pixel: u32,
    pub jitter_source: SmolStr,
    pub item_count_expression: SmolStr,
    pub output_item_record: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryVisibilityPassContract {
    pub query_contract: QueryContractId,
    pub primary_hit_attachment: SmolStr,
    pub depth_attachment: Option<SmolStr>,
    pub world_normal_attachment: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceResolvePassContract {
    pub query_contract: QueryContractId,
    pub primary_hit_attachment: SmolStr,
    pub surface_attachment: SmolStr,
    pub explicit_miss_default: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParticipantsResolvePassContract {
    pub radiance_query_contract: Option<QueryContractId>,
    pub medium_query_contract: Option<QueryContractId>,
    pub primary_hit_attachment: SmolStr,
    pub screen_samples: SmolStr,
    pub radiance_attachment: Option<SmolStr>,
    pub medium_attachment: Option<SmolStr>,
    pub miss_sample_distance: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadePrimaryPassContract {
    pub primary_hit_attachment: SmolStr,
    pub surface_attachment: SmolStr,
    pub radiance_attachment: Option<SmolStr>,
    pub medium_attachment: Option<SmolStr>,
    pub output_attachment: SmolStr,
    pub compatibility_recipe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeColorPassContract {
    pub input_attachment: SmolStr,
    pub output_attachment: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionResolvePassContract {
    pub primary_hit_attachment: SmolStr,
    pub output_attachment: SmolStr,
    pub history_primary_hit_attachment: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalResolvePassContract {
    pub input_attachment: SmolStr,
    pub primary_hit_attachment: SmolStr,
    pub motion_attachment: SmolStr,
    pub history_color_attachment: SmolStr,
    pub history_primary_hit_attachment: Option<SmolStr>,
    pub output_attachment: SmolStr,
    pub neighborhood_clamp: bool,
    pub history_weight_numerator: u32,
    pub history_weight_denominator: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameArtifactContract {
    pub id: SmolStr,
    pub attachment: SmolStr,
    pub producer_pass: SmolStr,
    pub materialized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationAccelerationHook {
    ScreenLattice,
    WorldBatch,
    SemanticSupport,
    TemporalHistory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationObservability {
    pub pass_graph: bool,
    pub materialized_intermediates: bool,
    pub query_dependencies: bool,
    pub backend_dispatch: bool,
    pub future_acceleration_hooks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthoredOutputSelection {
    color: bool,
    depth: bool,
    world_normal: bool,
    motion: bool,
}

impl Default for AuthoredOutputSelection {
    fn default() -> Self {
        Self {
            color: true,
            depth: true,
            world_normal: true,
            motion: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthoredTemporalHistorySelection {
    color: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthoredLightingSelection {
    key_light: bool,
    fill_direction: bool,
    fill_strength: bool,
    ambient_color: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationPlanValidationError {
    pub message: SmolStr,
}

impl PresentationPlan {
    pub fn from_view_function(
        func: &hir::Function,
        default_backend: DispatchBackend,
    ) -> Option<Self> {
        let metadata = func.presentation.as_ref()?;
        match func.role {
            hir::FunctionRole::View => Some(Self::canonical_view_from_metadata(
                func.name.clone(),
                metadata,
                default_backend,
            )),
            _ => None,
        }
    }

    pub fn canonical_view_from_metadata(
        name: SmolStr,
        metadata: &hir::PresentationMetadata,
        default_backend: DispatchBackend,
    ) -> Self {
        Self::presentation_path_from_metadata(
            name,
            metadata,
            default_backend,
            ViewContract::canonical(),
            true,
            true,
        )
    }

    fn presentation_path_from_metadata(
        name: SmolStr,
        metadata: &hir::PresentationMetadata,
        default_backend: DispatchBackend,
        view: ViewContract,
        export_color: bool,
        enable_temporal: bool,
    ) -> Self {
        let viewport_sources = authored_viewport_sources(metadata);
        let temporal_history = authored_temporal_history_selection(metadata).unwrap_or(
            AuthoredTemporalHistorySelection {
                color: enable_temporal,
            },
        );
        let enable_temporal = enable_temporal && temporal_history.color;
        let authored_outputs = authored_output_selection(metadata);
        let authored_lighting = authored_lighting_selection(metadata);
        let mut view = view;
        view.screen_lattice.width_source = SmolStr::new(viewport_sources.0);
        view.screen_lattice.height_source = SmolStr::new(viewport_sources.1);

        let primary_hit_attachment = FrameAttachmentContract::primary_hit("primary_hit");
        let depth_attachment = authored_outputs
            .depth
            .then(|| FrameAttachmentContract::depth("depth"));
        let world_normal_attachment = authored_outputs
            .world_normal
            .then(|| FrameAttachmentContract::world_normal("world_normal"));
        let surface_attachment = FrameAttachmentContract::surface("surface");
        let radiance_attachment = FrameAttachmentContract::radiance("radiance");
        let medium_attachment = FrameAttachmentContract::medium("medium");
        let shaded_color_attachment = FrameAttachmentContract::transient_color("shaded_color");
        let motion_attachment = (enable_temporal || authored_outputs.motion)
            .then(|| FrameAttachmentContract::motion("motion"));
        let history_color_attachment =
            enable_temporal.then(|| FrameAttachmentContract::history_color("history_color", 0));
        let history_primary_hit_attachment = enable_temporal
            .then(|| FrameAttachmentContract::history_primary_hit("history_primary_hit", 1));
        let color_attachment = FrameAttachmentContract::exported_color("color");
        let primary_hit_name = primary_hit_attachment.name.clone();
        let depth_name = depth_attachment
            .as_ref()
            .map(|attachment| attachment.name.clone());
        let world_normal_name = world_normal_attachment
            .as_ref()
            .map(|attachment| attachment.name.clone());
        let surface_name = surface_attachment.name.clone();
        let radiance_name = radiance_attachment.name.clone();
        let medium_name = medium_attachment.name.clone();
        let shaded_color_name = shaded_color_attachment.name.clone();
        let motion_name = motion_attachment
            .as_ref()
            .map(|attachment| attachment.name.clone());
        let history_color_name = history_color_attachment
            .as_ref()
            .map(|attachment| attachment.name.clone());
        let history_primary_hit_name = history_primary_hit_attachment
            .as_ref()
            .map(|attachment| attachment.name.clone());
        let color_name = color_attachment.name.clone();
        let observability = PresentationObservability::preview_compatibility();
        let mut bindings = vec![
            PresentationBindingSummary::screen_samples(default_backend),
            PresentationBindingSummary::primary_visibility(default_backend),
            PresentationBindingSummary::surface_resolve(default_backend),
            PresentationBindingSummary::participants_resolve(default_backend),
            PresentationBindingSummary::shade_primary(default_backend),
        ];
        if enable_temporal {
            bindings.push(PresentationBindingSummary::motion_resolve(default_backend));
            bindings.push(PresentationBindingSummary::temporal_resolve(
                default_backend,
            ));
        } else {
            bindings.push(PresentationBindingSummary::composite_color(default_backend));
        }
        if export_color {
            bindings.push(PresentationBindingSummary::ppm_export_attachment(
                default_backend,
                color_name.clone(),
            ));
        }

        let mut passes = vec![
            PresentationPass {
                id: SmolStr::new("generate_screen_samples"),
                kind: PresentationPassKind::GenerateScreenSamples {
                    contract: ScreenSampleGenerationContract::from_view(&view),
                },
                consumes: vec![
                    SmolStr::new("view.screen_lattice"),
                    SmolStr::new("view.camera"),
                    SmolStr::new("view.canonical_ray"),
                ],
                materializes: vec![SmolStr::new("screen_samples")],
                binding: Some(bindings[0].id.clone()),
                query_dependencies: Vec::new(),
                future_acceleration_hooks: vec![PresentationAccelerationHook::ScreenLattice],
                observability: observability.clone(),
            },
            PresentationPass {
                id: SmolStr::new("primary_visibility"),
                kind: PresentationPassKind::PrimaryVisibility {
                    contract: PrimaryVisibilityPassContract {
                        query_contract: crate::query_contract::SPATIAL_NEAREST_BATCH_WORLD,
                        primary_hit_attachment: primary_hit_name.clone(),
                        depth_attachment: depth_name.clone(),
                        world_normal_attachment: world_normal_name.clone(),
                    },
                },
                consumes: vec![SmolStr::new("screen_samples"), SmolStr::new("frame.domain")],
                materializes: {
                    let mut attachments = vec![primary_hit_name.clone()];
                    if let Some(depth_name) = &depth_name {
                        attachments.push(depth_name.clone());
                    }
                    if let Some(world_normal_name) = &world_normal_name {
                        attachments.push(world_normal_name.clone());
                    }
                    attachments
                },
                binding: Some(bindings[1].id.clone()),
                query_dependencies: vec![crate::query_contract::SPATIAL_NEAREST_BATCH_WORLD],
                future_acceleration_hooks: vec![
                    PresentationAccelerationHook::WorldBatch,
                    PresentationAccelerationHook::SemanticSupport,
                ],
                observability: observability.clone(),
            },
            PresentationPass {
                id: SmolStr::new("surface_resolve"),
                kind: PresentationPassKind::SurfaceResolve {
                    contract: SurfaceResolvePassContract {
                        query_contract: crate::query_contract::SURFACE_SAMPLE_BATCH_WORLD,
                        primary_hit_attachment: primary_hit_name.clone(),
                        surface_attachment: surface_name.clone(),
                        explicit_miss_default: true,
                    },
                },
                consumes: vec![primary_hit_name.clone(), SmolStr::new("frame.domain")],
                materializes: vec![surface_name.clone()],
                binding: Some(bindings[2].id.clone()),
                query_dependencies: vec![crate::query_contract::SURFACE_SAMPLE_BATCH_WORLD],
                future_acceleration_hooks: vec![PresentationAccelerationHook::WorldBatch],
                observability: observability.clone(),
            },
            PresentationPass {
                id: SmolStr::new("participants_resolve"),
                kind: PresentationPassKind::ParticipantsResolve {
                    contract: ParticipantsResolvePassContract {
                        radiance_query_contract: Some(
                            crate::query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD,
                        ),
                        medium_query_contract: Some(
                            crate::query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD,
                        ),
                        primary_hit_attachment: primary_hit_name.clone(),
                        screen_samples: SmolStr::new("screen_samples"),
                        radiance_attachment: Some(radiance_name.clone()),
                        medium_attachment: Some(medium_name.clone()),
                        miss_sample_distance: 4.0,
                    },
                },
                consumes: vec![
                    primary_hit_name.clone(),
                    SmolStr::new("screen_samples"),
                    SmolStr::new("frame.domain"),
                ],
                materializes: vec![radiance_name.clone(), medium_name.clone()],
                binding: Some(bindings[3].id.clone()),
                query_dependencies: vec![
                    crate::query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD,
                    crate::query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD,
                ],
                future_acceleration_hooks: vec![PresentationAccelerationHook::WorldBatch],
                observability: observability.clone(),
            },
            PresentationPass {
                id: SmolStr::new("shade_primary"),
                kind: PresentationPassKind::ShadePrimary {
                    contract: ShadePrimaryPassContract {
                        primary_hit_attachment: primary_hit_name.clone(),
                        surface_attachment: surface_name.clone(),
                        radiance_attachment: Some(radiance_name.clone()),
                        medium_attachment: Some(medium_name.clone()),
                        output_attachment: shaded_color_name.clone(),
                        compatibility_recipe: true,
                    },
                },
                consumes: vec![
                    primary_hit_name.clone(),
                    surface_name.clone(),
                    radiance_name.clone(),
                    medium_name.clone(),
                    SmolStr::new("lighting.key_light"),
                    SmolStr::new("lighting.fill_direction"),
                    SmolStr::new("lighting.fill_strength"),
                    SmolStr::new("lighting.ambient_color"),
                ],
                materializes: vec![shaded_color_name.clone()],
                binding: Some(bindings[4].id.clone()),
                query_dependencies: Vec::new(),
                future_acceleration_hooks: Vec::new(),
                observability: observability.clone(),
            },
            PresentationPass {
                id: if enable_temporal {
                    SmolStr::new("motion_resolve")
                } else {
                    SmolStr::new("composite_color")
                },
                kind: if enable_temporal {
                    PresentationPassKind::MotionResolve {
                        contract: MotionResolvePassContract {
                            primary_hit_attachment: primary_hit_name.clone(),
                            output_attachment: motion_name
                                .as_ref()
                                .expect("temporal motion attachment")
                                .clone(),
                            history_primary_hit_attachment: history_primary_hit_name.clone(),
                        },
                    }
                } else {
                    PresentationPassKind::CompositeColor {
                        contract: crate::presentation_plan::CompositeColorPassContract {
                            input_attachment: shaded_color_name.clone(),
                            output_attachment: color_name.clone(),
                        },
                    }
                },
                consumes: if enable_temporal {
                    let mut consumes = vec![primary_hit_name.clone()];
                    if let Some(history_primary_hit_name) = &history_primary_hit_name {
                        consumes.push(history_primary_hit_name.clone());
                    }
                    consumes
                } else {
                    vec![shaded_color_name.clone()]
                },
                materializes: if enable_temporal {
                    vec![
                        motion_name
                            .as_ref()
                            .expect("temporal motion attachment")
                            .clone(),
                    ]
                } else {
                    vec![color_name.clone()]
                },
                binding: Some(bindings[5].id.clone()),
                query_dependencies: Vec::new(),
                future_acceleration_hooks: if enable_temporal {
                    vec![PresentationAccelerationHook::TemporalHistory]
                } else {
                    Vec::new()
                },
                observability: observability.clone(),
            },
        ];
        if enable_temporal {
            passes.push(PresentationPass {
                id: SmolStr::new("temporal_resolve"),
                kind: PresentationPassKind::TemporalResolve {
                    contract: TemporalResolvePassContract {
                        input_attachment: shaded_color_name.clone(),
                        primary_hit_attachment: primary_hit_name.clone(),
                        motion_attachment: motion_name
                            .as_ref()
                            .expect("temporal motion attachment")
                            .clone(),
                        history_color_attachment: history_color_name
                            .as_ref()
                            .expect("temporal history color attachment")
                            .clone(),
                        history_primary_hit_attachment: history_primary_hit_name.clone(),
                        output_attachment: color_name.clone(),
                        neighborhood_clamp: true,
                        history_weight_numerator: 3,
                        history_weight_denominator: 4,
                    },
                },
                consumes: vec![
                    shaded_color_name.clone(),
                    primary_hit_name.clone(),
                    motion_name
                        .as_ref()
                        .expect("temporal motion attachment")
                        .clone(),
                    history_color_name
                        .as_ref()
                        .expect("temporal history color attachment")
                        .clone(),
                    history_primary_hit_name
                        .as_ref()
                        .expect("temporal history primary hit attachment")
                        .clone(),
                ],
                materializes: vec![
                    color_name.clone(),
                    history_color_name
                        .as_ref()
                        .expect("temporal history color attachment")
                        .clone(),
                    history_primary_hit_name
                        .as_ref()
                        .expect("temporal history primary hit attachment")
                        .clone(),
                ],
                binding: Some(bindings[6].id.clone()),
                query_dependencies: Vec::new(),
                future_acceleration_hooks: vec![PresentationAccelerationHook::TemporalHistory],
                observability: observability.clone(),
            });
        }
        if export_color {
            let export_binding = bindings
                .last()
                .expect("export pass should append a binding")
                .id
                .clone();
            passes.push(PresentationPass {
                id: SmolStr::new("export_color"),
                kind: PresentationPassKind::ExportAttachment {
                    attachment: color_name.clone(),
                },
                consumes: vec![color_name.clone()],
                materializes: Vec::new(),
                binding: Some(export_binding),
                query_dependencies: Vec::new(),
                future_acceleration_hooks: Vec::new(),
                observability: observability.clone(),
            });
        }

        let mut outputs = vec![
            primary_hit_attachment,
            surface_attachment,
            radiance_attachment,
            medium_attachment,
            shaded_color_attachment,
            color_attachment,
        ];
        if let Some(depth_attachment) = depth_attachment {
            outputs.push(depth_attachment);
        }
        if let Some(world_normal_attachment) = world_normal_attachment {
            outputs.push(world_normal_attachment);
        }
        if let Some(motion_attachment) = motion_attachment {
            outputs.push(motion_attachment);
        }
        if let Some(history_color_attachment) = history_color_attachment.clone() {
            outputs.push(history_color_attachment);
        }
        if let Some(history_primary_hit_attachment) = history_primary_hit_attachment.clone() {
            outputs.push(history_primary_hit_attachment);
        }

        let temporal = if enable_temporal {
            Some(TemporalContract::first_color_path(
                &view,
                history_color_attachment
                    .as_ref()
                    .expect("temporal history color attachment"),
                history_primary_hit_attachment
                    .as_ref()
                    .expect("temporal history primary hit attachment"),
                1,
            ))
        } else {
            None
        };
        let quality = authored_quality_contract(
            metadata,
            temporal
                .as_ref()
                .map(|contract| contract.reuse)
                .unwrap_or(TemporalReuseMode::Disabled),
        );

        let mut frame_artifacts = vec![
            FrameArtifactContract {
                id: SmolStr::new("artifact.primary_hit"),
                attachment: primary_hit_name.clone(),
                producer_pass: SmolStr::new("primary_visibility"),
                materialized: true,
            },
            FrameArtifactContract {
                id: SmolStr::new("artifact.surface"),
                attachment: surface_name,
                producer_pass: SmolStr::new("surface_resolve"),
                materialized: true,
            },
            FrameArtifactContract {
                id: SmolStr::new("artifact.radiance"),
                attachment: radiance_name,
                producer_pass: SmolStr::new("participants_resolve"),
                materialized: true,
            },
            FrameArtifactContract {
                id: SmolStr::new("artifact.medium"),
                attachment: medium_name,
                producer_pass: SmolStr::new("participants_resolve"),
                materialized: true,
            },
            FrameArtifactContract {
                id: SmolStr::new("artifact.shaded_color"),
                attachment: shaded_color_name,
                producer_pass: SmolStr::new("shade_primary"),
                materialized: true,
            },
        ];
        if let Some(depth_name) = depth_name.clone() {
            frame_artifacts.push(FrameArtifactContract {
                id: SmolStr::new("artifact.depth"),
                attachment: depth_name,
                producer_pass: SmolStr::new("primary_visibility"),
                materialized: true,
            });
        }
        if let Some(world_normal_name) = world_normal_name.clone() {
            frame_artifacts.push(FrameArtifactContract {
                id: SmolStr::new("artifact.world_normal"),
                attachment: world_normal_name,
                producer_pass: SmolStr::new("primary_visibility"),
                materialized: true,
            });
        }
        if enable_temporal {
            frame_artifacts.push(FrameArtifactContract {
                id: SmolStr::new("artifact.motion"),
                attachment: motion_name.clone().expect("temporal motion attachment"),
                producer_pass: SmolStr::new("motion_resolve"),
                materialized: true,
            });
            frame_artifacts.push(FrameArtifactContract {
                id: SmolStr::new("artifact.history_color"),
                attachment: history_color_name
                    .clone()
                    .expect("temporal history color attachment"),
                producer_pass: SmolStr::new("temporal_resolve"),
                materialized: true,
            });
            frame_artifacts.push(FrameArtifactContract {
                id: SmolStr::new("artifact.history_primary_hit"),
                attachment: history_primary_hit_name
                    .clone()
                    .expect("temporal history primary hit attachment"),
                producer_pass: SmolStr::new("temporal_resolve"),
                materialized: true,
            });
        }
        frame_artifacts.push(FrameArtifactContract {
            id: SmolStr::new("artifact.color"),
            attachment: color_name,
            producer_pass: if enable_temporal {
                SmolStr::new("temporal_resolve")
            } else {
                SmolStr::new("composite_color")
            },
            materialized: true,
        });

        Self {
            name,
            view: view.clone(),
            frame: FrameContract {
                outputs,
                primary_hit: Some(PrimaryHitAttachmentContract::hit3(primary_hit_name.clone())),
                temporal,
                quality,
                lighting: LightingContract::first_color_path(
                    authored_lighting.key_light,
                    metadata.lighting.light_compatibility_alias
                        && metadata.lighting.grouped.is_none(),
                    authored_lighting.fill_direction,
                    metadata.lighting.fill_dir_compatibility_alias
                        && metadata.lighting.grouped.is_none(),
                    authored_lighting.fill_strength,
                    authored_lighting.ambient_color,
                    metadata.lighting.lights.is_some(),
                ),
                observability: PresentationObservabilityProfile::preview_compatibility(),
            },
            passes,
            frame_artifacts,
            bindings,
            observability,
        }
    }

    pub fn binding(&self, id: &PresentationBindingId) -> Option<&PresentationBindingSummary> {
        self.bindings.iter().find(|binding| binding.id == *id)
    }

    pub fn export_binding(&self) -> Option<&PresentationBindingSummary> {
        self.passes
            .iter()
            .find(|pass| matches!(pass.kind, PresentationPassKind::ExportAttachment { .. }))
            .and_then(|pass| pass.binding.as_ref())
            .and_then(|id| self.binding(id))
    }

    pub fn semantic_artifact_contracts(&self) -> Vec<SemanticArtifactContract> {
        let mut out = self
            .frame_artifacts
            .iter()
            .filter_map(|artifact| presentation_semantic_artifact_contract(self, artifact))
            .collect::<Vec<_>>();
        out.extend(acceleration::observer_acceleration_contracts(
            AccelerationObserver::Presentation,
            self.name.as_str(),
        ));
        out
    }

    pub fn artifact_uses(&self) -> Vec<ArtifactUse> {
        let contracts = self
            .semantic_artifact_contracts()
            .into_iter()
            .map(|contract| (contract.id.clone(), contract))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut uses = Vec::new();
        for artifact in &self.frame_artifacts {
            uses.push(ArtifactUse {
                actor: artifact.producer_pass.clone(),
                artifact_id: artifact.id.clone(),
                kind: ArtifactUseKind::Produce,
                source: ArtifactUseSource::Plan,
                required_validity: None,
            });
        }
        for pass in &self.passes {
            for attachment in &pass.consumes {
                let Some(artifact) = self
                    .frame_artifacts
                    .iter()
                    .find(|artifact| artifact.attachment == *attachment)
                else {
                    continue;
                };
                let Some(attachment_contract) = self.frame.attachment(attachment.as_str()) else {
                    continue;
                };
                let (source, required_validity) = if matches!(
                    attachment_contract.lifetime,
                    AttachmentLifetime::HistorySlot(_)
                ) {
                    (
                        ArtifactUseSource::ArtifactStore,
                        contracts
                            .get(&artifact.id)
                            .map(|contract| contract.validity.clone()),
                    )
                } else {
                    (ArtifactUseSource::Plan, None)
                };
                uses.push(ArtifactUse {
                    actor: pass.id.clone(),
                    artifact_id: artifact.id.clone(),
                    kind: ArtifactUseKind::Load,
                    source,
                    required_validity,
                });
            }
            for attachment in &pass.materializes {
                let Some(artifact) = self
                    .frame_artifacts
                    .iter()
                    .find(|artifact| artifact.attachment == *attachment)
                else {
                    continue;
                };
                let Some(attachment_contract) = self.frame.attachment(attachment.as_str()) else {
                    continue;
                };
                if matches!(
                    attachment_contract.lifetime,
                    AttachmentLifetime::HistorySlot(_)
                ) {
                    uses.push(ArtifactUse {
                        actor: pass.id.clone(),
                        artifact_id: artifact.id.clone(),
                        kind: ArtifactUseKind::Preserve,
                        source: ArtifactUseSource::Plan,
                        required_validity: contracts
                            .get(&artifact.id)
                            .map(|contract| contract.validity.clone()),
                    });
                }
            }
        }
        uses.extend(
            acceleration::observer_acceleration_contracts(
                AccelerationObserver::Presentation,
                self.name.as_str(),
            )
            .into_iter()
            .map(|contract| ArtifactUse {
                actor: contract.producer.clone(),
                artifact_id: contract.id.clone(),
                kind: ArtifactUseKind::Produce,
                source: ArtifactUseSource::Plan,
                required_validity: None,
            }),
        );
        uses
    }

    pub fn validate_acceleration_contracts(&self) -> Vec<SmolStr> {
        acceleration::validate_observer_acceleration_contracts(
            AccelerationObserver::Presentation,
            self.name.as_str(),
            &self.semantic_artifact_contracts(),
        )
    }

    pub fn apply_participant_policy(&mut self, radiance_enabled: bool, medium_enabled: bool) {
        if !radiance_enabled {
            self.frame
                .outputs
                .retain(|attachment| attachment.name != "radiance");
            self.frame_artifacts
                .retain(|artifact| artifact.attachment != "radiance");
        }
        if !medium_enabled {
            self.frame
                .outputs
                .retain(|attachment| attachment.name != "medium");
            self.frame_artifacts
                .retain(|artifact| artifact.attachment != "medium");
        }

        if let Some(shade_pass) = self
            .passes
            .iter_mut()
            .find(|pass| matches!(pass.kind, PresentationPassKind::ShadePrimary { .. }))
        {
            if let PresentationPassKind::ShadePrimary { contract } = &mut shade_pass.kind {
                if !radiance_enabled {
                    contract.radiance_attachment = None;
                }
                if !medium_enabled {
                    contract.medium_attachment = None;
                }
            }
            if !radiance_enabled {
                shade_pass.consumes.retain(|item| item != "radiance");
            }
            if !medium_enabled {
                shade_pass.consumes.retain(|item| item != "medium");
            }
        }

        if let Some(index) = self
            .passes
            .iter()
            .position(|pass| matches!(pass.kind, PresentationPassKind::ParticipantsResolve { .. }))
        {
            let pass = &mut self.passes[index];
            if let PresentationPassKind::ParticipantsResolve { contract } = &mut pass.kind {
                if !radiance_enabled {
                    contract.radiance_query_contract = None;
                    contract.radiance_attachment = None;
                }
                if !medium_enabled {
                    contract.medium_query_contract = None;
                    contract.medium_attachment = None;
                }
            }
            if !radiance_enabled {
                pass.materializes.retain(|item| item != "radiance");
                pass.consumes.retain(|item| item != "radiance");
            }
            if !medium_enabled {
                pass.materializes.retain(|item| item != "medium");
                pass.consumes.retain(|item| item != "medium");
            }
            pass.query_dependencies = pass
                .query_dependencies
                .iter()
                .copied()
                .filter(|dependency| {
                    (radiance_enabled
                        || *dependency != crate::query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD)
                        && (medium_enabled
                            || *dependency
                                != crate::query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD)
                })
                .collect();
            if !radiance_enabled && !medium_enabled {
                self.passes.remove(index);
            }
        }
    }

    pub fn validate(&self) -> Vec<PresentationPlanValidationError> {
        validate_plan(self)
    }
}

impl PresentationObservability {
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

fn presentation_semantic_artifact_contract(
    plan: &PresentationPlan,
    artifact: &FrameArtifactContract,
) -> Option<SemanticArtifactContract> {
    let attachment = plan.frame.attachment(artifact.attachment.as_str())?;
    let history_slot = plan.frame.temporal.as_ref().and_then(|temporal| {
        temporal
            .history_slots
            .iter()
            .find(|slot| slot.attachment == artifact.attachment)
            .map(|slot| (temporal, slot))
    });
    let kind = if history_slot.is_some() {
        SemanticArtifactKind::PresentationHistory
    } else {
        SemanticArtifactKind::PresentationAttachment
    };
    let compatibility = if let Some((temporal, _)) = history_slot {
        ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::PreviousSnapshotEpoch,
            transition: ArtifactTransitionRelation {
                compatibility: Some(temporal.transition_compatibility),
                requires_previous_snapshot: true,
            },
            policy: ArtifactPolicyCompatibility {
                mode: crate::artifact_key::ArtifactPolicyDigestMode::CompatibleRange,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: SemanticEvidenceSummary::contract_bound().origin,
                scope: SemanticEvidenceSummary::contract_bound().scope,
            },
        }
    } else {
        ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::ExactSnapshot,
            transition: ArtifactTransitionRelation {
                compatibility: None,
                requires_previous_snapshot: false,
            },
            policy: ArtifactPolicyCompatibility {
                mode: crate::artifact_key::ArtifactPolicyDigestMode::Exact,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: SemanticEvidenceSummary::contract_bound().origin,
                scope: SemanticEvidenceSummary::contract_bound().scope,
            },
        }
    };
    let validity = if let Some((temporal, slot)) = history_slot {
        let mut predicates = vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::PreviousSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::LayoutSignatureMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::HistoryCompatibilityMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::CompatibleChange(
                temporal.transition_compatibility,
            )),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::MaxPresentationFrameAge(
                u64::from(slot.max_age_frames),
            )),
        ];
        if temporal.requires_snapshot_lineage_match {
            predicates.push(ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::SnapshotLineageMatchesCurrent,
            ));
        }
        ArtifactValidityRule::all(predicates)
    } else {
        ArtifactValidityRule::Always
    };
    Some(SemanticArtifactContract {
        id: artifact.id.clone(),
        kind,
        logical_schema: presentation_artifact_logical_schema(
            attachment,
            history_slot.map(|(_, slot)| slot),
        ),
        compatibility,
        acceleration: None,
        validity,
        producer: artifact.producer_pass.clone(),
        consumer: SmolStr::new("presentation.frame"),
        deterministic: true,
        version: PRESENTATION_CONTRACT_VERSION,
        transition: None,
        evidence_summary: SemanticEvidenceSummary::contract_bound(),
    })
}

fn presentation_artifact_logical_schema(
    attachment: &FrameAttachmentContract,
    history_slot: Option<&crate::presentation_contract::TemporalHistorySlotContract>,
) -> ArtifactLogicalSchema {
    let mut fields = vec![
        ArtifactLogicalField::new("attachment", attachment.name.clone()),
        ArtifactLogicalField::new("kind", format!("{:?}", attachment.kind)),
        ArtifactLogicalField::new("element_schema", format!("{:?}", attachment.element_schema)),
        ArtifactLogicalField::new("lifetime", format!("{:?}", attachment.lifetime)),
        ArtifactLogicalField::new("resolution", format!("{:?}", attachment.resolution)),
        ArtifactLogicalField::new(
            "scale",
            format!(
                "{}x{}",
                attachment.scale.divisor_x, attachment.scale.divisor_y
            ),
        ),
        ArtifactLogicalField::new("clear_policy", format!("{:?}", attachment.clear_policy)),
    ];
    if let Some(slot) = history_slot {
        fields.push(ArtifactLogicalField::new(
            "history_slot",
            slot.slot.to_string(),
        ));
        fields.push(ArtifactLogicalField::new(
            "history_role",
            format!("{:?}", slot.role),
        ));
        fields.push(ArtifactLogicalField::new(
            "history_compatibility_hash",
            slot.compatibility.compatibility_hash().to_string(),
        ));
    }
    ArtifactLogicalSchema {
        namespace: SmolStr::new("presentation"),
        name: SmolStr::new(if history_slot.is_some() {
            "history-slot"
        } else {
            "attachment"
        }),
        fields,
    }
}

fn authored_viewport_sources(metadata: &hir::PresentationMetadata) -> (&'static str, &'static str) {
    if metadata
        .view
        .viewport
        .as_ref()
        .and_then(|body| body_terminal_call(body))
        .is_some_and(|(callee, _)| matches!(callee.as_str(), "viewport" | "Viewport"))
    {
        ("view.viewport.width", "view.viewport.height")
    } else {
        ("view.width", "view.height")
    }
}

fn authored_output_selection(metadata: &hir::PresentationMetadata) -> AuthoredOutputSelection {
    let mut selection = AuthoredOutputSelection::default();
    let Some(body) = metadata.frame.outputs.as_ref() else {
        return selection;
    };
    let Some((callee, args)) = body_terminal_call(body) else {
        return selection;
    };
    if !matches!(callee.as_str(), "frame_outputs" | "FrameOutputs") {
        return selection;
    }
    if let Some(value) = call_named_arg_bool(body, args, "color") {
        selection.color = value;
    }
    if let Some(value) = call_named_arg_bool(body, args, "depth") {
        selection.depth = value;
    }
    if let Some(value) = call_named_arg_bool(body, args, "normal") {
        selection.world_normal = value;
    }
    if let Some(value) = call_named_arg_bool(body, args, "motion") {
        selection.motion = value;
    }
    selection
}

fn authored_temporal_history_selection(
    metadata: &hir::PresentationMetadata,
) -> Option<AuthoredTemporalHistorySelection> {
    let body = metadata.frame.history.as_ref()?;
    let (callee, args) = body_terminal_call(body)?;
    if !matches!(callee.as_str(), "temporal_history" | "TemporalHistory") {
        return None;
    }
    Some(AuthoredTemporalHistorySelection {
        color: call_named_arg_bool(body, args, "color").unwrap_or(true),
    })
}

fn authored_quality_contract(
    metadata: &hir::PresentationMetadata,
    temporal_mode: TemporalReuseMode,
) -> RealtimeQualityContract {
    let Some(body) = metadata.frame.quality.as_ref() else {
        return RealtimeQualityContract::named(RealtimeQualityTier::Realtime60)
            .with_temporal_mode(temporal_mode);
    };
    let Some((callee, args)) = body_terminal_call(body) else {
        return RealtimeQualityContract::named(RealtimeQualityTier::Realtime60)
            .with_temporal_mode(temporal_mode);
    };
    if !matches!(callee.as_str(), "realtime_quality" | "RealtimeQuality") {
        return RealtimeQualityContract::named(RealtimeQualityTier::Realtime60)
            .with_temporal_mode(temporal_mode);
    }

    let target_fps = call_named_arg_u32(body, args, "target_fps").unwrap_or(60);
    let mut contract = RealtimeQualityContract::named(quality_tier_for_target_fps(target_fps))
        .with_temporal_mode(temporal_mode);
    contract.target_fps = target_fps;
    if let Some(value) = call_named_arg_bool(body, args, "allow_dynamic_resolution") {
        contract.allow_dynamic_resolution = value;
        if !value {
            contract.internal_resolution_scale = 1.0;
        }
    }
    if let Some(value) = call_named_arg_i32(body, args, "primary_max_steps") {
        contract.primary_max_steps = value;
    }
    if let Some(value) = call_named_arg_bool(body, args, "allow_radiance") {
        contract.allow_radiance = value;
    }
    if let Some(value) = call_named_arg_bool(body, args, "allow_media") {
        contract.allow_media = value;
    }
    if let Some(value) = call_named_arg_bool(body, args, "allow_half_res_participants") {
        contract.allow_half_res_participants = value;
    }
    if let Some(value) = call_named_arg_bool(body, args, "allow_hit_compaction") {
        contract.allow_hit_compaction = value;
    }
    contract
}

fn quality_tier_for_target_fps(target_fps: u32) -> RealtimeQualityTier {
    match target_fps {
        120.. => RealtimeQualityTier::Realtime120,
        0..=30 => RealtimeQualityTier::Debug,
        _ => RealtimeQualityTier::Realtime60,
    }
}

fn authored_lighting_selection(metadata: &hir::PresentationMetadata) -> AuthoredLightingSelection {
    let mut selection = AuthoredLightingSelection {
        key_light: metadata.lighting.light.is_some(),
        fill_direction: metadata.lighting.fill_dir.is_some(),
        fill_strength: metadata.lighting.fill_strength.is_some(),
        ambient_color: metadata.lighting.ambient_color.is_some(),
    };
    let Some(body) = metadata.lighting.grouped.as_ref() else {
        return selection;
    };
    let Some((callee, args)) = body_terminal_call(body) else {
        return selection;
    };
    if !matches!(callee.as_str(), "key_light" | "PresentationLighting") {
        return selection;
    }
    selection.key_light = call_named_arg_expr(args, "light").is_some() || selection.key_light;
    selection.fill_direction =
        call_named_arg_expr(args, "fill_direction").is_some() || selection.fill_direction;
    selection.fill_strength =
        call_named_arg_expr(args, "fill_strength").is_some() || selection.fill_strength;
    selection.ambient_color =
        call_named_arg_expr(args, "ambient_color").is_some() || selection.ambient_color;
    selection
}

fn body_terminal_call<'a>(body: &'a hir::Body) -> Option<(&'a SmolStr, &'a [hir::Arg])> {
    let stmt = body.root_stmts.last()?;
    let expr_id = match &body.stmts[*stmt] {
        hir::Stmt::Expr(expr) => *expr,
        hir::Stmt::Return(Some(expr)) => *expr,
        _ => return None,
    };
    let hir::Expr::Call { callee, args, .. } = &body.exprs[expr_id] else {
        return None;
    };
    let hir::Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    Some((name, args.as_slice()))
}

fn call_named_arg_expr(args: &[hir::Arg], name: &str) -> Option<hir::Idx<hir::Expr>> {
    args.iter().find_map(|arg| match arg {
        hir::Arg::Named {
            name: arg_name,
            value,
            ..
        } if arg_name == name => Some(*value),
        _ => None,
    })
}

fn call_named_arg_bool(body: &hir::Body, args: &[hir::Arg], name: &str) -> Option<bool> {
    call_named_arg_expr(args, name).and_then(|expr| match &body.exprs[expr] {
        hir::Expr::Literal(hir::Literal::Boolean(value)) => Some(*value),
        _ => None,
    })
}

fn call_named_arg_u32(body: &hir::Body, args: &[hir::Arg], name: &str) -> Option<u32> {
    call_named_arg_expr(args, name).and_then(|expr| match &body.exprs[expr] {
        hir::Expr::Literal(hir::Literal::Integer(value)) => Some(*value as u32),
        hir::Expr::Literal(hir::Literal::Float(value)) => Some(*value as u32),
        _ => None,
    })
}

fn call_named_arg_i32(body: &hir::Body, args: &[hir::Arg], name: &str) -> Option<i32> {
    call_named_arg_expr(args, name).and_then(|expr| match &body.exprs[expr] {
        hir::Expr::Literal(hir::Literal::Integer(value)) => Some(*value as i32),
        hir::Expr::Literal(hir::Literal::Float(value)) => Some(*value as i32),
        _ => None,
    })
}

impl ScreenSampleGenerationContract {
    pub fn from_view(view: &ViewContract) -> Self {
        let samples_per_pixel = 1;
        Self {
            viewport_width_source: view.screen_lattice.width_source.clone(),
            viewport_height_source: view.screen_lattice.height_source.clone(),
            samples_per_pixel,
            jitter_source: SmolStr::new("view.jitter_pixels"),
            item_count_expression: screen_sample_item_count_expression(
                &view.screen_lattice.width_source,
                &view.screen_lattice.height_source,
                samples_per_pixel,
            ),
            output_item_record: SmolStr::new("ScreenSampleQuery"),
        }
    }

    fn expected_item_count_expression(&self) -> SmolStr {
        screen_sample_item_count_expression(
            &self.viewport_width_source,
            &self.viewport_height_source,
            self.samples_per_pixel,
        )
    }
}

fn screen_sample_item_count_expression(
    width_source: &SmolStr,
    height_source: &SmolStr,
    samples_per_pixel: u32,
) -> SmolStr {
    SmolStr::new(format!(
        "{} * {} * {}",
        width_source, height_source, samples_per_pixel
    ))
}

pub fn plans_for_module(
    module: &hir::Module,
    default_backend: DispatchBackend,
) -> Vec<PresentationPlan> {
    module
        .functions
        .iter()
        .filter_map(|(_, func)| PresentationPlan::from_view_function(func, default_backend))
        .collect()
}

pub fn validate_plan(plan: &PresentationPlan) -> Vec<PresentationPlanValidationError> {
    let mut errors = Vec::new();
    if plan.passes.is_empty() {
        errors.push(validation_error(
            "presentation plan must contain at least one pass",
        ));
    }

    let binding_ids = plan
        .bindings
        .iter()
        .map(|binding| binding.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    for (_pass_index, pass) in plan.passes.iter().enumerate() {
        if let Some(binding) = &pass.binding
            && !binding_ids.contains(binding.as_str())
        {
            errors.push(validation_error(format!(
                "presentation pass '{}' references missing binding '{}'",
                pass.id,
                binding.as_str()
            )));
        }
        if let PresentationPassKind::GenerateScreenSamples { contract } = &pass.kind {
            if contract.samples_per_pixel == 0 {
                errors.push(validation_error(format!(
                    "screen sample pass '{}' must generate at least one sample per pixel",
                    pass.id
                )));
            }
            if !pass
                .materializes
                .iter()
                .any(|item| item == "screen_samples")
            {
                errors.push(validation_error(format!(
                    "screen sample pass '{}' must materialize 'screen_samples'",
                    pass.id
                )));
            }
            let expected = contract.expected_item_count_expression();
            if contract.item_count_expression != expected {
                errors.push(validation_error(format!(
                    "screen sample pass '{}' item count '{}' does not match viewport lattice '{}'",
                    pass.id, contract.item_count_expression, expected
                )));
            }
            if contract.output_item_record != "ScreenSampleQuery" {
                errors.push(validation_error(format!(
                    "screen sample pass '{}' must output ScreenSampleQuery items",
                    pass.id
                )));
            }
        }
    }

    let output_names = plan
        .frame
        .outputs
        .iter()
        .map(|output| output.name.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let mut history_slots = BTreeSet::new();
    for attachment in &plan.frame.outputs {
        if attachment.scale != expected_scale_for_resolution(attachment.resolution) {
            errors.push(validation_error(format!(
                "attachment '{}' scale {:?} does not match resolution class {:?}",
                attachment.name, attachment.scale, attachment.resolution
            )));
        }
        if !output_names.contains(attachment.name.as_str()) {
            errors.push(validation_error(format!(
                "attachment '{}' must appear exactly once in frame outputs",
                attachment.name
            )));
        }
        if let AttachmentLifetime::HistorySlot(slot) = attachment.lifetime {
            if !history_slots.insert(slot) {
                errors.push(validation_error(format!(
                    "history slot {} is assigned to multiple frame attachments",
                    slot
                )));
            }
            if attachment.clear_policy != AttachmentClearPolicy::PreservePrevious {
                errors.push(validation_error(format!(
                    "history attachment '{}' must preserve previous contents",
                    attachment.name
                )));
            }
        } else if attachment.clear_policy == AttachmentClearPolicy::PreservePrevious {
            errors.push(validation_error(format!(
                "non-history attachment '{}' cannot preserve previous contents",
                attachment.name
            )));
        }
    }
    if output_names.len() != plan.frame.outputs.len() {
        errors.push(validation_error(
            "frame attachment names must be unique within a presentation plan",
        ));
    }
    let screen_samples_per_pixel = plan
        .passes
        .iter()
        .find_map(|pass| match &pass.kind {
            PresentationPassKind::GenerateScreenSamples { contract } => {
                Some(contract.samples_per_pixel)
            }
            _ => None,
        })
        .unwrap_or(1);
    let has_motion_attachment = plan
        .frame
        .outputs
        .iter()
        .any(|attachment| attachment.kind == FrameAttachmentKind::Motion);
    let has_history_attachment = plan
        .frame
        .outputs
        .iter()
        .any(|attachment| matches!(attachment.lifetime, AttachmentLifetime::HistorySlot(_)));
    let has_motion_pass = plan
        .passes
        .iter()
        .any(|pass| matches!(pass.kind, PresentationPassKind::MotionResolve { .. }));
    let has_temporal_pass = plan
        .passes
        .iter()
        .any(|pass| matches!(pass.kind, PresentationPassKind::TemporalResolve { .. }));
    for error in plan.frame.quality.validate() {
        errors.push(validation_error(error));
    }
    if plan.frame.quality.temporal_mode
        != plan
            .frame
            .temporal
            .as_ref()
            .map(|temporal| temporal.reuse)
            .unwrap_or(TemporalReuseMode::Disabled)
    {
        errors.push(validation_error(
            "frame quality temporal_mode must match the frame temporal contract",
        ));
    }
    if let Some(temporal) = &plan.frame.temporal {
        if matches!(temporal.reuse, TemporalReuseMode::Disabled) {
            errors.push(validation_error(
                "temporal contract must not declare Disabled reuse when attached to a frame",
            ));
        }
        if temporal.history_slots.is_empty() {
            errors.push(validation_error(
                "temporal contract must declare at least one history slot",
            ));
        }
        if !has_temporal_pass {
            errors.push(validation_error(
                "temporal contract requires a temporal resolve pass",
            ));
        }
        if matches!(temporal.reuse, TemporalReuseMode::ReprojectColorAndMotion)
            && (!has_motion_attachment || !has_motion_pass)
        {
            errors.push(validation_error(
                "ReprojectColorAndMotion requires both a motion attachment and a motion resolve pass",
            ));
        }
        let mut temporal_slots = BTreeSet::new();
        let mut has_color_slot = false;
        let mut has_continuation_slot = false;
        for history_slot in &temporal.history_slots {
            if !temporal_slots.insert(history_slot.slot) {
                errors.push(validation_error(format!(
                    "temporal contract reuses history slot {} more than once",
                    history_slot.slot
                )));
            }
            if history_slot.max_age_frames == 0 {
                errors.push(validation_error(format!(
                    "temporal history attachment '{}' must allow at least one frame of reuse",
                    history_slot.attachment
                )));
            }
            let Some(attachment) = plan.frame.attachment(history_slot.attachment.as_str()) else {
                errors.push(validation_error(format!(
                    "temporal contract references missing history attachment '{}'",
                    history_slot.attachment
                )));
                continue;
            };
            if attachment.lifetime != AttachmentLifetime::HistorySlot(history_slot.slot) {
                errors.push(validation_error(format!(
                    "temporal history attachment '{}' must bind slot {} through AttachmentLifetime::HistorySlot",
                    history_slot.attachment, history_slot.slot
                )));
            }
            let expected_key =
                crate::presentation_contract::HistoryCompatibilityKey::from_attachment(
                    &plan.view,
                    attachment,
                    screen_samples_per_pixel,
                );
            if history_slot.compatibility != expected_key {
                errors.push(validation_error(format!(
                    "temporal history attachment '{}' must preserve the canonical compatibility key",
                    history_slot.attachment
                )));
            }
            match history_slot.role {
                TemporalHistoryRole::ReprojectedColor => {
                    has_color_slot = true;
                    if attachment.kind != FrameAttachmentKind::Color {
                        errors.push(validation_error(format!(
                            "temporal color history attachment '{}' must use Color semantics",
                            history_slot.attachment
                        )));
                    }
                }
                TemporalHistoryRole::ContinuationPrimaryHit => {
                    has_continuation_slot = true;
                    if attachment.kind != FrameAttachmentKind::PrimaryHit {
                        errors.push(validation_error(format!(
                            "temporal continuation attachment '{}' must preserve PrimaryHit semantics",
                            history_slot.attachment
                        )));
                    }
                }
            }
        }
        if !has_color_slot {
            errors.push(validation_error(
                "temporal contract must declare a reprojected color history slot",
            ));
        }
        if matches!(temporal.reuse, TemporalReuseMode::ReprojectColorAndMotion)
            && !has_continuation_slot
        {
            errors.push(validation_error(
                "ReprojectColorAndMotion requires a continuation primary-hit history slot",
            ));
        }
    } else {
        if has_history_attachment {
            errors.push(validation_error(
                "history attachments require an explicit temporal contract",
            ));
        }
        if has_motion_pass || has_temporal_pass {
            errors.push(validation_error(
                "temporal presentation passes require an explicit temporal contract",
            ));
        }
    }
    if let Some(primary_hit) = &plan.frame.primary_hit {
        match plan.frame.attachment(primary_hit.attachment.as_str()) {
            Some(attachment)
                if attachment.kind == FrameAttachmentKind::PrimaryHit
                    && attachment.element_schema
                        == AttachmentElementSchema::NamedRecord(primary_hit.record.clone()) => {}
            Some(attachment) if attachment.kind == FrameAttachmentKind::PrimaryHit => {
                errors.push(validation_error(format!(
                    "primary hit attachment '{}' must use '{}' element schema",
                    primary_hit.attachment, primary_hit.record
                )))
            }
            Some(_) => errors.push(validation_error(format!(
                "primary hit attachment '{}' must use PrimaryHit semantics",
                primary_hit.attachment
            ))),
            None => errors.push(validation_error(format!(
                "primary hit attachment '{}' is not declared in frame outputs",
                primary_hit.attachment
            ))),
        }
        let expected_fields =
            PrimaryHitAttachmentContract::hit3(primary_hit.attachment.clone()).fields;
        if primary_hit.record != "Hit3" {
            errors.push(validation_error(format!(
                "primary hit attachment '{}' must preserve Hit3 provenance",
                primary_hit.attachment
            )));
        }
        if primary_hit.fields != expected_fields {
            errors.push(validation_error(format!(
                "primary hit attachment '{}' must preserve canonical Hit3 fields",
                primary_hit.attachment
            )));
        }
        if primary_hit.sample_identity != "screen_lattice.row_major_top_left_pixel_center" {
            errors.push(validation_error(format!(
                "primary hit attachment '{}' must preserve canonical sample identity",
                primary_hit.attachment
            )));
        }
    }
    for artifact in &plan.frame_artifacts {
        if !output_names.contains(artifact.attachment.as_str()) {
            errors.push(validation_error(format!(
                "frame artifact '{}' references missing attachment '{}'",
                artifact.id, artifact.attachment
            )));
        }
    }
    let semantic_artifacts = plan
        .semantic_artifact_contracts()
        .into_iter()
        .map(|contract| (contract.id.clone(), contract))
        .collect::<std::collections::BTreeMap<_, _>>();
    for note in plan.validate_acceleration_contracts() {
        errors.push(validation_error(note));
    }
    let produced_by = plan
        .frame_artifacts
        .iter()
        .map(|artifact| (artifact.id.clone(), artifact.producer_pass.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for artifact in &plan.frame_artifacts {
        if !plan
            .passes
            .iter()
            .any(|pass| pass.id == artifact.producer_pass)
        {
            errors.push(validation_error(format!(
                "frame artifact '{}' producer pass '{}' is missing from the plan",
                artifact.id, artifact.producer_pass
            )));
        }
    }
    for use_record in plan.artifact_uses() {
        match (use_record.kind, use_record.source) {
            (ArtifactUseKind::Load, ArtifactUseSource::Plan) => {
                if !produced_by.contains_key(&use_record.artifact_id) {
                    errors.push(validation_error(format!(
                        "artifact '{}' is loaded by '{}' without a producer in the plan",
                        use_record.artifact_id, use_record.actor
                    )));
                }
            }
            (ArtifactUseKind::Load, ArtifactUseSource::ArtifactStore) => {
                let Some(contract) = semantic_artifacts.get(&use_record.artifact_id) else {
                    errors.push(validation_error(format!(
                        "artifact '{}' is loaded from the artifact store by '{}' but is not declared",
                        use_record.artifact_id, use_record.actor
                    )));
                    continue;
                };
                if !contract.validity.is_explicit() {
                    errors.push(validation_error(format!(
                        "artifact '{}' is reused by '{}' without an explicit validity rule",
                        use_record.artifact_id, use_record.actor
                    )));
                }
                let Some(frame_artifact) = plan
                    .frame_artifacts
                    .iter()
                    .find(|artifact| artifact.id == use_record.artifact_id)
                else {
                    continue;
                };
                let Some(attachment) = plan.frame.attachment(frame_artifact.attachment.as_str())
                else {
                    continue;
                };
                if !matches!(attachment.lifetime, AttachmentLifetime::HistorySlot(_)) {
                    errors.push(validation_error(format!(
                        "artifact '{}' is marked as store-reused by '{}' but is not a history slot",
                        use_record.artifact_id, use_record.actor
                    )));
                }
            }
            (ArtifactUseKind::Preserve, _) => {
                let Some(frame_artifact) = plan
                    .frame_artifacts
                    .iter()
                    .find(|artifact| artifact.id == use_record.artifact_id)
                else {
                    continue;
                };
                let Some(attachment) = plan.frame.attachment(frame_artifact.attachment.as_str())
                else {
                    continue;
                };
                if !matches!(attachment.lifetime, AttachmentLifetime::HistorySlot(_)) {
                    errors.push(validation_error(format!(
                        "artifact '{}' is preserved by '{}' but is not declared as a history slot",
                        use_record.artifact_id, use_record.actor
                    )));
                }
            }
            _ => {}
        }
    }
    for (pass_index, pass) in plan.passes.iter().enumerate() {
        match &pass.kind {
            PresentationPassKind::PrimaryVisibility { contract } => {
                if contract.query_contract != crate::query_contract::SPATIAL_NEAREST_BATCH_WORLD {
                    errors.push(validation_error(format!(
                        "primary visibility pass '{}' must route through spatial.nearest.batch.world",
                        pass.id
                    )));
                }
                if !output_names.contains(contract.primary_hit_attachment.as_str()) {
                    errors.push(validation_error(format!(
                        "primary visibility pass '{}' materializes missing primary hit attachment '{}'",
                        pass.id, contract.primary_hit_attachment
                    )));
                }
                if let Some(depth) = &contract.depth_attachment
                    && !output_names.contains(depth.as_str())
                {
                    errors.push(validation_error(format!(
                        "primary visibility pass '{}' materializes missing depth attachment '{}'",
                        pass.id, depth
                    )));
                }
                if let Some(world_normal) = &contract.world_normal_attachment
                    && !output_names.contains(world_normal.as_str())
                {
                    errors.push(validation_error(format!(
                        "primary visibility pass '{}' materializes missing world-normal attachment '{}'",
                        pass.id, world_normal
                    )));
                }
            }
            PresentationPassKind::SurfaceResolve { contract } => {
                if contract.query_contract != crate::query_contract::SURFACE_SAMPLE_BATCH_WORLD {
                    errors.push(validation_error(format!(
                        "surface resolve pass '{}' must route through surface.sample.batch.world",
                        pass.id
                    )));
                }
                if !output_names.contains(contract.primary_hit_attachment.as_str()) {
                    errors.push(validation_error(format!(
                        "surface resolve pass '{}' requires primary-hit attachment '{}'",
                        pass.id, contract.primary_hit_attachment
                    )));
                }
                if !output_names.contains(contract.surface_attachment.as_str()) {
                    errors.push(validation_error(format!(
                        "surface resolve pass '{}' materializes missing surface attachment '{}'",
                        pass.id, contract.surface_attachment
                    )));
                }
            }
            PresentationPassKind::ParticipantsResolve { contract } => {
                if contract.radiance_attachment.is_some()
                    && contract.radiance_query_contract
                        != Some(crate::query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD)
                {
                    errors.push(validation_error(format!(
                        "participants resolve pass '{}' must route radiance through participants.radiance.batch.world",
                        pass.id
                    )));
                }
                if contract.medium_attachment.is_some()
                    && contract.medium_query_contract
                        != Some(crate::query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD)
                {
                    errors.push(validation_error(format!(
                        "participants resolve pass '{}' must route medium through participants.medium.batch.world",
                        pass.id
                    )));
                }
                if contract.radiance_attachment.is_none() && contract.medium_attachment.is_none() {
                    errors.push(validation_error(format!(
                        "participants resolve pass '{}' must materialize radiance, medium, or both",
                        pass.id
                    )));
                }
            }
            PresentationPassKind::ShadePrimary { contract } => {
                if !output_names.contains(contract.primary_hit_attachment.as_str()) {
                    errors.push(validation_error(format!(
                        "shade pass '{}' requires primary-hit attachment '{}'",
                        pass.id, contract.primary_hit_attachment
                    )));
                }
                if !output_names.contains(contract.surface_attachment.as_str()) {
                    errors.push(validation_error(format!(
                        "shade pass '{}' requires surface attachment '{}'",
                        pass.id, contract.surface_attachment
                    )));
                }
                if !output_names.contains(contract.output_attachment.as_str()) {
                    errors.push(validation_error(format!(
                        "shade pass '{}' materializes missing color attachment '{}'",
                        pass.id, contract.output_attachment
                    )));
                }
            }
            PresentationPassKind::CompositeColor { contract } => {
                if !output_names.contains(contract.input_attachment.as_str())
                    || !output_names.contains(contract.output_attachment.as_str())
                {
                    errors.push(validation_error(format!(
                        "composite pass '{}' must read and write declared color attachments",
                        pass.id
                    )));
                }
            }
            PresentationPassKind::MotionResolve { contract } => {
                if !output_names.contains(contract.primary_hit_attachment.as_str()) {
                    errors.push(validation_error(format!(
                        "motion resolve pass '{}' requires primary-hit attachment '{}'",
                        pass.id, contract.primary_hit_attachment
                    )));
                }
                match plan.frame.attachment(contract.output_attachment.as_str()) {
                    Some(attachment) if attachment.kind == FrameAttachmentKind::Motion => {}
                    Some(_) => errors.push(validation_error(format!(
                        "motion resolve pass '{}' must materialize a Motion attachment",
                        pass.id
                    ))),
                    None => errors.push(validation_error(format!(
                        "motion resolve pass '{}' materializes missing motion attachment '{}'",
                        pass.id, contract.output_attachment
                    ))),
                }
                if plan.frame.temporal.as_ref().is_some_and(|temporal| {
                    matches!(temporal.reuse, TemporalReuseMode::ReprojectColorAndMotion)
                }) && contract.history_primary_hit_attachment.is_none()
                {
                    errors.push(validation_error(format!(
                        "motion resolve pass '{}' must preserve continuation primary-hit history for ReprojectColorAndMotion",
                        pass.id
                    )));
                }
                if let Some(history_primary_hit_attachment) =
                    &contract.history_primary_hit_attachment
                    && !output_names.contains(history_primary_hit_attachment.as_str())
                {
                    errors.push(validation_error(format!(
                        "motion resolve pass '{}' references missing history primary-hit attachment '{}'",
                        pass.id, history_primary_hit_attachment
                    )));
                }
            }
            PresentationPassKind::TemporalResolve { contract } => {
                if plan.frame.temporal.is_none() {
                    errors.push(validation_error(format!(
                        "temporal resolve pass '{}' requires a temporal contract",
                        pass.id
                    )));
                }
                for attachment in [
                    contract.input_attachment.as_str(),
                    contract.primary_hit_attachment.as_str(),
                    contract.motion_attachment.as_str(),
                    contract.history_color_attachment.as_str(),
                    contract.output_attachment.as_str(),
                ] {
                    if !output_names.contains(attachment) {
                        errors.push(validation_error(format!(
                            "temporal resolve pass '{}' references missing attachment '{}'",
                            pass.id, attachment
                        )));
                    }
                }
                if contract.history_weight_denominator == 0
                    || contract.history_weight_numerator > contract.history_weight_denominator
                {
                    errors.push(validation_error(format!(
                        "temporal resolve pass '{}' must use a normalized history blend fraction",
                        pass.id
                    )));
                }
                if plan.frame.temporal.as_ref().is_some_and(|temporal| {
                    matches!(temporal.reuse, TemporalReuseMode::ReprojectColorAndMotion)
                }) && contract.history_primary_hit_attachment.is_none()
                {
                    errors.push(validation_error(format!(
                        "temporal resolve pass '{}' must preserve continuation primary-hit history for ReprojectColorAndMotion",
                        pass.id
                    )));
                }
                if let Some(history_primary_hit_attachment) =
                    &contract.history_primary_hit_attachment
                    && !output_names.contains(history_primary_hit_attachment.as_str())
                {
                    errors.push(validation_error(format!(
                        "temporal resolve pass '{}' references missing history primary-hit attachment '{}'",
                        pass.id, history_primary_hit_attachment
                    )));
                }
            }
            PresentationPassKind::ExportAttachment { attachment } => {
                if !output_names.contains(attachment.as_str()) {
                    errors.push(validation_error(format!(
                        "export pass '{}' references missing attachment '{}'",
                        pass.id, attachment
                    )));
                }
                if pass_index + 1 != plan.passes.len() {
                    errors.push(validation_error(format!(
                        "export pass '{}' must be terminal in the framegraph",
                        pass.id
                    )));
                }
            }
            _ => {}
        }
    }

    errors
}

fn validation_error(message: impl Into<SmolStr>) -> PresentationPlanValidationError {
    PresentationPlanValidationError {
        message: message.into(),
    }
}

fn expected_scale_for_resolution(
    resolution: AttachmentResolutionClass,
) -> AttachmentResolutionScale {
    match resolution {
        AttachmentResolutionClass::Viewport => AttachmentResolutionScale::full(),
        AttachmentResolutionClass::HalfViewport => AttachmentResolutionScale::half(),
        AttachmentResolutionClass::QuarterViewport => AttachmentResolutionScale::quarter(),
    }
}

pub fn quality_tier_name(tier: RealtimeQualityTier) -> &'static str {
    match tier {
        RealtimeQualityTier::Realtime60 => "realtime_60",
        RealtimeQualityTier::Realtime120 => "realtime_120",
        RealtimeQualityTier::High => "high",
        RealtimeQualityTier::Ultra => "ultra",
        RealtimeQualityTier::Debug => "debug",
    }
}

pub fn quality_degradation_step_name(step: QualityDegradationStep) -> &'static str {
    match step {
        QualityDegradationStep::ReduceInternalResolution => "reduce_internal_resolution",
        QualityDegradationStep::EnableHitCompaction => "enable_hit_compaction",
        QualityDegradationStep::LowerPrimarySteps => "lower_primary_steps",
        QualityDegradationStep::DisableMedia => "disable_media",
        QualityDegradationStep::LowerRadianceQuality => "lower_radiance_quality",
        QualityDegradationStep::DisableRadiance => "disable_radiance",
        QualityDegradationStep::HalfResolutionParticipants => "half_res_participants",
    }
}

pub mod validate {
    pub use super::{PresentationPlanValidationError, validate_plan};
}
