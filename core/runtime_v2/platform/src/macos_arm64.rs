use crate::{
    as_const_void_ptr, as_void_ptr, clamp_i64_to_i32, null_buffer, null_events, null_path, slice_from_raw_mut,
    timeout_to_timespec, InputOutputEvent, InputOutputOperation, Token, INPUT_OUTPUT_OPERATION_ACCEPT,
    INPUT_OUTPUT_OPERATION_CONNECT, INPUT_OUTPUT_OPERATION_POLL_READ, INPUT_OUTPUT_OPERATION_POLL_WRITE,
    INPUT_OUTPUT_OPERATION_READ, INPUT_OUTPUT_OPERATION_WRITE, ThreadIdentifier,
};
use core::ffi::c_void;
use core::ptr;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

const MAP_ANON: i32 = 0x1000;
const MAP_PRIVATE: i32 = 0x0002;
const MAP_FAILED: *mut c_void = !0 as *mut c_void;

const PROT_READ: i32 = 0x1;
const PROT_WRITE: i32 = 0x2;
const PROT_EXEC: i32 = 0x4;

const CLOCK_MONOTONIC: i32 = 6;

const EV_ADD: u16 = 0x0001;
const EV_DELETE: u16 = 0x0002;
const EV_ONESHOT: u16 = 0x0010;
const EVFILT_READ: i16 = -1;
const EVFILT_WRITE: i16 = -2;

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 0x0004;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
struct Kevent {
    ident: usize,
    filter: i16,
    flags: u16,
    fflags: u32,
    data: isize,
    udata: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PthreadT {
    value: usize,
}

extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> i32;
    fn mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
    fn madvise(addr: *mut c_void, len: usize, advice: i32) -> i32;
    fn pthread_create(
        thread: *mut PthreadT,
        attr: *const c_void,
        start_routine: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> i32;
    fn pthread_detach(thread: PthreadT) -> i32;
    fn clock_gettime(clock_id: i32, tp: *mut Timespec) -> i32;
    fn nanosleep(req: *const Timespec, rem: *mut Timespec) -> i32;
    fn kqueue() -> i32;
    fn kevent(
        kq: i32,
        changelist: *const Kevent,
        nchanges: i32,
        eventlist: *mut Kevent,
        nevents: i32,
        timeout: *const Timespec,
    ) -> i32;
    fn openat(dirfd: i32, pathname: *const u8, flags: i32, mode: u32) -> i32;
    fn close(fd: i32) -> i32;
    fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn socket(domain: i32, type_: i32, protocol: i32) -> i32;
    fn bind(fd: i32, addr: *const c_void, len: u32) -> i32;
    fn listen(fd: i32, backlog: i32) -> i32;
    fn accept(fd: i32, addr: *mut c_void, len: *mut u32) -> i32;
    fn connect(fd: i32, addr: *const c_void, len: u32) -> i32;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
    fn _exit(code: i32) -> !;
    fn __error() -> *mut i32;
}

#[derive(Clone, Copy)]
struct PendingOperation {
    operation: InputOutputOperation,
}

struct PlatformState {
    kqueue_fd: i32,
    next_token: AtomicU64,
    pending: Mutex<HashMap<Token, PendingOperation>>,
    parks: Mutex<HashMap<usize, Arc<ParkState>>>,
}

struct ParkState {
    mutex: Mutex<()>,
    condvar: Condvar,
}

static STATE: OnceLock<PlatformState> = OnceLock::new();

fn state() -> &'static PlatformState {
    STATE.get_or_init(|| PlatformState {
        kqueue_fd: unsafe { kqueue() },
        next_token: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        parks: Mutex::new(HashMap::new()),
    })
}

fn errno() -> i32 {
    unsafe { *__error() }
}

