use crate::asset_factory::texture_gen::TextureGenerationAdapter;
use crate::asset_factory::tripo::TripoMeshAdapter;
use crate::asset_factory::{
    AssetAdapterKind, AssetArtifactCache, AssetGenerationRequest, AsyncAssetAdapter,
    cache_root_for_workspace, canonical_prompt, replay_hash,
};
use crate::scene_ir::{AssetRef, SceneIR};
use std::path::PathBuf;
use std::sync::Arc;

pub struct ResolveConfig {
    pub scene_ir_path: PathBuf,
    pub workspace_root: PathBuf,
    pub dry_run: bool,
    pub force: bool,
    pub parallel: usize,
}

pub struct ResolveResult {
    pub total_assets: usize,
    pub cached: usize,
    pub generated: usize,
    pub failed: usize,
    pub resolved_scene_ir: SceneIR,
}

/// Maps an `AssetRef` kind string to the adapter kind enum used by the asset factory.
fn adapter_kind_for_ref(asset_ref: &AssetRef) -> AssetAdapterKind {
    match asset_ref.kind.as_str() {
        "mesh" => AssetAdapterKind::Mesh3d,
        "texture" => AssetAdapterKind::Image,
        "audio" => AssetAdapterKind::Audio,
        _ => AssetAdapterKind::Mesh3d,
    }
}

/// Selects the provider string for a given asset kind.
fn provider_for_kind(kind: &str) -> &'static str {
    match kind {
        "mesh" => "tripo3d",
        "texture" => "procedural-texture",
        _ => "tripo3d",
    }
}

/// Builds an `AssetGenerationRequest` from an `AssetRef`.
fn request_from_asset_ref(asset_ref: &AssetRef) -> AssetGenerationRequest {
    AssetGenerationRequest {
        asset_id: asset_ref.name.clone(),
        prompt: asset_ref.prompt.clone(),
        style_profile: asset_ref.style.clone().unwrap_or_default(),
        seed: 0,
        negative_constraints: asset_ref
            .negative
            .as_ref()
            .map(|n| vec![n.clone()])
            .unwrap_or_default(),
    }
}

/// Computes the cache key (replay hash) for an asset ref.
#[cfg(test)]
fn cache_key_for_asset_ref(asset_ref: &AssetRef) -> String {
    let request = request_from_asset_ref(asset_ref);
    let adapter_kind = adapter_kind_for_ref(asset_ref);
    let provider = provider_for_kind(asset_ref.kind.as_str());
    let canonical = canonical_prompt(&request);
    replay_hash(adapter_kind, provider, canonical.as_str(), request.seed)
}

/// Information about a single asset resolution attempt.
struct AssetResolution {
    entity_index: usize,
    mesh_path: Option<String>,
    cached: bool,
    generated: bool,
    failed: bool,
    error: Option<String>,
}

pub async fn resolve(config: ResolveConfig) -> Result<ResolveResult, String> {
    // 1. Read scene IR JSON from file.
    let scene_json = std::fs::read_to_string(&config.scene_ir_path)
        .map_err(|e| format!("failed to read scene IR: {e}"))?;
    let mut scene_ir = SceneIR::from_json(&scene_json)?;

    // 2. Collect asset refs that need resolution (entities with no mesh_path yet).
    let cache_root = cache_root_for_workspace(&config.workspace_root);
    let cache = AssetArtifactCache::new(&cache_root);

    let total_assets = scene_ir.entities.len();

    if config.dry_run {
        return resolve_dry_run(&scene_ir, &cache, total_assets);
    }

    // 3. Build resolution tasks.
    let parallel = config.parallel.max(1);
    let cache = Arc::new(cache);
    let cache_root = Arc::new(cache_root);
    let force = config.force;

    let semaphore = Arc::new(tokio::sync::Semaphore::new(parallel));
    let mut handles = Vec::with_capacity(total_assets);

    for (idx, entity) in scene_ir.entities.iter().enumerate() {
        let asset_ref = entity.asset_ref.clone();
        let cache = Arc::clone(&cache);
        let cache_root = Arc::clone(&cache_root);
        let sem = Arc::clone(&semaphore);

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed unexpectedly");
            resolve_single_asset(idx, &asset_ref, &cache, &cache_root, force).await
        });
        handles.push(handle);
    }

    // 4. Collect results.
    let mut cached_count = 0usize;
    let mut generated_count = 0usize;
    let mut failed_count = 0usize;

    for handle in handles {
        let resolution = handle.await.map_err(|e| format!("task join error: {e}"))?;
        if resolution.cached {
            cached_count += 1;
        }
        if resolution.generated {
            generated_count += 1;
        }
        if resolution.failed {
            failed_count += 1;
            if let Some(ref err) = resolution.error {
                eprintln!(
                    "warning: failed to resolve asset '{}': {err}",
                    scene_ir.entities[resolution.entity_index].asset_ref.name
                );
            }
        }
        if let Some(ref mesh_path) = resolution.mesh_path {
            scene_ir.entities[resolution.entity_index].mesh_path = Some(mesh_path.clone());
        }
    }

    // 5. Write resolved scene IR.
    let output_path = resolved_output_path(&config.scene_ir_path);
    let resolved_json = scene_ir.to_json()?;
    std::fs::write(&output_path, &resolved_json)
        .map_err(|e| format!("failed to write resolved scene IR: {e}"))?;
    eprintln!("wrote resolved scene IR to {}", output_path.display());

    Ok(ResolveResult {
        total_assets,
        cached: cached_count,
        generated: generated_count,
        failed: failed_count,
        resolved_scene_ir: scene_ir,
    })
}

