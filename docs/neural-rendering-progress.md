# Neural rendering progress

## Operating constraints

- User authorized autonomous pursuit of `neural-rendering-goals.md` on 2026-10-26.
- AMD Pop!_OS only for builds, training, rendering, and measurements. The Mac is occupied.
- Work alone: no subagents. No paid compute/hardware purchases.
- Remote host: `wrela-popos` over Tailscale SSH.
- Worktree: `/home/ryanwible/worktrees/wrela-neural-frontier`.
- Branch: `codex/neural-rendering-frontier`.
- Source branch: `impl-world-class-v2`, commit `a07cf136494860f20a2590cacf0851e17f41b585`.
- The initial working state also carries the Mac's uncommitted `docs/bench.md`, `wrela.toml`,
  `docs/neural-rendering-goals.md`, and `docs/neural-rendering-research.md` as a snapshot.
- Existing remote checkout and Mac checkout remain on their original branches.
- Pin: Wrela 0.9.0, compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
- All seven goals are active; none is achieved. Unsupported vendor SDKs and unavailable human
  judgments remain unverified, not successful. Do not claim global leadership without evidence.

## Immediate milestone

A synchronized linear-HDR instrument, real clearing capture and exact analytic motion reference
now run on the hardware AMD GPU. The first RGB residual model has been trained, evaluated
recurrently and rejected. Improve field/coverage information and real-scene reference quality
before promoting any reconstruction change. Keep capture time fixed while spatial
samples accumulate; advance simulation only between output frames. Do not reuse the existing
post-developed RGBA8 average as radiometric truth.

## Evidence recorded so far

SSH works. The source commit and uncommitted documents were transferred using an incremental Git
bundle and an explicit four-file archive. The worktree is based on the exact source commit.
`vulkaninfo --summary` reports hardware AMD RADV RENOIR and a separate llvmpipe software adapter;
all GPU evidence must name and use the hardware adapter. The pinned toolchain downloaded and
reported 0.9.0 successfully.


### First implementation checkpoint

- `engine::supersample::Reference` accumulates scene-linear RGB in f32, rejects dimensions and
  scene-moment mismatches, snapshots a float buffer, and resets without retaining old samples.
  Alpha is forced to 1 because scene alpha holds material tags. This guard requires the caller
  to supply an honest moment; it does not freeze arbitrary scene state automatically.
- `engine/tests/supersample` checks exact analytic edge coverage, radiance above 1, small samples
  following a 4096-valued sample, snapshot isolation, reset and rejected samples. Native GPU and
  Chrome pass. Deliberately clipping RGB to 1 made the intended 4-valued pixel fail; restored
  code passes. The tool's decode/metric test rejects NaN, wrong dimensions and material alpha.
- Found and fixed an existing F12 error: entering reference mode enabled history. The new
  `engine/tests/frame-reference` failed on that behavior, then passed on native and Chrome
  after disabling history. Both original history settings are restored on exit.
- `wrela tool reconstruct -- --width 192 --height 108 --warmup 240 --cells 4` builds the bare
  ground/sky scene and produces raw little-endian RGBA f32 captures, CSV scores and capture
  metadata. Simulation time and camera stay fixed; caches warm independently for each load.
  The reference reads scene HDR before temporal output/develop; candidates read the temporal
  output before develop. Production quality is shared except render scale and history.
- This first smoke run used the Radeon Vulkan driver explicitly through
  `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json`. No RTX or Mac work was involved.
  Its raw artifacts, built fixture and SHA-256 provenance are preserved outside tracked source
  in `build/neural-evidence/initial-hdr-192x108-16-64/` on Pop!_OS.

The first 16-versus-64 spatial-sample check had raw RGB MSE 0.0000002828 and diagnostic-view
MSE 0.0000000494. Two separate 64-sample loads were byte-identical. Against the 64-sample mean:

| Existing temporal path | Raw RGB MSE | Fixed-view PSNR dB |
|---|---:|---:|
| 1× | 0.0001343105 | 46.6860 |
| 2× | 0.0007972659 | 38.5476 |
| 3× | 0.0015658517 | 35.5980 |
| 4× | 0.0026618024 | 32.6094 |

