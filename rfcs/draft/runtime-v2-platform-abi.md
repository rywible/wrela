# RFC: Runtime v2 Ownership and Platform ABI

Status: Draft
Date: 2026-02-05
Targets: Linux arm64, macOS arm64

## Summary
Define a small, stable platform ABI implemented in a minimal shim, then rewrite the runtime in Wrela to own allocator, scheduler, and IO policy. Linux uses io_uring as the primary IO backend. macOS uses kqueue readiness with nonblocking syscalls. The current runtime remains in place for reference while the v2 runtime lives in a separate folder. The long-term goal is to remove the Rust toolchain requirement by self-hosting the compiler and replacing the Rust shim.

## Goals
- Minimize Rust dependencies in the runtime core.
- Own allocator and scheduling policy in Wrela for predictable performance.
- Use a stable platform ABI so the runtime does not depend on OS-specific calls.
- Use io_uring on Linux for IO operations.
- Keep v1 runtime intact for reference while v2 is built in parallel.
- Remove the Rust toolchain requirement for end users via a Wrela bootstrap compiler artifact.

## Non-Goals
- Replace the kernel or syscalls.
- Provide a full libc replacement.
- Build HTTP/DB libraries in v0.
- Provide POSIX completeness.
- Build an internal linker in v0 (system linker is acceptable for now).

## Project Structure
- Keep v1 runtime unchanged at `core/runtime`.
- Create v2 runtime at `core/runtime_v2`.
- v2 layout (proposed):
  - `core/runtime_v2/platform` (minimal shim, Rust first, later replaceable)
  - `core/runtime_v2/runtime` (Wrela runtime core)
  - `core/runtime_v2/tests`

## Platform ABI
The platform ABI is the only boundary between Wrela runtime code and the OS. It must remain small, versioned, and stable.

### Memory
- `allocate_pages(size, flags) -> Pointer`
- `release_pages(pointer, size)`
- `protect_pages(pointer, size, protection)`
- `advise_pages(pointer, size, advice)` (optional)

### Threads and Sync
- `spawn_system_thread(entry, argument) -> ThreadIdentifier`
- `park_on_address(address, expected, timeout_nanoseconds)`
- `unpark_on_address(address, count)`

### Time
- `get_monotonic_time_in_nanoseconds() -> Unsigned64`
- `sleep_for_nanoseconds(nanoseconds)`

### IO and Eventing
The runtime IO layer uses two backends with the same high-level semantics.

Linux arm64:
- io_uring is the primary IO backend.
- The platform exposes submission and completion primitives for read, write, accept, connect, and poll.

macOS arm64:
- kqueue provides readiness notification.
- The runtime performs nonblocking syscalls after readiness events.

Platform surface:
- `submit_input_output_operation(operation) -> Token`
- `wait_for_input_output_events(timeout_nanoseconds, events_pointer, maximum) -> Integer`
- `cancel_input_output_token(token)` (optional)
- `open_file_at(directory, path, flags, mode) -> Integer`
- `close_file(file_descriptor)`
- `read_from_file(file_descriptor, buffer_pointer, length) -> Integer`
- `write_to_file(file_descriptor, buffer_pointer, length) -> Integer`
- `create_socket(domain, type, protocol) -> Integer`
- `bind_socket(file_descriptor, address_pointer, length)`
- `listen_on_socket(file_descriptor, backlog)`
- `accept_connection(file_descriptor) -> Integer`
- `connect_socket(file_descriptor, address_pointer, length)`

Note:
On macOS, `io_submit` is implemented as readiness registration plus nonblocking operations in the runtime. The platform ABI stays constant even though backend strategy differs.

### Process
- `exit_process(code)`

## Runtime v2 Core (Wrela)

### Allocation
- Arena-first allocation model.
- Default allocation uses the current scope arena.
- Global allocator for long-lived allocations.
- Global allocator features:
  - size classes
  - per-thread caches
  - large allocations direct from `page_alloc`
  - coalescing for large blocks

