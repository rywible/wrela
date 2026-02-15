# Wrela-v2 Self-Hosted Native Toolchain Plan (No-Cheating, Linux-First Production Architecture)

## Summary

Build a pure `.wr` v2 toolchain that emits native binaries and owns linking/runtime behavior, while freezing Rust V1 as fallback root-of-trust only. We cut over on `Darwin arm64`, but runtime semantics are defined by Linux production targets (`linux x86_64`, `linux arm64`). Darwin is a fast dev lane and compatibility lane, not the canonical runtime model.

## Progress Snapshot (February 15, 2026)

- Completed: M0–M10 cutover baseline, including strict v2-default launcher on Darwin arm64.
- Completed: M10-B cutover hardening, governance/purity coherence, readiness lane passing.
- Completed: v2 check/test/build certified lanes green with current contracts.
- Completed: platform adapter identity/ops surface for darwin+kqueue, linux+io_uring, linux+epoll.
- Next: M11 runtime core realization with Linux semantics as canonical behavior.

## Non-Negotiables

- No cheating.
- No shortcuts.
- No hidden Rust dependency in v2 execution path.
- No temporary delegation from v2 compiler/runtime/linker to V1 for production code paths.
- No parity bypass flags that skip contract behavior.
- No fake or stubbed success paths for cert, diagnostics, linker, or runtime behavior.
- `language/spec/spec.wr` is sacred and untouched.

## Anti-Cheating Enforcement

1. `v2-purity` gate:
   - Fails if `wrela-v2` contains non-`.wr` implementation code.
   - Fails if v2 compile/build path shells out to `cargo`, `rustc`, or V1 `wrela`.
2. `v2-provenance` gate:
   - Build artifacts embed v2 toolchain provenance stamp.
   - CI/readiness verifies artifacts were produced by v2 path.
3. `no-bypass` gate:
   - Fails if any v2 path conditionally disables contract/cert/parity logic.
4. `platform-boundary` gate:
   - Fails if host/runtime primitives leak outside approved boundary modules.
5. Shadow parity lane:
   - V2 and frozen V1 both run; parity diffs remain explicit and tracked.

## Locked Decisions

- Entire v2 toolchain implementation is pure `.wr`.
- 3-stage bootstrap remains: Stage0 frozen V1, Stage1 self-host, Stage2 reproducible self-host.
- Full CLI parity required at cutover and maintained post-cutover.
- Native binaries required.
- In-house linker from day one.
- M10 cutover remains host-only (`Darwin arm64`) for developer velocity.
- Runtime semantics are Linux-first post-cutover.
- Linux Docker lanes are authoritative runtime gates post-M10.
- Darwin syscall work is out of scope; Darwin remains libc-backed.
- Linux syscall substrate is a planned optimization lane (not day-1 requirement).
- ABI policy: Phase 0 additive ABI work, then hard freeze.

## Linux Syscall Boundary Policy (Locked)

- Linux is the canonical low-level runtime target; macOS is a compatibility/dev adapter target over the same contract.
- We keep a small Linux syscall boundary and build everything else in `.wr` above it.
- The syscall boundary is contract-first:
  - `fd/path`: `openat`, `read`, `write`, `close`, metadata/path mutation family
  - `memory`: `mmap`, `munmap`, `mprotect`
  - `time`: `clock_gettime`, `nanosleep`
  - `process`: spawn/exec/wait/exit primitives
  - `sync`: futex-style wait/wake primitives
  - `reactor`: `io_uring` primary, `epoll` fallback
- Darwin must implement the same runtime contract shape through libc/POSIX-backed adapters; no direct Darwin syscall optimization lane is planned.
- Core/app code must never import host primitives directly; only platform contracts/adapters may touch OS-facing calls.

## Public Interface and Type Changes

1. Phase 0 additive thin-core intrinsics (frozen):
   - Filesystem/metadata/rename/remove/mkdir/chmod primitives.
   - Process primitives (`argv`, `cwd`, `run`, `exit`).
2. Runtime platform abstraction contracts (v2):
   - `ReactorPort`
   - `ClockPort`
   - `FsPort`
   - `ProcessPort`
   - `NetPort` where required
3. Platform adapters:
   - `darwin_kqueue_adapter` (dev/compat lane)
   - `linux_io_uring_adapter` (primary Linux runtime)
   - `linux_epoll_adapter` (required Linux fallback)
