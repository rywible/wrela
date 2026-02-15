# platform adapters

Platform-specific host integrations live here.

- `adapters/darwin_kqueue_adapter.wr` is the active host implementation for development.
- `adapters/linux_io_uring_adapter.wr` is the intended primary Linux implementation.
- `adapters/linux_epoll_adapter.wr` is the Linux fallback path.

Core runtime and toolchain code must depend only on `src/domain/platform/contracts.wr`.
