use crate::parser::ast::AstNode;
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use notify::{Event, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tower_http::services::ServeDir;

pub struct PreviewServerConfig {
    pub app_path: PathBuf,
    pub port: u16,
    pub open_browser: bool,
    pub client_dist_path: PathBuf,
    pub asset_cache_path: PathBuf,
}

struct AppState {
    scene_ir_json: RwLock<String>,
    tx: broadcast::Sender<String>,
}

pub async fn run_preview_server(config: PreviewServerConfig) -> Result<(), String> {
    let scene_ir = compile_app(&config.app_path)?;
    let scene_ir_json = serde_json::to_string_pretty(&scene_ir)
        .map_err(|e| format!("Failed to serialize Scene IR: {e}"))?;

    let (tx, _) = broadcast::channel::<String>(16);

    let state = Arc::new(AppState {
        scene_ir_json: RwLock::new(scene_ir_json),
        tx: tx.clone(),
    });

    let watcher_state = state.clone();
    let watcher_app_path = config.app_path.clone();
    tokio::spawn(async move {
        watch_files(watcher_app_path, watcher_state).await;
    });

    let app = Router::new()
        .route("/scene.json", get(scene_json_handler))
        .route("/preview", get(ws_handler))
        .route("/health", get(health_handler))
        .nest_service("/assets", ServeDir::new(&config.asset_cache_path))
        .fallback_service(ServeDir::new(&config.client_dist_path))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", config.port);
    println!("Preview server running at http://localhost:{}", config.port);

    if config.open_browser {
        let _ = open_browser(&format!("http://localhost:{}?mode=preview", config.port));
    }

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind to {addr}: {e}"))?;
    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {e}"))?;

    Ok(())
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn scene_json_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let json = state.scene_ir_json.read().await;
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        json.clone(),
    )
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    let initial = state.scene_ir_json.read().await.clone();
    let msg = serde_json::json!({
        "type": "scene_update",
        "scene": serde_json::from_str::<serde_json::Value>(&initial).unwrap_or_default()
    });
    let _ = socket.send(Message::Text(msg.to_string().into())).await;

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
                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
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
}

async fn watch_files(app_path: PathBuf, state: Arc<AppState>) {
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel(32);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if event.kind.is_modify() || event.kind.is_create() {
                for path in &event.paths {
                    if path.extension().is_some_and(|ext| ext == "wr") {
                        let _ = notify_tx.blocking_send(path.clone());
                    }
                }
            }
        }
    })
    .expect("Failed to create file watcher");

    watcher
        .watch(&app_path, RecursiveMode::Recursive)
        .expect("Failed to watch app directory");

    // Keep watcher alive by holding it in scope
    let _watcher = watcher;

    loop {
        if notify_rx.recv().await.is_none() {
            break;
        }
        // Debounce: wait 100ms after last change before recompiling
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        while notify_rx.try_recv().is_ok() {}

        match compile_app(&app_path) {
            Ok(scene_ir) => match serde_json::to_string_pretty(&scene_ir) {
                Ok(json) => {
                    *state.scene_ir_json.write().await = json.clone();
                    let _ = state.tx.send(json);
                    println!("Scene recompiled successfully.");
                }
                Err(e) => eprintln!("Failed to serialize Scene IR: {e}"),
            },
            Err(e) => {
                eprintln!("Compilation error: {e}");
                let status = serde_json::json!({
                    "type": "status",
                    "message": format!("Compilation error: {e}")
                });
                let _ = state.tx.send(status.to_string());
            }
        }
    }
}

fn compile_app(app_path: &Path) -> Result<serde_json::Value, String> {
    // Collect all .wr files from the app directory
    let wr_files = collect_wr_files(app_path);
    if wr_files.is_empty() {
        return Ok(default_scene_ir());
    }

    // Concatenate all .wr source
    let mut source = String::new();
    for file in &wr_files {
        let content = std::fs::read_to_string(file)
            .map_err(|e| format!("Failed to read {}: {e}", file.display()))?;
        source.push_str(&content);
        source.push('\n');
    }

    // Parse → HIR → Scene IR
    let (node, errors) = crate::parser::parse_with_errors(&source);
    if !errors.is_empty() {
        return Err(format!("Parse errors: {:?}", errors));
    }

    let root = match crate::parser::ast::Root::cast(node) {
        Some(r) => r,
        None => return Ok(default_scene_ir()),
    };

    let module = crate::hir::lower::lower(root);

    if module.scene_declarations.is_empty() {
        return Ok(default_scene_ir());
    }

    // Use the first scene declaration
    let scene = &module.scene_declarations[0];
    let ir = crate::scene_ir::SceneIR::from_hir(scene, &module.asset_declarations);
    serde_json::to_value(&ir).map_err(|e| format!("Failed to serialize Scene IR: {e}"))
}

