use crate::presentation_plan::PresentationPassKind;
use crate::query_plan::DispatchBackend;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationBindingId(pub SmolStr);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PresentationPassRecipeKind {
    GenerateScreenSamples,
    PrimaryVisibility,
    SurfaceResolve,
    ParticipantsResolve,
    ShadePrimary,
    CompositeColor,
    MotionResolve,
    TemporalResolve,
    WorldBatchQuery,
    KernelDispatch,
    ExportAttachment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationBindingSummary {
    pub id: PresentationBindingId,
    pub pass_kind: PresentationPassKind,
    pub recipe: PresentationPassRecipeKind,
    pub default_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationExecutionBinding {
    pub summary: PresentationBindingSummary,
    pub helper_name: Option<&'static str>,
}

impl PresentationBindingId {
    pub fn new(value: impl Into<SmolStr>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl PresentationBindingSummary {
    pub fn screen_samples(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("screen.samples"),
            pass_kind: PresentationPassKind::GenerateScreenSamples {
                contract: crate::presentation_plan::ScreenSampleGenerationContract::from_view(
                    &crate::presentation_contract::ViewContract::canonical(),
                ),
            },
            recipe: PresentationPassRecipeKind::GenerateScreenSamples,
            default_backend,
        }
    }

    pub fn world_batch_query(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("world.batch.query"),
            pass_kind: PresentationPassKind::WorldBatchQuery {
                contract_id: crate::query_contract::SPATIAL_NEAREST_BATCH_WORLD,
            },
            recipe: PresentationPassRecipeKind::WorldBatchQuery,
            default_backend,
        }
    }

    pub fn primary_visibility(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("primary.visibility"),
            pass_kind: PresentationPassKind::PrimaryVisibility {
                contract: crate::presentation_plan::PrimaryVisibilityPassContract {
                    query_contract: crate::query_contract::SPATIAL_NEAREST_BATCH_WORLD,
                    primary_hit_attachment: SmolStr::new("primary_hit"),
                    depth_attachment: Some(SmolStr::new("depth")),
                    world_normal_attachment: Some(SmolStr::new("world_normal")),
                },
            },
            recipe: PresentationPassRecipeKind::PrimaryVisibility,
            default_backend,
        }
    }

    pub fn surface_resolve(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("surface.resolve"),
            pass_kind: PresentationPassKind::SurfaceResolve {
                contract: crate::presentation_plan::SurfaceResolvePassContract {
                    query_contract: crate::query_contract::SURFACE_SAMPLE_BATCH_WORLD,
                    primary_hit_attachment: SmolStr::new("primary_hit"),
                    surface_attachment: SmolStr::new("surface"),
                    explicit_miss_default: true,
                },
            },
            recipe: PresentationPassRecipeKind::SurfaceResolve,
            default_backend,
        }
    }

    pub fn participants_resolve(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("participants.resolve"),
            pass_kind: PresentationPassKind::ParticipantsResolve {
                contract: crate::presentation_plan::ParticipantsResolvePassContract {
                    radiance_query_contract: Some(
                        crate::query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD,
                    ),
                    medium_query_contract: Some(
                        crate::query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD,
                    ),
                    primary_hit_attachment: SmolStr::new("primary_hit"),
                    screen_samples: SmolStr::new("screen_samples"),
                    radiance_attachment: Some(SmolStr::new("radiance")),
                    medium_attachment: Some(SmolStr::new("medium")),
                    miss_sample_distance: 4.0,
                },
            },
            recipe: PresentationPassRecipeKind::ParticipantsResolve,
            default_backend,
        }
    }

    pub fn shade_primary(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("shade.primary"),
            pass_kind: PresentationPassKind::ShadePrimary {
                contract: crate::presentation_plan::ShadePrimaryPassContract {
                    primary_hit_attachment: SmolStr::new("primary_hit"),
                    surface_attachment: SmolStr::new("surface"),
                    radiance_attachment: Some(SmolStr::new("radiance")),
                    medium_attachment: Some(SmolStr::new("medium")),
                    output_attachment: SmolStr::new("shaded_color"),
                    compatibility_recipe: true,
                },
            },
            recipe: PresentationPassRecipeKind::ShadePrimary,
            default_backend,
        }
    }

    pub fn composite_color(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("composite.color"),
            pass_kind: PresentationPassKind::CompositeColor {
                contract: crate::presentation_plan::CompositeColorPassContract {
                    input_attachment: SmolStr::new("shaded_color"),
                    output_attachment: SmolStr::new("color"),
                },
            },
            recipe: PresentationPassRecipeKind::CompositeColor,
            default_backend,
        }
    }

    pub fn motion_resolve(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("motion.resolve"),
            pass_kind: PresentationPassKind::MotionResolve {
                contract: crate::presentation_plan::MotionResolvePassContract {
                    primary_hit_attachment: SmolStr::new("primary_hit"),
                    output_attachment: SmolStr::new("motion"),
                    history_primary_hit_attachment: Some(SmolStr::new("history_primary_hit")),
                },
            },
            recipe: PresentationPassRecipeKind::MotionResolve,
            default_backend,
        }
    }

    pub fn temporal_resolve(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("temporal.resolve"),
            pass_kind: PresentationPassKind::TemporalResolve {
                contract: crate::presentation_plan::TemporalResolvePassContract {
                    input_attachment: SmolStr::new("shaded_color"),
                    primary_hit_attachment: SmolStr::new("primary_hit"),
                    motion_attachment: SmolStr::new("motion"),
                    history_color_attachment: SmolStr::new("history_color"),
                    history_primary_hit_attachment: Some(SmolStr::new("history_primary_hit")),
                    output_attachment: SmolStr::new("color"),
                    neighborhood_clamp: true,
                    history_weight_numerator: 3,
                    history_weight_denominator: 4,
                },
            },
            recipe: PresentationPassRecipeKind::TemporalResolve,
            default_backend,
        }
    }

    pub fn kernel_dispatch(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("kernel.dispatch"),
            pass_kind: PresentationPassKind::KernelDispatch,
            recipe: PresentationPassRecipeKind::KernelDispatch,
            default_backend,
        }
    }

    pub fn export_attachment(default_backend: DispatchBackend) -> Self {
        Self {
            id: PresentationBindingId::new("attachment.export"),
            pass_kind: PresentationPassKind::ExportAttachment {
                attachment: SmolStr::new("color"),
            },
            recipe: PresentationPassRecipeKind::ExportAttachment,
            default_backend,
        }
    }

    pub fn ppm_export_attachment(default_backend: DispatchBackend, attachment: SmolStr) -> Self {
        Self {
            id: PresentationBindingId::new("attachment.export.ppm"),
            pass_kind: PresentationPassKind::ExportAttachment { attachment },
            recipe: PresentationPassRecipeKind::ExportAttachment,
            default_backend,
        }
    }
}