Fixed view means `max(x,0)/(1+max(x,0))` at exposure 1, without gamma. These are diagnostic
quality numbers, not timings or FLIP/LPIPS measurements. The initial CSV prints an artificial
300 dB floor for the identical reference; the tool now writes `inf` for exact equality.
The first capture has **no trained model**, moving objects, foliage, external scene, sealed
split or human judgment. Numerical convergence does not prove unbiased lighting. No G1–G7
acceptance gate has been met. This tiny, predominantly sky image is unsuitable as a frontier
training or validation set by itself.

Next: inspect higher-sample convergence, add pinned FLIP evaluation with its viewing conditions,
expand field/detail/motion scenes and data provenance, then train against the reproducible
baseline. Preserve the development/validation/sealed distinction throughout the hill climb.


### Perceptual reference check (same AMD host)

Installed upstream `flip-evaluator==1.7` and `numpy==2.2.6` in a dedicated evaluation environment
outside the source worktree. Evaluation is LDR-FLIP at **67 pixels/degree**, on float RGB after
`max(x,0)/(1+max(x,0))`, `inputsRGB=false`; no PNG quantization or extra gamma. It runs on the
Pop!_OS CPU. The GPU renders every source image. External utility:
`/home/ryanwible/.local/state/wrela-neural-frontier/evaluate_flip.py`. Each `flip.json` embeds its
complete evaluator source, source SHA-256, package versions, image hashes and named conditions.

The 16→64 check's mean FLIP was **0.005671**, and 64→256 was **0.005428**: both exceed the
0.003 reference discrepancy bound implied by a 0.03 candidate mean-error budget. The latter
was dominated by the horizon row (54). Do not mistake the small MSE for perceptual convergence.
The 256→1024 check fell to **0.001369** (upper/lower halves 0.000258/0.002480). Its repeat was
byte-identical. This passes the necessary mean-discrepancy check for this one development view;
independent randomized sample sets, semantic masks and unbiased lighting remain unresolved.

Against the 1024-sample reference, the existing temporal paths scored:

| Scale | Mean FLIP | Pixel p99 FLIP | Fixed-view PSNR dB |
|---|---:|---:|---:|
| 1× | 0.017349 | 0.058823 | 47.9563 |
| 2× | 0.043401 | 0.181581 | 38.6891 |
| 3× | 0.055001 | 0.221766 | 35.5970 |
| 4× | 0.070001 | 0.334024 | 32.5630 |

These are image diagnostics at **192×108**, not 1080p/4K goal scores, confidence intervals or a
ranking against other products. They expose room to improve even before richer field scenes.
The two additional runs, each with its own built fixture and raw data, are:
`build/reconstruct/runs/1793054860-492654431` (64/256) and
`build/reconstruct/runs/1793055065-64207453` (256/1024). The tool now creates a unique timestamped
run directory rather than overwriting outputs. Reproduce with `--cells 8` and `--cells 16`,
respectively, keeping the earlier dimensions and warmup; run the pinned evaluator on that
new directory. The original run's exact source build is retained in its evidence folder.

A second history regression was found: the clearing reapplies its history preference every
frame after handling F12. `Frame::on(History)` now suppresses it throughout reference mode.
The regression test deliberately reapplies history, failed before the guard, and passes in
native and Chrome after it. Entry preferences still restore correctly on exit.

Focused checks pass: HDR accumulation (both hosts), reference mode transitions (both hosts),
float capture decoding/metrics, fixture/tool type checks, changed-file formatting, and workspace
allowlist. Whole-workspace `wrela suite fmt` reports one pre-existing unformatted file:
`examples/clearing/tests/checks/tests.wrela`, untouched by this work. Full suite not run yet;
required before the scope's PR. Inherited `docs/bench.md` edits remain uncommitted and unaltered.


### Rich field scene and independent motion reference

The capture tool now accepts `--scene bare|clearing|edges --shot N`. The shared
`studio/reconstruct/fixture/session.wrela` owns accumulation, while each fixture owns its
simulation state. Refactoring the bare fixture preserved all seven images byte-for-byte
(run `1793055528-841559011` versus the initial evidence). The clearing uses its actual
terrain, trees, stones, grass and field-realized fawn, held at tick 0 without starting its
simulation. It waits for probes, sky and the fawn before capture. Its three shots are the
clearing's existing shots. Production scale also changes its production LOD policy.

