use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::presentation_binding::resolve_execution_binding;
use wrela::presentation_contract::{
    AttachmentClearPolicy, AttachmentLifetime, CanonicalCameraInput, CanonicalRayBudget,
    CanonicalViewportInput, DepthAttachmentSemantics, FrameAttachmentKind,
    LightingInputBindingSource, TemporalHistoryRole, TemporalReuseMode,
    canonical_screen_sample_query,
};
use wrela::presentation_plan::{PresentationPassKind, PresentationPlan};
use wrela::query_contract;
use wrela::query_plan::DispatchBackend;

fn lower_inline_module(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn render_function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
    module
        .functions
        .iter()
        .find(|(_, func)| func.name == name)
        .map(|(_, func)| func)
        .unwrap_or_else(|| panic!("missing render function '{name}'"))
}

fn presentation_function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
    module
        .functions
        .iter()
        .find(|(_, func)| func.name == name)
        .map(|(_, func)| func)
        .unwrap_or_else(|| panic!("missing presentation function '{name}'"))
}

fn render_plan_source() -> &'static str {
    r#"
render preview_ppm(world: RegionCapture, camera: Camera) {
    width = 4
    height = 3
    world_up = camera.up
    view_scale = 0.82
    fill_dir = normalize(vec3(-0.4, 0.5, 0.2))
}
"#
}

fn view_plan_source() -> &'static str {
    r#"
view primary_view(world: RegionCapture, camera: Camera) {
    domain = primary_domain(world = world)
    width = 4
    height = 3
    key_light = Light(
        position = camera.position,
        direction = normalize(vec3(0.0, -1.0, 0.0)),
        intensity = vec3(1.0, 1.0, 1.0),
        range = 8.0
    )
    fill_direction = normalize(vec3(-0.25, 0.6, 0.4))
    fill_strength = 0.35
    ambient_color = vec3(0.08, 0.11, 0.14)
}
"#
}

#[test]
fn render_metadata_is_split_into_view_frame_lighting_and_compatibility_buckets() {
    let module = lower_inline_module(render_plan_source());
    let render = render_function(&module, "preview_ppm");
    let metadata = render.render.as_ref().expect("render metadata");

    assert_eq!(
        metadata.view.projection.source,
        hir::RenderProjectionSource::CameraVerticalFovDegrees
    );
    assert!(metadata.view.width.is_some());
    assert!(metadata.view.height.is_some());
    assert!(metadata.frame.domain.is_none());
    assert!(metadata.lighting.fill_dir.is_some());
    assert!(metadata.lighting.fill_strength.is_none());
    assert!(metadata.lighting.ambient_color.is_none());
    assert!(!metadata.lighting.light_compatibility_alias);
    assert!(metadata.lighting.fill_dir_compatibility_alias);
    assert!(metadata.compatibility.world_up.is_some());
    assert!(metadata.compatibility.view_scale.is_some());
}

