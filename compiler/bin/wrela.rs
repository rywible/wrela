#[path = "wrela/mod.rs"]
mod wrela_cmd;

fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if std::env::var("WRELA_LAUNCHER_INTERNAL_RUST").ok().as_deref() == Some("1") {
        let spec = wrela_cmd::cli_args::parse(raw_args);
        wrela_cmd::command_handlers::execute(spec);
        return;
    }

    match wrela_cmd::v2_launcher::try_run_cutover_launcher(raw_args) {
        Ok(code) => std::process::exit(code),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}
