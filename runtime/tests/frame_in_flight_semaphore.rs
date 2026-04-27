use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use wrela_runtime::platform::frame_pacing::FrameInFlightSemaphore;

#[test]
fn single_frame_in_flight_blocks_until_release() {
    let semaphore = Arc::new(FrameInFlightSemaphore::new(1));
    semaphore.acquire();
    let acquired = Arc::new(AtomicBool::new(false));
    let worker_semaphore = Arc::clone(&semaphore);
    let worker_acquired = Arc::clone(&acquired);
    let handle = thread::spawn(move || {
        worker_semaphore.acquire();
        worker_acquired.store(true, Ordering::SeqCst);
        worker_semaphore.release();
    });

    thread::sleep(Duration::from_millis(10));
    assert!(!acquired.load(Ordering::SeqCst));
    semaphore.release();
    handle.join().expect("worker");
    assert!(acquired.load(Ordering::SeqCst));
}

#[test]
fn release_after_submitted_work_done_waits_for_gpu_completion_callback() {
    let semaphore = Arc::new(FrameInFlightSemaphore::new(1));
    semaphore.acquire();
    let pending_callback = Arc::new(std::sync::Mutex::new(None::<Box<dyn FnOnce() + Send>>));
    let pending_for_register = Arc::clone(&pending_callback);
    semaphore.release_after_submitted_work_done_with(move |callback| {
        *pending_for_register.lock().expect("callback slot") = Some(callback);
    });

    let acquired = Arc::new(AtomicBool::new(false));
    let worker_semaphore = Arc::clone(&semaphore);
    let worker_acquired = Arc::clone(&acquired);
    let handle = thread::spawn(move || {
        worker_semaphore.acquire();
        worker_acquired.store(true, Ordering::SeqCst);
        worker_semaphore.release();
    });

    thread::sleep(Duration::from_millis(10));
    assert!(
        !acquired.load(Ordering::SeqCst),
        "frame N+1 must stay blocked until submitted work for frame N is done"
    );
    let callback = pending_callback
        .lock()
        .expect("callback slot")
        .take()
        .expect("completion callback was registered");
    callback();
    handle.join().expect("worker");
    assert!(acquired.load(Ordering::SeqCst));
}
