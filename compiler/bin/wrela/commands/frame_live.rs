use super::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wrela::frame_live::{
    FrameLiveCameraConfig, FrameLiveError, FrameLiveErrorKind, FrameLiveLaunchConfig,
    FrameLiveQueryBackend, FramePixel,
};

const WRELA_FRAME_LIVE_HEADLESS_ENV: &str = "WRELA_FRAME_LIVE_HEADLESS";
const WRELA_FRAME_LIVE_TEST_CLICK_ENV: &str = "WRELA_FRAME_LIVE_TEST_CLICK";
const FRAME_LIVE_APP_REL_PATH: &str = ".artifacts/apps/Wrela Frame Live.app";
const FRAME_LIVE_APP_BUILD_LANE: &str = "just bundle-frame-live-app";

pub(crate) fn execute_frame_live_command(args: FrameLiveCommandArgs) {
    if matches!(args.output_format, OutputFormat::Sarif) {
        eprintln!("error: frame-live supports human output or --json, not --sarif");
        std::process::exit(EXIT_USAGE);
    }
    let entry_path = match resolve_entry_path(args.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let entry_path = match canonicalize_entry_path(&entry_path) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let launch_config = frame_live_launch_config(&entry_path, &args);
    if env_flag_truthy(WRELA_FRAME_LIVE_HEADLESS_ENV) {
        run_headless_frame_live(&launch_config, args.output_format);
        return;
    }
    if let Err(err) = launch_frame_live_app(&launch_config) {
        eprintln!("error: {err}");
        std::process::exit(EXIT_CODEGEN);
    }
}

fn frame_live_launch_config(
    entry_path: &Path,
    args: &FrameLiveCommandArgs,
) -> FrameLiveLaunchConfig {
    FrameLiveLaunchConfig {
        entry_path: entry_path.to_path_buf(),
        view: args.options.view.clone(),
        region: args.options.region.clone(),
        domain: args.options.domain.clone(),
        camera: FrameLiveCameraConfig {
            position: args.options.camera_position,
            forward: args.options.camera_forward,
            up: args.options.camera_up,
            vertical_fov_degrees: args.options.vertical_fov_degrees,
        },
        width: args.options.width,
        height: args.options.height,
        frame_index: args.options.frame_index,
        delta_seconds: args.options.delta_seconds,
        query_backend: FrameLiveQueryBackend::from(args.query_backend),
    }
}

fn canonicalize_entry_path(entry_path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(entry_path)
        .map_err(|err| format!("canonicalize entry path `{}`: {err}", entry_path.display()))
}

fn run_headless_frame_live(config: &FrameLiveLaunchConfig, output_format: OutputFormat) {
    let session = match wrela::frame_live::FrameLiveSession::load(config.clone()) {
        Ok(session) => session,
        Err(err) => {
            emit_frame_live_error(&err, output_format);
            std::process::exit(exit_code_for_frame_live_error(err.kind()));
        }
    };
    let requested_pixel = std::env::var(WRELA_FRAME_LIVE_TEST_CLICK_ENV)
        .ok()
        .and_then(|value| parse_test_click(&value));
    if let Some(record) = session.headless_selection_record(requested_pixel) {
        emit_selection_record(&record, output_format);
    }
}

fn launch_frame_live_app(config: &FrameLiveLaunchConfig) -> Result<(), String> {
    let bundle_path = frame_live_bundle_path();
    ensure_bundle_exists(&bundle_path)?;
    let launch_config_path = write_launch_config_temp(config)?;
    let status = Command::new("open")
        .arg("-na")
        .arg(&bundle_path)
        .arg("--args")
        .arg("--launch-config")
        .arg(&launch_config_path)
        .status()
        .map_err(|err| format!("failed to launch frame-live app: {err}"))?;
    if !status.success() {
        return Err(format!(
            "failed to launch frame-live app (open exited with status {status})"
        ));
    }
    Ok(())
}

fn frame_live_bundle_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join(FRAME_LIVE_APP_REL_PATH)
}