#[test]
fn legacy_render_plan_separates_semantic_contracts_from_execution_binding() {
    let module = lower_inline_module(render_plan_source());
    let render = render_function(&module, "preview_ppm");
    let plan = PresentationPlan::from_render_function(render, DispatchBackend::Auto).expect("plan");

    assert!(plan.validate().is_empty());
    assert!(plan.view.canonical_projection);
    assert!(plan.view.compatibility_projection.legacy_path_active);
    assert!(
        plan.view
            .compatibility_projection
            .authored_world_up_override
    );
    assert!(
        plan.view
            .compatibility_projection
            .authored_view_scale_override
    );
    assert_eq!(plan.frame.outputs.len(), 8);
    assert_eq!(
        plan.frame
            .outputs
            .last()
            .expect("exported color attachment")
            .kind,
        FrameAttachmentKind::Color
    );
    assert_eq!(
        plan.frame
            .outputs
            .last()
            .expect("exported color attachment")
            .lifetime,
        AttachmentLifetime::Exported
    );
    assert_eq!(
        plan.frame.lighting.key_light.source,
        LightingInputBindingSource::DefaultCompatibilityRecipe
    );
    assert!(!plan.frame.lighting.key_light.temporary_compatibility_alias);
    assert_eq!(
        plan.frame.lighting.fill_direction.source,
        LightingInputBindingSource::AuthoredMetadata
    );
    assert!(
        plan.frame
            .lighting
            .fill_direction
            .temporary_compatibility_alias
    );
    assert_eq!(
        plan.frame.lighting.fill_strength.source,
        LightingInputBindingSource::DefaultCompatibilityRecipe
    );
    assert_eq!(
        plan.frame.lighting.ambient_color.source,
        LightingInputBindingSource::DefaultCompatibilityRecipe
    );
    assert_eq!(plan.passes.len(), 7);
    assert_eq!(
        plan.passes
            .iter()
            .find(|pass| matches!(pass.kind, PresentationPassKind::PrimaryVisibility { .. }))
            .expect("primary visibility pass")
            .query_dependencies,
        vec![query_contract::SPATIAL_NEAREST_BATCH_WORLD]
    );
    assert!(
        plan.passes
            .iter()
            .any(|pass| matches!(pass.kind, PresentationPassKind::SurfaceResolve { .. }))
    );
    assert!(
        plan.passes
            .iter()
            .any(|pass| matches!(pass.kind, PresentationPassKind::ParticipantsResolve { .. }))
    );
    assert!(
        plan.passes
            .iter()
            .any(|pass| matches!(pass.kind, PresentationPassKind::ShadePrimary { .. }))
    );
    assert!(
        plan.passes
            .iter()
            .any(|pass| matches!(pass.kind, PresentationPassKind::CompositeColor { .. }))
    );
    assert!(
        plan.passes
            .iter()
            .any(|pass| matches!(pass.kind, PresentationPassKind::ExportAttachment { .. }))
    );

    let binding_summary = plan.export_binding().expect("ppm export binding");
    assert_eq!(binding_summary.id.as_str(), "attachment.export.ppm");
    assert!(
        !format!("{:?}", plan.bindings).contains("__wr_render_capture_to_ppm"),
        "presentation plan should keep helper names behind execution binding resolution"
    );
    let binding = resolve_execution_binding(binding_summary).expect("execution binding");
    assert_eq!(binding.helper_name, Some("__wr_render_capture_to_ppm"));

    assert!(
        !format!("{:?}", plan.view).contains("__wr_render_capture_to_ppm"),
        "view contract must not leak helper names"
    );
    assert!(
        !format!("{:?}", plan.frame).contains("__wr_render_capture_to_ppm"),
        "frame contract must not leak helper names"
    );
}

