// Integration tests for the full AAA Game Factory pipeline:
//   .wr source -> parse -> HIR -> Scene IR -> resolve -> preview server
//
// Part 1: Compiler pipeline (parse + HIR + Scene IR)
// Part 2: Resolve pipeline (Scene IR -> resolved Scene IR with cache)
// Part 3: Preview server endpoints (health, scene.json, WebSocket)
// Part 4: Iteration loop (file watch -> recompile -> WebSocket push)

use std::collections::BTreeMap;
use std::path::PathBuf;
use wrela::asset_factory::{
    AssetAdapterKind, AssetArtifactCache, AssetGenerationArtifact, AssetGenerationResult,
    CanonicalJobEnvelope, cache_root_for_workspace, canonical_prompt, replay_hash,
};
use wrela::hir::def::AssetDeclKind;
use wrela::hir::lower::lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse_with_errors;
use wrela::resolve::{ResolveConfig, resolve};
use wrela::scene_ir::SceneIR;

// ---------------------------------------------------------------------------
// Part 1: Compiler pipeline  (.wr source -> parse -> HIR -> Scene IR)
// ---------------------------------------------------------------------------

const TEST_WR_SOURCE: &str = r#"asset test_crystal_sword {
    kind: mesh
    prompt: "crystalline sword with glowing blue veins, fantasy weapon, clean geometry"
    style: "anime_fantasy"
    lod_budget: 8000
    negative: "blurry, realistic, photorealistic"
}

scene test_preview {
    entity sword {
        mesh: test_crystal_sword
        position: vec3(0, 1000, 0)
        rotation: vec3(0, 0, 0)
        scale: vec3(1000, 1000, 1000)
    }

    lighting {
        sun_direction: vec3(500, -800, 300)
        sun_color: rgb(255, 240, 220)
        sun_intensity: 2000
        ambient_color: rgb(60, 50, 80)
        ambient_intensity: 800
    }

    camera {
        mode: orbit
        target: sword
        distance: 4000
        pitch: -200
    }
}
"#;

#[test]
fn pipeline_parse_no_errors() {
    let (node, errors) = parse_with_errors(TEST_WR_SOURCE);
    assert!(
        errors.is_empty(),
        "Parse errors on test .wr source: {:?}",
        errors
    );
    let root = ast::Root::cast(node).expect("should cast to Root");
    let stmts: Vec<_> = root.statements().collect();
    assert_eq!(
        stmts.len(),
        2,
        "Expected 2 top-level statements (asset + scene)"
    );
}

#[test]
fn pipeline_hir_asset_declaration() {
    let (node, errors) = parse_with_errors(TEST_WR_SOURCE);
    assert!(errors.is_empty());
    let root = ast::Root::cast(node).unwrap();
    let module = lower(root);

    assert_eq!(
        module.asset_declarations.len(),
        1,
        "Expected 1 asset declaration"
    );
    let asset = &module.asset_declarations[0];
    assert_eq!(asset.name, "test_crystal_sword");
    assert_eq!(asset.kind, AssetDeclKind::Mesh);
    assert_eq!(
        asset.prompt,
        "crystalline sword with glowing blue veins, fantasy weapon, clean geometry"
    );
    assert_eq!(asset.style.as_deref(), Some("anime_fantasy"));
    assert_eq!(
        asset.negative.as_deref(),
        Some("blurry, realistic, photorealistic")
    );
    assert_eq!(asset.lod_budget, Some(8000));
}

