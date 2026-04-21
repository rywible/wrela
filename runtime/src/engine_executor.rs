use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EngineTaskAffinity {
    Cpu,
    Gpu,
    External,
}

impl EngineTaskAffinity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::External => "external",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineExecutorConfig {
    pub cpu_worker_threads: usize,
    pub external_worker_threads: usize,
}

impl Default for EngineExecutorConfig {
    fn default() -> Self {
        let cpu_worker_threads = thread::available_parallelism()
            .map(|parallelism| parallelism.get().saturating_sub(1).max(1))
            .unwrap_or(1);
        let external_worker_threads = cpu_worker_threads.min(2).max(1);
        Self {
            cpu_worker_threads,
            external_worker_threads,
        }
    }
}

pub struct EngineTask {
    pub task_id: u64,
    pub label: String,
    pub affinity: EngineTaskAffinity,
    pub order_key: u64,
    pub task: Box<dyn FnOnce() -> Result<(), String> + Send + 'static>,
}

#[derive(Debug, Clone)]
pub struct EngineTaskOutcome {
    pub task_id: u64,
    pub label: String,
    pub affinity: EngineTaskAffinity,
    pub thread_name: String,
    pub started_at: Instant,
    pub ended_at: Instant,
    pub error: Option<String>,
    pub on_tokio_runtime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EngineExecutorReport {
    pub task_count: usize,
    pub max_parallel_tasks: usize,
    pub ready_overflow_count: u32,
    pub tokio_runtime_violations: u32,
}

#[derive(Clone)]
pub struct EngineExecutor {
    inner: Arc<EngineExecutorInner>,
}

impl std::fmt::Debug for EngineExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineExecutor")
            .field("config", &self.inner.config)
            .finish()
    }
}

struct EngineExecutorInner {
    config: EngineExecutorConfig,
    cpu_pool: WorkerPool,
    gpu_pool: WorkerPool,
    external_pool: WorkerPool,
}

struct WorkerPool {
    affinity: EngineTaskAffinity,
    capacity: usize,
    sender: mpsc::Sender<WorkerMessage>,
    handles: Mutex<Vec<JoinHandle<()>>>,
}

enum WorkerMessage {
    Task(QueuedTask),
    Shutdown,
}

struct QueuedTask {
    task: EngineTask,
    outcome_tx: mpsc::Sender<EngineTaskOutcome>,
    batch_control: Arc<BatchControl>,
}

#[derive(Default)]
struct BatchControl {
    canceled: AtomicBool,
}

impl BatchControl {
    fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::SeqCst)
    }

    fn cancel(&self) {
        self.canceled.store(true, Ordering::SeqCst);
    }
}

impl Default for EngineExecutor {
    fn default() -> Self {
        Self::new(EngineExecutorConfig::default())
    }
}

