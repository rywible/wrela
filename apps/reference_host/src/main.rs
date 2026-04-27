//! Generic interactive reference host entry (RFC 0011 Phase 70).

fn main() {
    let frames = std::env::var("WRELA_REFERENCE_HOST_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());
    let project_path = std::env::var("WRELA_REFERENCE_HOST_PROJECT")
        .ok()
        .map(std::path::PathBuf::from);
    if std::env::var("WRELA_REFERENCE_HOST_HEADLESS").is_ok() {
        let result = match project_path {
            Some(path) => {
                wrela_reference_host::run_headless_smoke_for_project(frames.unwrap_or(8), path)
            }
            None => wrela_reference_host::run_headless_smoke(frames.unwrap_or(8)),
        };
        if let Err(err) = result {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
        return;
    }
    let mut config = wrela_reference_host::ReferenceHostConfig::default();
    config.frames = frames;
    config.project_path = project_path;
    if let Err(err) = wrela_reference_host::run_interactive(config) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