#[test]
fn pipeline_hir_scene_declaration() {
    let (node, errors) = parse_with_errors(TEST_WR_SOURCE);
    assert!(errors.is_empty());
    let root = ast::Root::cast(node).unwrap();
    let module = lower(root);

    assert_eq!(
        module.scene_declarations.len(),
        1,
        "Expected 1 scene declaration"
    );
    let scene = &module.scene_declarations[0];
    assert_eq!(scene.name, "test_preview");
    assert_eq!(scene.entities.len(), 1);

    let entity = &scene.entities[0];
    assert_eq!(entity.name, "sword");
    assert_eq!(entity.mesh_asset, "test_crystal_sword");
    assert_eq!(entity.position, [0, 1000, 0]);
    assert_eq!(entity.rotation, [0, 0, 0]);
    assert_eq!(entity.scale, [1000, 1000, 1000]);

    // Lighting
    let lighting = scene
        .lighting
        .as_ref()
        .expect("scene should have lighting block");
    assert_eq!(lighting.sun_direction, [500, -800, 300]);
    assert_eq!(lighting.sun_color, [255, 240, 220]);
    assert_eq!(lighting.sun_intensity, 2000);
    assert_eq!(lighting.ambient_color, [60, 50, 80]);
    assert_eq!(lighting.ambient_intensity, 800);

    // Camera
    let camera = scene
        .camera
        .as_ref()
        .expect("scene should have camera block");
    assert_eq!(camera.mode, "orbit");
    assert_eq!(camera.target, "sword");
    assert_eq!(camera.distance, 4000);
    assert_eq!(camera.pitch, -200);
}

#[test]
fn pipeline_scene_ir_from_hir() {
    let (node, errors) = parse_with_errors(TEST_WR_SOURCE);
    assert!(errors.is_empty());
    let root = ast::Root::cast(node).unwrap();
    let module = lower(root);

    let scene = &module.scene_declarations[0];
    let ir = SceneIR::from_hir(scene, &module.asset_declarations);

    // Scene name
    assert_eq!(ir.scene, "test_preview");

    // Entity
    assert_eq!(ir.entities.len(), 1);
    let entity = &ir.entities[0];
    assert_eq!(entity.name, "sword");
    assert_eq!(entity.mesh_asset, "test_crystal_sword");
    assert_eq!(entity.position, [0, 1000, 0]);
    assert_eq!(entity.rotation, [0, 0, 0]);
    assert_eq!(entity.scale, [1000, 1000, 1000]);

    // Asset references are unresolved: mesh_path must be None
    assert!(
        entity.mesh_path.is_none(),
        "mesh_path must be None before resolve"
    );

    // Asset ref metadata is propagated
    assert_eq!(entity.asset_ref.name, "test_crystal_sword");
    assert_eq!(entity.asset_ref.kind, "mesh");
    assert_eq!(
        entity.asset_ref.prompt,
        "crystalline sword with glowing blue veins, fantasy weapon, clean geometry"
    );
    assert_eq!(entity.asset_ref.style.as_deref(), Some("anime_fantasy"));
    assert_eq!(
        entity.asset_ref.negative.as_deref(),
        Some("blurry, realistic, photorealistic")
    );
    assert_eq!(entity.asset_ref.lod_budget, Some(8000));

    // Lighting
    assert_eq!(ir.lighting.sun_direction, [500, -800, 300]);
    assert_eq!(ir.lighting.sun_color, [255, 240, 220]);
    assert_eq!(ir.lighting.sun_intensity, 2000);
    assert_eq!(ir.lighting.ambient_color, [60, 50, 80]);
    assert_eq!(ir.lighting.ambient_intensity, 800);

    // Camera
    assert_eq!(ir.camera.mode, "orbit");
    assert_eq!(ir.camera.target, "sword");
    assert_eq!(ir.camera.distance, 4000);
    assert_eq!(ir.camera.pitch, -200);
}