fn ensure_bundle_exists(bundle_path: &Path) -> Result<(), String> {
    if bundle_path.exists() {
        return Ok(());
    }
    Err(format!(
        "frame-live app bundle not found at {}; run `{FRAME_LIVE_APP_BUILD_LANE}` first",
        bundle_path.display()
    ))
}

fn write_launch_config_temp(config: &FrameLiveLaunchConfig) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "wrela_frame_live_launch_{}_{}.json",
        std::process::id(),
        timestamp
    ));
    let body = serde_json::to_vec_pretty(config)
        .map_err(|err| format!("serialize launch config: {err}"))?;
    fs::write(&path, body)
        .map_err(|err| format!("write launch config `{}`: {err}", path.display()))?;
    Ok(path)
}

fn parse_test_click(value: &str) -> Option<FramePixel> {
    let (x, y) = value.split_once(',')?;
    let x: u32 = x.trim().parse().ok()?;
    let y: u32 = y.trim().parse().ok()?;
    Some(FramePixel { x, y })
}

fn env_flag_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn emit_selection_record(record: &wrela::frame_live::SelectionRecord, output_format: OutputFormat) {
    match output_format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string(record).unwrap_or_else(|_| "{}".to_string())
            );
        }
        _ => println!(
            "{}",
            wrela::frame_live::render_selection_record_human(record)
        ),
    }
}

fn emit_frame_live_error(err: &FrameLiveError, output_format: OutputFormat) {
    if err.diagnostics().is_empty() {
        eprintln!("error: {}", err.message());
        return;
    }
    let records = err
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.record.clone(), diagnostic.source.clone()))
        .collect();
    diag_emit::emit_deduped_records_with_sources(output_format, records);
}

fn exit_code_for_frame_live_error(kind: FrameLiveErrorKind) -> i32 {
    match kind {
        FrameLiveErrorKind::Usage => EXIT_USAGE,
        FrameLiveErrorKind::Parse => EXIT_PARSE,
        FrameLiveErrorKind::Type => EXIT_TYPE,
        FrameLiveErrorKind::Codegen => EXIT_CODEGEN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn bundle_missing_error_mentions_build_lane() {
        let temp = TempDir::new().expect("tempdir");
        let err =
            ensure_bundle_exists(&temp.path().join("missing.app")).expect_err("missing bundle");
        assert!(err.contains(FRAME_LIVE_APP_BUILD_LANE));
        assert!(err.contains("frame-live app bundle not found"));
    }

    #[test]
    fn launch_config_file_round_trips() {
        let config = FrameLiveLaunchConfig {
            entry_path: PathBuf::from("/tmp/world/src/main.wr"),
            view: Some("main_view".to_string()),
            region: Some("scene_region".to_string()),
            domain: Some("scene_domain".to_string()),
            camera: FrameLiveCameraConfig {
                position: [0.0, 1.0, 2.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                vertical_fov_degrees: 60.0,
            },
            width: Some(640),
            height: Some(360),
            frame_index: 1,
            delta_seconds: 1.0 / 60.0,
            query_backend: FrameLiveQueryBackend::Cpu,
        };
        let path = write_launch_config_temp(&config).expect("write config");
        let body = fs::read_to_string(&path).expect("read config");
        let decoded: FrameLiveLaunchConfig = serde_json::from_str(&body).expect("decode config");
        assert_eq!(decoded, config);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn canonicalize_entry_path_makes_relative_paths_absolute() {
        let cwd = std::env::current_dir().expect("cwd");
        let temp = tempfile::Builder::new()
            .prefix("frame-live-relative-entry-")
            .tempdir_in(&cwd)
            .expect("tempdir");
        let entry = temp.path().join("src").join("main.wr");
        fs::create_dir_all(entry.parent().expect("entry parent")).expect("create src");
        fs::write(&entry, "fn run() {}").expect("write entry");
        let relative = entry
            .strip_prefix(&cwd)
            .expect("entry under cwd")
            .to_path_buf();

        let canonical = canonicalize_entry_path(&relative).expect("canonicalize");

        assert!(canonical.is_absolute());
        assert_eq!(
            canonical,
            fs::canonicalize(&entry).expect("canonical entry")
        );
    }
}
