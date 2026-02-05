#![allow(dead_code)]

use core::ffi::c_void;

pub const PLATFORM_ABI_VERSION: u32 = 1;

pub type Token = u64;
pub type ThreadIdentifier = u64;

pub const INPUT_OUTPUT_OPERATION_READ: u32 = 1;
pub const INPUT_OUTPUT_OPERATION_WRITE: u32 = 2;
pub const INPUT_OUTPUT_OPERATION_ACCEPT: u32 = 3;
pub const INPUT_OUTPUT_OPERATION_CONNECT: u32 = 4;
pub const INPUT_OUTPUT_OPERATION_POLL_READ: u32 = 5;
pub const INPUT_OUTPUT_OPERATION_POLL_WRITE: u32 = 6;

pub const INPUT_OUTPUT_FLAG_NONE: u32 = 0;

#[repr(C)]
#[derive(Clone, Copy)]
/// Size: 48 bytes, Alignment: 8 bytes
pub struct InputOutputOperation {
    pub opcode: u32,
    pub flags: u32,
    pub file_descriptor: i32,
    pub reserved: u32,
    pub buffer: *mut u8,
    pub length: u64,
    pub offset: u64,
    pub token: Token,
}

#[repr(C)]
/// Size: 16 bytes, Alignment: 8 bytes
pub struct InputOutputEvent {
    pub token: Token,
    pub result: i32,
    pub flags: u32,
}

const _: [(); 48] = [(); core::mem::size_of::<InputOutputOperation>()];
const _: [(); 8] = [(); core::mem::align_of::<InputOutputOperation>()];
const _: [(); 16] = [(); core::mem::size_of::<InputOutputEvent>()];
const _: [(); 8] = [(); core::mem::align_of::<InputOutputEvent>()];

unsafe impl Send for InputOutputOperation {}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
mod linux_arm64;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos_arm64;

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use linux_arm64 as platform;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use macos_arm64 as platform;

#[cfg(not(any(
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
mod platform {
    use super::*;

    pub fn allocate_pages(_size: usize, _flags: u32) -> *mut u8 {
        core::ptr::null_mut()
    }

    pub fn release_pages(_pointer: *mut u8, _size: usize) {}

    pub fn protect_pages(_pointer: *mut u8, _size: usize, _protection: u32) -> i32 {
        -1
    }

    pub fn advise_pages(_pointer: *mut u8, _size: usize, _advice: u32) -> i32 {
        -1
    }

    pub fn spawn_system_thread(_entry: extern "C" fn(*mut u8) -> *mut u8, _argument: *mut u8) -> ThreadIdentifier {
        0
    }

    pub fn park_on_address(_address: *const u32, _expected: u32, _timeout_nanoseconds: u64) -> i32 {
        -1
    }

    pub fn unpark_on_address(_address: *const u32, _count: u32) -> i32 {
        -1
    }

    pub fn get_monotonic_time_in_nanoseconds() -> u64 {
        0
    }

    pub fn sleep_for_nanoseconds(_nanoseconds: u64) {}

    pub fn submit_input_output_operation(_operation: InputOutputOperation) -> Token {
        0
    }

    pub fn wait_for_input_output_events(_timeout_nanoseconds: u64, _events_pointer: *mut InputOutputEvent, _maximum: u32) -> i32 {
        -1
    }

    pub fn cancel_input_output_token(_token: Token) -> i32 {
        -1
    }

    pub fn open_file_at(_directory: i32, _path: *const u8, _flags: i32, _mode: u32) -> i32 {
        -1
    }

    pub fn close_file(_file_descriptor: i32) {}

    pub fn read_from_file(_file_descriptor: i32, _buffer_pointer: *mut u8, _length: usize) -> i32 {
        -1
    }

    pub fn write_to_file(_file_descriptor: i32, _buffer_pointer: *const u8, _length: usize) -> i32 {
        -1
    }

    pub fn create_socket(_domain: i32, _type_: i32, _protocol: i32) -> i32 {
        -1
    }

    pub fn bind_socket(_file_descriptor: i32, _address_pointer: *const u8, _length: u32) -> i32 {
        -1
    }

    pub fn listen_on_socket(_file_descriptor: i32, _backlog: i32) -> i32 {
        -1
    }

    pub fn accept_connection(_file_descriptor: i32, _address_pointer: *mut u8, _length: *mut u32) -> i32 {
        -1
    }

    pub fn connect_socket(_file_descriptor: i32, _address_pointer: *const u8, _length: u32) -> i32 {
        -1
    }

    pub fn exit_process(_code: i32) -> ! {
        loop {}
    }
}

pub fn allocate_pages(size: usize, flags: u32) -> *mut u8 {
    platform::allocate_pages(size, flags)
}

