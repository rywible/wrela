#![cfg(target_os = "linux")]

use crate::reactor::unix_poll::UnixPollBackend;
use crate::reactor::{
    ReactorBackend, ReactorBackendKind, ReactorError, ReactorEvent, ReactorInterest,
};
use std::time::Duration;

pub struct LinuxEpollBackend {
    inner: UnixPollBackend,
}

impl LinuxEpollBackend {
    pub fn new() -> Self {
        Self {
            inner: UnixPollBackend::new(ReactorBackendKind::LinuxEpoll, "epoll"),
        }
    }
}

impl ReactorBackend for LinuxEpollBackend {
    fn kind(&self) -> ReactorBackendKind {
        ReactorBackendKind::LinuxEpoll
    }

    fn register(&self, token: u64) -> Result<(), ReactorError> {
        self.inner.register(token)
    }

    fn deregister(&self, token: u64) -> Result<(), ReactorError> {
        self.inner.deregister(token)
    }

    fn register_file_descriptor(
        &self,
        token: u64,
        file_descriptor: i64,
        interest: ReactorInterest,
    ) -> Result<(), ReactorError> {
        self.inner
            .register_file_descriptor(token, file_descriptor, interest)
    }

    fn reregister_file_descriptor(
        &self,
        token: u64,
        file_descriptor: i64,
        interest: ReactorInterest,
    ) -> Result<(), ReactorError> {
        self.inner
            .reregister_file_descriptor(token, file_descriptor, interest)
    }

    fn deregister_file_descriptor(
        &self,
        token: u64,
        file_descriptor: i64,
    ) -> Result<(), ReactorError> {
        self.inner
            .deregister_file_descriptor(token, file_descriptor)
    }

    fn arm_timer(&self, token: u64, after: Duration) -> Result<(), ReactorError> {
        self.inner.arm_timer(token, after)
    }

    fn poll(&self, timeout: Duration) -> Result<Option<ReactorEvent>, ReactorError> {
        self.inner.poll(timeout)
    }
}
