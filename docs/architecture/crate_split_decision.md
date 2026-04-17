# Crate Split Decision

Date: 2026-04-17
Phase: 53
Decision focus: evaluate the next bounded-context extraction candidate after the
first evidence-led split decision.

## Decision

Do not extract a new crate in Phase 53.

Record collision as the next extraction candidate, but defer the split until the
current query/substrate coupling and test ownership are inverted enough to buy
real isolation instead of directory churn.

## Evidence Snapshot

Measured on 2026-04-17 from a warm incremental `cargo check -p wrela --tests`
on the same git SHA:

- warm baseline after a priming run: `1.89s`
- presentation probe: touch `compiler/presentation_exec/wgsl/mod.rs`, then rerun
  `cargo check -p wrela --tests` -> `2.40s` (`+0.51s`, `1.27x`)
- collision probe: touch `compiler/collision_exec/gpu.rs`, then rerun
  `cargo check -p wrela --tests` -> `2.20s` (`+0.31s`, `1.16x`)
- substrate probe A: touch `compiler/gpu_runtime/layout.rs`, then rerun
  `cargo check -p wrela --tests` -> `2.19s` (`+0.30s`, `1.16x`)
- substrate probe B: touch `compiler/artifact_store/mod.rs`, then rerun
  `cargo check -p wrela --tests` -> `2.22s` (`+0.33s`, `1.17x`)

The post-Phase-53 module tree gives enough readability and coupling evidence to
compare the three required candidates alongside those timings:

| Candidate | Scope snapshot | Cross-context coupling | Test/bin surface pressure | Compile-burst evidence |
| --- | --- | --- | --- | --- |
| Presentation | `22` Rust files / `19,120` lines across `presentation_contract`, `presentation_binding`, `presentation_plan`, `presentation_exec` | `40` direct imports of query/substrate contexts from presentation files | `681` references under `compiler/tests` and `compiler/bin/wrela` | `2.40s` after touching `compiler/presentation_exec/wgsl/mod.rs` |
| Collision | `5` Rust files / `6,674` lines across `collision_contract`, `collision_plan`, `collision_exec` | `21` direct imports of query/substrate contexts from collision files | `236` references under `compiler/tests` and `compiler/bin/wrela` | `2.20s` after touching `compiler/collision_exec/gpu.rs` |
| Artifact/runtime substrate | `16` Rust files / `3,264` lines across compiler-side `artifact_*`, `gpu_runtime`, `world_identity`, and `time_semantics` | imported elsewhere `71` times across the compiler | `223` references under `compiler/tests` and `compiler/bin/wrela` | `2.19s` after touching `compiler/gpu_runtime/layout.rs`; `2.22s` after touching `compiler/artifact_store/mod.rs` |

The compile-burst cluster matters: none of the candidate touches creates a
step-change win by itself. All three stay within `+0.30s` to `+0.51s` of the
same warm baseline, so the next split must be justified by ownership readiness
and API shape, not by claiming a dramatic throughput win that the measurements
do not show.

## Candidate Evaluation

| Candidate | Compile isolation gain | API stability | Dependency inversion cost | Test migration complexity | Readability gain | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| Presentation | Medium-high | Medium-low | Very high | Very high | High | Not ready |
| Collision | Medium | Medium | Medium | Medium | Medium-high | Next candidate after blockers |
| Artifact/runtime substrate | Low | Low | Very high | High | Medium | Not ready |

## Why Presentation Is Not The Next Split

`compiler/presentation_exec/mod.rs` is now easier to navigate after the Phase 53
module breakup, but the context still reaches directly into `query_exec`,
`gpu_runtime`, artifact reuse, acceleration, and world-identity surfaces.
That coupling is visible in the current imports, and the surrounding proof
surface is still broad: preview, frame, presentation-debug, whole-frame
benchmarks, and a large CLI/test footprint all move together. The measured
touch probe is also the highest of the three candidates (`2.40s`, `+0.51s`
against the `1.89s` baseline), which suggests there is real coupling to untangle
first, but not yet a clean boundary that is ready to crystallize into a crate.

A presentation crate split today would create a large crate boundary without
first shrinking the number of concepts that have to cross it.

## Why Artifact/Runtime Substrate Is Not The Next Split

`runtime/` is already the separate runtime crate.
What remains on the compiler side is the shared substrate glue:
`artifact_contract`, `artifact_store`, `artifact_key`, `artifact_layout`,
`gpu_runtime`, `world_identity`, and `time_semantics`.

That surface is relatively small in raw line count, but it is imported across
the compiler `71` times. Its measured touch probes (`2.19s` and `2.22s`) are
not materially cheaper than the collision probe, which means a split would
mostly centralize high-churn shared types rather than isolate a bounded context
with a visibly better edit loop.

The next extraction should reduce coupling at the edges, not freeze a
still-moving shared core too early.

## Why Collision Is The Next Candidate

Collision is the smallest of the three evaluated contexts, and its public
surface is already comparatively crisp:

- `compiler/collision_contract`
- `compiler/collision_plan`
- `compiler/collision_exec/mod.rs`

That makes collision the best next candidate once the remaining inversions land.
It offers a meaningful readability win without the presentation context's huge
proof surface or the substrate context's repo-wide fan-out. Its measured touch
probe (`2.20s`, `+0.31s`) is not a dramatic throughput outlier, but it is the
smallest bounded context with the cleanest public nouns, which makes it the
best next candidate once the remaining shared seams are narrowed.

## Blockers Before A Collision Split Is Worth Doing

1. Stop making the extraction boundary reach into query/shared substrate details
   directly. Collision needs a smaller adapter seam for shared acceleration,
   query execution context, and artifact reuse behavior before it becomes a
   stable crate API.
2. Shrink the proof surface that depends on CLI ownership. The current
   `compiler/tests` and `compiler/bin/wrela` references are still broad enough
   that a split would mostly move files without changing the default workflow.
3. Re-run the compile-burst protocol with an explicit collision-edit slice after
   those inversions land, the same way Phase 52 used a measured baseline before
   making a crate decision.

## Follow-Through

Phase 53 therefore ends with a real decision record instead of a vague future
note:

- no new crate is extracted now
- collision is the next candidate
- the required dependency inversions are explicit and testable
