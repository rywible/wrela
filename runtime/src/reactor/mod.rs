#[cfg(target_os = "linux")]
mod linux_epoll;
#[cfg(target_os = "linux")]
mod linux_io_uring;
#[cfg(target_os = "macos")]
mod macos_kqueue;
pub mod task {
    use std::ptr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicU64, AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    const WAITER_WAITING: u8 = 0;
    const WAITER_NOTIFIED: u8 = 1;
    const WAITER_CANCELLED: u8 = 2;

    struct WaiterNode {
        next: AtomicPtr<WaiterNode>,
        thread: thread::Thread,
        state: AtomicU8,
    }

    pub struct TaskSignal {
        epoch: AtomicU64,
        waiter_count: AtomicUsize,
        waiters_head: AtomicPtr<WaiterNode>,
    }

    impl TaskSignal {
        pub fn new() -> Self {
            Self {
                epoch: AtomicU64::new(0),
                waiter_count: AtomicUsize::new(0),
                waiters_head: AtomicPtr::new(ptr::null_mut()),
            }
        }

        pub fn notify_one(&self) {
            self.epoch.fetch_add(1, Ordering::AcqRel);
            if self.waiter_count.load(Ordering::Acquire) == 0 {
                return;
            }
            while let Some(waiter) = self.pop_waiter() {
                let state = waiter.state.swap(WAITER_NOTIFIED, Ordering::AcqRel);
                if state == WAITER_WAITING {
                    waiter.thread.unpark();
                    break;
                }
            }
        }

        pub fn notify_waiters(&self) {
            self.epoch.fetch_add(1, Ordering::AcqRel);
            if self.waiter_count.load(Ordering::Acquire) == 0 {
                return;
            }
            while let Some(waiter) = self.pop_waiter() {
                let state = waiter.state.swap(WAITER_NOTIFIED, Ordering::AcqRel);
                if state == WAITER_WAITING {
                    waiter.thread.unpark();
                }
            }
        }

        pub fn wait(&self, observed_epoch: u64) -> u64 {
            let epoch = self.epoch.load(Ordering::Acquire);
            if epoch > observed_epoch {
                return epoch;
            }
            let waiter = Arc::new(WaiterNode {
                next: AtomicPtr::new(ptr::null_mut()),
                thread: thread::current(),
                state: AtomicU8::new(WAITER_WAITING),
            });
            self.push_waiter(waiter.clone());
            loop {
                let epoch = self.epoch.load(Ordering::Acquire);
                if epoch > observed_epoch {
                    self.cancel_waiter(&waiter);
                    return epoch;
                }
                if waiter.state.load(Ordering::Acquire) == WAITER_NOTIFIED {
                    return epoch;
                }
                thread::park();
            }
        }

        pub fn wait_timeout(&self, observed_epoch: u64, timeout: Duration) -> (u64, bool) {
            let epoch = self.epoch.load(Ordering::Acquire);
            if epoch > observed_epoch {
                return (epoch, true);
            }
            let deadline = Instant::now() + timeout;
            let waiter = Arc::new(WaiterNode {
                next: AtomicPtr::new(ptr::null_mut()),
                thread: thread::current(),
                state: AtomicU8::new(WAITER_WAITING),
            });
            self.push_waiter(waiter.clone());
            loop {
                let epoch = self.epoch.load(Ordering::Acquire);
                if epoch > observed_epoch {
                    self.cancel_waiter(&waiter);
                    return (epoch, true);
                }
                if waiter.state.load(Ordering::Acquire) == WAITER_NOTIFIED {
                    return (epoch, true);
                }
                let now = Instant::now();
                if now >= deadline {
                    self.cancel_waiter(&waiter);
                    let epoch = self.epoch.load(Ordering::Acquire);
                    return (epoch, epoch > observed_epoch);
                }
                thread::park_timeout(deadline.saturating_duration_since(now));
            }
        }

        pub fn snapshot(&self) -> u64 {
            self.epoch.load(Ordering::Acquire)
        }

        fn push_waiter(&self, waiter: Arc<WaiterNode>) {
            let raw = Arc::into_raw(waiter.clone()) as *mut WaiterNode;
            loop {
                let head = self.waiters_head.load(Ordering::Acquire);
                waiter.next.store(head, Ordering::Release);
                if self
                    .waiters_head
                    .compare_exchange(head, raw, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    self.waiter_count.fetch_add(1, Ordering::AcqRel);
                    return;
                }
            }
        }

