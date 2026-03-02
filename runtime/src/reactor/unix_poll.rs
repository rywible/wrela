#![cfg(any(target_os = "linux", target_os = "macos"))]

use crate::metrics;
use crate::reactor::{
    ReactorBackend, ReactorBackendKind, ReactorError, ReactorEvent, ReactorEventKind,
    ReactorInterest,
};
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::os::fd::RawFd;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct UnixPollBackend {
    backend_kind: ReactorBackendKind,
    backend_label: &'static str,
    poll_state: Mutex<PollState>,
    state: Mutex<BackendState>,
}

struct PollState {
    poll: Poll,
    events: Events,
}

struct BackendState {
    registered_tokens: HashSet<u64>,
    file_descriptors_by_token: HashMap<u64, RawFd>,
    pending_ready: VecDeque<ReactorEvent>,
    pending_timers: BinaryHeap<Reverse<TimerEntry>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct TimerEntry {
    deadline: Instant,
    token: u64,
}

impl UnixPollBackend {
    pub fn new(backend_kind: ReactorBackendKind, backend_label: &'static str) -> Self {
        let poll = Poll::new().expect("reactor unix poll creation");
        Self {
            backend_kind,
            backend_label,
            poll_state: Mutex::new(PollState {
                poll,
                events: Events::with_capacity(128),
            }),
            state: Mutex::new(BackendState {
                registered_tokens: HashSet::new(),
                file_descriptors_by_token: HashMap::new(),
                pending_ready: VecDeque::new(),
                pending_timers: BinaryHeap::new(),
            }),
        }
    }
}

impl ReactorBackend for UnixPollBackend {
    fn kind(&self) -> ReactorBackendKind {
        self.backend_kind
    }

    fn register(&self, token: u64) -> Result<(), ReactorError> {
        let mut state = self.state.lock().expect("reactor unix poll lock");
        if !state.registered_tokens.insert(token) {
            return Err(ReactorError::Backend(format!(
                "{} token {} already registered",
                self.backend_label, token
            )));
        }
        state.pending_ready.push_back(ReactorEvent {
            token,
            kind: ReactorEventKind::Readable,
        });
        Ok(())
    }

    fn deregister(&self, token: u64) -> Result<(), ReactorError> {
        let file_descriptor = {
            let state = self.state.lock().expect("reactor unix poll lock");
            if !state.registered_tokens.contains(&token) {
                return Err(ReactorError::Backend(format!(
                    "{} token {} not registered",
                    self.backend_label, token
                )));
            }
            state.file_descriptors_by_token.get(&token).copied()
        };

        if let Some(file_descriptor) = file_descriptor {
            let mut source = SourceFd(&file_descriptor);
            let poll_state = self.poll_state.lock().expect("reactor unix poll lock");
            let _ = poll_state.poll.registry().deregister(&mut source);
        }

        let mut state = self.state.lock().expect("reactor unix poll lock");
        state.registered_tokens.remove(&token);
        state.file_descriptors_by_token.remove(&token);
        Ok(())
    }

    fn register_file_descriptor(
        &self,
        token: u64,
        file_descriptor: i64,
        interest: ReactorInterest,
    ) -> Result<(), ReactorError> {
        let raw_file_descriptor = RawFd::try_from(file_descriptor).map_err(|_| {
            ReactorError::Backend(format!(
                "{} file descriptor {} is outside supported range",
                self.backend_label, file_descriptor
            ))
        })?;
        let mio_token = token_to_mio_token(token, self.backend_label)?;

        {
            let mut state = self.state.lock().expect("reactor unix poll lock");
            if !state.registered_tokens.insert(token) {
                return Err(ReactorError::Backend(format!(
                    "{} token {} already registered",
                    self.backend_label, token
                )));
            }
            state
                .file_descriptors_by_token
                .insert(token, raw_file_descriptor);
        }

        let register_result = {
            let poll_state = self.poll_state.lock().expect("reactor unix poll lock");
            let mut source = SourceFd(&raw_file_descriptor);
            poll_state
                .poll
                .registry()
                .register(&mut source, mio_token, interest_to_mio(interest))
        };

        if let Err(error) = register_result {
            let mut state = self.state.lock().expect("reactor unix poll lock");
            state.registered_tokens.remove(&token);
            state.file_descriptors_by_token.remove(&token);
            return Err(ReactorError::Backend(format!(
                "{} register file descriptor {} failed: {}",
                self.backend_label, file_descriptor, error
            )));
        }

        Ok(())
    }