4. No public CLI surface drift without explicit roadmap phase.

## Implementation Plan

1. M0: Baseline + Freeze V1
   - Tag stage0 baseline and lock V1 to bug/security/fallback fixes only.
2. M1: Phase 0 ABI Additions + Freeze
   - Add minimal intrinsic pack needed for pure `.wr` parity.
   - Snapshot once, freeze ABI/symbol surface.
3. M2: Platform Abstraction Layer in v2
   - Define OS-neutral runtime/service interfaces.
   - Ban OS-specific calls outside adapters/composition boundaries.
4. M3: v2 Frontend Parity
   - Lexer/parser/semantic/type/naming/diagnostics in `.wr`.
5. M4: v2 MIR + Optimization Parity
   - MIR lowering/validation/rewrite/check-oracle behavior.
6. M5: Native Backend + In-House Linker (Darwin arm64)
   - Object emission, relocation, executable link/write in `.wr`.
7. M6: Runtime Thin-Core Parity via Abstraction
   - Adapter identity/ops model, composition wiring, contract tests.
8. M7: Full CLI Parity
   - `init/update/check/build/compile/verify-cert/run/dev/test/perf/perfcmp/matrix/parity`.
9. M8: Certification + Determinism + Perf Parity
   - Cert schema, deterministic outputs, perf gates/matrix artifacts.
10. M9: Self-Hosting + Reproducibility
   - Stage0/Stage1/Stage2 with reproducibility gate.
11. M10: Cutover (Host-Only)
   - Default toolchain switched to v2 on Darwin arm64.
   - V1 fallback kept explicit/env-gated only.
12. M11: Runtime Core Port in Pure `.wr` (Linux-Semantics Canon)
   - Define runtime core behavior semantics against Linux contracts first.
   - Route runtime operations through core/runtime modules, not ad-hoc host calls.
   - Keep Darwin libc-backed compatibility behavior.
13. M12: Linux Runtime Bring-Up via Local Docker Engine (Authoritative)
   - Required Docker lanes for `linux x86_64` and `linux arm64`.
   - `io_uring` preferred path + deterministic `epoll` fallback.
   - Linux runtime parity/conformance is required before phase completion.
14. M13: Linux Syscall Substrate Phase (No-libc Fast Path, Linux-only)
   - Introduce Linux-only syscall substrate contracts under existing runtime abstractions.
   - Keep libc fallback while incrementally promoting syscall implementations.
15. M14: Linux Full-Stack Runtime Optimization + Syscall Promotion
   - Optimize allocator/reactor/dispatch/IO hot paths on Linux syscall backend.
   - Promote syscall backend where correctness + deterministic perf gates pass.
   - Keep libc fallback for safety/debug.

## Test Cases and Scenarios

1. Anti-cheating tests:
   - Detect forbidden subprocess delegation to Rust/V1.
   - Detect bypass toggles for parity/cert logic.
2. Abstraction conformance tests:
   - Same runtime contract suite across Darwin, Linux io_uring, Linux epoll.
3. Full contract parity tests:
   - CLI/help/exit codes/diagnostics/cert schema/public-surface rules.
4. Backend/linker tests:
   - Native executable generation, relocation correctness, runtime symbol resolution.
5. Self-host tests:
   - Stage0/Stage1/Stage2 reproducibility pipeline.
6. Linux authoritative runtime tests (post-M10):
   - Docker lanes required for `linux x86_64` and `linux arm64`.
   - `io_uring` preferred path and `epoll` fallback conformance.
7. Darwin compatibility runtime tests:
   - libc-backed behavior parity against Linux contract expectations where applicable.
8. Linux syscall parity tests (M13/M14):
   - libc vs syscall backend correctness and deterministic perf gating.

## Assumptions and Defaults

- Development host remains `Darwin arm64`.
- Production targets are Linux (`x86_64`, `arm64`).
- Linux `io_uring` is primary but not exclusive; `epoll` fallback is required until explicitly deprecated.
- Darwin remains libc-backed; no direct-syscall optimization phase is planned for macOS.
- Linux runtime validation is executed locally via Docker before dedicated Linux-host promotion.
- Runtime order is locked post-M10: Linux semantics first, then Linux runtime bring-up, then Linux syscall promotion.
- No modifications to `language/spec/spec.wr`.
- Any ABI changes after Phase 0 require an explicit approved migration phase.
