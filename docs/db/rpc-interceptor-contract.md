# RPC Interceptor Contract (Phase 3 Slice)

## Boundary

- All RPC handlers should be wrapped by `intercept_rpc(identity, rpc_class, handler)`.
- Authorization is evaluated before handler execution.

## Fail-Closed Guarantee

- If authorization fails, the handler is never executed.
- Missing/invalid identity fields are denied by default.
- When PKI-aware path is used, cert serial must be valid, unrevoked, unexpired, and cluster/node-bound.

## Implementation

- `/runtime/src/db/net/interceptor.rs`
- Uses `/runtime/src/db/security/authz.rs`
- Uses `/runtime/src/db/security/pki.rs` for PKI-aware interception path.

## Verification

- `cargo test -p wrela_runtime interceptor_ -- --nocapture`