#[test]
fn canonical_view_plan_materializes_primary_visibility_and_semantic_attachments() {
    let module = lower_inline_module(view_plan_source());
    let view = presentation_function(&module, "primary_view");
    assert_eq!(view.role, hir::FunctionRole::View);
    assert_eq!(
        view.ret_type.as_ref().map(|ty| ty.name.as_str()),
        Some("FrameState")
    );

    let plan = PresentationPlan::from_render_function(view, DispatchBackend::Wgsl).expect("plan");
    assert!(plan.validate().is_empty());
    assert!(plan.view.canonical_projection);
    assert!(!plan.view.compatibility_projection.legacy_path_active);
    assert_eq!(plan.view.screen_lattice.width_source, "view.width");
    assert_eq!(plan.view.screen_lattice.height_source, "view.height");
    assert!(plan.view.canonical_view_ray.normalized_direction);

    let primary_hit = plan.frame.primary_hit.as_ref().expect("primary hit schema");
    assert_eq!(primary_hit.attachment, "primary_hit");
    assert_eq!(primary_hit.record, "Hit3");
    assert!(
        primary_hit
            .fields
            .iter()
            .any(|field| field == "root_shape_id")
    );
    assert!(primary_hit.fields.iter().any(|field| field == "payload"));
    assert_eq!(
        primary_hit.depth_semantics,
        DepthAttachmentSemantics::RayParameterDistance
    );
    assert_eq!(
        primary_hit.sample_identity,
        "screen_lattice.row_major_top_left_pixel_center"
    );
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "primary_hit" && attachment.kind == FrameAttachmentKind::PrimaryHit
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "depth" && attachment.kind == FrameAttachmentKind::Depth
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "world_normal" && attachment.kind == FrameAttachmentKind::WorldNormal
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "surface" && attachment.kind == FrameAttachmentKind::Surface
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "radiance" && attachment.kind == FrameAttachmentKind::Radiance
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "medium" && attachment.kind == FrameAttachmentKind::Medium
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "color"
            && attachment.kind == FrameAttachmentKind::Color
            && attachment.lifetime == AttachmentLifetime::Exported
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "motion" && attachment.kind == FrameAttachmentKind::Motion
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "history_color"
            && attachment.kind == FrameAttachmentKind::Color
            && attachment.lifetime == AttachmentLifetime::HistorySlot(0)
    }));
    assert!(plan.frame.outputs.iter().any(|attachment| {
        attachment.name == "history_primary_hit"
            && attachment.kind == FrameAttachmentKind::PrimaryHit
            && attachment.lifetime == AttachmentLifetime::HistorySlot(1)
    }));
    assert!(
        plan.frame
            .outputs
            .iter()
            .any(|attachment| attachment.name == "primary_hit"
                && attachment.clear_policy == AttachmentClearPolicy::SemanticDefault)
    );
    let temporal = plan.frame.temporal.as_ref().expect("temporal contract");
    assert_eq!(temporal.reuse, TemporalReuseMode::ReprojectColorAndMotion);
    assert_eq!(temporal.history_slots.len(), 2);
    assert!(temporal.history_slots.iter().any(|slot| {
        slot.attachment == "history_color" && slot.role == TemporalHistoryRole::ReprojectedColor
    }));
    assert!(temporal.history_slots.iter().any(|slot| {
        slot.attachment == "history_primary_hit"
            && slot.role == TemporalHistoryRole::ContinuationPrimaryHit
    }));

    assert!(plan.passes.iter().any(|pass| {
        matches!(
            pass.kind,
            PresentationPassKind::GenerateScreenSamples { .. }
        ) && pass
            .materializes
            .iter()
            .any(|item| item == "screen_samples")
    }));
    let screen_pass = plan
        .passes
        .iter()
        .find(|pass| {
            matches!(
                pass.kind,
                PresentationPassKind::GenerateScreenSamples { .. }
            )
        })
        .expect("screen sample pass");
    let PresentationPassKind::GenerateScreenSamples { contract } = &screen_pass.kind else {
        unreachable!("screen pass is already matched");
    };
    assert_eq!(contract.output_item_record, "ScreenSampleQuery");
    assert_eq!(contract.samples_per_pixel, 1);
    assert_eq!(
        contract.item_count_expression,
        "view.width * view.height * 1"
    );
    let primary_visibility = plan
        .passes
        .iter()
        .find(|pass| matches!(pass.kind, PresentationPassKind::PrimaryVisibility { .. }))
        .expect("primary visibility pass");
    let PresentationPassKind::PrimaryVisibility { contract } = &primary_visibility.kind else {
        unreachable!("already matched primary visibility");
    };
    assert_eq!(
        contract.query_contract,
        query_contract::SPATIAL_NEAREST_BATCH_WORLD
    );
    assert_eq!(contract.primary_hit_attachment, "primary_hit");
    assert_eq!(contract.depth_attachment.as_deref(), Some("depth"));
    assert_eq!(
        contract.world_normal_attachment.as_deref(),
        Some("world_normal")
    );
    assert_eq!(
        primary_visibility.query_dependencies,
        vec![query_contract::SPATIAL_NEAREST_BATCH_WORLD]
    );
    let surface_resolve = plan
        .passes
        .iter()
        .find(|pass| matches!(pass.kind, PresentationPassKind::SurfaceResolve { .. }))
        .expect("surface resolve pass");
    let PresentationPassKind::SurfaceResolve { contract } = &surface_resolve.kind else {
        unreachable!("already matched surface resolve");
    };
    assert_eq!(
        contract.query_contract,
        query_contract::SURFACE_SAMPLE_BATCH_WORLD
    );
    assert_eq!(contract.primary_hit_attachment, "primary_hit");
    assert_eq!(contract.surface_attachment, "surface");
    assert!(contract.explicit_miss_default);
    assert_eq!(
        surface_resolve.query_dependencies,
        vec![query_contract::SURFACE_SAMPLE_BATCH_WORLD]
    );

    let participants_resolve = plan
        .passes
        .iter()
        .find(|pass| matches!(pass.kind, PresentationPassKind::ParticipantsResolve { .. }))
        .expect("participants resolve pass");
    let PresentationPassKind::ParticipantsResolve { contract } = &participants_resolve.kind else {
        unreachable!("already matched participants resolve");
    };
    assert_eq!(
        contract.radiance_query_contract,
        Some(query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD)
    );
    assert_eq!(
        contract.medium_query_contract,
        Some(query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD)
    );
    assert_eq!(contract.radiance_attachment.as_deref(), Some("radiance"));
    assert_eq!(contract.medium_attachment.as_deref(), Some("medium"));
    assert_eq!(
        participants_resolve.query_dependencies,
        vec![
            query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD,
            query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD,
        ]
    );

    let shade_primary = plan
        .passes
        .iter()
        .find(|pass| matches!(pass.kind, PresentationPassKind::ShadePrimary { .. }))
        .expect("shade primary pass");
    let PresentationPassKind::ShadePrimary { contract } = &shade_primary.kind else {
        unreachable!("already matched shade primary");
    };
    assert_eq!(contract.primary_hit_attachment, "primary_hit");
    assert_eq!(contract.surface_attachment, "surface");
    assert_eq!(contract.radiance_attachment.as_deref(), Some("radiance"));
    assert_eq!(contract.medium_attachment.as_deref(), Some("medium"));
    assert_eq!(contract.output_attachment, "shaded_color");
    assert!(contract.compatibility_recipe);

    let motion_resolve = plan
        .passes
        .iter()
        .find(|pass| matches!(pass.kind, PresentationPassKind::MotionResolve { .. }))
        .expect("motion resolve pass");
    let PresentationPassKind::MotionResolve { contract } = &motion_resolve.kind else {
        unreachable!("already matched motion resolve");
    };
    assert_eq!(contract.primary_hit_attachment, "primary_hit");
    assert_eq!(contract.output_attachment, "motion");
    assert_eq!(
        contract.history_primary_hit_attachment.as_deref(),
        Some("history_primary_hit")
    );

    let temporal_resolve = plan
        .passes
        .iter()
        .find(|pass| matches!(pass.kind, PresentationPassKind::TemporalResolve { .. }))
        .expect("temporal resolve pass");
    let PresentationPassKind::TemporalResolve { contract } = &temporal_resolve.kind else {
        unreachable!("already matched temporal resolve");
    };
    assert_eq!(contract.input_attachment, "shaded_color");
    assert_eq!(contract.motion_attachment, "motion");
    assert_eq!(contract.history_color_attachment, "history_color");
    assert_eq!(
        contract.history_primary_hit_attachment.as_deref(),
        Some("history_primary_hit")
    );
    assert_eq!(contract.output_attachment, "color");

    assert_eq!(
        plan.frame.lighting.key_light.source,
        LightingInputBindingSource::AuthoredMetadata
    );
    assert!(!plan.frame.lighting.key_light.temporary_compatibility_alias);
    assert_eq!(
        plan.frame.lighting.fill_direction.source,
        LightingInputBindingSource::AuthoredMetadata
    );
    assert!(
        !plan
            .frame
            .lighting
            .fill_direction
            .temporary_compatibility_alias
    );
    assert_eq!(
        plan.frame.lighting.fill_strength.source,
        LightingInputBindingSource::AuthoredMetadata
    );
    assert_eq!(
        plan.frame.lighting.ambient_color.source,
        LightingInputBindingSource::AuthoredMetadata
    );
}

