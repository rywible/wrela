use wrela_platform_v2::{
    submit_input_output_operation, wait_for_input_output_events, InputOutputEvent, InputOutputOperation,
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos {
    use super::*;
    const AF_UNIX: i32 = 1;
    const SOCK_STREAM: i32 = 1;

    extern "C" {
        fn socketpair(domain: i32, type_: i32, protocol: i32, fds: *mut i32) -> i32;
        fn close(fd: i32) -> i32;
        fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    }

    #[test]
    fn smoke_kqueue_socket_readiness() {
        let mut fds = [0i32; 2];
        let pair_result = unsafe { socketpair(AF_UNIX, SOCK_STREAM, 0, fds.as_mut_ptr()) };
        assert_eq!(pair_result, 0);
        let read_fd = fds[0];
        let write_fd = fds[1];

        let mut read_buffer = [0u8; 5];
        let write_buffer = [b'h', b'e', b'l', b'l', b'o'];

        let operation = InputOutputOperation {
            opcode: wrela_platform_v2::INPUT_OUTPUT_OPERATION_READ,
            flags: 0,
            file_descriptor: read_fd,
            reserved: 0,
            buffer: read_buffer.as_mut_ptr(),
            length: read_buffer.len() as u64,
            offset: 0,
            token: 0,
        };
        let token = submit_input_output_operation(operation);
        assert!(token != 0);
        let write_result = unsafe { write(write_fd, write_buffer.as_ptr(), write_buffer.len()) };
        assert_eq!(write_result, write_buffer.len() as isize);

        let mut event = InputOutputEvent {
            token: 0,
            result: 0,
            flags: 0,
        };
        let count = wait_for_input_output_events(1_000_000_000, &mut event as *mut InputOutputEvent, 1);
        assert_eq!(count, 1);
        assert_eq!(event.token, token);
        assert_eq!(event.result, 5);
        assert_eq!(&read_buffer, &write_buffer);

        unsafe {
            close(read_fd);
            close(write_fd);
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
mod linux {
    use super::*;
    use wrela_platform_v2::{INPUT_OUTPUT_OPERATION_READ, INPUT_OUTPUT_OPERATION_WRITE};

    extern "C" {
        fn pipe(fds: *mut i32) -> i32;
        fn close(fd: i32) -> i32;
    }

    #[test]
    fn smoke_io_uring_read_write() {
        let mut fds = [0i32; 2];
        let pipe_result = unsafe { pipe(fds.as_mut_ptr()) };
        assert_eq!(pipe_result, 0);
        let read_fd = fds[0];
        let write_fd = fds[1];

        let mut read_buffer = [0u8; 5];
        let write_buffer = [b'h', b'e', b'l', b'l', b'o'];

        let read_operation = InputOutputOperation {
            opcode: INPUT_OUTPUT_OPERATION_READ,
            flags: 0,
            file_descriptor: read_fd,
            reserved: 0,
            buffer: read_buffer.as_mut_ptr(),
            length: read_buffer.len() as u64,
            offset: 0,
            token: 0,
        };
        let write_operation = InputOutputOperation {
            opcode: INPUT_OUTPUT_OPERATION_WRITE,
            flags: 0,
            file_descriptor: write_fd,
            reserved: 0,
            buffer: write_buffer.as_ptr() as *mut u8,
            length: write_buffer.len() as u64,
            offset: 0,
            token: 0,
        };

        let read_token = submit_input_output_operation(read_operation);
        let write_token = submit_input_output_operation(write_operation);
        assert!(read_token != 0);
        assert!(write_token != 0);

        let mut events = [InputOutputEvent { token: 0, result: 0, flags: 0 }; 2];
        let mut received = 0;
        while received < 2 {
            let count = wait_for_input_output_events(
                1_000_000_000,
                unsafe { events.as_mut_ptr().add(received) },
                (2 - received) as u32,
            );
            assert!(count >= 0);
            received += count as usize;
        }

        let mut read_result = None;
        let mut write_result = None;
        for event in events.iter() {
            if event.token == read_token {
                read_result = Some(event.result);
            } else if event.token == write_token {
                write_result = Some(event.result);
            }
        }
        assert_eq!(read_result, Some(5));
        assert_eq!(write_result, Some(5));
        assert_eq!(&read_buffer, &write_buffer);

        unsafe {
            close(read_fd);
            close(write_fd);
        }
    }
}
