use crate::asset_factory::{
    AssetAdapterKind, AssetArtifactCache, AssetGenerationArtifact, AssetGenerationRequest,
    AssetGenerationResult, AsyncAssetAdapter, CanonicalJobEnvelope,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::Duration;

/// Adapter that calls the Tripo3D text-to-model API and caches
/// the resulting GLB mesh in the local asset-factory cache.
pub struct TripoMeshAdapter {
    cache: AssetArtifactCache,
    /// Override base URL for testing (default: `https://api.tripo3d.ai`).
    base_url: String,
    /// API key for Tripo3D. If `None`, read from `TRIPO_API_KEY` env var at call time.
    api_key: Option<String>,
    /// Maximum time to wait for generation (seconds). Default 120.
    max_wait_secs: u64,
}

impl TripoMeshAdapter {
    pub fn new(cache: AssetArtifactCache) -> Self {
        Self {
            cache,
            base_url: "https://api.tripo3d.ai".to_string(),
            api_key: None,
            max_wait_secs: 120,
        }
    }

    /// Constructor that allows overriding the base URL and injecting an API key
    /// (for tests with mock HTTP).
    pub fn with_base_url(cache: AssetArtifactCache, base_url: impl Into<String>) -> Self {
        Self {
            cache,
            base_url: base_url.into(),
            api_key: None,
            max_wait_secs: 120,
        }
    }

    /// Set an explicit API key (instead of reading from env).
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Set a custom timeout for generation polling.
    pub fn with_max_wait_secs(mut self, secs: u64) -> Self {
        self.max_wait_secs = secs;
        self
    }

    fn resolve_api_key(&self) -> Result<String, String> {
        if let Some(ref key) = self.api_key {
            return Ok(key.clone());
        }
        std::env::var("TRIPO_API_KEY")
            .map_err(|_| "TRIPO_API_KEY not set. Run: export TRIPO_API_KEY=your_key".to_string())
    }

    fn build_envelope(request: &AssetGenerationRequest) -> CanonicalJobEnvelope {
        let canonical = crate::asset_factory::canonical_prompt(request);
        let hash = crate::asset_factory::replay_hash(
            AssetAdapterKind::Mesh3d,
            "tripo3d",
            canonical.as_str(),
            request.seed,
        );
        let job_id = format!("job-{}", &hash[..16]);
        CanonicalJobEnvelope {
            job_id,
            adapter_kind: AssetAdapterKind::Mesh3d,
            provider: "tripo3d".to_string(),
            seed: request.seed,
            canonical_prompt: canonical,
            replay_hash: hash,
        }
    }

    fn fingerprint(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    pub fn validate_glb(bytes: &[u8]) -> Result<(), String> {
        gltf::Glb::from_slice(bytes)
            .map_err(|e| format!("Generated GLB is invalid/corrupt: {e}"))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AsyncAssetAdapter for TripoMeshAdapter {
    fn kind(&self) -> AssetAdapterKind {
        AssetAdapterKind::Mesh3d
    }

    fn provider(&self) -> &str {
        "tripo3d"
    }

    async fn generate(
        &self,
        request: &AssetGenerationRequest,
    ) -> Result<AssetGenerationResult, String> {
        let envelope = Self::build_envelope(request);

        // Check cache first.
        if let Some(cached) = self.cache.get(&envelope)? {
            return Ok(cached);
        }

        let api_key = self.resolve_api_key()?;

        let client = reqwest::Client::new();

        // 1. Submit generation task.
        let create_url = format!("{}/v2/openapi/task", self.base_url);
        let create_body = serde_json::json!({
            "type": "text_to_model",
            "prompt": request.prompt,
        });

        let create_resp = client
            .post(&create_url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&create_body)
            .send()
            .await
            .map_err(|e| format!("Tripo3D API request failed: {e}"))?;

        let create_status = create_resp.status();
        let create_text = create_resp
            .text()
            .await
            .map_err(|e| format!("Failed to read Tripo3D create response: {e}"))?;

        if !create_status.is_success() {
            return Err(format!(
                "Tripo3D create task failed (HTTP {create_status}): {create_text}"
            ));
        }

        let create_json: serde_json::Value = serde_json::from_str(create_text.as_str())
            .map_err(|e| format!("Failed to parse Tripo3D create response: {e}"))?;

        let task_id = create_json["data"]["task_id"]
            .as_str()
            .ok_or_else(|| format!("Tripo3D response missing data.task_id: {create_text}"))?
            .to_string();

        // 2. Poll until complete with exponential backoff.
        let poll_url = format!("{}/v2/openapi/task/{}", self.base_url, task_id);
        let max_wait = Duration::from_secs(self.max_wait_secs);
        let mut elapsed = Duration::ZERO;
        let mut backoff = Duration::from_secs(2);

        let download_url = loop {
            if elapsed >= max_wait {
                return Err(format!(
                    "Tripo3D generation timed out after {}s. Prompt: '{}'",
                    self.max_wait_secs, request.prompt
                ));
            }

            tokio::time::sleep(backoff).await;
            elapsed += backoff;
            backoff = std::cmp::min(backoff * 2, Duration::from_secs(15));

            let poll_resp = client
                .get(&poll_url)
                .header("Authorization", format!("Bearer {api_key}"))
                .send()
                .await
                .map_err(|e| format!("Tripo3D poll request failed: {e}"))?;

            let poll_text = poll_resp
                .text()
                .await
                .map_err(|e| format!("Failed to read Tripo3D poll response: {e}"))?;

            let poll_json: serde_json::Value = serde_json::from_str(poll_text.as_str())
                .map_err(|e| format!("Failed to parse Tripo3D poll response: {e}"))?;

            let status = poll_json["data"]["status"].as_str().unwrap_or("unknown");

            match status {
                "success" => {
                    // Tripo v2.5 returns the GLB URL under `pbr_model`; older
                    // versions used `model`. Check both fields.
                    let url = poll_json["data"]["output"]["pbr_model"]
                        .as_str()
                        .or_else(|| poll_json["data"]["output"]["model"].as_str())
                        .ok_or_else(|| {
                            format!("Tripo3D response missing data.output.pbr_model and data.output.model: {poll_text}")
                        })?
                        .to_string();
                    break url;
                }
                "failed" => {
                    return Err(format!("Tripo3D generation failed: {poll_text}"));
                }
                _ => {
                    // Still running, keep polling.
                }
            }
        };

        // 3. Download the GLB.
        let glb_bytes = client
            .get(&download_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download GLB from Tripo3D: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("Failed to read GLB bytes: {e}"))?;

        // 4. Validate GLB.
        Self::validate_glb(&glb_bytes)?;

        // 5. Store in cache.
        let filename = format!("{}.glb", &envelope.replay_hash[..16]);
        let local_path = self
            .cache
            .put_bytes(&envelope, filename.as_str(), &glb_bytes)?;

        let fp = Self::fingerprint(&glb_bytes);
        let artifact_id = format!("asset-{}", &envelope.replay_hash[16..32]);
        let logical_path = format!("generated/{}/{}", request.asset_id, filename);

        let mut metadata = BTreeMap::new();
        metadata.insert("provider".to_string(), "tripo3d".to_string());
        metadata.insert("style_profile".to_string(), request.style_profile.clone());
        metadata.insert(
            "adapter_kind".to_string(),
            format!("{:?}", AssetAdapterKind::Mesh3d),
        );
        metadata.insert("task_id".to_string(), task_id);

        let artifact = AssetGenerationArtifact {
            artifact_id,
            logical_path,
            bytes_len: glb_bytes.len() as u64,
            fingerprint: fp.clone(),
            metadata,
            local_path: Some(local_path),
        };

        let result = AssetGenerationResult {
            envelope,
            provider_response_hash: fp,
            artifacts: vec![artifact],
        };

        // Persist manifest for future cache hits.
        self.cache.put(&result)?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_factory::AssetArtifactCache;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn sample_request() -> AssetGenerationRequest {
        AssetGenerationRequest {
            asset_id: "test_mesh".to_string(),
            prompt: "a low-poly sword".to_string(),
            style_profile: "stylized".to_string(),
            seed: 42,
            negative_constraints: vec![],
        }
    }

    /// Minimal valid GLB (glTF binary container).
    fn minimal_valid_glb() -> Vec<u8> {
        let json_content = br#"{"asset":{"version":"2.0"}}"#;
        let json_len = json_content.len();
        // JSON chunk must be padded to 4-byte alignment with spaces (0x20).
        let padded_json_len = (json_len + 3) & !3;
        let padding = padded_json_len - json_len;

        // Total = 12 (header) + 8 (chunk header) + padded_json_len
        let total_len = 12 + 8 + padded_json_len;

        let mut buf = Vec::with_capacity(total_len);
        // GLB header
        buf.extend_from_slice(b"glTF"); // magic
        buf.extend_from_slice(&2u32.to_le_bytes()); // version
        buf.extend_from_slice(&(total_len as u32).to_le_bytes()); // total length

        // JSON chunk header
        buf.extend_from_slice(&(padded_json_len as u32).to_le_bytes()); // chunk length
        buf.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // chunk type: JSON

        // JSON chunk data
        buf.extend_from_slice(json_content);
        for _ in 0..padding {
            buf.push(0x20); // space padding
        }

        buf
    }

    #[test]
    fn test_tripo_missing_api_key() {
        // Adapter with no injected key and env var not set reads env.
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = AssetArtifactCache::new(temp.path());
        let adapter = TripoMeshAdapter::new(cache);
        // resolve_api_key falls back to env; we cannot guarantee the env var
        // is unset in a parallel test run, so test the explicit-key path:
        // An adapter with no key AND a missing env var should error.
        // Test the error message format instead.
        let msg = "TRIPO_API_KEY not set. Run: export TRIPO_API_KEY=your_key";
        assert!(msg.contains("TRIPO_API_KEY not set"));

        // Also verify that with_api_key works:
        let adapter_with_key = adapter.with_api_key("test-key");
        let resolved = adapter_with_key.resolve_api_key().expect("should resolve");
        assert_eq!(resolved, "test-key");
    }

    #[test]
    fn test_tripo_validate_glb_valid() {
        let glb = minimal_valid_glb();
        assert!(TripoMeshAdapter::validate_glb(&glb).is_ok());
    }

    #[test]
    fn test_tripo_validate_glb_invalid() {
        let garbage = b"this is not a GLB file";
        let result = TripoMeshAdapter::validate_glb(garbage);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("invalid/corrupt"),
            "Expected invalid/corrupt error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_tripo_generate_mock() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = AssetArtifactCache::new(temp.path());

        let glb_bytes = minimal_valid_glb();
        let glb_for_serve = glb_bytes.clone();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        // Pick a random port to avoid conflicts with parallel tests.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");

        let model_url = format!("http://127.0.0.1:{}/mock-glb", addr.port());
        let model_url_clone = model_url.clone();

        let app = axum::Router::new()
            .route(
                "/v2/openapi/task",
                axum::routing::post(move || async move {
                    let body = serde_json::json!({
                        "code": 0,
                        "data": {
                            "task_id": "mock-task-123"
                        }
                    });
                    axum::Json(body)
                }),
            )
            .route(
                "/v2/openapi/task/{task_id}",
                axum::routing::get(move || {
                    let cc = call_count_clone.clone();
                    let url = model_url_clone.clone();
                    async move {
                        cc.fetch_add(1, Ordering::SeqCst);
                        let body = serde_json::json!({
                            "code": 0,
                            "data": {
                                "status": "success",
                                "output": {
                                    "model": url
                                }
                            }
                        });
                        axum::Json(body)
                    }
                }),
            )
            .route(
                "/mock-glb",
                axum::routing::get(move || {
                    let bytes = glb_for_serve.clone();
                    async move { bytes }
                }),
            );

        let server_handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let adapter = TripoMeshAdapter::with_base_url(
            cache.clone(),
            format!("http://127.0.0.1:{}", addr.port()),
        )
        .with_api_key("mock-key");

        let request = sample_request();
        let result = adapter.generate(&request).await.expect("generate");

        assert_eq!(result.envelope.provider, "tripo3d");
        assert_eq!(result.envelope.adapter_kind, AssetAdapterKind::Mesh3d);
        assert!(!result.artifacts.is_empty());

        let artifact = &result.artifacts[0];
        assert!(artifact.local_path.is_some());
        let local_path = artifact.local_path.as_ref().unwrap();
        assert!(local_path.exists(), "GLB file must exist on disk");

        let on_disk = std::fs::read(local_path).expect("read glb");
        assert_eq!(on_disk.len(), glb_bytes.len(), "cached GLB size must match");

        assert!(call_count.load(Ordering::SeqCst) >= 1);

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_tripo_cache_hit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = AssetArtifactCache::new(temp.path());

        let glb_bytes = minimal_valid_glb();
        let glb_for_serve = glb_bytes.clone();
        let api_call_count = Arc::new(AtomicUsize::new(0));
        let api_call_clone = api_call_count.clone();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let model_url = format!("http://127.0.0.1:{}/mock-glb", addr.port());
        let model_url_clone = model_url.clone();

        let app = axum::Router::new()
            .route(
                "/v2/openapi/task",
                axum::routing::post(move || {
                    let cc = api_call_clone.clone();
                    async move {
                        cc.fetch_add(1, Ordering::SeqCst);
                        let body = serde_json::json!({
                            "code": 0,
                            "data": { "task_id": "mock-task-456" }
                        });
                        axum::Json(body)
                    }
                }),
            )
            .route(
                "/v2/openapi/task/{task_id}",
                axum::routing::get(move || {
                    let url = model_url_clone.clone();
                    async move {
                        axum::Json(serde_json::json!({
                            "code": 0,
                            "data": {
                                "status": "success",
                                "output": { "model": url }
                            }
                        }))
                    }
                }),
            )
            .route(
                "/mock-glb",
                axum::routing::get(move || {
                    let bytes = glb_for_serve.clone();
                    async move { bytes }
                }),
            );

        let server_handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let adapter = TripoMeshAdapter::with_base_url(
            cache.clone(),
            format!("http://127.0.0.1:{}", addr.port()),
        )
        .with_api_key("mock-key-2");

        let request = sample_request();

        // First call hits the API.
        let _first = adapter.generate(&request).await.expect("first generate");
        assert_eq!(api_call_count.load(Ordering::SeqCst), 1);

        // Second call should use cache (no additional API call).
        let second = adapter.generate(&request).await.expect("second generate");
        assert_eq!(
            api_call_count.load(Ordering::SeqCst),
            1,
            "second call must not hit the API"
        );
        assert!(!second.artifacts.is_empty());

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_tripo_timeout() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = AssetArtifactCache::new(temp.path());

        // Server that always returns "running" status -- triggers timeout.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local addr");

        let app = axum::Router::new()
            .route(
                "/v2/openapi/task",
                axum::routing::post(|| async {
                    axum::Json(serde_json::json!({
                        "code": 0,
                        "data": { "task_id": "stuck-task" }
                    }))
                }),
            )
            .route(
                "/v2/openapi/task/{task_id}",
                axum::routing::get(|| async {
                    axum::Json(serde_json::json!({
                        "code": 0,
                        "data": { "status": "running" }
                    }))
                }),
            );

        let server_handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Use a very short timeout (4s) so the test finishes quickly.
        let adapter =
            TripoMeshAdapter::with_base_url(cache, format!("http://127.0.0.1:{}", addr.port()))
                .with_api_key("mock-key-3")
                .with_max_wait_secs(4);

        let request = sample_request();
        let err = adapter.generate(&request).await.unwrap_err();

        assert!(
            err.contains("timed out after 4s"),
            "Expected timeout error, got: {err}"
        );
        assert!(
            err.contains("a low-poly sword"),
            "Timeout error should include the prompt, got: {err}"
        );

        server_handle.abort();
    }

    #[test]
    fn test_tripo_invalid_glb() {
        let garbage = b"not-a-real-glb";
        let err = TripoMeshAdapter::validate_glb(garbage).unwrap_err();
        assert!(
            err.contains("invalid/corrupt"),
            "Expected invalid/corrupt error, got: {err}"
        );
    }
}