#[test]
fn presentation_plan_validation_catches_screen_lattice_item_count_drift() {
    let module = lower_inline_module(view_plan_source());
    let view = presentation_function(&module, "primary_view");
    let mut plan = PresentationPlan::from_render_function(view, DispatchBackend::Wgsl).unwrap();
    let screen_pass = plan
        .passes
        .iter_mut()
        .find(|pass| {
            matches!(
                pass.kind,
                PresentationPassKind::GenerateScreenSamples { .. }
            )
        })
        .expect("screen sample pass");
    let PresentationPassKind::GenerateScreenSamples { contract } = &mut screen_pass.kind else {
        unreachable!("screen pass is already matched");
    };
    contract.samples_per_pixel = 2;
    contract.item_count_expression = "view.width * view.height * 1".into();

    let errors = plan.validate();
    assert!(
        errors
            .iter()
            .any(|err| err.message.contains("does not match viewport lattice")),
        "expected viewport item count validation error, got {errors:?}"
    );
}

#[test]
fn presentation_plan_validation_rejects_duplicate_attachment_names_and_history_clear_mismatch() {
    let module = lower_inline_module(view_plan_source());
    let view = presentation_function(&module, "primary_view");
    let mut plan = PresentationPlan::from_render_function(view, DispatchBackend::Wgsl).unwrap();
    plan.frame.outputs[1].name = "primary_hit".into();
    plan.frame.outputs[2].lifetime = AttachmentLifetime::HistorySlot(0);
    plan.frame.outputs[2].clear_policy = AttachmentClearPolicy::SemanticDefault;
    plan.frame.primary_hit.as_mut().unwrap().fields = vec!["hit".into(), "distance".into()];

    let errors = plan.validate();
    assert!(
        errors.iter().any(|err| err
            .message
            .contains("frame attachment names must be unique")),
        "expected duplicate-name validation error, got {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|err| err.message.contains("must preserve previous contents")),
        "expected history/clear-policy validation error, got {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|err| err.message.contains("must preserve canonical Hit3 fields")),
        "expected primary-hit provenance validation error, got {errors:?}"
    );
}