fn collect_wr_files(path: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if path.is_file() {
        if path.extension().is_some_and(|ext| ext == "wr") {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                files.extend(collect_wr_files(&entry.path()));
            }
        }
    }
    files.sort();
    files
}

fn default_scene_ir() -> serde_json::Value {
    serde_json::json!({
        "scene": "default",
        "entities": [],
        "lighting": {
            "sun_direction": [500, -800, 300],
            "sun_color": [255, 240, 220],
            "sun_intensity": 2000,
            "ambient_color": [40, 35, 50],
            "ambient_intensity": 500
        },
        "camera": {
            "mode": "orbit",
            "target": "",
            "distance": 5000,
            "pitch": -300
        }
    })
}

fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_app_returns_default_for_empty_dir() {
        let tmp = std::env::temp_dir().join("wrela_compile_app_test_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let result = compile_app(&tmp);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["scene"], "default");
        assert!(json["entities"].as_array().unwrap().is_empty());
        assert_eq!(json["lighting"]["sun_intensity"], 2000);
        assert_eq!(json["camera"]["mode"], "orbit");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_compile_app_parses_wr_file() {
        let tmp = std::env::temp_dir().join("wrela_compile_app_test_wr");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("test.wr"),
            "asset sword {\n    kind: mesh\n    prompt: \"a sword\"\n}\n\nscene demo {\n    entity s {\n        mesh: sword\n        position: vec3(0, 0, 0)\n        rotation: vec3(0, 0, 0)\n        scale: vec3(1000, 1000, 1000)\n    }\n}\n",
        ).unwrap();
        let result = compile_app(&tmp);
        assert!(result.is_ok(), "compile_app failed: {:?}", result.err());
        let json = result.unwrap();
        assert_eq!(json["scene"], "demo");
        assert_eq!(json["entities"].as_array().unwrap().len(), 1);
        assert_eq!(json["entities"][0]["name"], "s");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let response = health_handler().await;
        assert_eq!(response, "ok");
    }

    #[tokio::test]
    async fn test_scene_json_endpoint() {
        let scene_ir = default_scene_ir();
        let scene_json = serde_json::to_string_pretty(&scene_ir).unwrap();
        let (tx, _) = broadcast::channel::<String>(16);
        let state = Arc::new(AppState {
            scene_ir_json: RwLock::new(scene_json.clone()),
            tx,
        });
        let (parts, body) = scene_json_handler(State(state))
            .await
            .into_response()
            .into_parts();
        assert_eq!(parts.status, StatusCode::OK);
        // Verify we can parse the body as JSON
        let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(parsed["scene"], "default");
    }

    #[tokio::test]
    async fn test_server_starts_and_serves_health() {
        let tmp = std::env::temp_dir().join("wrela_preview_test");
        let _ = std::fs::create_dir_all(&tmp);
        let dist = std::env::temp_dir().join("wrela_preview_test_dist");
        let _ = std::fs::create_dir_all(&dist);
        let assets = std::env::temp_dir().join("wrela_preview_test_assets");
        let _ = std::fs::create_dir_all(&assets);

        let scene_ir = default_scene_ir();
        let scene_json = serde_json::to_string_pretty(&scene_ir).unwrap();
        let (tx, _) = broadcast::channel::<String>(16);
        let state = Arc::new(AppState {
            scene_ir_json: RwLock::new(scene_json),
            tx,
        });

        let app = Router::new()
            .route("/scene.json", get(scene_json_handler))
            .route("/preview", get(ws_handler))
            .route("/health", get(health_handler))
            .nest_service("/assets", ServeDir::new(&assets))
            .fallback_service(ServeDir::new(&dist))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Test /health
        let resp = reqwest::get(format!("http://{addr}/health")).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "ok");

        // Test /scene.json
        let resp = reqwest::get(format!("http://{addr}/scene.json"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(json["scene"], "default");
        assert!(json["entities"].as_array().unwrap().is_empty());

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dist);
        let _ = std::fs::remove_dir_all(&assets);
    }
}