pub fn release_pages(pointer: *mut u8, size: usize) {
    platform::release_pages(pointer, size)
}

pub fn protect_pages(pointer: *mut u8, size: usize, protection: u32) -> i32 {
    platform::protect_pages(pointer, size, protection)
}

pub fn advise_pages(pointer: *mut u8, size: usize, advice: u32) -> i32 {
    platform::advise_pages(pointer, size, advice)
}

pub fn spawn_system_thread(entry: extern "C" fn(*mut u8) -> *mut u8, argument: *mut u8) -> ThreadIdentifier {
    platform::spawn_system_thread(entry, argument)
}

pub fn park_on_address(address: *const u32, expected: u32, timeout_nanoseconds: u64) -> i32 {
    platform::park_on_address(address, expected, timeout_nanoseconds)
}

pub fn unpark_on_address(address: *const u32, count: u32) -> i32 {
    platform::unpark_on_address(address, count)
}

pub fn get_monotonic_time_in_nanoseconds() -> u64 {
    platform::get_monotonic_time_in_nanoseconds()
}

pub fn sleep_for_nanoseconds(nanoseconds: u64) {
    platform::sleep_for_nanoseconds(nanoseconds)
}

pub fn submit_input_output_operation(operation: InputOutputOperation) -> Token {
    platform::submit_input_output_operation(operation)
}

pub fn wait_for_input_output_events(timeout_nanoseconds: u64, events_pointer: *mut InputOutputEvent, maximum: u32) -> i32 {
    platform::wait_for_input_output_events(timeout_nanoseconds, events_pointer, maximum)
}

pub fn cancel_input_output_token(token: Token) -> i32 {
    platform::cancel_input_output_token(token)
}

pub fn open_file_at(directory: i32, path: *const u8, flags: i32, mode: u32) -> i32 {
    platform::open_file_at(directory, path, flags, mode)
}

pub fn close_file(file_descriptor: i32) {
    platform::close_file(file_descriptor)
}

pub fn read_from_file(file_descriptor: i32, buffer_pointer: *mut u8, length: usize) -> i32 {
    platform::read_from_file(file_descriptor, buffer_pointer, length)
}

pub fn write_to_file(file_descriptor: i32, buffer_pointer: *const u8, length: usize) -> i32 {
    platform::write_to_file(file_descriptor, buffer_pointer, length)
}

pub fn create_socket(domain: i32, type_: i32, protocol: i32) -> i32 {
    platform::create_socket(domain, type_, protocol)
}

pub fn bind_socket(file_descriptor: i32, address_pointer: *const u8, length: u32) -> i32 {
    platform::bind_socket(file_descriptor, address_pointer, length)
}

pub fn listen_on_socket(file_descriptor: i32, backlog: i32) -> i32 {
    platform::listen_on_socket(file_descriptor, backlog)
}

pub fn accept_connection(file_descriptor: i32, address_pointer: *mut u8, length: *mut u32) -> i32 {
    platform::accept_connection(file_descriptor, address_pointer, length)
}

pub fn connect_socket(file_descriptor: i32, address_pointer: *const u8, length: u32) -> i32 {
    platform::connect_socket(file_descriptor, address_pointer, length)
}

pub fn exit_process(code: i32) -> ! {
    platform::exit_process(code)
}

#[inline]
pub(crate) fn null_path(path: *const u8) -> bool {
    path.is_null()
}

#[inline]
pub(crate) fn null_buffer(buffer: *mut u8) -> bool {
    buffer.is_null()
}

#[inline]
pub(crate) fn null_events(events: *mut InputOutputEvent) -> bool {
    events.is_null()
}

#[inline]
pub(crate) fn timeout_to_timespec(timeout_nanoseconds: u64) -> (i64, i64) {
    let seconds = (timeout_nanoseconds / 1_000_000_000) as i64;
    let nanoseconds = (timeout_nanoseconds % 1_000_000_000) as i64;
    (seconds, nanoseconds)
}

#[inline]
pub(crate) fn clamp_i64_to_i32(value: i64) -> i32 {
    if value > i32::MAX as i64 {
        i32::MAX
    } else if value < i32::MIN as i64 {
        i32::MIN
    } else {
        value as i32
    }
}

#[inline]
pub(crate) unsafe fn slice_from_raw_mut<'a, T>(pointer: *mut T, length: usize) -> &'a mut [T] {
    core::slice::from_raw_parts_mut(pointer, length)
}

#[inline]
pub(crate) fn as_void_ptr<T>(pointer: *mut T) -> *mut c_void {
    pointer as *mut c_void
}

#[inline]
pub(crate) fn as_const_void_ptr<T>(pointer: *const T) -> *const c_void {
    pointer as *const c_void
}
