# Runtime v2 Platform ABI

Version: 1

This document defines the stable ABI between the Wrela runtime and the platform
shim. Names are verb-first and avoid abbreviations.

## Types

- `Token` is an opaque identifier for in-flight input/output operations.
- `InputOutputOperation` describes a single platform input/output request:
  - `opcode`: operation kind (read, write, accept, connect, poll).
  - `flags`: per-operation flags (reserved for future use).
  - `file_descriptor`: target file or socket.
  - `buffer`: pointer to data buffer or socket address storage.
  - `length`: buffer size or socket address length.
  - `offset`: file offset for read/write.
  - `token`: optional user token (0 means auto-assign).
- `InputOutputEvent` describes a completion of an input/output request.

Type definitions live in `core/runtime_v2/platform/src/lib.rs` with explicit
size and alignment notes.

## Memory

- `allocate_pages(size, flags) -> Pointer`
- `release_pages(pointer, size)`
- `protect_pages(pointer, size, protection)`
- `advise_pages(pointer, size, advice)` (optional)

## Threads and Sync

- `spawn_system_thread(entry, argument) -> ThreadIdentifier`
- `park_on_address(address, expected, timeout_nanoseconds)`
- `unpark_on_address(address, count)`

## Time

- `get_monotonic_time_in_nanoseconds() -> Unsigned64`
- `sleep_for_nanoseconds(nanoseconds)`

## Input/Output

Linux arm64 uses io_uring. macOS arm64 uses kqueue readiness with nonblocking
syscalls. The ABI surface is identical:

- `submit_input_output_operation(operation) -> Token`
- `wait_for_input_output_events(timeout_nanoseconds, events_pointer, maximum) -> Integer`
- `cancel_input_output_token(token)` (optional)

File operations:

- `open_file_at(directory, path, flags, mode) -> Integer`
- `close_file(file_descriptor)`
- `read_from_file(file_descriptor, buffer_pointer, length) -> Integer`
- `write_to_file(file_descriptor, buffer_pointer, length) -> Integer`

Socket operations:

- `create_socket(domain, type, protocol) -> Integer`
- `bind_socket(file_descriptor, address_pointer, length)`
- `listen_on_socket(file_descriptor, backlog)`
- `accept_connection(file_descriptor) -> Integer`
- `connect_socket(file_descriptor, address_pointer, length)`

## Process

- `exit_process(code)`