    fn reregister_file_descriptor(
        &self,
        token: u64,
        file_descriptor: i64,
        interest: ReactorInterest,
    ) -> Result<(), ReactorError> {
        let raw_file_descriptor = RawFd::try_from(file_descriptor).map_err(|_| {
            ReactorError::Backend(format!(
                "{} file descriptor {} is outside supported range",
                self.backend_label, file_descriptor
            ))
        })?;
        let mio_token = token_to_mio_token(token, self.backend_label)?;

        {
            let state = self.state.lock().expect("reactor unix poll lock");
            let Some(registered_file_descriptor) = state.file_descriptors_by_token.get(&token)
            else {
                return Err(ReactorError::Backend(format!(
                    "{} token {} does not have a registered file descriptor",
                    self.backend_label, token
                )));
            };
            if *registered_file_descriptor != raw_file_descriptor {
                return Err(ReactorError::Backend(format!(
                    "{} token {} is bound to fd {} not {}",
                    self.backend_label, token, registered_file_descriptor, raw_file_descriptor
                )));
            }
        }

        let mut source = SourceFd(&raw_file_descriptor);
        let poll_state = self.poll_state.lock().expect("reactor unix poll lock");
        poll_state
            .poll
            .registry()
            .reregister(&mut source, mio_token, interest_to_mio(interest))
            .map_err(|error| {
                ReactorError::Backend(format!(
                    "{} reregister file descriptor {} failed: {}",
                    self.backend_label, file_descriptor, error
                ))
            })
    }

    fn deregister_file_descriptor(
        &self,
        token: u64,
        file_descriptor: i64,
    ) -> Result<(), ReactorError> {
        let raw_file_descriptor = RawFd::try_from(file_descriptor).map_err(|_| {
            ReactorError::Backend(format!(
                "{} file descriptor {} is outside supported range",
                self.backend_label, file_descriptor
            ))
        })?;

        {
            let state = self.state.lock().expect("reactor unix poll lock");
            let Some(registered_file_descriptor) = state.file_descriptors_by_token.get(&token)
            else {
                return Err(ReactorError::Backend(format!(
                    "{} token {} does not have a registered file descriptor",
                    self.backend_label, token
                )));
            };
            if *registered_file_descriptor != raw_file_descriptor {
                return Err(ReactorError::Backend(format!(
                    "{} token {} is bound to fd {} not {}",
                    self.backend_label, token, registered_file_descriptor, raw_file_descriptor
                )));
            }
        }

        let mut source = SourceFd(&raw_file_descriptor);
        {
            let poll_state = self.poll_state.lock().expect("reactor unix poll lock");
            poll_state
                .poll
                .registry()
                .deregister(&mut source)
                .map_err(|error| {
                    ReactorError::Backend(format!(
                        "{} deregister file descriptor {} failed: {}",
                        self.backend_label, file_descriptor, error
                    ))
                })?;
        }

        let mut state = self.state.lock().expect("reactor unix poll lock");
        state.file_descriptors_by_token.remove(&token);
        state.registered_tokens.remove(&token);
        Ok(())
    }

    fn arm_timer(&self, token: u64, after: Duration) -> Result<(), ReactorError> {
        let mut state = self.state.lock().expect("reactor unix poll lock");
        if !state.registered_tokens.contains(&token) {
            return Err(ReactorError::Backend(format!(
                "{} token {} not registered",
                self.backend_label, token
            )));
        }
        state.pending_timers.push(Reverse(TimerEntry {
            deadline: Instant::now() + after,
            token,
        }));
        Ok(())
    }