The clearing's shot 0 at 192×108, 240 warmup frames, still does **not** have a sufficiently
converged reference. Mean FLIP is 0.019003 for 16→64 spatial samples and 0.009981 for
256→1024. Both exceed 0.003. The latter's reference repeats byte-for-byte, but reproducibility
does not establish accuracy. Its provisional 1×/2×/3×/4× mean FLIP is
0.071734/0.095468/0.141504/0.182689. Do not train against this as certified truth.
Runs: `1793055673-957151421` and `1793056200-382945725`.

The analytic fixture drives the production temporal resolver, neighbourhoods, depth, motion,
jitter and history on moving thin slats over a checkerboard. Eight cases vary thickness,
period, HDR foreground colour and motion, including one stationary case. Its reference is the
exact linear-RGB pixel-area integral; it uses no history or learned predictions. It is a
diagnostic for reconstruction and coverage, not a replacement for diverse real scenes.

Two pure tests check HDR fractional coverage, negative motion and convergence of independent
midpoint quadrature. Deliberately clamping the integral to 1 failed the intended
`1.75`-valued pixel; restored code passes. An external, independently written float64
rectangle-subdivision integrator checked all 18 images in the first dataset, including held-out
cases and three times each. Maximum absolute RGB discrepancy was **0.00002604**. It splits
rectangles at boundaries and sums their constant-colour areas; the GPU uses pulse
antiderivatives/intersections. Audit source, hash and image hashes are in
`build/reconstruct/datasets/rgb-v1/integral-check.json`.

### Measure what a history-weight model can change

`engine::temporal::ReconstructionSample` exports 24 floats per output pixel before blending:
the existing 14 rejector inputs, current/clipped-history RGB, heuristic weight, forced-current
flag, settled cap and material tag. The capture uses the same half-float neighbourhoods as
the shared production resolver. The legacy `found_rows` instead recomputes neighbourhoods in
f32; its inputs can differ slightly. The new export is only dispatched by the instrument.

There is also a target problem in the legacy rejector trainer: it averages already nonlinear,
tamed luma samples for its reference. That is not the result of integrating linear HDR and
applying the view afterward. The new RGB experiment uses the latter. The legacy experiment
and its old claims have not been promoted to this goal's evidence.

Before training, a non-deployable oracle tried 257 current/history weights per pixel against
exact truth, preserving forced-current pixels and the settled cap. Baseline history is fixed.
This measures **one-step** headroom; it is neither a global bound on recurrent models nor
an oracle for FLIP. On four development cases at frame 31, its 4× fixed-view MSE reductions
were 2.29%, 22.48%, 10.79% and 24.54% (cases 0, 3, 5, 7). FLIP actually worsened on cases 0
and 5 despite the improved MSE. A better weight alone cannot be assumed to recover detail.

Oracle evidence: runs `1793056392-812779940`, `1793056869-566910369`,
`1793056873-11561460`, `1793056876-389106667`. Each `oracle.json` embeds the evaluator
source/hash, versions, input hashes and limitations. CPU replay of the captured blend agrees
with the GPU within 0.002 raw RGB in these cases; half-float output accounts for its small
difference. External utility:
`/home/ryanwible/.local/state/wrela-neural-frontier/evaluate_blend_oracle.py`.

### RGB residual experiment v1 — rejected

This is the first newly trained model in this work. Its architecture is 27→32→32→3, with
**2,051 weights**, packed into **4,104 bytes**. Inputs are the captured reconstruction
signals plus relative sample phase and render scale. They contain no scene identity, absolute
position, field parameters, target pixels or future samples. It predicts an RGB residual
in the fixed exposure-1 Reinhard view; inference returns to linear HDR. Forced-current pixels
bypass the learned correction. The decoder and feature construction are shared Wrela code
between trainer and shader.

The split was recorded before training: cases **0, 3, 5, 7** train; **1, 6** validate; frames
**7, 31, 55**, all at 192×108 and all four scales. These are whole cases within one analytic
family, not random-pixel validation, sealed goal clips or external-engine evidence. Cases
2 and 4 have not been rendered for this experiment. Training kept a deterministic random
quarter of eligible pixels (247,134 rows); validation kept all pixels (497,664 rows).
The trainer rejects case overlap; a deliberate same-case/different-frame manifest failed
with that specific error before training.

