# Wrela-On-Wrela Master Plan

Date: 2026-02-17
Owner: compiler self-host effort
Scope root: `/Users/ryanwible/projects/wrela/wrela-on-wrela`

## 1. Mission

Build a fully new compiler implementation in Wrela under `wrela-on-wrela/` that reaches semantic parity with the current Rust compiler for spec + app workloads, with native code generation in the same pass.

## 2. Locked Decisions

- No vertical-slice shortcut; full compiler replacement effort.
- Language/runtime surface stays frozen to `/Users/ryanwible/projects/wrela/language/spec/spec.wr` except genuine bug fixes.
- v1 command scope: `check`, `build`, `run`, `test`.
- Diagnostic parity target: semantic equivalence with useful spans (not byte-identical output).
- Backend target: direct native codegen.
- Platform target (v1): `macOS arm64` only.
- Artifact strategy: emit object files and invoke system linker.
- Runtime boundary: link to existing Rust runtime exports.
- Optimization strategy (v1): correctness-first, minimal/no optimization.
- Bootstrap validation strategy: 3-stage (stage0 -> stage1 -> stage2).
- Architecture style for this project: compiler-centric modules (`compiler/*`, backend/*), no forced DDD namespace structure.

## 3. Definition of Done

Project is complete when all of the following are true:

1. `wrela-on-wrela` can compile and run the language spec (`language/spec/spec.wr`) end-to-end.
2. `wrela-on-wrela` can build/run selected app workloads with semantic parity.
3. Native backend produces valid arm64 Mach-O object files linkable by system linker.
4. Generated binaries execute with runtime export integration parity.
5. `check/build/run/test` workflows function through Wrela implementation (no Rust compiler dependency for normal path).
6. Stage1 and Stage2 bootstrap outputs are behaviorally stable (no drift).
7. Parity tracking in this file is updated with evidence paths for each subsystem.

## 4. Current State (as of this update)

Completed:

- `wrela-on-wrela` folder scaffold exists and is wired.
- Core source tree and interfaces are created.
- Phase-0 audit content exists in this file.
- Entrypoint scaffold currently passes:
  - `cargo run -q -p wrela -- check wrela-on-wrela/src/main.wr`
  - `cargo run -q -p wrela -- build wrela-on-wrela/src/main.wr`

Not completed:

- Real lexer/parser/AST/semantic behavior.
- Real typed IR lowering parity.
- Real AArch64 instruction encoding.
- Real Mach-O serialization.
- Real linker invocation logic.
- Bootstrap harness and stage evidence.
- Spec/app parity beyond smoke tests.

## 5. Canonical Deliverables

Required docs:

- `docs/master-plan.md` (this file, canonical execution plan and checklist)

Required source contracts:

- `src/compiler/api.wr` (compiler entry contract)
- `src/compiler/diag/types.wr` (diagnostic shape)
- `src/compiler/ir/types.wr` (mid-end IR shape)
- `src/compiler/codegen/macho/types.wr` (object module shape)

Required tests:

- `tests/spec/*`
- `tests/apps/*`

## 6. Implementation Milestones

### Milestone A: Frontend Foundation

Goal: replace placeholder frontend with real language parsing pipeline.

Deliver:
- `compiler/lexer/mod.wr`: tokenization parity for spec syntax.
- `compiler/parser/mod.wr`: parse module/function/type/control-flow forms.
- `compiler/ast/mod.wr`: top-level and structural validation.

Exit criteria:
- Representative spec snippets tokenize+parse successfully.
- Negative parser tests produce expected failure classes.

### Milestone B: Semantic + Project Graph Parity

Goal: build loader, name resolution, type system enforcement.

Deliver:
- module resolution (`.` and `/` normalization)
- duplicate/visibility/import checks
- entrypoint rules (`run` ownership)
- type checking parity for core spec constructs
- check/purity constraints parity

Exit criteria:
- Semantic negative suite catches expected failure categories.
- Multi-module project behavior aligns with current compiler expectations.

### Milestone C: Mid-end IR + Validation

Goal: deterministic, validated typed IR for backend handoff.

Deliver:
- robust `ModuleIr`/`FunctionIr` instruction lowering
- IR invariant validator
- deterministic ordering and stable IDs

Exit criteria:
- repeated lowering of same input produces deterministic IR snapshot.
- backend can consume IR without ad hoc assumptions.

### Milestone D: Native Backend (AArch64 + Mach-O)

Goal: generate valid native objects from IR.

Deliver:
- instruction selection and encoding for initial supported op subset
- stack frame and calling convention implementation
- symbol + relocation generation
- Mach-O object writing

Exit criteria:
- emitted `.o` links successfully via system linker.
- linked binary executes representative corpus correctly.

### Milestone E: Link + Runtime Integration

Goal: reliable executable generation and runtime bridge correctness.

Deliver:
- linker invocation orchestration
- runtime export binding per thin-core snapshot
- deterministic link command/profile capture

Exit criteria:
- generated binaries run `run/test` paths.
- runtime intrinsic behavior matches baseline expectations.

### Milestone F: Bootstrap + Parity Closure

Goal: complete self-host credibility and parity evidence.

Deliver:
- stage0/stage1/stage2 harness scripts
- behavior-diff evidence for stage1 vs stage2
- spec parity and selected app parity evidence

Exit criteria:
- stage pipeline stable and reproducible.
- parity tracking rows are marked implemented with evidence.

## 7. Risk Register

1. Compiler complexity (~60k LOC baseline) creates hidden coupling.
2. Native backend correctness on first target is high-risk (ABI + relocations).
3. Runtime boundary drift risk against thin-core export contract.
4. Determinism regressions can silently undermine bootstrap confidence.
5. Naming/type checker strictness in Wrela can invalidate otherwise reasonable scaffolding quickly.

Mitigation strategy:
- keep contracts explicit and narrow
- land invariants before optimization
- gate milestones with concrete executable checks
- continuously update parity evidence in this file

## 8. Execution Rules

- Do not edit `/Users/ryanwible/projects/wrela/language/spec/spec.wr` unless explicitly handling a verified compiler bug mismatch.
- Keep new Wrela modules under `wrela-on-wrela/src/compiler/*` unless we intentionally reshape the architecture.
- Keep AGENTS notes updated with newly discovered Wrela authoring constraints.
- Treat `docs/master-plan.md` as canonical planning reference; update this file when scope/status changes.

## 9. Immediate Next Actions

1. Implement real lexer in `src/compiler/lexer/mod.wr`.
2. Implement parser in `src/compiler/parser/mod.wr`.
3. Replace semantic placeholder in `src/compiler/semantic/mod.wr`.
4. Expand spec/app test suites from smoke into behavior-focused cases.
5. Update the parity/status sections in this file after each subsystem lands.

## 10. Parity Tracking

| Area | Rust Baseline Source | Wrela Replacement | Current Status | Evidence Path |
|---|---|---|---|---|
| CLI dispatch (`check/build/run/test`) | `compiler/bin/wrela/cli_args.rs`, `compiler/bin/wrela/command_handlers.rs` | `wrela-on-wrela/src/compiler/cli/*.wr` | Scaffolded | `wrela-on-wrela/src/compiler/cli/driver.wr` |
| Compile API contract | N/A (new stable contract) | `wrela-on-wrela/src/compiler/api.wr` | Scaffolded | `wrela-on-wrela/src/compiler/api.wr` |
| Diagnostic model | `compiler/diag/*` | `wrela-on-wrela/src/compiler/diag/types.wr` | Scaffolded | `wrela-on-wrela/src/compiler/diag/types.wr` |
| Lexer parity | `compiler/lexer/*` | `wrela-on-wrela/src/compiler/lexer/mod.wr` | Not implemented | `wrela-on-wrela/src/compiler/lexer/mod.wr` |
| Parser parity | `compiler/parser/*` | `wrela-on-wrela/src/compiler/parser/mod.wr` | Not implemented | `wrela-on-wrela/src/compiler/parser/mod.wr` |
| AST validation parity | `compiler/parser/validate.rs` | `wrela-on-wrela/src/compiler/ast/mod.wr` | Not implemented | `wrela-on-wrela/src/compiler/ast/mod.wr` |
| Project/module graph | `compiler/hir/project.rs` | `wrela-on-wrela/src/compiler/semantic/mod.wr` | Seed implementation | `wrela-on-wrela/src/compiler/semantic/mod.wr` |
| Type checker parity | `compiler/hir/typeck.rs` | `wrela-on-wrela/src/compiler/semantic/*` | Not implemented | `wrela-on-wrela/src/compiler/semantic/mod.wr` |
| Mid-end IR contract | `compiler/mir/*` | `wrela-on-wrela/src/compiler/ir/types.wr` | Defined | `wrela-on-wrela/src/compiler/ir/types.wr` |
| AArch64 codegen | `compiler/backend/cranelift.rs` | `wrela-on-wrela/src/compiler/codegen/aarch64/mod.wr` | Placeholder | `wrela-on-wrela/src/compiler/codegen/aarch64/mod.wr` |
| Mach-O writer | `compiler/backend/cranelift.rs` | `wrela-on-wrela/src/compiler/codegen/macho/*.wr` | Placeholder | `wrela-on-wrela/src/compiler/codegen/macho/mod.wr` |
| Link orchestration | Rust command handlers + linker shelling | `wrela-on-wrela/src/compiler/link/mod.wr` | Placeholder | `wrela-on-wrela/src/compiler/link/mod.wr` |
| Spec lane smoke | `language/spec/spec.wr` | `wrela-on-wrela/tests/spec/*.wr` | Added smoke test | `wrela-on-wrela/tests/spec/self_host_spec_smoke_test.wr` |
| App lane smoke | `apps/ledger-lite` | `wrela-on-wrela/tests/apps/*.wr` | Added smoke test | `wrela-on-wrela/tests/apps/self_host_apps_smoke_test.wr` |

## 11. Subsystem Audit Summary

- Baseline compiler size is about 60k LOC under `compiler/**/*.rs`; complexity concentration is highest in command handlers, type checking, MIR optimization/lowering, backend, and project loader.
- Runnable language contract: `language/spec/spec.wr`.
- Runtime/export contract anchor: `language/spec/thin_core_snapshot.txt`.

Replacement mapping:

- CLI and command routing:
  - baseline: `compiler/bin/wrela.rs`, `compiler/bin/wrela/cli_args.rs`, `compiler/bin/wrela/command_handlers.rs`
  - replacement: `wrela-on-wrela/src/compiler/cli/args.wr`, `wrela-on-wrela/src/compiler/cli/driver.wr`, `wrela-on-wrela/src/compiler/api.wr`
- Lexer:
  - baseline: `compiler/lexer/*`
  - replacement: `wrela-on-wrela/src/compiler/lexer/mod.wr`
- Parser + AST validation:
  - baseline: `compiler/parser/*`
  - replacement: `wrela-on-wrela/src/compiler/parser/mod.wr`, `wrela-on-wrela/src/compiler/ast/mod.wr`
- Project graph + semantic/type checking:
  - baseline: `compiler/hir/project.rs`, `compiler/hir/typeck.rs`, `compiler/hir/semantic.rs`, `compiler/hir/checkir.rs`
  - replacement: `wrela-on-wrela/src/compiler/semantic/*`
- Mid-end IR:
  - baseline: `compiler/mir/*`
  - replacement: `wrela-on-wrela/src/compiler/ir/types.wr` (+ future lowering/validation modules)
- Native backend:
  - baseline: `compiler/backend/cranelift.rs`
  - replacement: `wrela-on-wrela/src/compiler/codegen/aarch64/mod.wr`, `wrela-on-wrela/src/compiler/codegen/macho/mod.wr`, `wrela-on-wrela/src/compiler/codegen/macho/types.wr`
- Link/runtime boundary:
  - replacement: `wrela-on-wrela/src/compiler/link/mod.wr`, binding to runtime exports from `thin_core_snapshot.txt`

## 12. Native Backend Contract (v1)

Scope:

- Target triple: `aarch64-apple-darwin`.
- Output: Mach-O object files (`.o`).
- Linking: system linker (external ownership in v1).
- Runtime boundary: existing Rust runtime exports.
- Optimization: correctness-first, minimal optimization.

Pipeline:

1. Typed IR input (`ModuleIr`, `FunctionIr`, `InstructionIr`).
2. Instruction selection and frame/calling-convention lowering.
3. Symbol and relocation recording.
4. Mach-O section/symbol/relocation serialization.
5. System linker invocation.

Verification:

1. Instruction encoding unit tests.
2. Relocation generation unit tests.
3. Link-and-run integration tests for representative programs.
4. Differential behavior checks against Rust compiler outputs.
