# GPU-Resident Framegraph Playbook

This is the production-path playbook for the timed WGSL presentation lane.

From the repo root, use `just perf-closure` for the canonical whole-frame closure workflow.
The raw `cargo run -p wrela -- ...` commands below are the underlying `wrela` invocations when
you need resident-lane diagnostics directly.

Use it when you need to answer one of these questions:

- Is this change still on the resident framegraph path?
- Which GPU uploads or readbacks are still legal in the measured lane?
- Why did the `1080p120` closure report fail?
- Which `1080p120` command is the fast canonical measurement lane versus the deeper diagnostic lane?
- How do I debug or export a frame without polluting the timed path?

## The Model

The resident framegraph is the steady-state WGSL execution story for presentation:

1. snapshot-scoped scene and acceleration payloads upload on snapshot/layout changes
2. presentation attachments stay GPU-resident between passes
3. the timed frame records a small number of GPU passes and queue submits
4. CPU materialization is reserved for explicit export, history reconstruction, oracle checks, or debug

The CPU backend is still the semantic oracle. The resident lane is the performance path, not the truth source.

## Measurement Versus Diagnostics

Use the default closure command first:

- `cargo run -p wrela -- perf benchmarks/realtime_presentation --profile=1080p120`

That is the canonical measurement lane. It should run exactly one resident WGSL presentation report per closure scenario, emit steady-state FPS for each scenario, and report the suite median FPS without paying for workgroup sweeps or hybrid-vs-dense-only A/B comparisons.

Escalate only when you need more evidence:

- `cargo run -p wrela -- perf benchmarks/realtime_presentation --profile=1080p120 --why-not-120`

That keeps the same closure verdict but also turns on the expensive diagnostic payloads:

- WGSL workgroup-size sweep
- hybrid-vs-dense-only comparison

If the default measurement lane cannot finish or cannot emit a report for every closure scenario, treat that as a benchmark-collection failure, not as a normal “too slow” result.

## Resident Scene Cache

The resident scene cache is keyed by snapshot identity, detail, layout identity, and selection signature.

That means a cache hit is only correct when all of these still match:

- the captured world/snapshot
- the detail level used to lower the query
- the GPU layout/features used to compile the pipeline
- the selection or root-shape scope used to build the resident scene payload

If closure reports show `scene_reupload_bytes > 0` in the measured lane, treat that as a real execution-model regression unless the snapshot truly changed.

## Attachment Storage Policy

Attachments should be chosen for how later passes consume them:

- Structured semantic payloads such as hits, surfaces, radiance, and media should stay in storage buffers.
- Color/history buffers may justify texture-backed storage only when later sampling/presentation behavior needs it.
- Precision changes are policy decisions, not silent optimizations. If `shader_f16` is enabled, parity tests must say what tolerance is allowed.

In the timed WGSL lane, attachment decode/encode counts should stay at zero. A nonzero `attachment_cpu_bounce_count` means the frame is still using CPU-owned pass glue.

## Legal And Illegal Helpers

Timed resident frames:

- Legal: explicit framegraph readback scheduling for timestamps and explicit attachment exports
- Legal: untimed CPU materialization after the measured frame when building history/debug output
- Illegal: convenience helpers that submit and immediately read back a storage buffer as part of the steady-state pass path
- Illegal: CPU attachment decode/encode glue between WGSL passes
- Illegal: reintroducing CPU screen-sample allocation into the primary visibility hot path

The legacy CPU-bounce WGSL helpers are kept only for tests and verification scaffolding. They must not become the path of least resistance for production passes.

## Reading Closure Output

For either closure command, read the execution-model section first. Use `--why-not-120` only when you need the additional diagnostic evidence:

- `hot_path_readback_bytes`: timed readback after subtracting timestamp traffic
- `scene_reupload_bytes`: resident scene payload churn inside the measured loop
- `cpu_screen_sample_allocations`: CPU-owned primary setup still happening
- `attachment_cpu_bounce_count`: decode + encode count for CPU attachment glue
- `queue_submit_count`: how fragmented the frame recording still is
- `primary_visibility_dispatch_count`: whether primary visibility still fans out too aggressively
- `timestamps_supported` / `timestamped_pass_count`: whether GPU timing is actually active when the adapter supports it

If these gates fail, fix them before celebrating shader-level wins.

## Safe Debug And Export Flow

Debug and export are allowed, but they must be explicit side lanes:

1. run the timed lane first
2. schedule explicit attachment exports or untimed readbacks afterward
3. use `presentation-debug` when you need pass-level evidence
4. keep benchmark/closure paths free of accidental debug materialization

If you need to compare WGSL output against CPU output, do that in tests or debug commands, not in the measured frame loop.

## CPU Fallback

Use CPU fallback when:

- parity is in doubt
- optional GPU features are unavailable
- you are building oracle evidence for a semantic change
- a debug/export workflow genuinely needs CPU-owned values

Do not use CPU fallback to paper over a resident-path regression. If the resident lane is broken, the fix is to restore the resident contract, then re-run CPU parity.
