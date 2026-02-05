use crate::{
    as_const_void_ptr, as_void_ptr, clamp_i64_to_i32, null_events, null_path, slice_from_raw_mut, timeout_to_timespec,
    InputOutputEvent, InputOutputOperation, Token, INPUT_OUTPUT_OPERATION_POLL_READ, INPUT_OUTPUT_OPERATION_POLL_WRITE,
    INPUT_OUTPUT_OPERATION_READ, INPUT_OUTPUT_OPERATION_WRITE, ThreadIdentifier,
};
use core::ffi::c_void;
use core::mem::size_of;
use core::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

const MAP_ANON: i32 = 0x20;
const MAP_PRIVATE: i32 = 0x02;
const MAP_SHARED: i32 = 0x01;

const PROT_READ: i32 = 0x1;
const PROT_WRITE: i32 = 0x2;
const PROT_EXEC: i32 = 0x4;

const CLOCK_MONOTONIC: i32 = 1;

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 0x800;

const FUTEX_WAIT: i32 = 0;
const FUTEX_WAKE: i32 = 1;

const SYS_FUTEX: i64 = 98;
const SYS_IO_URING_SETUP: i64 = 425;
const SYS_IO_URING_ENTER: i64 = 426;

const IORING_OFF_SQ_RING: i64 = 0;
const IORING_OFF_CQ_RING: i64 = 0x8000000;
const IORING_OFF_SQES: i64 = 0x10000000;

const IORING_ENTER_GETEVENTS: u32 = 1;

const IORING_OP_READ: u8 = 22;
const IORING_OP_WRITE: u8 = 23;
const IORING_OP_POLL_ADD: u8 = 6;

const POLLIN: u32 = 0x0001;
const POLLOUT: u32 = 0x0004;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
struct IoSqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    flags: u32,
    dropped: u32,
    array: u32,
    resv1: u32,
    resv2: u64,
}

#[repr(C)]
struct IoCqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    overflow: u32,
    cqes: u32,
    resv1: u64,
    resv2: u64,
}

#[repr(C)]
struct IoUringParams {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: IoSqringOffsets,
    cq_off: IoCqringOffsets,
}

#[repr(C)]
struct IoUringSqe {
    opcode: u8,
    flags: u8,
    ioprio: u16,
    fd: i32,
    off: u64,
    addr: u64,
    len: u32,
    rw_flags: u32,
    user_data: u64,
    buf_index: u16,
    personality: u16,
    splice_fd_in: i32,
    pad2: [u64; 2],
}

#[repr(C)]
struct IoUringCqe {
    user_data: u64,
    res: i32,
    flags: u32,
}

#[repr(C)]
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
    fn __errno_location() -> *mut i32;
    fn syscall(number: i64, ...) -> i64;
}

struct IoUringState {
    fd: i32,
    sq_ring: *mut u8,
    cq_ring: *mut u8,
    sqes: *mut IoUringSqe,
    sq_head: *mut u32,
    sq_tail: *mut u32,
    sq_ring_mask: *mut u32,
    sq_ring_entries: *mut u32,
    sq_array: *mut u32,
    cq_head: *mut u32,
    cq_tail: *mut u32,
    cq_ring_mask: *mut u32,
    cq_ring_entries: *mut u32,
    cqes: *mut IoUringCqe,
    next_token: AtomicU64,
}

static IO_URING_STATE: OnceLock<Mutex<IoUringState>> = OnceLock::new();

fn errno() -> i32 {
    unsafe { *__errno_location() }
}

fn set_nonblocking(fd: i32) {
    unsafe {
        let flags = fcntl(fd, F_GETFL, 0);
        if flags >= 0 {
            fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
    }
}

fn io_uring_state() -> &'static Mutex<IoUringState> {
    IO_URING_STATE.get_or_init(|| initialize_io_uring())
}

