fn run_dev_loop(
    entry_path: &Path,
    poll_ms: u64,
    output_format: OutputFormat,
    emit_mir: bool,
    emit_mir_opt: bool,
    strict_naming: bool,
    program_args: &[String],
) {
    let src_root = find_src_root(entry_path)
        .unwrap_or_else(|| entry_path.parent().unwrap_or(entry_path).to_path_buf());
    eprintln!("dev: watching {} (poll {}ms)", src_root.display(), poll_ms);
    let mut last = snapshot_sources(&src_root);
    let mut child: Option<std::process::Child> = None;
    loop {
        if sources_changed(&src_root, &mut last) {
            if let Some(mut running) = child.take() {
                let _ = running.kill();
                let _ = running.wait();
            }
            let mir_module = match compile_to_mir(
                entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
                true,
                true,
                strict_naming,
                false,
            ) {
                Ok(mir) => mir,
                Err(code) => {
                    if code != EXIT_USAGE {
                        eprintln!("dev: build failed (exit {code})");
                    }
                    sleep_ms(poll_ms);
                    continue;
                }
            };
            let output = temp_exe_path();
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                sleep_ms(poll_ms);
                continue;
            }
            match Command::new(&output).args(program_args).spawn() {
                Ok(proc) => {
                    child = Some(proc);
                }
                Err(err) => {
                    eprintln!("dev: run failed: {err}");
                }
            }
        }
        sleep_ms(poll_ms);
    }
}

fn find_src_root(entry_path: &Path) -> Option<PathBuf> {
    for ancestor in entry_path.ancestors() {
        if ancestor.file_name().map(|n| n == "src").unwrap_or(false) {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn snapshot_sources(root: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut out = Vec::new();
    collect_sources(root, &mut out);
    out
}

fn sources_changed(root: &Path, last: &mut Vec<(PathBuf, SystemTime)>) -> bool {
    let mut current = Vec::new();
    collect_sources(root, &mut current);
    if current.len() != last.len() {
        *last = current;
        return true;
    }
    current.sort_by(|a, b| a.0.cmp(&b.0));
    last.sort_by(|a, b| a.0.cmp(&b.0));
    for (a, b) in current.iter().zip(last.iter()) {
        if a.0 != b.0 || a.1 != b.1 {
            *last = current;
            return true;
        }
    }
    false
}

fn collect_sources(root: &Path, out: &mut Vec<(PathBuf, SystemTime)>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, out);
        } else if is_source_file(&path)
            && let Ok(meta) = entry.metadata()
            && let Ok(modified) = meta.modified()
        {
            out.push((path, modified));
        }
    }
}

fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("wr") | Some("sp")
    )
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn update_toolchain(prefix_override: Option<&str>) -> Result<(), String> {
    let prefix = prefix_override
        .map(PathBuf::from)
        .or_else(|| env::var("PREFIX").ok().map(PathBuf::from))
        .unwrap_or_else(|| {
            env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".local")
                .join("wrela")
        });

    let target = resolve_target_triple()?;
    let tag = env::var("WRELA_TAG").ok().filter(|s| !s.is_empty());
    let tag = match tag {
        Some(tag) => tag,
        None => fetch_latest_tag()?,
    };
    let url =
        format!("https://github.com/rywible/wrela/releases/download/{tag}/wrela-{target}.tar.gz");

    fs::create_dir_all(&prefix).map_err(|err| format!("create prefix failed: {err}"))?;
    let tmp_path = env::temp_dir().join(format!(
        "wrela_update_{}_{}.tar.gz",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_nanos()
    ));

    let mut curl = Command::new("curl");
    curl.args(["-fsSL", "-o"]).arg(&tmp_path).arg(&url);
    let curl_out = curl.output().map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            "curl not found (install curl to use `wrela update`)".to_string()
        } else {
            format!("failed to run curl: {err}")
        }
    })?;
    if !curl_out.status.success() {
        let stderr = String::from_utf8_lossy(&curl_out.stderr);
        return Err(format!("download failed: {}", stderr.trim()));
    }

    let mut tar = Command::new("tar");
    tar.args(["-xzf"]).arg(&tmp_path).arg("-C").arg(&prefix);
    let tar_out = tar.output().map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            "tar not found (install tar to use `wrela update`)".to_string()
        } else {
            format!("failed to run tar: {err}")
        }
    })?;
    if !tar_out.status.success() {
        let stderr = String::from_utf8_lossy(&tar_out.stderr);
        return Err(format!("extract failed: {}", stderr.trim()));
    }

    let _ = fs::remove_file(&tmp_path);
    println!("Updated Wrela at: {}", prefix.display());
    Ok(())
}

fn resolve_target_triple() -> Result<&'static str, String> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    match (os, arch) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        _ => Err(format!("unsupported platform: {os}/{arch}")),
    }
}

fn fetch_latest_tag() -> Result<String, String> {
    let mut curl = Command::new("curl");
    curl.args([
        "-fsSL",
        "https://api.github.com/repos/rywible/wrela/releases",
    ]);
    let output = curl.output().map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            "curl not found (install curl to use `wrela update`)".to_string()
        } else {
            format!("failed to run curl: {err}")
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("failed to fetch releases: {}", stderr.trim()));
    }
    let body = String::from_utf8_lossy(&output.stdout);
    parse_first_tag(&body).ok_or_else(|| {
        "failed to resolve a release tag (set WRELA_TAG to a specific release)".to_string()
    })
}

fn parse_first_tag(body: &str) -> Option<String> {
    let key = "\"tag_name\"";
    let pos = body.find(key)?;
    let after = &body[pos + key.len()..];
    let quote_start = after.find('"')?;
    let after_quote = &after[quote_start + 1..];
    let quote_end = after_quote.find('"')?;
    let tag = &after_quote[..quote_end];
    if tag.is_empty() || tag == "null" {
        None
    } else {
        Some(tag.to_string())
    }
}