impl EngineExecutor {
    pub fn new(config: EngineExecutorConfig) -> Self {
        install_engine_executor_panic_hook();
        let cpu_capacity = config.cpu_worker_threads.max(1);
        let external_capacity = config.external_worker_threads.max(1);
        let inner = EngineExecutorInner {
            cpu_pool: WorkerPool::new(EngineTaskAffinity::Cpu, cpu_capacity),
            gpu_pool: WorkerPool::new(EngineTaskAffinity::Gpu, 1),
            external_pool: WorkerPool::new(EngineTaskAffinity::External, external_capacity),
            config,
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn config(&self) -> &EngineExecutorConfig {
        &self.inner.config
    }

    pub fn execute_batch(
        &self,
        mut tasks: Vec<EngineTask>,
    ) -> Result<(Vec<EngineTaskOutcome>, EngineExecutorReport), String> {
        if tasks.is_empty() {
            return Ok((Vec::new(), EngineExecutorReport::default()));
        }
        tasks.sort_by_key(|task| (task.order_key, task.task_id));

        let total_capacity = self.total_capacity();
        let cpu_task_count = tasks
            .iter()
            .filter(|task| matches!(task.affinity, EngineTaskAffinity::Cpu))
            .count();
        let gpu_task_count = tasks
            .iter()
            .filter(|task| matches!(task.affinity, EngineTaskAffinity::Gpu))
            .count();
        let external_task_count = tasks
            .iter()
            .filter(|task| matches!(task.affinity, EngineTaskAffinity::External))
            .count();

        let mut report = EngineExecutorReport {
            task_count: tasks.len(),
            max_parallel_tasks: cpu_task_count.min(self.inner.cpu_pool.capacity)
                + gpu_task_count.min(self.inner.gpu_pool.capacity)
                + external_task_count.min(self.inner.external_pool.capacity),
            ..EngineExecutorReport::default()
        };
        if tasks.len() > total_capacity {
            report.ready_overflow_count = tasks.len().saturating_sub(total_capacity) as u32;
        }

        let (outcome_tx, outcome_rx) = mpsc::channel::<EngineTaskOutcome>();
        let batch_control = Arc::new(BatchControl::default());
        for task in tasks {
            self.pool(task.affinity).submit(QueuedTask {
                task,
                outcome_tx: outcome_tx.clone(),
                batch_control: Arc::clone(&batch_control),
            })?;
        }
        drop(outcome_tx);

        let mut outcomes = Vec::with_capacity(report.task_count);
        while outcomes.len() < report.task_count {
            let outcome = outcome_rx
                .recv()
                .map_err(|err| format!("engine executor lost a task outcome: {err}"))?;
            let fail_fast = outcome.error.is_some() || outcome.on_tokio_runtime;
            if outcome.on_tokio_runtime {
                report.tokio_runtime_violations = report.tokio_runtime_violations.saturating_add(1);
            }
            outcomes.push(outcome);
            if fail_fast {
                batch_control.cancel();
                break;
            }
        }
        outcomes.sort_by_key(|outcome| outcome.task_id);
        Ok((outcomes, report))
    }

    fn pool(&self, affinity: EngineTaskAffinity) -> &WorkerPool {
        match affinity {
            EngineTaskAffinity::Cpu => &self.inner.cpu_pool,
            EngineTaskAffinity::Gpu => &self.inner.gpu_pool,
            EngineTaskAffinity::External => &self.inner.external_pool,
        }
    }

    fn total_capacity(&self) -> usize {
        self.inner
            .cpu_pool
            .capacity
            .saturating_add(self.inner.gpu_pool.capacity)
            .saturating_add(self.inner.external_pool.capacity)
    }
}

impl WorkerPool {
    fn new(affinity: EngineTaskAffinity, capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let (sender, receiver) = mpsc::channel::<WorkerMessage>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut handles = Vec::with_capacity(capacity);
        for worker_index in 0..capacity {
            let receiver = Arc::clone(&receiver);
            let thread_name = format!("wrela-engine-{}-worker-{worker_index}", affinity.as_str());
            let builder = thread::Builder::new().name(thread_name);
            let handle = builder
                .spawn(move || worker_loop(receiver))
                .unwrap_or_else(|err| {
                    panic!(
                        "failed to spawn {} engine executor worker {}: {err}",
                        affinity.as_str(),
                        worker_index
                    )
                });
            handles.push(handle);
        }
        Self {
            affinity,
            capacity,
            sender,
            handles: Mutex::new(handles),
        }
    }

    fn submit(&self, task: QueuedTask) -> Result<(), String> {
        self.sender.send(WorkerMessage::Task(task)).map_err(|err| {
            format!(
                "failed to queue {} engine task: {err}",
                self.affinity.as_str()
            )
        })
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        let worker_count = self
            .handles
            .lock()
            .map(|handles| handles.len())
            .unwrap_or_default();
        for _ in 0..worker_count {
            let _ = self.sender.send(WorkerMessage::Shutdown);
        }
        if let Ok(mut handles) = self.handles.lock() {
            for handle in handles.drain(..) {
                let _ = handle.join();
            }
        }
    }
}

fn worker_loop(receiver: Arc<Mutex<mpsc::Receiver<WorkerMessage>>>) {
    loop {
        let message = {
            let receiver = receiver.lock().unwrap_or_else(|poison| poison.into_inner());
            receiver.recv()
        };
        match message {
            Ok(WorkerMessage::Task(task)) => run_queued_task(task),
            Ok(WorkerMessage::Shutdown) | Err(_) => break,
        }
    }
}

fn run_queued_task(task: QueuedTask) {
    let QueuedTask {
        task,
        outcome_tx,
        batch_control,
    } = task;
    let EngineTask {
        task_id,
        label,
        affinity,
        task,
        ..
    } = task;
    let thread_name = thread::current()
        .name()
        .unwrap_or("wrela-engine-worker")
        .to_string();
    let started_at = Instant::now();
    let on_tokio_runtime = tokio::runtime::Handle::try_current().is_ok();
    let error = if batch_control.is_canceled() {
        Some("engine task canceled after a prior batch failure".to_string())
    } else {
        let result =
            with_engine_executor_panic_suppressed(|| panic::catch_unwind(AssertUnwindSafe(task)));
        let error = match result {
            Ok(result) => result.err(),
            Err(payload) => Some(format!(
                "engine task panicked: {}",
                format_panic_payload(payload)
            )),
        };
        if error.is_some() || on_tokio_runtime {
            batch_control.cancel();
        }
        error
    };
    let ended_at = Instant::now();
    let _ = outcome_tx.send(EngineTaskOutcome {
        task_id,
        label,
        affinity,
        thread_name,
        started_at,
        ended_at,
        error,
        on_tokio_runtime,
    });
}

fn format_panic_payload(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

thread_local! {
    static SUPPRESS_ENGINE_EXECUTOR_PANIC_HOOK: Cell<bool> = const { Cell::new(false) };
}

fn install_engine_executor_panic_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            let suppress = SUPPRESS_ENGINE_EXECUTOR_PANIC_HOOK.with(Cell::get);
            if !suppress {
                previous_hook(panic_info);
            }
        }));
    });
}

