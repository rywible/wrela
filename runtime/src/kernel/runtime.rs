use std::sync::OnceLock;

use super::config;

static TOKIO_RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    TOKIO_RT.get_or_init(|| {
        if config::deterministic_runtime_enabled() {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder
                .enable_all()
                .worker_threads(1)
                .thread_name("wrela-deterministic")
                .max_blocking_threads(config::tokio_blocking_threads());
            builder.build().expect("tokio runtime")
        } else {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder
                .enable_all()
                .thread_name("wrela-worker")
                .max_blocking_threads(config::tokio_blocking_threads());
            if let Some(n) = config::tokio_worker_threads_opt() {
                builder.worker_threads(n);
            }
            builder.build().expect("tokio runtime")
        }
    })
}