pub fn resolve_execution_binding(
    summary: &PresentationBindingSummary,
) -> Option<PresentationExecutionBinding> {
    match (summary.id.as_str(), summary.recipe) {
        ("screen.samples", PresentationPassRecipeKind::GenerateScreenSamples)
        | ("primary.visibility", PresentationPassRecipeKind::PrimaryVisibility)
        | ("surface.resolve", PresentationPassRecipeKind::SurfaceResolve)
        | ("participants.resolve", PresentationPassRecipeKind::ParticipantsResolve)
        | ("shade.primary", PresentationPassRecipeKind::ShadePrimary)
        | ("composite.color", PresentationPassRecipeKind::CompositeColor)
        | ("motion.resolve", PresentationPassRecipeKind::MotionResolve)
        | ("temporal.resolve", PresentationPassRecipeKind::TemporalResolve)
        | ("world.batch.query", PresentationPassRecipeKind::WorldBatchQuery)
        | ("kernel.dispatch", PresentationPassRecipeKind::KernelDispatch)
        | ("attachment.export", PresentationPassRecipeKind::ExportAttachment) => {
            Some(PresentationExecutionBinding {
                summary: summary.clone(),
                helper_name: None,
            })
        }
        ("attachment.export.ppm", PresentationPassRecipeKind::ExportAttachment) => {
            Some(PresentationExecutionBinding {
                summary: summary.clone(),
                helper_name: Some("__wr_presentation_attachment_to_ppm"),
            })
        }
        _ => None,
    }
}