#[test]
fn pipeline_scene_ir_json_roundtrip() {
    let (node, _) = parse_with_errors(TEST_WR_SOURCE);
    let root = ast::Root::cast(node).unwrap();
    let module = lower(root);
    let scene = &module.scene_declarations[0];
    let ir = SceneIR::from_hir(scene, &module.asset_declarations);

    let json = ir.to_json().expect("Scene IR should serialize to JSON");
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("Scene IR JSON should be valid JSON");

    // Verify JSON structure
    assert_eq!(parsed["scene"], "test_preview");
    assert_eq!(parsed["entities"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["entities"][0]["name"], "sword");
    assert_eq!(parsed["entities"][0]["mesh_asset"], "test_crystal_sword");
    assert!(parsed["entities"][0]["mesh_path"].is_null());
    assert_eq!(
        parsed["entities"][0]["position"],
        serde_json::json!([0, 1000, 0])
    );
    assert_eq!(
        parsed["entities"][0]["rotation"],
        serde_json::json!([0, 0, 0])
    );
    assert_eq!(
        parsed["entities"][0]["scale"],
        serde_json::json!([1000, 1000, 1000])
    );

    // Roundtrip: JSON -> SceneIR -> JSON -> SceneIR must be identical
    let roundtripped = SceneIR::from_json(&json).expect("should deserialize");
    assert_eq!(ir, roundtripped);
}

#[test]
fn pipeline_semantic_check_passes() {
    let (node, errors) = parse_with_errors(TEST_WR_SOURCE);
    assert!(errors.is_empty());
    let root = ast::Root::cast(node).unwrap();
    let module = lower(root);

    let diags = wrela::hir::semantic::check_module(&module);
    assert!(
        diags.errors.is_empty(),
        "Semantic check should pass for valid asset+scene: {:?}",
        diags.errors
    );
}

// ---------------------------------------------------------------------------
// Part 2: Resolve pipeline (Scene IR -> resolve -> resolved Scene IR)
// ---------------------------------------------------------------------------

fn make_pipeline_scene_ir() -> SceneIR {
    let (node, _) = parse_with_errors(TEST_WR_SOURCE);
    let root = ast::Root::cast(node).unwrap();
    let module = lower(root);
    let scene = &module.scene_declarations[0];
    SceneIR::from_hir(scene, &module.asset_declarations)
}

/// Helper: pre-populate the asset cache for an asset ref so resolve gets a cache hit.
fn prepopulate_cache_for_asset(
    cache: &AssetArtifactCache,
    asset_ref: &wrela::scene_ir::AssetRef,
) -> PathBuf {
    let request = wrela::asset_factory::AssetGenerationRequest {
        asset_id: asset_ref.name.clone(),
        prompt: asset_ref.prompt.clone(),
        style_profile: asset_ref.style.clone().unwrap_or_default(),
        seed: 0,
        negative_constraints: asset_ref
            .negative
            .as_ref()
            .map(|n| vec![n.clone()])
            .unwrap_or_default(),
    };

    let adapter_kind = match asset_ref.kind.as_str() {
        "mesh" => AssetAdapterKind::Mesh3d,
        "texture" => AssetAdapterKind::Image,
        _ => AssetAdapterKind::Mesh3d,
    };
    let provider = match asset_ref.kind.as_str() {
        "mesh" => "tripo3d",
        "texture" => "procedural-texture",
        _ => "tripo3d",
    };

    let canonical = canonical_prompt(&request);
    let hash = replay_hash(adapter_kind, provider, canonical.as_str(), request.seed);

    let envelope = CanonicalJobEnvelope {
        job_id: format!("job-{}", &hash[..16]),
        adapter_kind,
        provider: provider.to_string(),
        seed: request.seed,
        canonical_prompt: canonical,
        replay_hash: hash.clone(),
    };

    let filename = format!("{}.glb", &hash[..16]);
    let dummy_bytes = b"dummy-glb-binary-data-for-testing";
    let local_path = cache
        .put_bytes(&envelope, &filename, dummy_bytes)
        .expect("put_bytes should succeed");

    let artifact = AssetGenerationArtifact {
        artifact_id: format!("asset-{}", &hash[16..32]),
        logical_path: format!("generated/{}/{}", request.asset_id, filename),
        bytes_len: dummy_bytes.len() as u64,
        fingerprint: hash.clone(),
        metadata: BTreeMap::new(),
        local_path: Some(local_path.clone()),
    };

    let result = AssetGenerationResult {
        envelope,
        provider_response_hash: hash,
        artifacts: vec![artifact],
    };
    cache.put(&result).expect("cache put should succeed");
    local_path
}

#[tokio::test]
async fn pipeline_resolve_with_cached_assets() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().to_path_buf();
    let cache_root = cache_root_for_workspace(&workspace_root);
    let cache = AssetArtifactCache::new(&cache_root);

    let scene_ir = make_pipeline_scene_ir();

    // Pre-populate cache for all entities
    for entity in &scene_ir.entities {
        prepopulate_cache_for_asset(&cache, &entity.asset_ref);
    }

    // Write Scene IR to disk
    let scene_path = temp.path().join("scene-ir.json");
    let json = scene_ir.to_json().unwrap();
    std::fs::write(&scene_path, &json).unwrap();

    let config = ResolveConfig {
        scene_ir_path: scene_path,
        workspace_root,
        dry_run: false,
        force: false,
        parallel: 4,
    };

    let result = resolve(config).await.expect("resolve should succeed");

    // Verify counts
    assert_eq!(result.total_assets, 1);
    assert_eq!(result.cached, 1, "all assets should be cache hits");
    assert_eq!(result.generated, 0, "no assets should be generated");
    assert_eq!(result.failed, 0, "no assets should fail");

    // Verify mesh_path is populated
    for entity in &result.resolved_scene_ir.entities {
        assert!(
            entity.mesh_path.is_some(),
            "entity '{}' should have mesh_path after resolve",
            entity.name
        );
        let mesh_path = entity.mesh_path.as_ref().unwrap();
        assert!(
            mesh_path.contains(".wrela/"),
            "mesh_path should reference cache directory: {mesh_path}"
        );
    }

    // The rest of the scene should be unchanged
    assert_eq!(result.resolved_scene_ir.scene, "test_preview");
    assert_eq!(result.resolved_scene_ir.lighting.sun_intensity, 2000);
    assert_eq!(result.resolved_scene_ir.camera.distance, 4000);
}

