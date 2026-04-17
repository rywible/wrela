use super::*;

#[test]
fn cli_query_contracts_json_lists_family_catalog() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("query-contracts")
        .arg("--json")
        .output()
        .expect("run query-contracts");
    assert!(
        output.status.success(),
        "query-contracts failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("query contract catalog json");
    assert_eq!(
        catalog.get("schema_version").and_then(|v| v.as_u64()),
        Some(1)
    );
    let contracts = catalog
        .get("contracts")
        .and_then(|v| v.as_array())
        .expect("contracts array");
    assert!(
        contracts
            .iter()
            .all(|contract| contract.get("helper").is_none()),
        "public query contract catalog must not expose internal helper names"
    );
    assert!(contracts.iter().any(|contract| {
        contract
            .get("contract_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == "spatial.nearest.capture.shape")
            && contract
                .get("call")
                .and_then(|v| v.as_str())
                .is_some_and(|call| call == "spatial.nearest")
            && contract
                .get("legacy_builtin")
                .and_then(|v| v.as_str())
                .is_some_and(|name| name == "trace_shape")
    }));
    assert!(contracts.iter().any(|contract| {
        contract
            .get("contract_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == "support.summary.world")
            && contract
                .get("backends")
                .and_then(|v| v.as_array())
                .is_some_and(|backends| {
                    backends.iter().any(|backend| backend == "cpu")
                        && backends.iter().any(|backend| backend == "virtual_gpu")
                        && !backends.iter().any(|backend| backend == "wgsl")
                })
    }));
    assert!(contracts.iter().any(|contract| {
        contract
            .get("contract_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == "spatial.nearest.batch.world")
            && contract
                .get("target")
                .and_then(|v| v.as_str())
                .is_some_and(|target| target == "world")
            && contract
                .get("cardinality")
                .and_then(|v| v.as_str())
                .is_some_and(|cardinality| cardinality == "batch")
            && contract
                .get("surface")
                .and_then(|v| v.as_str())
                .is_some_and(|surface| surface == "batch.world")
    }));
    let aliases = catalog
        .get("aliases")
        .and_then(|v| v.as_array())
        .expect("aliases array");
    assert!(aliases.iter().any(|alias| {
        alias
            .get("alias_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == "spatial.trace.world")
            && alias
                .get("canonical_id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == "spatial.nearest.world")
    }));
}

#[test]
fn cli_collision_contracts_human_lists_collision_catalog() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("collision-contracts")
        .output()
        .expect("run collision-contracts");
    assert!(
        output.status.success(),
        "collision-contracts failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("collision contract catalog schema v2"));
    assert!(stdout.contains("collision.point_occupancy.world"));
    assert!(stdout.contains("collision.ray_cast.world"));
    assert!(stdout.contains("collision.sphere_overlap.world"));
    assert!(stdout.contains("collision.sphere_sweep.transition"));
    assert!(stdout.contains("collision.time_of_impact.transition"));
    assert!(stdout.contains("authority=scope=snapshot"));
    assert!(stdout.contains("authority=scope=transition"));
    assert!(stdout.contains(
        "policy=backend_preference=cpu required_guarantee=exact selected_method=exact_oracle"
    ));
    assert!(stdout.contains("witness=CollisionPointWitness"));
    assert!(stdout.contains("witness=CollisionRayWitness"));
    assert!(stdout.contains("witness=CollisionSphereWitness"));
    assert!(stdout.contains("witness=CollisionSweepWitness"));
    assert!(stdout.contains("witness=CollisionTimeOfImpactWitness"));
}

#[test]
fn cli_collision_contracts_json_lists_collision_catalog() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("collision-contracts")
        .arg("--json")
        .output()
        .expect("run collision-contracts json");
    assert!(
        output.status.success(),
        "collision-contracts --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("collision contract catalog json");
    assert_eq!(
        catalog
            .get("schema_version")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    let contracts = catalog
        .get("contracts")
        .and_then(|value| value.as_array())
        .expect("contracts array");
    assert_eq!(contracts.len(), 5);
    let point = contracts
        .iter()
        .find(|contract| {
            contract
                .get("contract_id")
                .and_then(|value| value.as_str())
                .is_some_and(|id| id == "collision.point_occupancy.world")
        })
        .expect("point occupancy contract");
    assert_eq!(
        point
            .pointer("/policy/backend_preference")
            .and_then(|value| value.as_str()),
        Some("cpu")
    );
    assert_eq!(
        point
            .pointer("/backends")
            .and_then(|value| value.as_array())
            .map(|arr| {
                let backends = arr
                    .iter()
                    .filter_map(|backend| backend.as_str())
                    .collect::<Vec<_>>();
                (
                    backends.len(),
                    backends.contains(&"cpu"),
                    backends.contains(&"wgsl"),
                )
            }),
        Some((2, true, true))
    );
    assert_eq!(
        point
            .pointer("/witness_schema/name")
            .and_then(|value| value.as_str()),
        Some("CollisionPointWitness")
    );
    assert_eq!(
        contracts
            .iter()
            .find(|contract| {
                contract
                    .get("contract_id")
                    .and_then(|value| value.as_str())
                    .is_some_and(|id| id == "collision.sphere_sweep.transition")
            })
            .and_then(|contract| contract.pointer("/authority/scope"))
            .and_then(|value| value.as_str()),
        Some("transition")
    );
}

#[test]
fn cli_collision_plan_json_reports_validation_and_policy() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("collision-plan")
        .arg("--json")
        .output()
        .expect("run collision-plan json");
    assert!(
        output.status.success(),
        "collision-plan --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("collision plan json");
    assert_eq!(
        dump.get("schema_version").and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        dump.get("backend").and_then(|value| value.as_str()),
        Some("auto")
    );
    let plans = dump
        .get("plans")
        .and_then(|value| value.as_array())
        .expect("plans array");
    assert_eq!(plans.len(), 5);
    let point = plans
        .iter()
        .find(|plan| {
            plan.get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "collision.point_occupancy.world")
        })
        .expect("point occupancy collision plan");
    assert_eq!(
        point
            .pointer("/validation/status")
            .and_then(|value| value.as_str()),
        Some("ok")
    );
    assert_eq!(
        point
            .pointer("/policy/required_guarantee")
            .and_then(|value| value.as_str()),
        Some("exact")
    );
    assert_eq!(
        point
            .pointer("/policy/selected_method")
            .and_then(|value| value.as_str()),
        Some("exact_oracle")
    );
    let sweep = plans
        .iter()
        .find(|plan| {
            plan.get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "collision.sphere_sweep.transition")
        })
        .expect("sphere sweep transition collision plan");
    assert_eq!(
        sweep
            .pointer("/authority_scope")
            .and_then(|value| value.as_str()),
        Some("transition")
    );
    assert_eq!(
        sweep
            .pointer("/artifacts")
            .and_then(|value| value.as_array())
            .map(|arr| arr.len()),
        Some(4)
    );
    assert_eq!(
        sweep
            .pointer("/passes/1/kind")
            .and_then(|value| value.as_str()),
        Some("build_broadphase_candidates")
    );
    assert_eq!(
        sweep
            .pointer("/passes/2/kind")
            .and_then(|value| value.as_str()),
        Some("sweep_sphere_first_contact")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/observer_kind")
            .and_then(|value| value.as_str()),
        Some("collision")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/spine/observer_kind")
            .and_then(|value| value.as_str()),
        Some("collision")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/spine/inputs/0/binding")
            .and_then(|value| value.as_str()),
        Some("world")
    );
    assert!(
        sweep
            .pointer("/observer_projection/spine/nodes")
            .and_then(|value| value.as_array())
            .is_some_and(|nodes| nodes.iter().any(|node| {
                node.pointer("/family").and_then(|value| value.as_str())
                    == Some("policy_requirement")
            }) && nodes.iter().any(|node| {
                node.pointer("/label").and_then(|value| value.as_str())
                    == Some("sweep_sphere_first_contact")
            }))
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/spine/outputs/0/binding")
            .and_then(|value| value.as_str()),
        Some("sweep_contact")
    );
    assert!(
        sweep
            .pointer("/observer_projection/lossy_boundaries")
            .and_then(|value| value.as_array())
            .is_some_and(|boundaries| boundaries.iter().any(|boundary| {
                boundary.pointer("/reason").and_then(|value| value.as_str())
                    == Some("runtime_trace")
            }))
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/dependency/status")
            .and_then(|value| value.as_str()),
        Some("valid")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/artifact_lifetimes/status")
            .and_then(|value| value.as_str()),
        Some("valid")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/policy/status")
            .and_then(|value| value.as_str()),
        Some("valid")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/backend/status")
            .and_then(|value| value.as_str()),
        Some("valid")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/dependency/policy_edge_count")
            .and_then(|value| value.as_u64()),
        Some(4)
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/dependency/output_edge_count")
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/policy/requirements/0/backend_preference")
            .and_then(|value| value.as_str()),
        Some("cpu")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/policy/requirements/0/authority_scope")
            .and_then(|value| value.as_str()),
        Some("transition")
    );
    assert_eq!(
        sweep
            .pointer("/observer_projection/analysis/policy/requirements/0/supported_backends/0")
            .and_then(|value| value.as_str()),
        Some("cpu")
    );
    assert!(
        sweep
            .pointer("/observer_projection/analysis/observability/local_only_channels")
            .and_then(|value| value.as_array())
            .is_some_and(|channels| channels
                .iter()
                .any(|channel| { channel.as_str() == Some("runtime_trace") })
                && channels
                    .iter()
                    .any(|channel| { channel.as_str() == Some("observer_metrics") }))
    );
}

#[test]
fn cli_collision_plan_wgsl_reports_valid_transition_support() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("collision-plan")
        .arg("--query-backend=wgsl")
        .output()
        .expect("run collision-plan wgsl");
    assert!(
        output.status.success(),
        "collision-plan --query-backend=wgsl failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend: wgsl"));
    assert!(stdout.contains("validation: ok"));
    assert!(stdout.contains("authority_scope: transition"));
    assert!(stdout.contains("required_guarantee=conservative_no_false_miss"));
    assert!(stdout.contains("selected_method=conservative_solver"));
    assert!(stdout.contains("required_guarantee=interval_bounded"));
    assert!(stdout.contains("selected_method=interval_solver"));
    assert!(stdout.contains("shared spine: observer=collision owner=CollisionPlan"));
    assert!(stdout.contains("shared spine primitive nodes:"));
    assert!(stdout.contains(
        "shared policy summary: status=valid requirements=collision_policy[legal=true backends=wgsl supported=cpu|wgsl required_guarantee=conservative_no_false_miss selected_method=conservative_solver]"
    ));
    assert!(stdout.contains(
        "shared policy summary: status=valid requirements=collision_policy[legal=true backends=wgsl supported=cpu|wgsl required_guarantee=interval_bounded selected_method=interval_solver]"
    ));
    assert!(stdout.contains("shared observability report: common="));
}

#[test]
fn cli_collision_run_human_reports_runtime_results_and_reuse_trace() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("collision-run")
        .output()
        .expect("run collision-run");
    assert!(
        output.status.success(),
        "collision-run failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("collision run schema v1"));
    assert!(stdout.contains("backend: cpu"));
    assert!(stdout.contains("execution point-occupancy"));
    assert!(stdout.contains("execution sphere-sweep-first"));
    assert!(stdout.contains("execution sphere-sweep-reused"));
    assert!(stdout.contains("execution sphere-sweep-rejected"));
    assert!(stdout.contains("result: occupancy occupied="));
    assert!(stdout.contains("result: sweep hit=true"));
    assert!(stdout.contains("trace: contract=collision.point_occupancy.world"));
    assert!(stdout.contains("trace: contract=collision.sphere_sweep.transition"));
    assert!(stdout.contains("broadphase: candidate_count="));
    assert!(stdout.contains("rejected_candidate_count="));
    assert!(stdout.contains("pruned_node_count="));
    assert!(stdout.contains("fallback_count="));
    assert!(stdout.contains("interval bracket: ["));
    assert!(stdout.contains("contact normal provenance:"));
    assert!(stdout.contains("normal_provenance="));
    assert!(stdout.contains("reuse metrics: available=0 consumed=0 rejected=0 unavailable=2"));
    assert!(stdout.contains("reuse metrics: available=2 consumed=2 rejected=0 unavailable=0"));
    assert!(stdout.contains("reuse metrics: available=0 consumed=0 rejected=2 unavailable=0"));
    assert!(stdout.contains("verdict=consumed"));
    assert!(stdout.contains("verdict=rejected"));
}

#[test]
fn cli_collision_run_json_reports_results_and_reuse_diagnostics() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("collision-run")
        .arg("--json")
        .output()
        .expect("run collision-run json");
    assert!(
        output.status.success(),
        "collision-run --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("collision run json");
    assert_eq!(
        dump.get("schema_version").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        dump.get("backend").and_then(|value| value.as_str()),
        Some("cpu")
    );
    let executions = dump
        .get("executions")
        .and_then(|value| value.as_array())
        .expect("executions array");
    assert_eq!(executions.len(), 9);
    assert!(executions.iter().all(|execution| {
        execution
            .get("runtime_ns")
            .and_then(|value| value.as_u64())
            .is_some()
    }));
    let reused = executions
        .iter()
        .find(|execution| {
            execution
                .get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "sphere-sweep-reused")
        })
        .expect("reused execution");
    assert_eq!(
        reused
            .pointer("/trace/reuse_metrics/consumed_count")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        reused
            .pointer("/trace/broadphase_candidate_count")
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    assert!(
        reused
            .pointer("/trace/broadphase_rejected_candidate_count")
            .and_then(|value| value.as_u64())
            .is_some_and(|value| value > 0)
    );
    assert!(
        reused
            .pointer("/trace/broadphase_pruned_node_count")
            .and_then(|value| value.as_u64())
            .is_some_and(|value| value > 0)
    );
    assert!(
        reused
            .pointer("/trace/interval_bracket/0")
            .and_then(|value| value.as_f64())
            .is_some()
    );
    assert_eq!(
        reused
            .pointer("/trace/fallback_count")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    assert!(
        reused
            .pointer("/trace/interval_subdivisions")
            .and_then(|value| value.as_u64())
            .is_some()
    );
    let reused_trace_provenance = reused
        .pointer("/trace/contact_normal_provenance")
        .and_then(|value| value.as_str())
        .expect("trace contact normal provenance");
    assert_eq!(
        reused
            .pointer("/trace/reuse_decisions/0/verdict")
            .and_then(|value| value.as_str()),
        Some("consumed")
    );
    assert_eq!(
        reused
            .pointer("/result/kind")
            .and_then(|value| value.as_str()),
        Some("sweep")
    );
    assert_eq!(
        reused
            .pointer("/result/witness/normal_provenance")
            .and_then(|value| value.as_str()),
        Some(reused_trace_provenance)
    );
    assert_eq!(
        reused
            .pointer("/runtime_ns")
            .and_then(|value| value.as_u64())
            .map(|value| value > 0),
        Some(true)
    );
    let rejected = executions
        .iter()
        .find(|execution| {
            execution
                .get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "sphere-sweep-rejected")
        })
        .expect("rejected execution");
    assert_eq!(
        rejected
            .pointer("/trace/reuse_metrics/rejected_count")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    assert!(
        rejected
            .pointer("/trace/certificate_successes")
            .and_then(|value| value.as_u64())
            .is_some()
    );
    assert_eq!(
        rejected
            .pointer("/trace/reuse_decisions/0/reason")
            .and_then(|value| value.as_str()),
        Some("validity_rejected")
    );
    assert_eq!(
        rejected
            .pointer("/trace/transition/change_class")
            .and_then(|value| value.as_str()),
        Some("Presentation")
    );
    let ray_cast = executions
        .iter()
        .find(|execution| {
            execution
                .get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "ray-cast-first")
        })
        .expect("ray cast execution");
    assert_eq!(
        ray_cast
            .pointer("/result/kind")
            .and_then(|value| value.as_str()),
        Some("ray_cast")
    );
    let toi_reused = executions
        .iter()
        .find(|execution| {
            execution
                .get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "time-of-impact-reused")
        })
        .expect("toi reused execution");
    assert_eq!(
        toi_reused
            .pointer("/result/kind")
            .and_then(|value| value.as_str()),
        Some("time_of_impact")
    );
}

#[test]
fn cli_presentation_plan_human_shows_contracts_without_helper_names() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-plan")
        .arg(temp.path())
        .output()
        .expect("run presentation-plan");
    assert!(
        output.status.success(),
        "presentation-plan failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("presentation plan schema v1"));
    assert!(stdout.contains("plan cli_plan_view"));
    assert!(stdout.contains("plan cli_plan_fast_view"));
    assert!(stdout.contains("canonical_projection=true"));
    assert!(stdout.contains("input=Camera.vertical_fov_degrees"));
    assert!(stdout.contains("screen lattice: sample_position=PixelCenter origin=TopLeft"));
    assert!(stdout.contains("canonical view rays: space=World normalized_direction=true"));
    assert!(stdout.contains("compatibility_legacy_path=false"));
    assert!(stdout.contains("GenerateScreenSamples"));
    assert!(stdout.contains("PrimaryVisibility"));
    assert!(stdout.contains("SurfaceResolve"));
    assert!(stdout.contains("ParticipantsResolve"));
    assert!(stdout.contains("ShadePrimary"));
    assert!(stdout.contains("CompositeColor"));
    assert!(stdout.contains("TemporalResolve(shaded_color->color)"));
    assert!(stdout.contains("screen samples: viewport=view.viewport.widthxview.viewport.height"));
    assert!(stdout.contains("spatial.nearest.batch.world"));
    assert!(stdout.contains("surface.sample.batch.world"));
    assert!(stdout.contains("participants.radiance.batch.world"));
    assert!(stdout.contains("participants.medium.batch.world"));
    assert!(
        stdout
            .contains("primary hit attachment: primary_hit record=Hit3 depth=RayParameterDistance")
    );
    assert!(stdout.contains(
        "frame outputs: primary_hit(PrimaryHit,Hit3,Transient,Viewport,1x1,SemanticDefault)"
    ));
    assert!(
        stdout.contains(
            "history_color(Color,vec3<f32>,HistorySlot(0),Viewport,1x1,PreservePrevious)"
        )
    );
    assert!(stdout.contains(
        "history_primary_hit(PrimaryHit,Hit3,HistorySlot(1),Viewport,1x1,PreservePrevious)"
    ));
    assert!(stdout.contains("quality: tier=realtime_120 target_fps=120"));
    assert!(stdout.contains("quality: tier=realtime_60 target_fps=60"));
    assert!(stdout.contains(
        "lighting: key_light=lighting.key_light:Light:AuthoredMetadata:compat_alias=false"
    ));
    assert!(stdout.contains(
        "fill_direction=lighting.fill_direction:vec3<f32>:AuthoredMetadata:compat_alias=false"
    ));
    assert!(
        stdout.contains(
            "fill_strength=lighting.fill_strength:f32:AuthoredMetadata:compat_alias=false"
        )
    );
    assert!(stdout.contains(
        "ambient_color=lighting.ambient_color:vec3<f32>:AuthoredMetadata:compat_alias=false"
    ));
    assert!(stdout.contains(
        "fill_direction=lighting.fill_direction:vec3<f32>:DefaultCompatibilityRecipe:compat_alias=false"
    ));
    assert!(stdout.contains(
        "fill_strength=lighting.fill_strength:f32:DefaultCompatibilityRecipe:compat_alias=false"
    ));
    assert!(stdout.contains(
        "ambient_color=lighting.ambient_color:vec3<f32>:DefaultCompatibilityRecipe:compat_alias=false"
    ));
    assert!(stdout.contains("future acceleration hooks: ScreenLattice"));
    assert!(stdout.contains("future acceleration hooks: WorldBatch, SemanticSupport"));
    assert!(stdout.contains("motion(Motion,MotionVector,Transient,Viewport,1x1,SemanticDefault)"));
    assert!(stdout.contains("materializes: motion"));
    assert!(stdout.contains("materializes: color, history_color, history_primary_hit"));
    assert!(stdout.contains("shared spine: observer=presentation owner=PresentationPlan"));
    assert!(
        stdout
            .contains("shared spine primitive nodes: generate_screen_samples, primary_visibility")
    );
    assert!(stdout.contains("shared spine artifacts:"));
    assert!(stdout.contains("history_color[artifact.history_color]"));
    assert!(stdout.contains("validation_summary=true"));
    assert!(stdout.contains("observer_metrics_local_only=true"));
    assert!(stdout.contains("shared dependency graph: status=valid"));
    assert!(stdout.contains("shared artifact lifetimes: status=valid"));
    assert!(stdout.contains("shared backend summary: status=valid active="));
    assert!(stdout.contains("shared observability report: common="));
    assert!(!stdout.contains("shared artifact lifetime issues:"));
    assert!(!stdout.contains("shared dependency issues:"));
    assert!(stdout.contains("normalized projection (compat): family=presentation mode=temporal"));
    assert!(stdout.contains("normalized projection (compat): family=presentation mode=composite"));
    assert!(stdout.contains("passes=generate_screen_samples, primary_visibility"));
    assert!(stdout.contains(
        "queries=participants.medium.batch.world, participants.radiance.batch.world, spatial.nearest.batch.world, surface.sample.batch.world"
    ));
    assert!(stdout.contains("future acceleration hooks: TemporalHistory"));
    assert!(stdout.contains("semantic artifacts:"));
    assert!(stdout.contains("artifact.history_color kind=PresentationHistory"));
    assert!(stdout.contains("artifact uses:"));
    assert!(stdout.contains("actor=temporal_resolve artifact=artifact.history_color kind=produce"));
    assert!(
        stdout.contains("actor=temporal_resolve artifact=artifact.history_color kind=preserve")
    );
    assert!(stdout.contains("validation: ok"));
    assert!(
        stdout
            .contains("motion.resolve recipe=MotionResolve backend=auto execution=motion_resolve")
    );
    assert!(stdout.contains(
        "temporal.resolve recipe=TemporalResolve backend=auto execution=temporal_resolve"
    ));
    assert!(
        stdout.contains(
            "composite.color recipe=CompositeColor backend=auto execution=composite_color"
        )
    );
    assert!(!stdout.contains("__wr_render_capture_to_ppm"));
}

#[test]
fn cli_presentation_plan_json_reports_passes_bindings_and_query_dependencies() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-plan")
        .arg(temp.path())
        .arg("--json")
        .output()
        .expect("run presentation-plan json");
    assert!(
        output.status.success(),
        "presentation-plan --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("presentation plan json");
    assert_eq!(
        dump.get("schema_version").and_then(|value| value.as_u64()),
        Some(1)
    );
    let plans = dump
        .get("plans")
        .and_then(|value| value.as_array())
        .expect("plans array");
    assert_eq!(plans.len(), 2);
    let view_plan = plans
        .iter()
        .find(|plan| {
            plan.get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "cli_plan_view")
        })
        .expect("helper-rich view plan");
    assert_eq!(
        view_plan.get("name").and_then(|value| value.as_str()),
        Some("cli_plan_view")
    );
    assert_eq!(
        view_plan
            .pointer("/view/canonical_projection_input")
            .and_then(|value| value.as_str()),
        Some("Camera.vertical_fov_degrees")
    );
    assert_eq!(
        view_plan
            .pointer("/view/compatibility_projection/legacy_path_active")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/observer_kind")
            .and_then(|value| value.as_str()),
        Some("presentation")
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/spine/observer_kind")
            .and_then(|value| value.as_str()),
        Some("presentation")
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/spine/inputs/0/binding")
            .and_then(|value| value.as_str()),
        Some("world")
    );
    assert!(
        view_plan
            .pointer("/observer_projection/spine/nodes")
            .and_then(|value| value.as_array())
            .is_some_and(|nodes| nodes.iter().any(|node| {
                node.pointer("/family").and_then(|value| value.as_str())
                    == Some("primitive_invocation")
                    && node.pointer("/label").and_then(|value| value.as_str())
                        == Some("temporal_resolve")
            }))
    );
    assert!(
        view_plan
            .pointer("/observer_projection/lossy_boundaries")
            .and_then(|value| value.as_array())
            .is_some_and(|boundaries| boundaries.iter().any(|boundary| {
                boundary.pointer("/reason").and_then(|value| value.as_str())
                    == Some("temporal_detail")
            }))
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/spine/observability/runtime_trace_local_only")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/analysis/dependency/status")
            .and_then(|value| value.as_str()),
        Some("valid")
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/analysis/artifact_lifetimes/status")
            .and_then(|value| value.as_str()),
        Some("valid")
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/analysis/policy/status")
            .and_then(|value| value.as_str()),
        Some("valid")
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/analysis/backend/status")
            .and_then(|value| value.as_str()),
        Some("valid")
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/analysis/backend/binding_count")
            .and_then(|value| value.as_u64()),
        Some(8)
    );
    assert!(
        view_plan
            .pointer("/observer_projection/analysis/artifact_lifetimes/store_backed_loads")
            .and_then(|value| value.as_array())
            .is_some_and(|loads| loads.iter().any(|load| {
                load.pointer("/artifact_id")
                    .and_then(|value| value.as_str())
                    == Some("artifact.history_color")
            }))
    );
    assert!(
        view_plan
            .pointer("/observer_projection/analysis/observability/local_only_channels")
            .and_then(|value| value.as_array())
            .is_some_and(|channels| channels
                .iter()
                .any(|channel| { channel.as_str() == Some("runtime_trace") })
                && channels
                    .iter()
                    .any(|channel| { channel.as_str() == Some("observer_metrics") }))
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/analysis/artifact_lifetimes/issues")
            .and_then(|value| value.as_array())
            .map(|issues| issues.len()),
        Some(0)
    );
    assert_eq!(
        view_plan
            .pointer("/observer_projection/analysis/dependency/issues")
            .and_then(|value| value.as_array())
            .map(|issues| issues.len()),
        Some(0)
    );
    assert_eq!(
        view_plan
            .pointer("/normalized_projection/schema_version")
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        view_plan
            .pointer("/normalized_projection/source_plan")
            .and_then(|value| value.as_str()),
        Some("cli_plan_view")
    );
    assert_eq!(
        view_plan
            .pointer("/normalized_projection/family")
            .and_then(|value| value.as_str()),
        Some("presentation")
    );
    assert_eq!(
        view_plan
            .pointer("/normalized_projection/execution_mode")
            .and_then(|value| value.as_str()),
        Some("temporal")
    );
    assert!(
        view_plan
            .pointer("/normalized_projection/pass_kinds")
            .and_then(|value| value.as_array())
            .is_some_and(|values| values
                .iter()
                .any(|value| value.as_str() == Some("primary_visibility")))
    );
    assert!(
        view_plan
            .pointer("/normalized_projection/query_contracts")
            .and_then(|value| value.as_array())
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_str() == Some("spatial.nearest.batch.world"))
            })
    );
    assert!(
        view_plan
            .pointer("/normalized_projection/frame_artifacts")
            .and_then(|value| value.as_array())
            .is_some_and(|values| values
                .iter()
                .any(|value| value.as_str() == Some("history_color")))
    );
    assert!(
        view_plan
            .pointer("/semantic_artifacts")
            .and_then(|value| value.as_array())
            .is_some_and(|values| values.iter().any(|value| {
                value.pointer("/id").and_then(|field| field.as_str())
                    == Some("artifact.history_color")
                    && value.pointer("/kind").and_then(|field| field.as_str())
                        == Some("PresentationHistory")
            }))
    );
    assert!(
        view_plan
            .pointer("/artifact_uses")
            .and_then(|value| value.as_array())
            .is_some_and(|values| values.iter().any(|value| {
                value
                    .pointer("/artifact_id")
                    .and_then(|field| field.as_str())
                    == Some("artifact.history_color")
                    && value.pointer("/source").and_then(|field| field.as_str())
                        == Some("artifact-store")
            }))
    );
    assert_eq!(
        view_plan
            .pointer("/validation/status")
            .and_then(|value| value.as_str()),
        Some("ok")
    );
    assert!(
        view_plan
            .pointer("/validation/errors")
            .and_then(|value| value.as_array())
            .is_some_and(|values| values.is_empty())
    );
    assert_eq!(
        view_plan
            .pointer("/view/screen_lattice/width_source")
            .and_then(|value| value.as_str()),
        Some("view.viewport.width")
    );
    assert_eq!(
        view_plan
            .pointer("/view/screen_lattice/height_source")
            .and_then(|value| value.as_str()),
        Some("view.viewport.height")
    );
    assert_eq!(
        view_plan
            .pointer("/frame/quality/tier")
            .and_then(|value| value.as_str()),
        Some("realtime_120")
    );
    assert_eq!(
        view_plan
            .pointer("/frame/quality/target_fps")
            .and_then(|value| value.as_u64()),
        Some(120)
    );
    assert_eq!(
        view_plan
            .pointer("/frame/quality/allow_dynamic_resolution")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        view_plan
            .pointer("/frame/quality/primary_max_steps")
            .and_then(|value| value.as_i64()),
        Some(48)
    );
    assert_eq!(
        view_plan
            .pointer("/frame/lighting/key_light/source")
            .and_then(|value| value.as_str()),
        Some("AuthoredMetadata")
    );
    assert_eq!(
        view_plan
            .pointer("/frame/lighting/fill_strength/source")
            .and_then(|value| value.as_str()),
        Some("AuthoredMetadata")
    );
    assert_eq!(
        view_plan
            .pointer("/frame/temporal_reuse")
            .and_then(|value| value.as_str()),
        Some("ReprojectColorAndMotion")
    );
    let passes = view_plan
        .get("passes")
        .and_then(|value| value.as_array())
        .expect("passes array");
    assert_eq!(
        passes
            .iter()
            .map(|pass| pass.get("kind").and_then(|value| value.as_str()).unwrap())
            .collect::<Vec<_>>(),
        vec![
            "GenerateScreenSamples",
            "PrimaryVisibility(spatial.nearest.batch.world)",
            "SurfaceResolve(surface.sample.batch.world)",
            "ParticipantsResolve(radiance=participants.radiance.batch.world,medium=participants.medium.batch.world)",
            "ShadePrimary(shaded_color)",
            "MotionResolve(primary_hit->motion)",
            "TemporalResolve(shaded_color->color)",
            "ExportAttachment(color)",
        ]
    );
    assert!(passes.iter().any(|pass| {
        pass.get("kind").and_then(|value| value.as_str()) == Some("GenerateScreenSamples")
            && pass
                .pointer("/screen_samples/item_count_expression")
                .and_then(|value| value.as_str())
                == Some("view.viewport.width * view.viewport.height * 1")
    }));
    assert!(passes.iter().any(|pass| {
        pass.get("kind").and_then(|value| value.as_str())
            == Some("PrimaryVisibility(spatial.nearest.batch.world)")
            && pass
                .get("query_dependencies")
                .and_then(|value| value.as_array())
                .is_some_and(|deps| {
                    deps.iter().any(|dep| {
                        dep.get("contract_id").and_then(|value| value.as_str())
                            == Some("spatial.nearest.batch.world")
                    }) && deps.iter().any(|dep| {
                        dep.pointer("/solver_diagnostics/subject")
                            .and_then(|value| value.as_str())
                            == Some("spatial.nearest.batch.world")
                    }) && deps.iter().any(|dep| {
                        dep.pointer("/solver_diagnostics/fallback")
                            .and_then(|value| value.as_str())
                            == Some("exact-dense-sphere-tracing")
                    }) && deps.iter().any(|dep| {
                        dep.pointer("/evidence/origin")
                            .and_then(|value| value.as_str())
                            == Some("runtime-observed")
                            && dep
                                .pointer("/evidence/subject")
                                .and_then(|value| value.as_str())
                                == Some("spatial.nearest.batch.world::runtime")
                            && dep
                                .pointer("/evidence/scope")
                                .and_then(|value| value.as_str())
                                == Some("snapshot-local")
                            && dep
                                .pointer("/evidence/distance_semantics")
                                .and_then(|value| value.as_str())
                                == Some("conservative-lower-bound")
                            && dep
                                .pointer("/evidence/distance_refinement_path/0")
                                .and_then(|value| value.as_str())
                                == Some("runtime-observation(runtime planner placeholder)")
                            && dep
                                .pointer("/evidence/temporal_refinement_path/0")
                                .and_then(|value| value.as_str())
                                == Some("runtime-observation(runtime planner placeholder)")
                            && dep
                                .pointer("/evidence/support_class")
                                .and_then(|value| value.as_str())
                                == Some("unknown")
                            && dep
                                .pointer("/evidence/support_lower_bound_pruning")
                                .and_then(|value| value.as_str())
                                == Some("unknown")
                    })
                })
    }));
    let outputs = view_plan
        .pointer("/frame/outputs")
        .and_then(|value| value.as_array())
        .expect("view outputs");
    assert_eq!(outputs.len(), 11);
    assert!(
        outputs.iter().any(|output| {
            output.get("name").and_then(|value| value.as_str()) == Some("motion")
        })
    );
    assert!(outputs.iter().any(|output| {
        output.get("name").and_then(|value| value.as_str()) == Some("history_color")
    }));
    let bindings = view_plan
        .get("bindings")
        .and_then(|value| value.as_array())
        .expect("bindings array");
    assert_eq!(bindings.len(), 8);
    assert!(bindings.iter().any(|binding| {
        binding.get("id").and_then(|value| value.as_str()) == Some("motion.resolve")
            && binding.get("execution").and_then(|value| value.as_str()) == Some("motion_resolve")
    }));

    let fast_view = plans
        .iter()
        .find(|plan| {
            plan.get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "cli_plan_fast_view")
        })
        .expect("fast view plan");
    assert_eq!(
        fast_view
            .pointer("/frame/temporal_reuse")
            .and_then(|value| value.as_str()),
        None
    );
    assert_eq!(
        fast_view
            .pointer("/frame/quality/tier")
            .and_then(|value| value.as_str()),
        Some("realtime_60")
    );
    assert_eq!(
        fast_view
            .pointer("/frame/quality/temporal_mode")
            .and_then(|value| value.as_str()),
        Some("Disabled")
    );
    assert_eq!(
        fast_view
            .pointer("/frame/quality/allow_radiance")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        fast_view
            .pointer("/frame/lighting/fill_direction/source")
            .and_then(|value| value.as_str()),
        Some("DefaultCompatibilityRecipe")
    );
    let fast_outputs = fast_view
        .pointer("/frame/outputs")
        .and_then(|value| value.as_array())
        .expect("fast view outputs");
    assert_eq!(fast_outputs.len(), 6);
    assert!(
        !fast_outputs.iter().any(|output| {
            output.get("name").and_then(|value| value.as_str()) == Some("motion")
        })
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("__wr_render_capture_to_ppm"),
        "presentation-plan JSON should not expose raw helper names"
    );
}