        fn pop_waiter(&self) -> Option<Arc<WaiterNode>> {
            loop {
                let head = self.waiters_head.load(Ordering::Acquire);
                if head.is_null() {
                    return None;
                }
                let next = unsafe { (*head).next.load(Ordering::Acquire) };
                if self
                    .waiters_head
                    .compare_exchange(head, next, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    self.waiter_count.fetch_sub(1, Ordering::AcqRel);
                    return Some(unsafe { Arc::from_raw(head) });
                }
            }
        }

        fn cancel_waiter(&self, waiter: &Arc<WaiterNode>) {
            let state = waiter.state.swap(WAITER_CANCELLED, Ordering::AcqRel);
            if state != WAITER_WAITING {
                return;
            }
            let raw = Arc::as_ptr(waiter) as *mut WaiterNode;
            loop {
                let head = self.waiters_head.load(Ordering::Acquire);
                if head != raw {
                    break;
                }
                let next = waiter.next.load(Ordering::Acquire);
                if self
                    .waiters_head
                    .compare_exchange(head, next, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    self.waiter_count.fetch_sub(1, Ordering::AcqRel);
                    unsafe {
                        drop(Arc::from_raw(raw));
                    }
                    break;
                }
            }
        }
    }

    impl Default for TaskSignal {
        fn default() -> Self {
            Self::new()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::TaskSignal;
        use std::sync::Arc;
        use std::sync::atomic::Ordering;
        use std::thread;
        use std::time::Duration;

        #[test]
        fn wake_signal_timeout_semantics_parity() {
            let signal = Arc::new(TaskSignal::new());
            let observed = signal.snapshot();
            let (epoch, notified) = signal.wait_timeout(observed, Duration::from_millis(5));
            assert!(!notified);
            assert_eq!(epoch, observed);

            let signal_clone = signal.clone();
            let handle = thread::spawn(move || {
                let observed = signal_clone.snapshot();
                signal_clone.wait_timeout(observed, Duration::from_millis(200))
            });
            thread::sleep(Duration::from_millis(10));
            signal.notify_one();
            let (epoch, notified) = handle.join().expect("waiter join");
            assert!(notified);
            assert!(epoch > observed);
        }

        #[test]
        fn wake_signal_notify_one_wakes_single_waiter() {
            let signal = Arc::new(TaskSignal::new());
            let woke = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut handles = Vec::new();
            for _ in 0..2 {
                let signal = signal.clone();
                let woke = woke.clone();
                handles.push(thread::spawn(move || {
                    let observed = signal.snapshot();
                    let (_, notified) = signal.wait_timeout(observed, Duration::from_millis(200));
                    if notified {
                        woke.fetch_add(1, Ordering::AcqRel);
                    }
                }));
            }
            thread::sleep(Duration::from_millis(10));
            signal.notify_one();
            thread::sleep(Duration::from_millis(20));
            let count = woke.load(Ordering::Acquire);
            assert!(count <= 1, "notify_one should wake at most one waiter");
            signal.notify_waiters();
            for handle in handles {
                handle.join().expect("waiter join");
            }
        }

        #[test]
        fn wake_signal_notify_waiters_wakes_all() {
            let signal = Arc::new(TaskSignal::new());
            let mut handles = Vec::new();
            for _ in 0..3 {
                let signal = signal.clone();
                handles.push(thread::spawn(move || {
                    let observed = signal.snapshot();
                    let (_, notified) = signal.wait_timeout(observed, Duration::from_millis(200));
                    assert!(notified);
                }));
            }
            thread::sleep(Duration::from_millis(10));
            signal.notify_waiters();
            for handle in handles {
                handle.join().expect("waiter join");
            }
        }

        #[test]
        fn wake_signal_no_missed_wake_under_race() {
            let signal = Arc::new(TaskSignal::new());
            let mut handles = Vec::new();
            for _ in 0..50 {
                let signal_for_wait = signal.clone();
                handles.push(thread::spawn(move || {
                    let observed = signal_for_wait.snapshot();
                    let (_, notified) =
                        signal_for_wait.wait_timeout(observed, Duration::from_millis(50));
                    notified
                }));
                signal.notify_one();
            }
            let mut notified = 0usize;
            for handle in handles {
                if handle.join().expect("wait join") {
                    notified += 1;
                }
            }
            assert!(notified > 0, "race run should observe wakeups");
        }
    }
}

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
