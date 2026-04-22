fn main() {
    if let Err(err) = wrela_frame_live_app::run_from_args(std::env::args().skip(1)) {
        eprintln!("frame-live app error: {err}");
        std::process::exit(1);
    }
}