fn resolve_dry_run(
    scene_ir: &SceneIR,
    cache: &AssetArtifactCache,
    total_assets: usize,
) -> Result<ResolveResult, String> {
    let mut cached = 0usize;
    let mut would_generate = 0usize;

    for entity in &scene_ir.entities {
        let asset_ref = &entity.asset_ref;
        let request = request_from_asset_ref(asset_ref);
        let adapter_kind = adapter_kind_for_ref(asset_ref);
        let provider = provider_for_kind(asset_ref.kind.as_str());
        let canonical = canonical_prompt(&request);
        let hash = replay_hash(adapter_kind, provider, canonical.as_str(), request.seed);

        let envelope = crate::asset_factory::CanonicalJobEnvelope {
            job_id: format!("job-{}", &hash[..16]),
            adapter_kind,
            provider: provider.to_string(),
            seed: request.seed,
            canonical_prompt: canonical,
            replay_hash: hash.clone(),
        };

        match cache.get(&envelope) {
            Ok(Some(_)) => {
                eprintln!("  [cached]   {}", asset_ref.name);
                cached += 1;
            }
            _ => {
                eprintln!("  [generate] {} (kind={})", asset_ref.name, asset_ref.kind);
                would_generate += 1;
            }
        }
    }

    eprintln!(
        "dry-run: {} total, {} cached, {} would be generated",
        total_assets, cached, would_generate
    );

    Ok(ResolveResult {
        total_assets,
        cached,
        generated: 0,
        failed: 0,
        resolved_scene_ir: scene_ir.clone(),
    })
}

async fn resolve_single_asset(
    entity_index: usize,
    asset_ref: &AssetRef,
    cache: &AssetArtifactCache,
    cache_root: &std::path::Path,
    force: bool,
) -> AssetResolution {
    let request = request_from_asset_ref(asset_ref);
    let adapter_kind = adapter_kind_for_ref(asset_ref);
    let provider = provider_for_kind(asset_ref.kind.as_str());
    let canonical = canonical_prompt(&request);
    let hash = replay_hash(adapter_kind, provider, canonical.as_str(), request.seed);

    let envelope = crate::asset_factory::CanonicalJobEnvelope {
        job_id: format!("job-{}", &hash[..16]),
        adapter_kind,
        provider: provider.to_string(),
        seed: request.seed,
        canonical_prompt: canonical,
        replay_hash: hash.clone(),
    };

    // Check cache first (unless force).
    if !force {
        match cache.get(&envelope) {
            Ok(Some(cached_result)) => {
                // Find the first artifact with a local_path.
                let mesh_path = cached_result
                    .artifacts
                    .iter()
                    .find_map(|a| a.local_path.as_ref())
                    .map(|p| make_relative_path(p, cache_root));

                return AssetResolution {
                    entity_index,
                    mesh_path,
                    cached: true,
                    generated: false,
                    failed: false,
                    error: None,
                };
            }
            Ok(None) => {} // cache miss, continue to generate
            Err(e) => {
                eprintln!("warning: cache read error for '{}': {e}", asset_ref.name);
                // Fall through to generation.
            }
        }
    }

    // Generate via the appropriate adapter.
    let result = match asset_ref.kind.as_str() {
        "mesh" => {
            let adapter = TripoMeshAdapter::new(cache.clone());
            adapter.generate(&request).await
        }
        "texture" => {
            let adapter = TextureGenerationAdapter::new(cache.clone());
            adapter.generate(&request).await
        }
        other => Err(format!("unsupported asset kind: {other}")),
    };

    match result {
        Ok(gen_result) => {
            let mesh_path = gen_result
                .artifacts
                .iter()
                .find_map(|a| a.local_path.as_ref())
                .map(|p| make_relative_path(p, cache_root));

            AssetResolution {
                entity_index,
                mesh_path,
                cached: false,
                generated: true,
                failed: false,
                error: None,
            }
        }
        Err(e) => AssetResolution {
            entity_index,
            mesh_path: None,
            cached: false,
            generated: false,
            failed: true,
            error: Some(e),
        },
    }
}