fn with_engine_executor_panic_suppressed<T>(f: impl FnOnce() -> T) -> T {
    SUPPRESS_ENGINE_EXECUTOR_PANIC_HOOK.with(|flag| {
        let previous = flag.replace(true);
        let result = f();
        flag.set(previous);
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn engine_executor_serializes_gpu_affinity() {
        let executor = EngineExecutor::new(EngineExecutorConfig {
            cpu_worker_threads: 2,
            external_worker_threads: 1,
        });
        let active_gpu = Arc::new(AtomicUsize::new(0));
        let max_gpu = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for task_id in 0..3 {
            let active_gpu = Arc::clone(&active_gpu);
            let max_gpu = Arc::clone(&max_gpu);
            tasks.push(EngineTask {
                task_id,
                label: format!("gpu-{task_id}"),
                affinity: EngineTaskAffinity::Gpu,
                order_key: task_id,
                task: Box::new(move || {
                    let active = active_gpu.fetch_add(1, Ordering::SeqCst) + 1;
                    max_gpu.fetch_max(active, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(5));
                    active_gpu.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }),
            });
        }

        let (_, report) = executor.execute_batch(tasks).expect("gpu batch succeeds");
        assert_eq!(max_gpu.load(Ordering::SeqCst), 1);
        assert_eq!(report.tokio_runtime_violations, 0);
    }

    #[test]
    fn engine_executor_allows_cpu_parallelism() {
        let executor = EngineExecutor::new(EngineExecutorConfig {
            cpu_worker_threads: 2,
            external_worker_threads: 1,
        });
        let active_cpu = Arc::new(AtomicUsize::new(0));
        let max_cpu = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for task_id in 0..2 {
            let active_cpu = Arc::clone(&active_cpu);
            let max_cpu = Arc::clone(&max_cpu);
            tasks.push(EngineTask {
                task_id,
                label: format!("cpu-{task_id}"),
                affinity: EngineTaskAffinity::Cpu,
                order_key: task_id,
                task: Box::new(move || {
                    let active = active_cpu.fetch_add(1, Ordering::SeqCst) + 1;
                    max_cpu.fetch_max(active, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(10));
                    active_cpu.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }),
            });
        }

        let (_, report) = executor.execute_batch(tasks).expect("cpu batch succeeds");
        assert!(max_cpu.load(Ordering::SeqCst) >= 2);
        assert!(report.max_parallel_tasks >= 2);
    }

    #[test]
    fn engine_executor_reuses_worker_threads_across_batches() {
        let executor = EngineExecutor::new(EngineExecutorConfig {
            cpu_worker_threads: 1,
            external_worker_threads: 1,
        });
        let make_task = |task_id: u64| EngineTask {
            task_id,
            label: format!("cpu-{task_id}"),
            affinity: EngineTaskAffinity::Cpu,
            order_key: task_id,
            task: Box::new(|| Ok(())),
        };

        let (first_outcomes, _) = executor
            .execute_batch(vec![make_task(0)])
            .expect("first batch succeeds");
        let (second_outcomes, _) = executor
            .execute_batch(vec![make_task(1)])
            .expect("second batch succeeds");

        assert_eq!(first_outcomes.len(), 1);
        assert_eq!(second_outcomes.len(), 1);
        assert_eq!(
            first_outcomes[0].thread_name,
            second_outcomes[0].thread_name
        );
    }

    #[test]
    fn engine_executor_allows_external_parallelism_when_configured() {
        let executor = EngineExecutor::new(EngineExecutorConfig {
            cpu_worker_threads: 1,
            external_worker_threads: 2,
        });
        let active_external = Arc::new(AtomicUsize::new(0));
        let max_external = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for task_id in 0..2 {
            let active_external = Arc::clone(&active_external);
            let max_external = Arc::clone(&max_external);
            tasks.push(EngineTask {
                task_id,
                label: format!("external-{task_id}"),
                affinity: EngineTaskAffinity::External,
                order_key: task_id,
                task: Box::new(move || {
                    let active = active_external.fetch_add(1, Ordering::SeqCst) + 1;
                    max_external.fetch_max(active, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(10));
                    active_external.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }),
            });
        }

        let (_, report) = executor
            .execute_batch(tasks)
            .expect("external batch succeeds");
        assert!(max_external.load(Ordering::SeqCst) >= 2);
        assert!(report.max_parallel_tasks >= 2);
    }

    #[test]
    fn engine_executor_reports_panicking_tasks_instead_of_hanging() {
        let executor = EngineExecutor::new(EngineExecutorConfig {
            cpu_worker_threads: 1,
            external_worker_threads: 1,
        });
        let (outcomes, report) = executor
            .execute_batch(vec![
                EngineTask {
                    task_id: 0,
                    label: "panic-task".to_string(),
                    affinity: EngineTaskAffinity::Cpu,
                    order_key: 0,
                    task: Box::new(|| panic!("panic from engine task")),
                },
                EngineTask {
                    task_id: 1,
                    label: "ok-task".to_string(),
                    affinity: EngineTaskAffinity::Cpu,
                    order_key: 1,
                    task: Box::new(|| Ok(())),
                },
            ])
            .expect("batch returns outcomes for all tasks");

        assert_eq!(report.task_count, 2);
        assert!(!outcomes.is_empty());
        assert!(outcomes.len() <= 2);
        let panic_outcome = outcomes
            .iter()
            .find(|outcome| outcome.task_id == 0)
            .expect("panic outcome is recorded");
        assert_eq!(panic_outcome.label, "panic-task");
        assert!(
            panic_outcome
                .error
                .as_deref()
                .is_some_and(|error| error.contains("panic from engine task"))
        );
        if let Some(ok_outcome) = outcomes.iter().find(|outcome| outcome.task_id == 1) {
            assert!(ok_outcome.error.is_none());
        }
    }

    #[test]
    fn engine_executor_returns_before_slow_sibling_finishes_after_panic() {
        let executor = EngineExecutor::new(EngineExecutorConfig {
            cpu_worker_threads: 2,
            external_worker_threads: 1,
        });
        let started = Instant::now();
        let (outcomes, report) = executor
            .execute_batch(vec![
                EngineTask {
                    task_id: 0,
                    label: "panic-task".to_string(),
                    affinity: EngineTaskAffinity::Cpu,
                    order_key: 0,
                    task: Box::new(|| panic!("panic from engine task")),
                },
                EngineTask {
                    task_id: 1,
                    label: "slow-task".to_string(),
                    affinity: EngineTaskAffinity::Cpu,
                    order_key: 1,
                    task: Box::new(|| {
                        thread::sleep(Duration::from_millis(250));
                        Ok(())
                    }),
                },
            ])
            .expect("batch still returns a structured failure outcome");

        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_millis(150));
        assert_eq!(report.task_count, 2);
        assert!(outcomes.iter().any(|outcome| outcome.task_id == 0));
    }
}