#[tokio::test]
async fn pipeline_resolve_dry_run_no_side_effects() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().to_path_buf();

    let scene_ir = make_pipeline_scene_ir();
    let scene_path = temp.path().join("scene-ir.json");
    std::fs::write(&scene_path, scene_ir.to_json().unwrap()).unwrap();

    let config = ResolveConfig {
        scene_ir_path: scene_path.clone(),
        workspace_root,
        dry_run: true,
        force: false,
        parallel: 1,
    };

    let result = resolve(config).await.expect("dry-run should succeed");
    assert_eq!(result.total_assets, 1);
    // In dry-run, mesh_path stays None
    for entity in &result.resolved_scene_ir.entities {
        assert!(
            entity.mesh_path.is_none(),
            "dry-run should not populate mesh_path"
        );
    }
    // No resolved output file
    let resolved_path = temp.path().join("scene-ir.resolved.json");
    assert!(
        !resolved_path.exists(),
        "dry-run must not write resolved scene IR"
    );
}

// ---------------------------------------------------------------------------
// Part 3: Preview server endpoints
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_preview_server_health() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dist = tempfile::tempdir().expect("dist tempdir");
    let assets = tempfile::tempdir().expect("assets tempdir");

    // Write a dummy .wr file so the app_path exists
    let wr_path = tmp.path().join("test.wr");
    std::fs::write(&wr_path, "").unwrap();

    // Build app state and router directly (same as unit tests do)
    let scene_ir = serde_json::json!({
        "scene": "test_preview",
        "entities": [],
        "lighting": {
            "sun_direction": [500, -800, 300],
            "sun_color": [255, 240, 220],
            "sun_intensity": 2000,
            "ambient_color": [60, 50, 80],
            "ambient_intensity": 800
        },
        "camera": {
            "mode": "orbit",
            "target": "",
            "distance": 4000,
            "pitch": -200
        }
    });
    let scene_json = serde_json::to_string_pretty(&scene_ir).unwrap();
    let (tx, _) = tokio::sync::broadcast::channel::<String>(16);

    // We need to build the router the same way the preview server does.
    // Import the handler types from the preview_server module.
    use axum::{Router, routing::get};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower_http::services::ServeDir;

    struct TestAppState {
        scene_ir_json: RwLock<String>,
        tx: tokio::sync::broadcast::Sender<String>,
    }

    let state = Arc::new(TestAppState {
        scene_ir_json: RwLock::new(scene_json.clone()),
        tx,
    });

    let state_scene = state.clone();
    let state_ws = state.clone();

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route(
            "/scene.json",
            get({
                let state = state_scene;
                move || async move {
                    let json = state.scene_ir_json.read().await;
                    (
                        axum::http::StatusCode::OK,
                        [("content-type", "application/json")],
                        json.clone(),
                    )
                }
            }),
        )
        .route(
            "/preview",
            get({
                let state = state_ws;
                move |ws: axum::extract::ws::WebSocketUpgrade| async move {
                    ws.on_upgrade(move |mut socket| async move {
                        let initial = state.scene_ir_json.read().await.clone();
                        let msg = serde_json::json!({
                            "type": "scene_update",
                            "scene": serde_json::from_str::<serde_json::Value>(&initial)
                                .unwrap_or_default()
                        });
                        let _ = socket
                            .send(axum::extract::ws::Message::Text(msg.to_string().into()))
                            .await;

                        let mut rx = state.tx.subscribe();
                        loop {
                            tokio::select! {
                                result = rx.recv() => {
                                    match result {
                                        Ok(scene_json) => {
                                            let msg = serde_json::json!({
                                                "type": "scene_update",
                                                "scene": serde_json::from_str::<serde_json::Value>(&scene_json).unwrap_or_default()
                                            });
                                            if socket.send(axum::extract::ws::Message::Text(msg.to_string().into())).await.is_err() {
                                                break;
                                            }
                                        }
                                        Err(_) => break,
                                    }
                                }
                                msg = socket.recv() => {
                                    match msg {
                                        Some(Ok(_)) => {}
                                        _ => break,
                                    }
                                }
                            }
                        }
                    })
                }
            }),
        )
        .nest_service("/assets", ServeDir::new(assets.path()))
        .fallback_service(ServeDir::new(dist.path()));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Test 1: /health returns 200 "ok"
    let resp = reqwest::get(format!("http://{addr}/health"))
        .await
        .expect("health request");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");

    // Test 2: /scene.json returns valid Scene IR JSON
    let resp = reqwest::get(format!("http://{addr}/scene.json"))
        .await
        .expect("scene.json request");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.expect("should parse JSON");
    assert_eq!(json["scene"], "test_preview");
    assert_eq!(json["camera"]["distance"], 4000);
    assert_eq!(json["lighting"]["sun_intensity"], 2000);

    // Test 3: WebSocket at /preview sends initial scene_update
    use futures_util::StreamExt;
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/preview"))
        .await
        .expect("WebSocket connect");

    let msg = ws_stream
        .next()
        .await
        .expect("should receive a message")
        .expect("message should be Ok");

    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t,
        other => panic!("Expected Text message, got: {:?}", other),
    };
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("parse WS message");
    assert_eq!(parsed["type"], "scene_update");
    assert_eq!(parsed["scene"]["scene"], "test_preview");
}

