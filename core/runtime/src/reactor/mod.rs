#[cfg(target_os = "linux")]
mod linux_epoll;
#[cfg(target_os = "linux")]
mod linux_io_uring;
#[cfg(target_os = "macos")]
mod macos_kqueue;
pub mod task;

use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactorEventKind {
    Readable,
    Timer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactorEvent {
    pub token: u64,
    pub kind: ReactorEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactorBackendKind {
    LinuxIoUring,
    LinuxEpoll,
    MacosKqueue,
    Unsupported,
}

#[derive(Debug, Clone, Copy)]
pub struct ReactorCapabilities {
    pub io_uring: bool,
    pub epoll: bool,
    pub kqueue: bool,
}

impl ReactorCapabilities {
    pub fn for_platform() -> Self {
        #[cfg(target_os = "linux")]
        {
            return Self {
                io_uring: !crate::config::reactor_disable_io_uring(),
                epoll: true,
                kqueue: false,
            };
        }
        #[cfg(target_os = "macos")]
        {
            return Self {
                io_uring: false,
                epoll: false,
                kqueue: true,
            };
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Self {
                io_uring: false,
                epoll: false,
                kqueue: false,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactorError {
    UnsupportedPlatform,
    InvalidTimeout,
    Backend(String),
}

impl std::fmt::Display for ReactorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("unsupported reactor platform"),
            Self::InvalidTimeout => formatter.write_str("invalid timeout"),
            Self::Backend(message) => write!(formatter, "backend operation failed: {message}"),
        }
    }
}

impl std::error::Error for ReactorError {}

pub trait ReactorBackend: Send + Sync {
    fn kind(&self) -> ReactorBackendKind;
    fn register(&self, token: u64) -> Result<(), ReactorError>;
    fn deregister(&self, token: u64) -> Result<(), ReactorError>;
    /// Arms a one-shot timer for a previously registered token.
    /// Implementations must reject unknown tokens.
    fn arm_timer(&self, token: u64, after: Duration) -> Result<(), ReactorError>;
    /// Polls for a single ready event.
    /// Returns `Ok(None)` when `timeout` elapses with no event.
    fn poll(&self, timeout: Duration) -> Result<Option<ReactorEvent>, ReactorError>;
}

pub struct Reactor {
    backend: Arc<dyn ReactorBackend>,
}

impl Reactor {
    pub fn new() -> Result<Self, ReactorError> {
        Self::with_capabilities(ReactorCapabilities::for_platform())
    }

    pub fn with_capabilities(capabilities: ReactorCapabilities) -> Result<Self, ReactorError> {
        let backend_kind = select_backend(std::env::consts::OS, capabilities);
        Self::with_backend_kind(backend_kind)
    }

    pub fn with_backend_kind(kind: ReactorBackendKind) -> Result<Self, ReactorError> {
        let backend: Arc<dyn ReactorBackend> = match kind {
            ReactorBackendKind::LinuxIoUring => {
                #[cfg(target_os = "linux")]
                {
                    Arc::new(linux_io_uring::LinuxIoUringBackend::new())
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return Err(ReactorError::UnsupportedPlatform);
                }
            }
            ReactorBackendKind::LinuxEpoll => {
                #[cfg(target_os = "linux")]
                {
                    Arc::new(linux_epoll::LinuxEpollBackend::new())
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return Err(ReactorError::UnsupportedPlatform);
                }
            }
            ReactorBackendKind::MacosKqueue => {
                #[cfg(target_os = "macos")]
                {
                    Arc::new(macos_kqueue::MacosKqueueBackend::new())
                }
                #[cfg(not(target_os = "macos"))]
                {
                    return Err(ReactorError::UnsupportedPlatform);
                }
            }
            ReactorBackendKind::Unsupported => return Err(ReactorError::UnsupportedPlatform),
        };

        Ok(Self { backend })
    }

    pub fn backend_kind(&self) -> ReactorBackendKind {
        self.backend.kind()
    }

    pub fn register(&self, token: u64) -> Result<(), ReactorError> {
        self.backend.register(token)
    }

    pub fn deregister(&self, token: u64) -> Result<(), ReactorError> {
        self.backend.deregister(token)
    }

    pub fn arm_timer_ms(&self, token: u64, timeout_ms: i64) -> Result<(), ReactorError> {
        if timeout_ms < 0 {
            return Err(ReactorError::InvalidTimeout);
        }
        self.backend
            .arm_timer(token, Duration::from_millis(timeout_ms as u64))
    }

    pub fn poll(&self, timeout_ms: i64) -> Result<Option<ReactorEvent>, ReactorError> {
        if timeout_ms < 0 {
            return Err(ReactorError::InvalidTimeout);
        }
        self.backend.poll(Duration::from_millis(timeout_ms as u64))
    }
}

pub fn select_backend(platform: &str, capabilities: ReactorCapabilities) -> ReactorBackendKind {
    match platform {
        "linux" => {
            if capabilities.io_uring {
                ReactorBackendKind::LinuxIoUring
            } else if capabilities.epoll {
                ReactorBackendKind::LinuxEpoll
            } else {
                ReactorBackendKind::Unsupported
            }
        }
        "macos" => {
            if capabilities.kqueue {
                ReactorBackendKind::MacosKqueue
            } else {
                ReactorBackendKind::Unsupported
            }
        }
        _ => ReactorBackendKind::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_selector_linux_prefers_io_uring() {
        let caps = ReactorCapabilities {
            io_uring: true,
            epoll: true,
            kqueue: false,
        };
        assert_eq!(
            select_backend("linux", caps),
            ReactorBackendKind::LinuxIoUring
        );
    }

    #[test]
    fn backend_selector_linux_falls_back_to_epoll() {
        let caps = ReactorCapabilities {
            io_uring: false,
            epoll: true,
            kqueue: false,
        };
        assert_eq!(
            select_backend("linux", caps),
            ReactorBackendKind::LinuxEpoll
        );
    }

    #[test]
    fn backend_selector_macos_uses_kqueue() {
        let caps = ReactorCapabilities {
            io_uring: false,
            epoll: false,
            kqueue: true,
        };
        assert_eq!(
            select_backend("macos", caps),
            ReactorBackendKind::MacosKqueue
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_backends_token_parity() {
        use crate::reactor::linux_epoll::LinuxEpollBackend;
        use crate::reactor::linux_io_uring::LinuxIoUringBackend;

        let io_uring = LinuxIoUringBackend::new();
        let epoll = LinuxEpollBackend::new();

        io_uring.register(11).expect("io_uring register");
        epoll.register(11).expect("epoll register");

        let io_event = io_uring
            .poll(Duration::from_millis(5))
            .expect("io_uring poll")
            .expect("io_uring event");
        let epoll_event = epoll
            .poll(Duration::from_millis(5))
            .expect("epoll poll")
            .expect("epoll event");

        assert_eq!(io_event.token, epoll_event.token);
        assert_eq!(io_event.kind, epoll_event.kind);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_kqueue_timer_and_readable_parity() {
        use crate::reactor::macos_kqueue::MacosKqueueBackend;

        let kqueue = MacosKqueueBackend::new();
        kqueue.register(12).expect("kqueue register");
        kqueue
            .arm_timer(12, Duration::from_millis(1))
            .expect("kqueue timer");

        let first = kqueue
            .poll(Duration::from_millis(5))
            .expect("kqueue poll")
            .expect("first event");
        let second = kqueue
            .poll(Duration::from_millis(10))
            .expect("kqueue poll")
            .expect("second event");

        assert_eq!(first.token, 12);
        assert_eq!(second.token, 12);
        assert!(matches!(
            (first.kind, second.kind),
            (ReactorEventKind::Readable, ReactorEventKind::Timer)
                | (ReactorEventKind::Timer, ReactorEventKind::Readable)
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_timeout_semantics_match_contract() {
        let reactor =
            Reactor::with_backend_kind(ReactorBackendKind::MacosKqueue).expect("kqueue reactor");
        let no_event = reactor.poll(0).expect("poll");
        assert_eq!(no_event, None);
    }
}