fn initialize_io_uring() -> Mutex<IoUringState> {
    let mut params = IoUringParams {
        sq_entries: 0,
        cq_entries: 0,
        flags: 0,
        sq_thread_cpu: 0,
        sq_thread_idle: 0,
        features: 0,
        wq_fd: 0,
        resv: [0; 3],
        sq_off: IoSqringOffsets {
            head: 0,
            tail: 0,
            ring_mask: 0,
            ring_entries: 0,
            flags: 0,
            dropped: 0,
            array: 0,
            resv1: 0,
            resv2: 0,
        },
        cq_off: IoCqringOffsets {
            head: 0,
            tail: 0,
            ring_mask: 0,
            ring_entries: 0,
            overflow: 0,
            cqes: 0,
            resv1: 0,
            resv2: 0,
        },
    };
    let fd = unsafe { syscall(SYS_IO_URING_SETUP, 256u32, &mut params as *mut IoUringParams) } as i32;
    if fd < 0 {
        return Mutex::new(IoUringState {
            fd,
            sq_ring: ptr::null_mut(),
            cq_ring: ptr::null_mut(),
            sqes: ptr::null_mut(),
            sq_head: ptr::null_mut(),
            sq_tail: ptr::null_mut(),
            sq_ring_mask: ptr::null_mut(),
            sq_ring_entries: ptr::null_mut(),
            sq_array: ptr::null_mut(),
            cq_head: ptr::null_mut(),
            cq_tail: ptr::null_mut(),
            cq_ring_mask: ptr::null_mut(),
            cq_ring_entries: ptr::null_mut(),
            cqes: ptr::null_mut(),
            next_token: AtomicU64::new(1),
        });
    }
    let sq_ring_size = (params.sq_off.array as usize) + (params.sq_entries as usize * size_of::<u32>());
    let cq_ring_size = (params.cq_off.cqes as usize) + (params.cq_entries as usize * size_of::<IoUringCqe>());
    let sq_ring = unsafe {
        mmap(
            ptr::null_mut(),
            sq_ring_size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            fd,
            IORING_OFF_SQ_RING,
        )
    };
    let cq_ring = unsafe {
        mmap(
            ptr::null_mut(),
            cq_ring_size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            fd,
            IORING_OFF_CQ_RING,
        )
    };
    let sqes = unsafe {
        mmap(
            ptr::null_mut(),
            params.sq_entries as usize * size_of::<IoUringSqe>(),
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            fd,
            IORING_OFF_SQES,
        )
    };
    Mutex::new(IoUringState {
        fd,
        sq_ring: sq_ring as *mut u8,
        cq_ring: cq_ring as *mut u8,
        sqes: sqes as *mut IoUringSqe,
        sq_head: unsafe { (sq_ring as *mut u8).add(params.sq_off.head as usize) as *mut u32 },
        sq_tail: unsafe { (sq_ring as *mut u8).add(params.sq_off.tail as usize) as *mut u32 },
        sq_ring_mask: unsafe { (sq_ring as *mut u8).add(params.sq_off.ring_mask as usize) as *mut u32 },
        sq_ring_entries: unsafe { (sq_ring as *mut u8).add(params.sq_off.ring_entries as usize) as *mut u32 },
        sq_array: unsafe { (sq_ring as *mut u8).add(params.sq_off.array as usize) as *mut u32 },
        cq_head: unsafe { (cq_ring as *mut u8).add(params.cq_off.head as usize) as *mut u32 },
        cq_tail: unsafe { (cq_ring as *mut u8).add(params.cq_off.tail as usize) as *mut u32 },
        cq_ring_mask: unsafe { (cq_ring as *mut u8).add(params.cq_off.ring_mask as usize) as *mut u32 },
        cq_ring_entries: unsafe { (cq_ring as *mut u8).add(params.cq_off.ring_entries as usize) as *mut u32 },
        cqes: unsafe { (cq_ring as *mut u8).add(params.cq_off.cqes as usize) as *mut IoUringCqe },
        next_token: AtomicU64::new(1),
    })
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
    let mut timeout_spec = Timespec { tv_sec: 0, tv_nsec: 0 };
    let timeout_ptr = if timeout_nanoseconds == 0 {
        ptr::null()
    } else {
        let (seconds, nanos) = timeout_to_timespec(timeout_nanoseconds);
        timeout_spec = Timespec { tv_sec: seconds, tv_nsec: nanos };
        &timeout_spec as *const Timespec
    };
    let result = unsafe {
        syscall(
            SYS_FUTEX,
            address,
            FUTEX_WAIT,
            expected,
            timeout_ptr,
            ptr::null::<u32>(),
            0,
        )
    };
    if result == 0 {
        0
    } else {
        -errno()
    }
}