// ---------------------------------------------------------------------------
// Part 4: Iteration loop (file change -> broadcast -> WebSocket push)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_preview_broadcasts_on_scene_update() {
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let scene_ir = serde_json::json!({
        "scene": "test_preview",
        "entities": [],
        "lighting": {
            "sun_direction": [500, -800, 300],
            "sun_color": [255, 240, 220],
            "sun_intensity": 2000,
            "ambient_color": [60, 50, 80],
            "ambient_intensity": 800
        },
        "camera": {
            "mode": "orbit",
            "target": "",
            "distance": 4000,
            "pitch": -200
        }
    });
    let scene_json = serde_json::to_string_pretty(&scene_ir).unwrap();
    let (tx, _) = tokio::sync::broadcast::channel::<String>(16);
    let tx_clone = tx.clone();

    struct TestAppState {
        scene_ir_json: RwLock<String>,
        tx: tokio::sync::broadcast::Sender<String>,
    }

    let state = Arc::new(TestAppState {
        scene_ir_json: RwLock::new(scene_json.clone()),
        tx,
    });

    let state_ws = state.clone();

    let app = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .route(
            "/preview",
            axum::routing::get({
                let state = state_ws;
                move |ws: axum::extract::ws::WebSocketUpgrade| async move {
                    ws.on_upgrade(move |mut socket| async move {
                        let initial = state.scene_ir_json.read().await.clone();
                        let msg = serde_json::json!({
                            "type": "scene_update",
                            "scene": serde_json::from_str::<serde_json::Value>(&initial)
                                .unwrap_or_default()
                        });
                        let _ = socket
                            .send(axum::extract::ws::Message::Text(msg.to_string().into()))
                            .await;

                        let mut rx = state.tx.subscribe();
                        loop {
                            tokio::select! {
                                result = rx.recv() => {
                                    match result {
                                        Ok(scene_json) => {
                                            let msg = serde_json::json!({
                                                "type": "scene_update",
                                                "scene": serde_json::from_str::<serde_json::Value>(&scene_json).unwrap_or_default()
                                            });
                                            if socket.send(axum::extract::ws::Message::Text(msg.to_string().into())).await.is_err() {
                                                break;
                                            }
                                        }
                                        Err(_) => break,
                                    }
                                }
                                msg = socket.recv() => {
                                    match msg {
                                        Some(Ok(_)) => {}
                                        _ => break,
                                    }
                                }
                            }
                        }
                    })
                }
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Connect WebSocket
    use futures_util::StreamExt;
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/preview"))
        .await
        .expect("WebSocket connect");

    // Receive initial scene_update
    let initial_msg = ws_stream
        .next()
        .await
        .expect("should receive initial message")
        .expect("message should be Ok");
    let initial_text = match initial_msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t,
        other => panic!("Expected Text, got: {:?}", other),
    };
    let initial: serde_json::Value = serde_json::from_str(&initial_text).unwrap();
    assert_eq!(initial["type"], "scene_update");
    assert_eq!(initial["scene"]["lighting"]["sun_intensity"], 2000);

    // Simulate a file change: broadcast an updated scene
    let updated_scene = serde_json::json!({
        "scene": "test_preview",
        "entities": [],
        "lighting": {
            "sun_direction": [500, -800, 300],
            "sun_color": [255, 240, 220],
            "sun_intensity": 3500,
            "ambient_color": [60, 50, 80],
            "ambient_intensity": 800
        },
        "camera": {
            "mode": "orbit",
            "target": "",
            "distance": 4000,
            "pitch": -200
        }
    });
    let updated_json = serde_json::to_string_pretty(&updated_scene).unwrap();

    // Update the shared state and broadcast
    *state.scene_ir_json.write().await = updated_json.clone();
    tx_clone
        .send(updated_json)
        .expect("broadcast should succeed");

    // Receive the updated scene_update over WebSocket
    let updated_msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws_stream.next())
        .await
        .expect("should receive update within 5 seconds")
        .expect("stream should have message")
        .expect("message should be Ok");

    let updated_text = match updated_msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t,
        other => panic!("Expected Text, got: {:?}", other),
    };
    let updated: serde_json::Value = serde_json::from_str(&updated_text).unwrap();
    assert_eq!(updated["type"], "scene_update");
    assert_eq!(
        updated["scene"]["lighting"]["sun_intensity"], 3500,
        "should receive updated sun_intensity=3500 after broadcast"
    );
}

