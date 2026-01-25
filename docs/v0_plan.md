# v0 Plan (Production-Grade)

Status legend:
- [ ] pending
- [x] complete

## Scope Definition
- [ ] Define v0 feature set (language + runtime) in a short list
- [ ] Define supported platforms for v0 (macOS/Linux/Windows, x86_64/aarch64)
- [ ] Define supported build workflows (CLI only vs. library API)

## Must-Fix Compiler/Runtime Gaps
### Builtins + Stdlib
- [x] Implement runtime + codegen wiring for builtins: `parse_int`, `parse_float`, `read_file`, `write_file`
- [ ] Decide stdlib packaging/search path (embedded, $WRELA_HOME, or project-relative)
- [x] Add tests that compile+run builtins end-to-end

### MIR Validation / Safety
- [ ] Expand MIR validation beyond “missing terminator”:
  - [x] use-before-def for temps/locals
  - [x] block arg/phi consistency
  - [x] suspendable flag correctness (await present => suspendable)
  - [x] ActorCall only on actor handles
  - [x] RC ops are well-formed (no obvious leaks/double-dec)
- [ ] Make MIR validation a hard failure in CLI

### Actor + Method Dispatch Stability
- [x] Replace method name hashing with deterministic per-class method IDs
- [x] Ensure method IDs are stable across builds
- [ ] Add actor dispatch tests (missing method, invalid args, await/fire paths)

### Error Recovery / Diagnostics
- [ ] Remove “silent fallback” HIR lowering paths (panic/default placeholders)
- [ ] Emit explicit diagnostics for malformed syntax constructs
- [ ] Ensure parse/type errors always fail with non-zero exit codes

## Runtime Correctness / Stability
- [ ] Verify refcount invariants under common flows (class/map/list/pending)
- [ ] Validate crash behavior and result handling (err/otherwise/crash)
- [ ] Add runtime tests for maps/lists/strings/actors/resolved pending

## Compiler UX / CLI
- [ ] Make runtime build caching deterministic (avoid rebuilding per compile)
- [ ] Improve error messaging for missing toolchains/linkers
- [ ] Document CLI exit codes and error formats (pretty + JSON)

## Testing Matrix
- [ ] End-to-end smoke tests for each v0 feature:
  - [ ] classes + fields + methods
  - [ ] lists/maps/strings
  - [ ] numeric ops (int/float) + range
  - [ ] short-circuit and/or
  - [ ] actors (spawn/call/await/fire)
  - [ ] Result + otherwise + crash
  - [ ] builtins (parse/IO)
- [ ] Run tests for all supported platforms

## Release Checklist
- [ ] Update docs to reflect actual state (remove outdated gap notes)
- [ ] Tag v0 feature set and mark deferred items
- [ ] Create a minimal "getting started" example in `examples/`
- [ ] Define a versioning policy for runtime ABI