pub fn unpark_on_address(address: *const u32, count: u32) -> i32 {
    if address.is_null() {
        return -1;
    }
    let result = unsafe { syscall(SYS_FUTEX, address, FUTEX_WAKE, count, ptr::null::<u32>()) };
    if result >= 0 {
        result as i32
    } else {
        -errno()
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
    if operation.file_descriptor < 0 {
        return 0;
    }
    let state_lock = io_uring_state();
    let mut state = state_lock.lock().unwrap();
    if state.fd < 0 {
        return 0;
    }
    let token = if operation.token == 0 {
        state.next_token.fetch_add(1, Ordering::Relaxed)
    } else {
        operation.token
    };
    let tail = unsafe { *state.sq_tail };
    let mask = unsafe { *state.sq_ring_mask };
    let index = tail & mask;
    let sqe = unsafe { &mut *state.sqes.add(index as usize) };
    *sqe = IoUringSqe {
        opcode: 0,
        flags: 0,
        ioprio: 0,
        fd: operation.file_descriptor,
        off: operation.offset,
        addr: operation.buffer as u64,
        len: operation.length as u32,
        rw_flags: 0,
        user_data: token,
        buf_index: 0,
        personality: 0,
        splice_fd_in: 0,
        pad2: [0; 2],
    };
    match operation.opcode {
        INPUT_OUTPUT_OPERATION_READ => {
            sqe.opcode = IORING_OP_READ;
        }
        INPUT_OUTPUT_OPERATION_WRITE => {
            sqe.opcode = IORING_OP_WRITE;
        }
        INPUT_OUTPUT_OPERATION_POLL_READ => {
            sqe.opcode = IORING_OP_POLL_ADD;
            sqe.rw_flags = POLLIN;
        }
        INPUT_OUTPUT_OPERATION_POLL_WRITE => {
            sqe.opcode = IORING_OP_POLL_ADD;
            sqe.rw_flags = POLLOUT;
        }
        _ => {
            return 0;
        }
    }
    unsafe {
        *state.sq_array.add(index as usize) = index;
        *state.sq_tail = tail + 1;
    }
    unsafe {
        syscall(SYS_IO_URING_ENTER, state.fd, 1u32, 0u32, 0u32, ptr::null::<Timespec>(), 0u32);
    }
    token
}

pub fn wait_for_input_output_events(timeout_nanoseconds: u64, events_pointer: *mut InputOutputEvent, maximum: u32) -> i32 {
    if null_events(events_pointer) || maximum == 0 {
        return -1;
    }
    let state_lock = io_uring_state();
    let mut state = state_lock.lock().unwrap();
    if state.fd < 0 {
        return -1;
    }
    let mut head = unsafe { *state.cq_head };
    let tail = unsafe { *state.cq_tail };
    if head == tail {
        let _ = unsafe {
            syscall(
                SYS_IO_URING_ENTER,
                state.fd,
                0u32,
                1u32,
                IORING_ENTER_GETEVENTS,
                ptr::null::<Timespec>(),
                0u32,
            )
        };
        head = unsafe { *state.cq_head };
    }
    let tail = unsafe { *state.cq_tail };
    if head == tail {
        if timeout_nanoseconds != 0 {
            let (seconds, nanos) = timeout_to_timespec(timeout_nanoseconds);
            let timespec = Timespec { tv_sec: seconds, tv_nsec: nanos };
            unsafe {
                nanosleep(&timespec, ptr::null_mut());
            }
        }
        return 0;
    }
    let available = tail.wrapping_sub(head);
    let count = core::cmp::min(available, maximum) as usize;
    let mask = unsafe { *state.cq_ring_mask };
    let output = unsafe { slice_from_raw_mut(events_pointer, count) };
    for index in 0..count {
        let cqe = unsafe { &*state.cqes.add(((head + index as u32) & mask) as usize) };
        output[index] = InputOutputEvent {
            token: cqe.user_data,
            result: cqe.res,
            flags: cqe.flags,
        };
    }
    head = head.wrapping_add(count as u32);
    unsafe {
        *state.cq_head = head;
    }
    count as i32
}

pub fn cancel_input_output_token(_token: Token) -> i32 {
    -1
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