#[test]
fn presentation_plan_validation_rejects_broken_temporal_history_and_motion_wiring() {
    let module = lower_inline_module(view_plan_source());
    let view = presentation_function(&module, "primary_view");
    let mut plan = PresentationPlan::from_render_function(view, DispatchBackend::Wgsl).unwrap();
    plan.frame.outputs.retain(|attachment| attachment.name != "motion");
    plan.frame.temporal.as_mut().unwrap().history_slots[0].attachment = "surface".into();
    plan.frame
        .temporal
        .as_mut()
        .unwrap()
        .history_slots
        .retain(|slot| slot.role != TemporalHistoryRole::ContinuationPrimaryHit);
    if let Some(pass) = plan
        .passes
        .iter_mut()
        .find(|pass| matches!(pass.kind, PresentationPassKind::MotionResolve { .. }))
    {
        let PresentationPassKind::MotionResolve { contract } = &mut pass.kind else {
            unreachable!("already matched motion resolve pass");
        };
        contract.history_primary_hit_attachment = None;
    }
    if let Some(pass) = plan
        .passes
        .iter_mut()
        .find(|pass| matches!(pass.kind, PresentationPassKind::TemporalResolve { .. }))
    {
        let PresentationPassKind::TemporalResolve { contract } = &mut pass.kind else {
            unreachable!("already matched temporal resolve pass");
        };
        contract.history_primary_hit_attachment = None;
    }

    let errors = plan.validate();
    assert!(
        errors.iter().any(|err| err
            .message
            .contains("ReprojectColorAndMotion requires both a motion attachment and a motion resolve pass")),
        "expected motion-history validation error, got {errors:?}"
    );
    assert!(
        errors.iter().any(|err| err
            .message
            .contains("temporal history attachment 'surface' must bind slot 0")),
        "expected history-slot lifetime validation error, got {errors:?}"
    );
    assert!(
        errors.iter().any(|err| err
            .message
            .contains("temporal color history attachment 'surface' must use Color semantics")),
        "expected history color semantics validation error, got {errors:?}"
    );
    assert!(
        errors.iter().any(|err| err
            .message
            .contains("ReprojectColorAndMotion requires a continuation primary-hit history slot")),
        "expected continuation-slot validation error, got {errors:?}"
    );
    assert!(
        errors.iter().any(|err| err
            .message
            .contains("motion resolve pass 'motion_resolve' must preserve continuation primary-hit history for ReprojectColorAndMotion")),
        "expected motion continuation wiring error, got {errors:?}"
    );
    assert!(
        errors.iter().any(|err| err
            .message
            .contains("temporal resolve pass 'temporal_resolve' must preserve continuation primary-hit history for ReprojectColorAndMotion")),
        "expected temporal continuation wiring error, got {errors:?}"
    );
}