/// Constructs a path relative to the cache root for use in the scene IR.
fn make_relative_path(absolute: &std::path::Path, cache_root: &std::path::Path) -> String {
    match absolute.strip_prefix(cache_root) {
        Ok(relative) => {
            let cache_dir_name = cache_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("asset-factory-cache-v1");
            format!(".wrela/{}/{}", cache_dir_name, relative.display())
        }
        Err(_) => absolute.display().to_string(),
    }
}

/// Computes the output path for the resolved scene IR.
/// Input: `foo.json` -> `foo.resolved.json`
/// Input: `foo` -> `foo.resolved.json`
fn resolved_output_path(input: &std::path::Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("scene-ir");
    let parent = input.parent().unwrap_or(std::path::Path::new("."));
    parent.join(format!("{stem}.resolved.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_factory::{
        AssetAdapterKind, AssetArtifactCache, AssetGenerationArtifact, AssetGenerationResult,
        CanonicalJobEnvelope,
    };
    use crate::scene_ir::{AssetRef, SceneCamera, SceneEntity, SceneIR, SceneLighting};
    use std::collections::BTreeMap;

    fn make_test_scene_ir() -> SceneIR {
        SceneIR {
            scene: "test_scene".to_string(),
            entities: vec![
                SceneEntity {
                    name: "sword".to_string(),
                    mesh_asset: "crystal_sword".to_string(),
                    mesh_path: None,
                    position: [0, 0, 0],
                    rotation: [0, 0, 0],
                    scale: [1000, 1000, 1000],
                    asset_ref: AssetRef {
                        name: "crystal_sword".to_string(),
                        kind: "mesh".to_string(),
                        prompt: "a crystal sword".to_string(),
                        style: Some("fantasy".to_string()),
                        negative: Some("blurry".to_string()),
                        lod_budget: Some(5000),
                    },
                },
                SceneEntity {
                    name: "shield".to_string(),
                    mesh_asset: "oak_shield".to_string(),
                    mesh_path: None,
                    position: [1000, 0, 0],
                    rotation: [0, 0, 0],
                    scale: [1000, 1000, 1000],
                    asset_ref: AssetRef {
                        name: "oak_shield".to_string(),
                        kind: "texture".to_string(),
                        prompt: "wooden oak shield texture".to_string(),
                        style: None,
                        negative: None,
                        lod_budget: None,
                    },
                },
            ],
            lighting: SceneLighting {
                sun_direction: [0, -1000, 0],
                sun_color: [255, 255, 255],
                sun_intensity: 1000,
                ambient_color: [50, 50, 50],
                ambient_intensity: 500,
            },
            camera: SceneCamera {
                mode: "orbit".to_string(),
                target: "sword".to_string(),
                distance: 5000,
                pitch: -300,
            },
        }
    }

    fn write_scene_ir_to_file(scene_ir: &SceneIR, dir: &std::path::Path) -> PathBuf {
        let path = dir.join("scene-ir.json");
        let json = scene_ir.to_json().unwrap();
        std::fs::write(&path, &json).unwrap();
        path
    }

    /// Pre-populates the cache for an asset ref so it appears as a cache hit.
    fn prepopulate_cache(
        cache: &AssetArtifactCache,
        cache_root: &std::path::Path,
        asset_ref: &AssetRef,
    ) -> PathBuf {
        let request = request_from_asset_ref(asset_ref);
        let adapter_kind = adapter_kind_for_ref(asset_ref);
        let provider = provider_for_kind(asset_ref.kind.as_str());
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

        // Write a dummy file.
        let filename = format!("{}.glb", &hash[..16]);
        let dummy_bytes = b"dummy-asset-data";
        let local_path = cache
            .put_bytes(&envelope, &filename, dummy_bytes)
            .expect("put_bytes");

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
        cache.put(&result).expect("cache put");
        local_path
    }

    #[tokio::test]
    async fn test_resolve_all_cached() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_root = temp.path().to_path_buf();
        let cache_root = cache_root_for_workspace(&workspace_root);
        let cache = AssetArtifactCache::new(&cache_root);

        let scene_ir = make_test_scene_ir();

        // Pre-populate cache for all entities.
        for entity in &scene_ir.entities {
            prepopulate_cache(&cache, &cache_root, &entity.asset_ref);
        }

        let scene_path = write_scene_ir_to_file(&scene_ir, temp.path());

        let config = ResolveConfig {
            scene_ir_path: scene_path,
            workspace_root,
            dry_run: false,
            force: false,
            parallel: 4,
        };

        let result = resolve(config).await.expect("resolve should succeed");
        assert_eq!(result.total_assets, 2);
        assert_eq!(result.cached, 2);
        assert_eq!(result.generated, 0);
        assert_eq!(result.failed, 0);

        // Verify mesh_path was populated.
        for entity in &result.resolved_scene_ir.entities {
            assert!(
                entity.mesh_path.is_some(),
                "entity '{}' should have mesh_path set",
                entity.name
            );
        }
    }

    #[tokio::test]
    async fn test_resolve_dry_run() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_root = temp.path().to_path_buf();
        let cache_root = cache_root_for_workspace(&workspace_root);
        let cache = AssetArtifactCache::new(&cache_root);

        let scene_ir = make_test_scene_ir();

        // Pre-populate cache for first entity only.
        prepopulate_cache(&cache, &cache_root, &scene_ir.entities[0].asset_ref);

        let scene_path = write_scene_ir_to_file(&scene_ir, temp.path());

        let config = ResolveConfig {
            scene_ir_path: scene_path.clone(),
            workspace_root,
            dry_run: true,
            force: false,
            parallel: 4,
        };

        let result = resolve(config).await.expect("resolve should succeed");
        assert_eq!(result.total_assets, 2);
        assert_eq!(result.cached, 1);
        assert_eq!(result.generated, 0);
        assert_eq!(result.failed, 0);

        // Dry run should NOT produce a resolved output file.
        let output = resolved_output_path(&scene_path);
        assert!(
            !output.exists(),
            "dry-run should not write resolved scene IR"
        );

        // mesh_path should remain None in dry_run.
        for entity in &result.resolved_scene_ir.entities {
            assert!(
                entity.mesh_path.is_none(),
                "entity '{}' should still have mesh_path = None in dry-run",
                entity.name
            );
        }
    }

    #[tokio::test]
    async fn test_resolve_partial_failure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_root = temp.path().to_path_buf();
        let cache_root = cache_root_for_workspace(&workspace_root);
        let cache = AssetArtifactCache::new(&cache_root);

        // Create scene with a mesh entity (will fail - no API key)
        // and a texture entity (will succeed - procedural).
        let scene_ir = SceneIR {
            scene: "partial_test".to_string(),
            entities: vec![
                SceneEntity {
                    name: "mesh_item".to_string(),
                    mesh_asset: "api_mesh".to_string(),
                    mesh_path: None,
                    position: [0, 0, 0],
                    rotation: [0, 0, 0],
                    scale: [1000, 1000, 1000],
                    asset_ref: AssetRef {
                        name: "api_mesh".to_string(),
                        kind: "mesh".to_string(),
                        prompt: "a test mesh".to_string(),
                        style: None,
                        negative: None,
                        lod_budget: None,
                    },
                },
                SceneEntity {
                    name: "tex_item".to_string(),
                    mesh_asset: "procedural_tex".to_string(),
                    mesh_path: None,
                    position: [1000, 0, 0],
                    rotation: [0, 0, 0],
                    scale: [1000, 1000, 1000],
                    asset_ref: AssetRef {
                        name: "procedural_tex".to_string(),
                        kind: "texture".to_string(),
                        prompt: "stone wall".to_string(),
                        style: None,
                        negative: None,
                        lod_budget: None,
                    },
                },
            ],
            lighting: SceneLighting {
                sun_direction: [0, -1000, 0],
                sun_color: [255, 255, 255],
                sun_intensity: 1000,
                ambient_color: [50, 50, 50],
                ambient_intensity: 500,
            },
            camera: SceneCamera {
                mode: "orbit".to_string(),
                target: "mesh_item".to_string(),
                distance: 5000,
                pitch: 0,
            },
        };

        let scene_path = write_scene_ir_to_file(&scene_ir, temp.path());

        let config = ResolveConfig {
            scene_ir_path: scene_path,
            workspace_root,
            dry_run: false,
            force: false,
            parallel: 2,
        };

        let result = resolve(config).await.expect("resolve should succeed");
        assert_eq!(result.total_assets, 2);
        // Mesh should fail (no TRIPO_API_KEY), texture should succeed.
        assert_eq!(result.failed, 1, "mesh asset should fail without API key");
        assert_eq!(
            result.generated, 1,
            "texture asset should generate successfully"
        );

        // The texture entity should have a mesh_path.
        assert!(
            result.resolved_scene_ir.entities[1].mesh_path.is_some(),
            "texture entity should have mesh_path populated"
        );
    }

    #[tokio::test]
    async fn test_resolve_force_regenerates() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_root = temp.path().to_path_buf();
        let cache_root = cache_root_for_workspace(&workspace_root);
        let cache = AssetArtifactCache::new(&cache_root);

        // Scene with only a texture entity (procedural - no API key needed).
        let scene_ir = SceneIR {
            scene: "force_test".to_string(),
            entities: vec![SceneEntity {
                name: "tex_item".to_string(),
                mesh_asset: "force_tex".to_string(),
                mesh_path: None,
                position: [0, 0, 0],
                rotation: [0, 0, 0],
                scale: [1000, 1000, 1000],
                asset_ref: AssetRef {
                    name: "force_tex".to_string(),
                    kind: "texture".to_string(),
                    prompt: "red brick wall".to_string(),
                    style: None,
                    negative: None,
                    lod_budget: None,
                },
            }],
            lighting: SceneLighting {
                sun_direction: [0, -1000, 0],
                sun_color: [255, 255, 255],
                sun_intensity: 1000,
                ambient_color: [50, 50, 50],
                ambient_intensity: 500,
            },
            camera: SceneCamera {
                mode: "orbit".to_string(),
                target: "tex_item".to_string(),
                distance: 5000,
                pitch: 0,
            },
        };

        // Pre-populate cache.
        prepopulate_cache(&cache, &cache_root, &scene_ir.entities[0].asset_ref);

        let scene_path = write_scene_ir_to_file(&scene_ir, temp.path());

        // First resolve without force: should use cache.
        let config_cached = ResolveConfig {
            scene_ir_path: scene_path.clone(),
            workspace_root: workspace_root.clone(),
            dry_run: false,
            force: false,
            parallel: 1,
        };
        let result_cached = resolve(config_cached).await.expect("resolve cached");
        assert_eq!(result_cached.cached, 1);
        assert_eq!(result_cached.generated, 0);

        // Now resolve with force: should regenerate (bypass cache).
        // Re-write scene IR since resolved overwrites.
        write_scene_ir_to_file(&scene_ir, temp.path());

        let config_force = ResolveConfig {
            scene_ir_path: scene_path,
            workspace_root,
            dry_run: false,
            force: true,
            parallel: 1,
        };
        let result_force = resolve(config_force).await.expect("resolve force");
        assert_eq!(result_force.cached, 0, "force should bypass cache");
        assert_eq!(result_force.generated, 1, "force should regenerate");
    }

    #[tokio::test]
    async fn test_resolve_writes_resolved_scene_ir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_root = temp.path().to_path_buf();
        let cache_root = cache_root_for_workspace(&workspace_root);
        let cache = AssetArtifactCache::new(&cache_root);

        let scene_ir = SceneIR {
            scene: "write_test".to_string(),
            entities: vec![SceneEntity {
                name: "item".to_string(),
                mesh_asset: "write_tex".to_string(),
                mesh_path: None,
                position: [0, 0, 0],
                rotation: [0, 0, 0],
                scale: [1000, 1000, 1000],
                asset_ref: AssetRef {
                    name: "write_tex".to_string(),
                    kind: "texture".to_string(),
                    prompt: "blue sky".to_string(),
                    style: None,
                    negative: None,
                    lod_budget: None,
                },
            }],
            lighting: SceneLighting {
                sun_direction: [0, -1000, 0],
                sun_color: [255, 255, 255],
                sun_intensity: 1000,
                ambient_color: [50, 50, 50],
                ambient_intensity: 500,
            },
            camera: SceneCamera {
                mode: "orbit".to_string(),
                target: "item".to_string(),
                distance: 5000,
                pitch: 0,
            },
        };

        let scene_path = write_scene_ir_to_file(&scene_ir, temp.path());
        let expected_output = resolved_output_path(&scene_path);

        let config = ResolveConfig {
            scene_ir_path: scene_path,
            workspace_root,
            dry_run: false,
            force: false,
            parallel: 1,
        };

        let _result = resolve(config).await.expect("resolve should succeed");

        // Verify output file exists and is valid JSON.
        assert!(
            expected_output.exists(),
            "resolved scene IR file should exist"
        );

        let output_json = std::fs::read_to_string(&expected_output).expect("read output");
        let output_ir = SceneIR::from_json(&output_json).expect("parse output JSON");

        // The texture entity should have mesh_path populated.
        assert!(
            output_ir.entities[0].mesh_path.is_some(),
            "resolved entity should have mesh_path set"
        );

        let mesh_path = output_ir.entities[0].mesh_path.as_ref().unwrap();
        assert!(
            mesh_path.contains(".wrela/"),
            "mesh_path should be relative to workspace: {mesh_path}"
        );
    }

    #[test]
    fn test_resolved_output_path() {
        let input = PathBuf::from("/tmp/my-scene.json");
        let output = resolved_output_path(&input);
        assert_eq!(output, PathBuf::from("/tmp/my-scene.resolved.json"));
    }

    #[test]
    fn test_resolved_output_path_no_extension() {
        let input = PathBuf::from("/tmp/scene");
        let output = resolved_output_path(&input);
        assert_eq!(output, PathBuf::from("/tmp/scene.resolved.json"));
    }

    #[test]
    fn test_cache_key_deterministic() {
        let asset_ref = AssetRef {
            name: "test_asset".to_string(),
            kind: "mesh".to_string(),
            prompt: "a test prompt".to_string(),
            style: Some("hero".to_string()),
            negative: Some("blur".to_string()),
            lod_budget: None,
        };
        let key1 = cache_key_for_asset_ref(&asset_ref);
        let key2 = cache_key_for_asset_ref(&asset_ref);
        assert_eq!(key1, key2, "cache key must be deterministic");
        assert!(!key1.is_empty());
    }

    #[test]
    fn test_request_from_asset_ref() {
        let asset_ref = AssetRef {
            name: "my_asset".to_string(),
            kind: "mesh".to_string(),
            prompt: "a crystal sword".to_string(),
            style: Some("fantasy".to_string()),
            negative: Some("blurry".to_string()),
            lod_budget: Some(5000),
        };
        let request = request_from_asset_ref(&asset_ref);
        assert_eq!(request.asset_id, "my_asset");
        assert_eq!(request.prompt, "a crystal sword");
        assert_eq!(request.style_profile, "fantasy");
        assert_eq!(request.negative_constraints, vec!["blurry".to_string()]);
    }

    #[test]
    fn test_adapter_kind_mapping() {
        let mesh_ref = AssetRef {
            name: "x".to_string(),
            kind: "mesh".to_string(),
            prompt: "".to_string(),
            style: None,
            negative: None,
            lod_budget: None,
        };
        assert_eq!(adapter_kind_for_ref(&mesh_ref), AssetAdapterKind::Mesh3d);

        let tex_ref = AssetRef {
            name: "x".to_string(),
            kind: "texture".to_string(),
            prompt: "".to_string(),
            style: None,
            negative: None,
            lod_budget: None,
        };
        assert_eq!(adapter_kind_for_ref(&tex_ref), AssetAdapterKind::Image);
    }
}
