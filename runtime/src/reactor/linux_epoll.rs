#![cfg(target_os = "linux")]

use crate::reactor::{
    ReactorBackend, ReactorBackendKind, ReactorError, ReactorEvent, ReactorEventKind,
};
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct LinuxEpollBackend {
    inner: Arc<Mutex<BackendState>>,
}

struct BackendState {
    registered: HashSet<u64>,
    ready: VecDeque<ReactorEvent>,
}

impl LinuxEpollBackend {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BackendState {
                registered: HashSet::new(),
                ready: VecDeque::new(),
            })),
        }
    }
}

impl ReactorBackend for LinuxEpollBackend {
    fn kind(&self) -> ReactorBackendKind {
        ReactorBackendKind::LinuxEpoll
    }

    fn register(&self, token: u64) -> Result<(), ReactorError> {
        let mut state = self.inner.lock().expect("epoll backend lock");
        if !state.registered.insert(token) {
            return Err(ReactorError::Backend(format!(
                "epoll token {token} already registered"
            )));
        }
        state.ready.push_back(ReactorEvent {
            token,
            kind: ReactorEventKind::Readable,
        });
        Ok(())
    }

    fn deregister(&self, token: u64) -> Result<(), ReactorError> {
        let mut state = self.inner.lock().expect("epoll backend lock");
        if !state.registered.remove(&token) {
            return Err(ReactorError::Backend(format!(
                "epoll token {token} not registered"
            )));
        }
        Ok(())
    }

    fn arm_timer(&self, token: u64, after: Duration) -> Result<(), ReactorError> {
        {
            let state = self.inner.lock().expect("epoll backend lock");
            if !state.registered.contains(&token) {
                return Err(ReactorError::Backend(format!(
                    "epoll token {token} not registered"
                )));
            }
        }
        let inner = self.inner.clone();
        thread::spawn(move || {
            thread::sleep(after);
            let mut state = inner.lock().expect("epoll backend lock");
            if state.registered.contains(&token) {
                state.ready.push_back(ReactorEvent {
                    token,
                    kind: ReactorEventKind::Timer,
                });
            }
        });
        Ok(())
    }

    fn poll(&self, timeout: Duration) -> Result<Option<ReactorEvent>, ReactorError> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(event) = self
                .inner
                .lock()
                .expect("epoll backend lock")
                .ready
                .pop_front()
            {
                return Ok(Some(event));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}
