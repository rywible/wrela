# Acceleration Stack Playbook

This playbook is for the shared acceleration spine that powers both rendering and collision.
It is intentionally practical: use it when you want to inspect a plan, understand a report,
or reproduce a benchmark without guessing which subsystem owns the problem.

For the phase-48 production paths, pair this guide with:

- [GPU-Resident Framegraph Playbook](./gpu_resident_framegraph_playbook.md)
- [Collision GPU Batch Playbook](./collision_gpu_batch_playbook.md)

If you are working on the resident WGSL framegraph path or the GPU-batched collision path,
start with the specialized playbooks first:

- [GPU Resident Framegraph Playbook](./gpu_resident_framegraph_playbook.md)
- [Collision GPU Batch Playbook](./collision_gpu_batch_playbook.md)

## The Short Version

Treat the CPU path as the oracle. The acceleration stack is correct when the CPU-backed
reports and baselines say the world still means the same thing after a change.

Use the right command for the question:

- `cargo run -p wrela -- collision-plan`
- `cargo run -p wrela -- presentation-plan <path>`
- `cargo run -p wrela -- presentation-debug <path>`
- `cargo run -p wrela -- test --perf-debug <path>`
- `cargo run -p wrela -- perf benchmarks/realtime_presentation --profile=1080p120 --runs=1 --query-backend=cpu`
- `cargo run -p wrela -- perf benchmarks/collision_perf --profile=1080p120 --runs=1 --query-backend=cpu`
- `cargo run -p wrela -- perfcmp benchmarks/realtime_presentation --profile=standard --baseline-ref=origin/main --candidate-ref=HEAD`

If you are looking for the phase 41A closure diagnostic mode, use
`cargo run -p wrela -- perf <suite-root> --profile=1080p120 --why-not-120`.
That is the structured failure-analysis report for missed 120 FPS closure targets.
Use `presentation-debug` after that when you need pass-level attachments and rendered evidence.

For resident-path debugging, remember the split:

- this playbook: shared acceleration, cache, and oracle framing
- resident framegraph playbook: timed presentation WGSL lane rules
- collision GPU batch playbook: collision batching structure and certification behavior

For the resident framegraph and collision batch lanes, the key principle stays the same:
the CPU path is the oracle, while the WGSL path is the representative steady-state story.

## What Each Command Is For

`collision-plan` is the collision-side plan report. Use it to inspect the collision contract
surface and validation summaries before you go looking at runtime numbers.

`presentation-plan` is the rendering-side solver-plan report. Use it to inspect the compiled
presentation contracts and pass plan for a scene or fixture.

`presentation-debug` is the rendering diagnostic mode. Use it when you need pass-level
attachments and a concrete view of the frame pipeline rather than a summary alone.

`test --perf-debug` is the extra-report path. Use it when you want low-level perf counters
after a test or fixture run in addition to the higher-level plan and closure reports.

`perf` is the closure reproduction path. It writes the baseline JSON that contains the
closure report, plus the per-scenario `presentation_reports` or `collision_reports` when that
suite samples them. Use `--profile=1080p120` for the fixed 1920x1080, 120 FPS closure lane.

`perfcmp` is the paired regression check. Use it to compare the current branch against a known
reference and confirm that a change is either neutral or an intended improvement.

When a report looks suspicious, read the subsystem-specific playbook before guessing:

- [GPU Resident Framegraph Playbook](./gpu_resident_framegraph_playbook.md) explains resident scene cache behavior, attachment storage choices, legal timed-frame helpers, and frame export/debug safety.
- [Collision GPU Batch Playbook](./collision_gpu_batch_playbook.md) explains batch structure, candidate packing, observability, and CPU fallback for collision queries.

## Reading The Reports

When you inspect a plan or perf report, look for the same ideas in both rendering and collision:

- The contract or plan should tell you what backend, guarantee class, or witness path is in play.
- The debug dump should tell you which forest, node, cache, or rejection class was chosen.
- The closure report should tell you whether the sampled lane met its budget and, if not, which
  pass or metric dominated the failure.
- The benchmark JSON should tell you whether the scene was sampled as a rendering lane,
  a collision lane, or both.

For the forest/report-style dumps, read them as deterministic evidence, not as loose logs. A
good dump should let you answer questions like:

- Which forest or cache was built?
- Which nodes were accepted, rejected, or reused?
- Which pass or bottleneck dominated the sampled frame?
- Did the run stay on the CPU oracle path, or did it rely on a backend-specific shortcut?

## What "Do Not Change Without Oracle Parity" Means

This phrase is a guardrail, not a slogan.

Before changing acceleration contracts, report shape, solver selection, or closure thresholds,
re-run the same workload on the CPU backend and compare it to the current baseline. If the new
result is different, ask whether that difference is:

1. an intended semantic change that has been covered by tests and updated baselines, or
2. a regression that only looked like a performance win.

In practice, that means:

- Keep the CPU execution path green and use it as the source of truth.
- Compare `perf` and `perfcmp` output before and after the change.
- Update tests when the meaning changes intentionally.
- Do not relax budgets, tweak plan output, or hide a rejection class just to make a benchmark
  look better unless the CPU oracle and the relevant regression tests agree.

If a change only improves the GPU or WGSL path but fails CPU parity, it is not ready.

## A Safe Workflow

1. Start with `collision-plan` or `presentation-plan` so you know what the stack believes.
2. Use `presentation-debug` when you need richer rendering diagnostics.
3. Reproduce the workload with `perf` on the canonical benchmark suite.
4. Compare the result with `perfcmp` against a known reference.
5. Only then decide whether the change is a real improvement or a semantic drift.

That sequence keeps rendering and collision aligned instead of letting one subsystem drift
away from the shared world model.
