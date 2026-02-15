# application/composition

Dependency wiring boundary: compose domain/application with infrastructure adapters.

Platform wiring is implemented in `platform_ports.wr` and must return domain
contracts only (no raw adapter leakage to callers).