fn set_nonblocking(fd: i32) {
    unsafe {
        let flags = fcntl(fd, F_GETFL, 0);
        if flags >= 0 {
            fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
    }
}

pub fn allocate_pages(size: usize, _flags: u32) -> *mut u8 {
    unsafe {
        let pointer = mmap(
            ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANON,
            -1,
            0,
        );
        if pointer == MAP_FAILED {
            ptr::null_mut()
        } else {
            pointer as *mut u8
        }
    }
}

pub fn release_pages(pointer: *mut u8, size: usize) {
    if pointer.is_null() || size == 0 {
        return;
    }
    unsafe {
        munmap(pointer as *mut c_void, size);
    }
}

pub fn protect_pages(pointer: *mut u8, size: usize, protection: u32) -> i32 {
    if pointer.is_null() || size == 0 {
        return -1;
    }
    let prot = protection as i32 & (PROT_READ | PROT_WRITE | PROT_EXEC);
    unsafe { mprotect(pointer as *mut c_void, size, prot) }
}

pub fn advise_pages(pointer: *mut u8, size: usize, advice: u32) -> i32 {
    if pointer.is_null() || size == 0 {
        return -1;
    }
    unsafe { madvise(pointer as *mut c_void, size, advice as i32) }
}

pub fn spawn_system_thread(entry: extern "C" fn(*mut u8) -> *mut u8, argument: *mut u8) -> ThreadIdentifier {
    let mut thread = PthreadT { value: 0 };
    let start = unsafe { core::mem::transmute::<extern "C" fn(*mut u8) -> *mut u8, extern "C" fn(*mut c_void) -> *mut c_void>(entry) };
    let result = unsafe { pthread_create(&mut thread, ptr::null(), start, argument as *mut c_void) };
    if result == 0 {
        unsafe { pthread_detach(thread) };
        thread.value as ThreadIdentifier
    } else {
        0
    }
}

pub fn park_on_address(address: *const u32, expected: u32, timeout_nanoseconds: u64) -> i32 {
    if address.is_null() {
        return -1;
    }
    let current = unsafe { *address };
    if current != expected {
        return 0;
    }
    let entry = {
        let mut parks = state().parks.lock().unwrap();
        parks
            .entry(address as usize)
            .or_insert_with(|| Arc::new(ParkState {
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
            }))
            .clone()
    };
    let guard = entry.mutex.lock().unwrap();
    if timeout_nanoseconds == 0 {
        let _guard = entry.condvar.wait(guard).unwrap();
        0
    } else {
        let (seconds, nanos) = timeout_to_timespec(timeout_nanoseconds);
        let timeout = std::time::Duration::new(seconds as u64, nanos as u32);
        let _guard = entry.condvar.wait_timeout(guard, timeout).unwrap();
        0
    }
}

pub fn unpark_on_address(address: *const u32, count: u32) -> i32 {
    if address.is_null() {
        return -1;
    }
    let entry = {
        let parks = state().parks.lock().unwrap();
        parks.get(&(address as usize)).cloned()
    };
    if let Some(entry) = entry {
        if count <= 1 {
            entry.condvar.notify_one();
        } else {
            entry.condvar.notify_all();
        }
        0
    } else {
        -1
    }
}

pub fn get_monotonic_time_in_nanoseconds() -> u64 {
    let mut timespec = Timespec { tv_sec: 0, tv_nsec: 0 };
    let result = unsafe { clock_gettime(CLOCK_MONOTONIC, &mut timespec) };
    if result == 0 {
        (timespec.tv_sec as u64) * 1_000_000_000 + (timespec.tv_nsec as u64)
    } else {
        0
    }
}

pub fn sleep_for_nanoseconds(nanoseconds: u64) {
    let (seconds, nanos) = timeout_to_timespec(nanoseconds);
    let timespec = Timespec { tv_sec: seconds, tv_nsec: nanos };
    unsafe {
        nanosleep(&timespec, ptr::null_mut());
    }
}

pub fn submit_input_output_operation(operation: InputOutputOperation) -> Token {
    let state = state();
    if state.kqueue_fd < 0 {
        return 0;
    }
    if operation.file_descriptor < 0 {
        return 0;
    }
    let token = if operation.token == 0 {
        state.next_token.fetch_add(1, Ordering::Relaxed)
    } else {
        operation.token
    };
    let filter = match operation.opcode {
        INPUT_OUTPUT_OPERATION_WRITE | INPUT_OUTPUT_OPERATION_CONNECT | INPUT_OUTPUT_OPERATION_POLL_WRITE => EVFILT_WRITE,
        _ => EVFILT_READ,
    };
    let flags = EV_ADD | EV_ONESHOT;
    let change = Kevent {
        ident: operation.file_descriptor as usize,
        filter,
        flags,
        fflags: 0,
        data: 0,
        udata: token as *mut c_void,
    };
    let result = unsafe { kevent(state.kqueue_fd, &change, 1, ptr::null_mut(), 0, ptr::null()) };
    if result < 0 {
        return 0;
    }
    let mut pending = state.pending.lock().unwrap();
    pending.insert(token, PendingOperation { operation });
    token
}

pub fn wait_for_input_output_events(timeout_nanoseconds: u64, events_pointer: *mut InputOutputEvent, maximum: u32) -> i32 {
    if null_events(events_pointer) || maximum == 0 {
        return -1;
    }
    let state = state();
    let mut events: Vec<Kevent> = Vec::with_capacity(maximum as usize);
    unsafe {
        events.set_len(maximum as usize);
    }
    let mut timeout_spec = Timespec { tv_sec: 0, tv_nsec: 0 };
    let timeout_ptr = if timeout_nanoseconds == 0 {
        ptr::null()
    } else {
        let (seconds, nanos) = timeout_to_timespec(timeout_nanoseconds);
        timeout_spec.tv_sec = seconds;
        timeout_spec.tv_nsec = nanos;
        &timeout_spec as *const Timespec
    };
    let count = unsafe { kevent(state.kqueue_fd, ptr::null(), 0, events.as_mut_ptr(), maximum as i32, timeout_ptr) };
    if count <= 0 {
        return count;
    }
    let output = unsafe { slice_from_raw_mut(events_pointer, count as usize) };
    for (index, event) in events.iter().take(count as usize).enumerate() {
        let token = event.udata as u64;
        let pending_operation = {
            let mut pending = state.pending.lock().unwrap();
            pending.remove(&token)
        };
        let result = if let Some(pending) = pending_operation {
            execute_operation(pending.operation)
        } else {
            -1
        };
        output[index] = InputOutputEvent {
            token,
            result,
            flags: 0,
        };
    }
    count
}

fn execute_operation(operation: InputOutputOperation) -> i32 {
    let fd = operation.file_descriptor;
    match operation.opcode {
        INPUT_OUTPUT_OPERATION_READ => unsafe {
            if null_buffer(operation.buffer) {
                return -1;
            }
            clamp_i64_to_i32(read(fd, operation.buffer, operation.length as usize) as i64)
        },
        INPUT_OUTPUT_OPERATION_WRITE => unsafe {
            if operation.buffer.is_null() {
                return -1;
            }
            clamp_i64_to_i32(write(fd, operation.buffer, operation.length as usize) as i64)
        },
        INPUT_OUTPUT_OPERATION_ACCEPT => unsafe {
            let mut length = operation.length as u32;
            clamp_i64_to_i32(accept(fd, as_void_ptr(operation.buffer), &mut length) as i64)
        },
        INPUT_OUTPUT_OPERATION_CONNECT => unsafe {
            clamp_i64_to_i32(connect(fd, as_const_void_ptr(operation.buffer), operation.length as u32) as i64)
        },
        INPUT_OUTPUT_OPERATION_POLL_READ | INPUT_OUTPUT_OPERATION_POLL_WRITE => 0,
        _ => -1,
    }
}

pub fn cancel_input_output_token(token: Token) -> i32 {
    let state = state();
    let operation = {
        let mut pending = state.pending.lock().unwrap();
        pending.remove(&token)
    };
    if let Some(operation) = operation {
        let filter = match operation.operation.opcode {
            INPUT_OUTPUT_OPERATION_WRITE | INPUT_OUTPUT_OPERATION_CONNECT | INPUT_OUTPUT_OPERATION_POLL_WRITE => EVFILT_WRITE,
            _ => EVFILT_READ,
        };
        let change = Kevent {
            ident: operation.operation.file_descriptor as usize,
            filter,
            flags: EV_DELETE,
            fflags: 0,
            data: 0,
            udata: token as *mut c_void,
        };
        let result = unsafe { kevent(state.kqueue_fd, &change, 1, ptr::null_mut(), 0, ptr::null()) };
        if result < 0 {
            -1
        } else {
            0
        }
    } else {
        -1
    }
}

pub fn open_file_at(directory: i32, path: *const u8, flags: i32, mode: u32) -> i32 {
    if null_path(path) {
        return -1;
    }
    unsafe { openat(directory, path, flags, mode) }
}

pub fn close_file(file_descriptor: i32) {
    if file_descriptor < 0 {
        return;
    }
    unsafe {
        close(file_descriptor);
    }
}

pub fn read_from_file(file_descriptor: i32, buffer_pointer: *mut u8, length: usize) -> i32 {
    if file_descriptor < 0 || buffer_pointer.is_null() || length == 0 {
        return -1;
    }
    unsafe { clamp_i64_to_i32(read(file_descriptor, buffer_pointer, length) as i64) }
}

pub fn write_to_file(file_descriptor: i32, buffer_pointer: *const u8, length: usize) -> i32 {
    if file_descriptor < 0 || buffer_pointer.is_null() || length == 0 {
        return -1;
    }
    unsafe { clamp_i64_to_i32(write(file_descriptor, buffer_pointer, length) as i64) }
}

pub fn create_socket(domain: i32, type_: i32, protocol: i32) -> i32 {
    let fd = unsafe { socket(domain, type_, protocol) };
    if fd >= 0 {
        set_nonblocking(fd);
    }
    fd
}

pub fn bind_socket(file_descriptor: i32, address_pointer: *const u8, length: u32) -> i32 {
    if file_descriptor < 0 || address_pointer.is_null() {
        return -1;
    }
    unsafe { bind(file_descriptor, as_const_void_ptr(address_pointer), length) }
}

pub fn listen_on_socket(file_descriptor: i32, backlog: i32) -> i32 {
    if file_descriptor < 0 {
        return -1;
    }
    unsafe { listen(file_descriptor, backlog) }
}

pub fn accept_connection(file_descriptor: i32, address_pointer: *mut u8, length: *mut u32) -> i32 {
    if file_descriptor < 0 || address_pointer.is_null() || length.is_null() {
        return -1;
    }
    unsafe { accept(file_descriptor, as_void_ptr(address_pointer), length) }
}

pub fn connect_socket(file_descriptor: i32, address_pointer: *const u8, length: u32) -> i32 {
    if file_descriptor < 0 || address_pointer.is_null() {
        return -1;
    }
    unsafe { connect(file_descriptor, as_const_void_ptr(address_pointer), length) }
}

pub fn exit_process(code: i32) -> ! {
    unsafe { _exit(code) }
}