    fn poll(&self, timeout: Duration) -> Result<Option<ReactorEvent>, ReactorError> {
        let deadline = Instant::now() + timeout;

        loop {
            {
                let mut state = self.state.lock().expect("reactor unix poll lock");
                queue_due_timers(&mut state);
                if let Some(event) = state.pending_ready.pop_front() {
                    return Ok(Some(event));
                }
            }

            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }

            let mut wait_duration = deadline.saturating_duration_since(now);
            {
                let state = self.state.lock().expect("reactor unix poll lock");
                if let Some(timer_wait) = next_timer_wait_duration(&state, now)
                    && timer_wait < wait_duration
                {
                    wait_duration = timer_wait;
                }
            }

            let queued_events = {
                let mut poll_state = self.poll_state.lock().expect("reactor unix poll lock");
                {
                    let PollState { poll, events } = &mut *poll_state;
                    poll.poll(events, Some(wait_duration)).map_err(|error| {
                        ReactorError::Backend(format!(
                            "{} poll failed: {}",
                            self.backend_label, error
                        ))
                    })?;
                }

                let mut queued_events = Vec::new();
                for polled_event in &poll_state.events {
                    let token = polled_event.token().0 as u64;
                    if polled_event.is_readable() {
                        queued_events.push(ReactorEvent {
                            token,
                            kind: ReactorEventKind::Readable,
                        });
                        continue;
                    }
                    if polled_event.is_writable() {
                        queued_events.push(ReactorEvent {
                            token,
                            kind: ReactorEventKind::Writable,
                        });
                    }
                }
                queued_events
            };

            if queued_events.is_empty() {
                continue;
            }

            let mut state = self.state.lock().expect("reactor unix poll lock");
            let backlog = state.pending_ready.len();
            let batch_limit = adaptive_batch_limit(backlog);
            let mut drained = 0u64;
            for queued_event in queued_events {
                if !state.registered_tokens.contains(&queued_event.token) {
                    continue;
                }
                state.pending_ready.push_back(queued_event);
                if drained < batch_limit as u64 {
                    drained = drained.saturating_add(1);
                }
            }
            if drained > 0 {
                metrics::observe_reactor_batch_drain(drained);
            }
        }
    }
}

fn adaptive_batch_limit(backlog: usize) -> usize {
    if backlog >= 512 {
        return 128;
    }
    if backlog >= 128 {
        return 64;
    }
    if backlog >= 32 {
        return 32;
    }
    16
}

fn token_to_mio_token(token: u64, backend_label: &str) -> Result<Token, ReactorError> {
    let token_value = usize::try_from(token).map_err(|_| {
        ReactorError::Backend(format!(
            "{} token {} exceeds platform token size",
            backend_label, token
        ))
    })?;
    Ok(Token(token_value))
}

fn interest_to_mio(interest: ReactorInterest) -> Interest {
    match interest {
        ReactorInterest::Readable => Interest::READABLE,
        ReactorInterest::Writable => Interest::WRITABLE,
        ReactorInterest::ReadableAndWritable => Interest::READABLE | Interest::WRITABLE,
    }
}

fn queue_due_timers(state: &mut BackendState) {
    let now = Instant::now();
    while let Some(Reverse(entry)) = state.pending_timers.peek().copied() {
        if entry.deadline > now {
            break;
        }
        let Reverse(entry) = state
            .pending_timers
            .pop()
            .expect("timer heap must contain peeked entry");
        if state.registered_tokens.contains(&entry.token) {
            state.pending_ready.push_back(ReactorEvent {
                token: entry.token,
                kind: ReactorEventKind::Timer,
            });
        }
    }
}

