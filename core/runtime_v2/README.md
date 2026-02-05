# Runtime v2

**Purpose**

Runtime v2 is a clean-room runtime rewrite that keeps v1 intact. The goal is a
minimal platform shim plus a Wrela-owned runtime core (allocator, scheduler,
input/output), with future libraries layered on top.

**Layers**

Platform Shim (Rust, temporary)
- Implements the platform ABI using platform syscalls.
- No third-party dependencies.
- Swappable later for a C/asm shim and eventually Wrela FFI.

Runtime Core (Wrela)
- Arena-first allocation and global allocator.
- Structured concurrency and scheduler.
- Input/output runtime with io_uring (Linux) and kqueue (macOS).

Future Libraries (Wrela, not part of v0)
- HTTP, database clients, and higher-level services.
- Must be built on arenas, structured concurrency, and runtime IO handles.

**Text Diagram**

Platform Shim -> Runtime Core -> Future Libraries

**Scope**

v0 is core-only: platform ABI, allocator, scheduler, and IO runtime.
HTTP/DB libraries are explicitly out of scope but must integrate cleanly using
arena allocation, explicit IO handles, and structured concurrency.
