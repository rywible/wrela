# v0 Plan (Production-Grade)

Status legend:
- [ ] pending
- [x] complete

## Scope Definition
- [x] Define v0 feature set (language + runtime) in a short list
- [x] Define supported platforms for v0 (macOS/Linux/Windows, x86_64/aarch64)
- [x] Define supported build workflows (CLI only vs. library API)
  - Feature set (v0):
    - Modules + imports (`use ... from ...`), classes + fields + methods
    - Functions, control flow (`if`, `for`, `match`, `return`, `break`)
    - Lists, maps, strings (concat + interpolation), ranges
    - Numeric ops (int/float), short-circuit `and`/`or`
    - Actors (`spawn`, `await`, `fire`) + Result/otherwise/crash
    - Builtins: `parse_int`, `parse_float`, `read_file`, `write_file`
  - Platforms (v0):
    - macOS: `aarch64-apple-darwin`, `x86_64-apple-darwin`
    - Linux: `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`
    - Windows: deferred (no releases for v0)
  - Build workflows (v0):
    - CLI first: `wrela <path>` builds + runs via the compiler/runtime pipeline
    - Artifact output: `--emit-obj` and `--emit-bin` supported
    - Library API: available for internal use, not stable for v0

## Must-Fix Compiler/Runtime Gaps
### Builtins + Stdlib
- [x] Implement runtime + codegen wiring for builtins: `parse_int`, `parse_float`, `read_file`, `write_file`
- [x] Decide stdlib packaging/search path (embedded, $WRELA_HOME, or project-relative)
  - v0 decision: project-relative only (modules resolved under `src/`); no bundled stdlib beyond builtins
- [x] Add tests that compile+run builtins end-to-end

### MIR Validation / Safety
- [ ] Expand MIR validation beyond “missing terminator”:
  - [x] use-before-def for temps/locals
  - [x] block arg/phi consistency
  - [x] suspendable flag correctness (await present => suspendable)
  - [x] ActorCall only on actor handles
  - [x] RC ops are well-formed (no obvious leaks/double-dec)
- [x] Make MIR validation a hard failure in CLI

### Actor + Method Dispatch Stability
- [x] Replace method name hashing with deterministic per-class method IDs
- [x] Ensure method IDs are stable across builds
- [x] Add actor dispatch tests (missing method, invalid args, await/fire paths)

### Error Recovery / Diagnostics
- [x] Remove “silent fallback” HIR lowering paths (panic/default placeholders)
- [x] Emit explicit diagnostics for malformed syntax constructs
- [x] Ensure parse/type errors always fail with non-zero exit codes

## Runtime Correctness / Stability
- [x] Verify refcount invariants under common flows (class/map/list/pending)
- [x] Validate crash behavior and result handling (err/otherwise/crash)
- [x] Add runtime tests for maps/lists/strings/actors/resolved pending

## Compiler UX / CLI
- [x] Make runtime build caching deterministic (avoid rebuilding per compile)
- [x] Improve error messaging for missing toolchains/linkers
- [x] Document CLI exit codes and error formats (pretty + JSON)

## Testing Matrix
- [x] End-to-end smoke tests for each v0 feature:
  - [x] classes + fields + methods
  - [x] lists/maps/strings
  - [x] numeric ops (int/float) + range
  - [x] short-circuit and/or
  - [x] actors (spawn/call/await/fire)
  - [x] Result + otherwise + crash
  - [x] builtins (parse/IO)
- [ ] Run tests for all supported platforms
  - Requires CI or per-platform runs (macOS/Linux x86_64 + aarch64)

## Release Checklist
- [x] Update docs to reflect actual state (remove outdated gap notes)
- [ ] Tag v0 feature set and mark deferred items
  - Manual release step
- [x] Create a minimal "getting started" example in `examples/`
- [x] Define a versioning policy for runtime ABI