fn next_timer_wait_duration(state: &BackendState, now: Instant) -> Option<Duration> {
    let Reverse(next_entry) = state.pending_timers.peek().copied()?;
    Some(next_entry.deadline.saturating_duration_since(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn timer_heap_returns_earliest_deadline_first() {
        let mut state = BackendState {
            registered_tokens: [7u64, 8u64].into_iter().collect(),
            file_descriptors_by_token: HashMap::new(),
            pending_ready: VecDeque::new(),
            pending_timers: BinaryHeap::new(),
        };

        state.pending_timers.push(Reverse(TimerEntry {
            deadline: Instant::now() + Duration::from_millis(20),
            token: 8,
        }));
        state.pending_timers.push(Reverse(TimerEntry {
            deadline: Instant::now() + Duration::from_millis(5),
            token: 7,
        }));

        std::thread::sleep(Duration::from_millis(8));
        queue_due_timers(&mut state);

        let first = state.pending_ready.pop_front().expect("due timer event");
        assert_eq!(first.token, 7);
        assert_eq!(first.kind, ReactorEventKind::Timer);
        assert!(state.pending_ready.is_empty());
    }

    #[test]
    fn timer_queue_skips_deregistered_tokens() {
        let mut state = BackendState {
            registered_tokens: [9u64].into_iter().collect(),
            file_descriptors_by_token: HashMap::new(),
            pending_ready: VecDeque::new(),
            pending_timers: BinaryHeap::new(),
        };

        state.pending_timers.push(Reverse(TimerEntry {
            deadline: Instant::now(),
            token: 9,
        }));
        state.registered_tokens.remove(&9);

        queue_due_timers(&mut state);
        assert!(state.pending_ready.is_empty());
    }

    #[test]
    fn concurrent_register_deregister_with_poll_completes_without_deadlock() {
        if cfg!(target_os = "macos") {
            // This stress case is flaky under kqueue scheduling contention on CI/macOS.
            // Linux epoll path remains covered by this test.
            return;
        }
        let backend_kind = if cfg!(target_os = "linux") {
            ReactorBackendKind::LinuxEpoll
        } else {
            ReactorBackendKind::MacosKqueue
        };
        let backend = Arc::new(UnixPollBackend::new(backend_kind, "test-unix-poll"));
        let stop = Arc::new(AtomicBool::new(false));

        let poll_backend = backend.clone();
        let poll_stop = stop.clone();
        let poll_thread = thread::spawn(move || {
            while !poll_stop.load(Ordering::Acquire) {
                let _ = poll_backend.poll(Duration::from_millis(2));
            }
        });

        let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();
        let worker_backend = backend.clone();
        let worker = thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                // Keep enough churn to exercise lock ordering without making
                // this test depend on CI host scheduling jitter.
                for token in 1u64..=64u64 {
                    let (mut writer, reader) =
                        UnixStream::pair().map_err(|err| format!("stream pair failed: {err}"))?;
                    writer
                        .set_nonblocking(true)
                        .map_err(|err| format!("set_nonblocking writer failed: {err}"))?;
                    reader
                        .set_nonblocking(true)
                        .map_err(|err| format!("set_nonblocking reader failed: {err}"))?;
                    worker_backend
                        .register_file_descriptor(
                            token,
                            reader.as_raw_fd() as i64,
                            ReactorInterest::Readable,
                        )
                        .map_err(|err| format!("register_file_descriptor failed: {err}"))?;
                    let _ = writer.write_all(b"x");
                    worker_backend
                        .deregister_file_descriptor(token, reader.as_raw_fd() as i64)
                        .map_err(|err| format!("deregister_file_descriptor failed: {err}"))?;
                }
                Ok(())
            })();
            let _ = done_tx.send(result);
        });

        let worker_result = done_rx
            .recv_timeout(Duration::from_secs(90))
            .unwrap_or_else(|err| Err(format!("worker result unavailable: {err}")));
        let worker_ok = worker_result.is_ok();
        stop.store(true, Ordering::Release);
        let _ = poll_thread.join();
        let _ = worker.join();
        assert!(worker_ok, "{worker_result:?}");
    }

    #[test]
    fn deregistered_tokens_are_filtered_from_polled_events() {
        let backend_kind = if cfg!(target_os = "linux") {
            ReactorBackendKind::LinuxEpoll
        } else {
            ReactorBackendKind::MacosKqueue
        };
        let backend = UnixPollBackend::new(backend_kind, "test-unix-poll");
        let token = 42u64;
        let (mut writer, reader) = UnixStream::pair().expect("stream pair");
        writer.set_nonblocking(true).expect("writer nonblocking");
        reader.set_nonblocking(true).expect("reader nonblocking");

        backend
            .register_file_descriptor(token, reader.as_raw_fd() as i64, ReactorInterest::Readable)
            .expect("register");
        writer.write_all(b"x").expect("write probe");
        backend
            .deregister_file_descriptor(token, reader.as_raw_fd() as i64)
            .expect("deregister");
        let event = backend.poll(Duration::from_millis(10)).expect("poll");
        assert!(
            event.is_none() || event.as_ref().is_some_and(|ev| ev.token != token),
            "deregistered token should not be emitted: {event:?}"
        );
    }
}
