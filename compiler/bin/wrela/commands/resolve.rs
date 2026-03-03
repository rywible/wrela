fn handle_resolve_command(
    path_arg: Option<&str>,
    resolve_dry_run: bool,
    resolve_force: bool,
    resolve_parallel: usize,
) -> i32 {
    let scene_ir_path = match path_arg {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            eprintln!("error: wrela resolve requires a Scene IR JSON path");
            eprintln!("usage: wrela resolve <scene-ir.json> [--dry-run] [--force] [--parallel <N>]");
            return EXIT_USAGE;
        }
    };

    if !scene_ir_path.exists() {
        eprintln!("error: file does not exist: {}", scene_ir_path.display());
        return EXIT_USAGE;
    }

    let workspace_root = find_resolve_workspace_root(&scene_ir_path);

    let config = wrela::resolve::ResolveConfig {
        scene_ir_path,
        workspace_root,
        dry_run: resolve_dry_run,
        force: resolve_force,
        parallel: resolve_parallel,
    };

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    match rt.block_on(wrela::resolve::resolve(config)) {
        Ok(result) => {
            println!(
                "Resolved {} assets ({} cached, {} generated, {} failed)",
                result.total_assets, result.cached, result.generated, result.failed
            );
            if result.failed > 0 {
                EXIT_CODEGEN
            } else {
                EXIT_OK
            }
        }
        Err(e) => {
            eprintln!("error: resolve failed: {e}");
            EXIT_CODEGEN
        }
    }
}

fn find_resolve_workspace_root(path: &std::path::Path) -> std::path::PathBuf {
    let mut p = path.parent().unwrap_or(path).to_path_buf();
    loop {
        if p.join("Cargo.toml").exists() {
            return p;
        }
        if !p.pop() {
            return path.parent().unwrap_or(path).to_path_buf();
        }
    }
}