### Concurrency
- Structured concurrency by default.
- Explicit detached tasks are allowed via a named escape hatch.
- Cancellation propagates from parent to children.
- Task scopes own arenas to enable deterministic cleanup.

### Scheduler
- N:M task scheduler with work-stealing.
- Futex-like park/unpark for idle workers.
- Integrated timers.

### IO Runtime
- Linux: io_uring-based submission and completion.
- macOS: kqueue readiness + nonblocking operations.
- Buffer reuse via arenas.

## Language Primitives Required
Runtime v2 needs a minimal systems-capable subset in Wrela.

- Raw pointer types and conversion utilities.
- `size_of`, `align_of`, `addr_of`.
- `memcpy`, `memset`, `memcmp` intrinsics.
- Slices and byte slices.
- Atomic types with Acquire, Release, Relaxed semantics.
- Fixed-width integer types for ABI stability:
  - `Unsigned8`, `Unsigned16`, `Unsigned32`, `Unsigned64`
  - `Signed8`, `Signed16`, `Signed32`, `Signed64`
  - Optional alias: `Byte` = `Unsigned8`
- Explicit allocation context or allocator parameters.
- FFI declarations and C layout structs.
- `unsafe` blocks for pointer ops and FFI boundaries.

Layout syntax (locked):
```
A SocketAddress:
    laid out in memory as "c"
    has:
        family: Unsigned16
        port: Unsigned16
        address: Unsigned32
```
Rules:
- `laid out in memory as "c"` is only valid inside class bodies.
- Must appear before `has`.
- Only one layout clause per class.

## Dependency Policy
- `core/runtime_v2/platform` must avoid third-party deps.
- `core/runtime_v2/runtime` must avoid proc macros and async frameworks.
- Optional external deps are allowed only outside the runtime core.

Platform shim guidance:
- Use libc or generated bindings for constants and structs.
- io_uring on Linux should use raw syscalls if libc lacks wrappers.

## Future Library Compatibility
Runtime v2 must expose stable primitives that future libraries can build on.

- HTTP/DB libraries must be able to use arenas and async IO.
- IO handles are capability-like and passed explicitly.
- Structured concurrency should allow libraries to compose without leaking tasks.

## Milestones
1. Define and freeze the platform ABI.
2. Implement Linux arm64 io_uring shim.
3. Implement macOS arm64 kqueue shim.
4. Implement arena allocator and global allocator.
5. Implement scheduler with structured concurrency.
6. Implement IO runtime backend.
7. Add benchmark harness and counters.
8. Ship a bootstrap Wrela compiler per target to remove Rust toolchain dependency for users.

## Compiler Rewrite (Second Phase)
Rewrite the compiler in Wrela only after the runtime primitives above stabilize.

Phase plan:
- Keep the Rust compiler as source of truth.
- Port lexer and parser first.
- Port type checker and IR builder next.
- Port Cranelift driver last.
- Run Wrela compiler in CI and compare outputs.

Rationale:
The compiler rewrite depends on stable runtime primitives. Doing it first would cause churn and slow the runtime rewrite.

## Toolchain and Bootstrapping
Goal: users should not need a Rust toolchain to build or run Wrela.

Plan:
- Ship a prebuilt Wrela compiler per target as a bootstrap artifact in repo.
- Use that compiler to build the compiler from source.
- Keep Rust only for building or updating bootstrap artifacts during the transition.

Proposed location:
- `tools/bootstrap/<target>/` (example: `tools/bootstrap/linux-arm64/`)

This avoids a circular build requirement without forcing Rust on end users.

## Rust Removal Plan (Phased)
1. Runtime v2 owns allocator, scheduler, and IO policy.
2. Wrela compiler front and middle end are self-hosted.
3. Replace the Rust platform shim with a tiny C/asm shim.
4. Replace the C/asm shim with Wrela FFI once Wrela unsafe/FFI is stable enough.
5. Replace Cranelift with a Wrela-owned backend or a non-Rust backend.
6. Introduce an internal linker (optional long-term).