#[test]
fn canonical_screen_sample_query_uses_fov_pixel_center_aspect_y_axis_and_jitter_units() {
    let camera = CanonicalCameraInput {
        position: [1.0, 2.0, 3.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 90.0,
    };
    let viewport = CanonicalViewportInput {
        width: 3,
        height: 3,
    };
    let budget = CanonicalRayBudget {
        max_distance: 10.0,
        min_step: 0.01,
        hit_epsilon: 0.001,
        max_steps: 128,
    };

    let center = canonical_screen_sample_query(camera, viewport, 1, 1, [0.0, 0.0], budget);
    assert_eq!(center.pixel, [1.0, 1.0]);
    assert_close2(center.uv, [0.5, 0.5]);
    assert_close3(center.ray.origin, camera.position);
    assert_close3(center.ray.direction, [0.0, 0.0, -1.0]);

    let top = canonical_screen_sample_query(camera, viewport, 1, 0, [0.0, 0.0], budget);
    let bottom = canonical_screen_sample_query(camera, viewport, 1, 2, [0.0, 0.0], budget);
    assert!(
        top.ray.direction[1] > 0.0,
        "top-left origin should point top rows upward"
    );
    assert!(
        bottom.ray.direction[1] < 0.0,
        "bottom rows should point downward"
    );

    let left = canonical_screen_sample_query(camera, viewport, 0, 1, [0.0, 0.0], budget);
    let right = canonical_screen_sample_query(camera, viewport, 2, 1, [0.0, 0.0], budget);
    assert!(left.ray.direction[0] < 0.0);
    assert!(right.ray.direction[0] > 0.0);

    let wide = CanonicalViewportInput {
        width: 6,
        height: 3,
    };
    let wide_right = canonical_screen_sample_query(camera, wide, 5, 1, [0.0, 0.0], budget);
    assert!(
        wide_right.ray.direction[0] > right.ray.direction[0],
        "aspect ratio should widen horizontal FOV from the vertical FOV input"
    );

    let mut narrow_fov_camera = camera;
    narrow_fov_camera.vertical_fov_degrees = 45.0;
    let narrow_top =
        canonical_screen_sample_query(narrow_fov_camera, viewport, 1, 0, [0.0, 0.0], budget);
    assert!(
        narrow_top.ray.direction[1] < top.ray.direction[1],
        "Camera.vertical_fov_degrees should control canonical projection scale"
    );

    let jittered = canonical_screen_sample_query(camera, viewport, 1, 1, [0.5, -0.5], budget);
    assert_close2(jittered.uv, [2.0 / 3.0, 1.0 / 3.0]);
    assert!(jittered.ray.direction[0] > 0.0);
    assert!(jittered.ray.direction[1] > 0.0);

    let wgsl_prelude = include_str!("../query_exec/wgsl/prelude.wgsl");
    assert!(wgsl_prelude.contains("fn wr_canonical_screen_sample_query"));
    assert!(wgsl_prelude.contains("pixel + vec2<f32>(0.5, 0.5) + jitter_pixels"));
    assert!(wgsl_prelude.contains("camera.vertical_fov_degrees"));
    assert!(wgsl_prelude.contains("1.0 - uv.y * 2.0"));
}

fn assert_close2(actual: [f32; 2], expected: [f32; 2]) {
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "{actual} != {expected}"
        );
    }
}

fn assert_close3(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "{actual} != {expected}"
        );
    }
}