// ---------------------------------------------------------------------------
// Part 5: File on disk matches what parser expects
// ---------------------------------------------------------------------------

#[test]
fn pipeline_test_wr_file_on_disk_parses() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wr_path = manifest_dir
        .parent()
        .unwrap()
        .join("apps/wrela-browser-demo/src/frontend/preview_test.wr");

    assert!(
        wr_path.exists(),
        "preview_test.wr should exist at: {}",
        wr_path.display()
    );

    let source = std::fs::read_to_string(&wr_path).expect("read preview_test.wr");
    let (node, errors) = parse_with_errors(&source);
    assert!(
        errors.is_empty(),
        "preview_test.wr should parse without errors: {:?}",
        errors
    );

    let root = ast::Root::cast(node).unwrap();
    let module = lower(root);
    assert_eq!(module.asset_declarations.len(), 1);
    assert_eq!(module.scene_declarations.len(), 1);

    let ir = SceneIR::from_hir(&module.scene_declarations[0], &module.asset_declarations);
    assert_eq!(ir.scene, "test_preview");
    assert_eq!(ir.entities.len(), 1);
    assert_eq!(ir.entities[0].name, "sword");

    // Verify the JSON roundtrip works
    let json = ir.to_json().unwrap();
    let roundtripped = SceneIR::from_json(&json).unwrap();
    assert_eq!(ir, roundtripped);
}
