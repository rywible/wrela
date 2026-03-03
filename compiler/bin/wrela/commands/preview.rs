fn handle_preview_command(
    path_arg: Option<&str>,
    preview_port: u16,
    preview_open: bool,
) -> i32 {
    let app_path = match path_arg {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            eprintln!("error: wrela preview requires a path argument");
            eprintln!("usage: wrela preview <app-path> [--port <port>] [--open]");
            return EXIT_USAGE;
        }
    };

    if !app_path.exists() {
        eprintln!("error: path does not exist: {}", app_path.display());
        return EXIT_USAGE;
    }

    let config = wrela::preview_server::PreviewServerConfig {
        app_path: app_path.clone(),
        port: preview_port,
        open_browser: preview_open,
        client_dist_path: find_preview_client_dist(&app_path),
        asset_cache_path: wrela::asset_factory::cache_root_for_workspace(
            &find_preview_workspace_root(&app_path),
        ),
    };

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    match rt.block_on(wrela::preview_server::run_preview_server(config)) {
        Ok(()) => EXIT_OK,
        Err(e) => {
            eprintln!("error: preview server failed: {e}");
            EXIT_CODEGEN
        }
    }
}

fn find_preview_client_dist(app_path: &std::path::Path) -> std::path::PathBuf {
    let mut p = if app_path.is_file() {
        app_path.parent().unwrap_or(app_path).to_path_buf()
    } else {
        app_path.to_path_buf()
    };
    loop {
        if p.join("client").join("dist").exists() {
            return p.join("client").join("dist");
        }
        if p.join("Cargo.toml").exists() {
            return p.join("client").join("dist");
        }
        if !p.pop() {
            return app_path.join("client").join("dist");
        }
    }
}

fn find_preview_workspace_root(app_path: &std::path::Path) -> std::path::PathBuf {
    let mut p = if app_path.is_file() {
        app_path.parent().unwrap_or(app_path).to_path_buf()
    } else {
        app_path.to_path_buf()
    };
    loop {
        if p.join("Cargo.toml").exists() {
            return p;
        }
        if !p.pop() {
            return app_path.to_path_buf();
        }
    }
}