Training used the existing Wrela Adam implementation, seed 3, 2,000 steps, batch 128, rate
0.002 declining to 0.0002, and squared error on integrated-reference RGB in the named view.
The last layer starts at zero, giving the heuristic baseline. Validation uses packed half
weights. Its one-step mean view MSE improved from **0.018922014 to 0.015414034 (18.54%)**.

Full GPU rollout from frame zero changed the conclusion. The following averages give equal
weight to the six held-out case/time images at each scale. Every image has the same dimensions.

| Scale | Baseline view MSE | Recurrent model view MSE | Baseline mean FLIP | Recurrent model mean FLIP |
|---|---:|---:|---:|---:|
| 1× | 0.015578 | 0.009639 | 0.313014 | 0.288743 |
| 2× | 0.018478 | 0.014181 | 0.369414 | 0.369931 |
| 3× | 0.020217 | 0.017950 | 0.389618 | 0.399214 |
| 4× | 0.021423 | 0.022138 | 0.400725 | 0.412335 |

At 4×, aggregate MSE worsens **3.34%** and FLIP worsens **2.90%**. Case 1 at frame 55 has a
**37.85% FLIP regression**. Visual inspection shows invented diagonal patterns and missing
thin structure. The model is **rejected** as a general upscaler improvement and is not enabled
in the engine. The 1× improvement does not excuse the failures at other scales.

The shader's final learned output agrees with an independent NumPy replay of the same packed
weights and captured inputs: worst fixed-view RGB discrepancy **0.000244** across the six
validation rollouts. A zero-weight model reproduces the heuristic's scores within rounding
(mean-view MSE difference below 0.000000005 on case 0, frame 31). This distinguishes a model
failure from an upload or CPU/GPU feature mismatch.

All rendering, training, replay and scoring ran on Pop!_OS. Rendering/inference used the AMD
RADV RENOIR adapter; Wrela training and external scoring used its CPU. No latency, throughput,
1080p, 4K, memory-residency or G1–G7 claim follows from these tiny images.

Reproduction and evidence on Pop!_OS:

- Dataset and pre-training split: `build/reconstruct/datasets/rgb-v1/`. Its manifest names
  every run; `runs.json` records each command. A source archive and per-run provenance retain
  the exact Wrela sources, built fixture and hashes.
- Train with `wrela tool reconstruct -- train out/datasets/rgb-v1/manifest.txt 2000 3`.
  Model run `build/reconstruct/models/1793057348-651623540/` contains the manifest, settings,
  training curve, full/half weights and one-step scores. Nothing is written into game content.
- Evaluate a rollout with `wrela tool reconstruct -- --scene edges --shot 1 --width 192
  --height 108 --warmup 55 --cells 4 --model out/models/1793057348-651623540/weights.half`.
  The result preserves a copy of its model and full float captures.
- Rollout index: `build/reconstruct/datasets/rgb-v1-rollout/results.json`. Every run's
  `recurrent.json` embeds the independent replay/scorer source, versions and hashes.
  `comparison.png` shows both held-out cases at frame 55; its display encoding is sRGB after
  the fixed view. Metrics continue to use the earlier named linear-view FLIP conditions.
- External utilities are under
  `/home/ryanwible/.local/state/wrela-neural-frontier/`; renderer, model and trainer source
  remain Wrela. The source archives and embedded evaluators make these offline tools auditable.

Focused tests pass for analytic integrals, RGB feature/baseline rules and float capture, plus
native-GPU zero-model and learned-rollout replay checks, fixture type checks, changed-file
formatting and workspace allowlist. The workspace registers the analytic and model tests.
Whole-workspace formatting still reports the previously noted untouched clearing test file.
The full suite remains required before the PR.

Next iteration: fix real-scene reference convergence, add continuous motion/cut sequences, and
test field-derived footprint/coverage information against both a closed-form baseline and a
learned reconstruction. The evidence argues against merely increasing the current rejector's
size. Recurrence and perceptual checks must remain promotion gates; none of G1–G7 is achieved.
