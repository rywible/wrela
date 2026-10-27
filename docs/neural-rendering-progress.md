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


### Separate spatial sampling from shading phase

The former reference grid uses the same counter for pixel location and the renderer's
16/64-phase dithering and stochastic shading. That couples integration dimensions. The new
opt-in `--sampling phase64 --seed N` sampler crosses every spatial stratum with all 64
shading phases and uses independently hashed coordinates inside each stratum, while simulation
time stays fixed. A reference has `cells² × 64` samples. It follows stratification's principle
from [PBRT §8.5](https://pbr-book.org/4ed/Sampling_and_Reconstruction/Stratified_Sampler);
no published implementation was copied. Pure tests check the full stratum/phase product and
a separable visibility/position integrand. Their centroid bound is the exact half-stratum
bound, not an assumed probabilistic concentration threshold.

The independent reference check uses seed N+1; it no longer mistakes identical reruns for
accuracy. Regular-grid captures retain their old behavior. On the actual clearing, shot 0,
192×108, fixed time, 240 warmup frames, seed 17:

| Samples, coarse → fine | Coarse/fine mean FLIP | Independent fine-seed mean FLIP | Result |
|---|---:|---:|---|
| 256 → 1,024 | 0.012582 | 0.005576 | Both fail 0.003 |
| 4,096 → 16,384 | 0.004613 | 0.001422 | Sample-count check still fails |

Runs are `1793058183-330807687` and `1793058576-525722079`. The latter's lower-half
coarse/fine discrepancy is 0.006978, so its detailed region particularly needs care. Candidate
captures remain byte-identical across these sampler changes. The new reference's sampling
variation is separated more honestly; this does **not** establish that all prior error came
from the phase correlation or that production lighting is unbiased.

A 16,384→65,536 run is in progress at this checkpoint:
`build/reconstruct/runs/1793059465-54690141`, log
`build/reconstruct/phase64-clearing-16384-65536.log`. Its command is the same as above with
`--cells 16`. A live process was verified; the diagnostic record
`/home/ryanwible/.local/state/wrela-neural-frontier/active-measurement.json` records its process
identity, source archive and observation time. Recheck the process/tool handle before treating
that record as current. No result is claimed for this pending run.

### Continuous clips, independent targets and cut controls

`--frames N` now drives an analytic sequence (up to 300 frames), retaining causal history
between outputs. `--warmup 0` includes the cold start. `--start-frame N` changes the starting
simulation time without inventing prior history. `--cut-at N` declares a change to case
`(shot+3)%8`; `--reset-cut 0` deliberately suppresses the reset for a negative control.
The normal default resets history. Reconstruction runs first. Separate passes then generate
linear-RGB truth and foreground coverage/velocity masks from the captured moment. Neither
target buffer is bound to a decoder. The capture holds the case and frame together so an
observation cannot silently become the next scene's target.

The eight-frame cut test, case 0→3 at frame 4, proves that the production resolver's declared
reset removes history in this fixture. At 1×, 2×, 3× and 4×, the cut frame is **byte-identical**
to a separately initialized case 3 at simulation frame 4. With reset suppressed, respectively
**80.41%, 95.81%, 96.03% and 97.32%** of pixels differ from that fresh control by more than
2/255 in named-view linear luminance. Their current-frame truth and visibility are identical,
so the intervention isolates history. This tests the resolver under an explicit cut, not
automatic cut detection throughout the game, all stale trails, or G3 as a whole.

Evidence: reset run `1793058806-214392282` (`cut-check.json`), no-reset run
`1793058999-631101207`, fresh control `1793059017-891068057`.
The checker embeds its complete source, hashes and comparison conditions.

The first five-second development clip runs **300 frames at a 60 Hz simulation cadence**,
case 7 then case 2, cut at frame 150, cold start, 192×108. It is one constructed diagnostic
clip, not a sealed 100-clip suite or a presentation-rate measurement. Command:

`wrela tool reconstruct -- --scene edges --shot 7 --width 192 --height 108 --warmup 0
--frames 300 --cut-at 150`

The run is `build/reconstruct/runs/1793058998-313684639`. Every frame retains candidate,
analytic colour target and visibility/velocity data. Targets hash-match across all four
scales. An independent float64 rectangle-subdivision audit checked all pixels of frames
0, 1, 149, 150, 151, 298 and 299, using the declared simulation time and case. Its maximum
RGB discrepancy was 0.00002457; maximum coverage discrepancy was 0.00003052.
This checks the capture's time/cut indexing as well as its integrals.

### Perceptual and geometry-correspondence scoring

The external sequence evaluator now computes FLIP, LPIPS, display PSNR and temporal residuals.
LPIPS uses package 0.1.4, AlexNet metric version 0.1 with learned calibration, PyTorch 2.5.1+cpu
and torchvision 0.20.1+cpu. It receives the fixed exposure-1 Reinhard linear RGB view encoded
as sRGB, scaled to [-1,1], with no resizing. PSNR here uses that same **sRGB-encoded** view,
range [0,1], so it differs intentionally from the earlier linear-view diagnostic PSNR.
FLIP remains LDR-FLIP 1.7, 67 pixels/degree, linear input with `inputsRGB=false`.
Effective LPIPS state hash:
`cd2943aa26b6c7e7d100ab7e09a6c0975653ff57be42499de2ab48991fff82de`.

For temporal residuals, first subtract reference from candidate in displayed **linear** RGB.
Background correspondence is stationary; foreground correspondence uses the known horizontal
velocity. Bilinear history is accepted only where both taps and the current pixel have full
coverage of that same foreground. Visibility changes, unmatched/mixed pixels and newly exposed
background are reported separately. Across the declared cut there is no surface correspondence.
Empty masks are `null`, never a fabricated zero. The all-pixel unwarped residual remains
visible, so a method cannot gain by excluding its hard edges.

Identity images score zero in both perceptual metrics. A shifted-image negative control scores
positive. A synthetic moving error field has zero compensated residual while its uncompensated
residual is nonzero; a full reveal correctly has no valid old-surface correspondence.
Controls, evaluator source/hash, versions, effective metric-weight hash and every input hash
are embedded in `sequence-metrics.json`. Evaluation runs on the Pop!_OS CPU; AMD renders all
source frames. No evaluation/model latency is claimed here.

For the five-second clip, the existing temporal path scores:

| Scale | Mean FLIP | Mean LPIPS | sRGB-view PSNR dB | Opaque temporal MAE | Visibility-change temporal MAE |
|---|---:|---:|---:|---:|---:|
| 1× | 0.422584 | 0.191405 | 17.0724 | 0.008312 | 0.149890 |
| 2× | 0.453892 | 0.419704 | 14.7757 | 0.013122 | 0.179872 |
| 3× | 0.466486 | 0.711902 | 14.5206 | 0.007735 | 0.182363 |
| 4× | 0.467562 | 0.554060 | 14.4221 | 0.006435 | 0.185539 |

Temporal columns average the 298 non-cut frame transitions. Conservative opaque correspondence
covers only **36.53%** of pixels; visibility changes cover **63.47%**. This is a limitation of
single-surface correspondence on this thin-layer test, not evidence that the rest is stable.
The 4× path's small opaque temporal residual plainly does not compensate for its large spatial
and changing-visibility errors. This is why the charter requires all gates together.
There are no clip-level confidence intervals or population claims from this one clip.

External utilities: `evaluate_sequence.py`, `check_sequence_cut.py` and
`check_sequence_integrals.py` under the existing external state directory.
Source archive: `build/neural-evidence/sequence-phase64/source.tar.gz`.
Each completed run has a provenance file and retained built fixture. The renderer, sampler,
capture protocol and model code remain Wrela; external packages are measurement tools only.

The captured moment stores both tick and case. After that hardening, the eight-frame cut
control was rerun as `1793060012-470941299`: all 96 candidate, truth and visibility buffers
were byte-identical to `1793058806-214392282`. Six focused pure tests and the native
capture controls pass. Whole-workspace formatting still reports the existing unrelated
`examples/clearing/tests/checks/tests.wrela` issue; changed files are formatted. The 65,536
sample convergence measurement is still running and is not counted as a passing result.

### Retained raw observations: a useful control, not a promoted decoder

Added the explicit fixture-only option `--decoder raw`. It stores 16 causal raw colour,
motion and depth frames, with their known jitter positions. A gather pass reconstructs from
those observations using the nearest current sample's constant-velocity hypothesis. It reads
no field parameters, exact pixel integrals, reference visibility, future frames or case ID.
This is a conventional control for the next neural architecture, not a trained model.
The production engine and the existing neural experiment are unchanged.

The first version rejected historical observations with a different depth. That was wrong
for static fractional coverage: a pixel alternates between foreground and background without
either layer moving. Removing that rejection when motion agrees improves the static control.
It does not solve changing visibility or ambiguous correspondence. Both versions and their
measured failures remain in the evidence.

All rows below are **192×108, 32 consecutive frames from a cold start**, AMD RADV RENOIR.
They are short development clips, not the five-second population or sealed validation set.
The static case is 5. The motion clip changes from case 7 to 2 at frame 16, with an explicit
history reset. Production and candidate use byte-identical references and visibility masks.
FLIP and LPIPS follow the pinned definitions above.

| Clip | Scale | Production FLIP | Raw FLIP | Production LPIPS | Raw LPIPS |
| --- | ---: | ---: | ---: | ---: | ---: |
| Static | 1× | 0.290096 | 0.090682 | 0.104883 | 0.011957 |
| Static | 2× | 0.300883 | 0.125800 | 0.161901 | 0.031573 |
| Static | 3× | 0.306910 | 0.192211 | 0.230668 | 0.075269 |
| Static | 4× | 0.337088 | 0.231750 | 0.344775 | 0.121331 |
| Motion/cut | 1× | 0.393728 | 0.212599 | 0.168920 | 0.075582 |
| Motion/cut | 2× | 0.419706 | 0.237453 | 0.282647 | 0.088752 |
| Motion/cut | 3× | 0.497995 | 0.449301 | 0.438710 | 0.193064 |
| Motion/cut | 4× | 0.474830 | 0.538963 | 0.401666 | 0.310719 |

At 4×, static FLIP improves 31.25% and LPIPS improves 64.81%, but motion FLIP
**regresses 13.51%**, despite its LPIPS improvement. A retained comparison image also shows
substantial missing structure and aliasing. The control is **not promoted**. At moving case 7,
frame 31, 7.05% of 4× output pixels have no weighted compatible observation and fall back to
the nearest current sample. This is a narrow diagnostic frame, not a population failure rate.
It motivates a model with spatial context and explicit visibility handling; it does not
establish that a larger model will succeed.

The earlier depth-gated version's static 4× FLIP was 0.443984, worse than production.
At the moving cases, both raw versions give identical results because the two layers have
distinct velocities. This confirms that the static gain came from retaining fractional
coverage, not a new sample budget or a changed reference.

Independent CPU replay reads only the exported raw observations and declared timing,
never the field or target. On static case 5 and moving case 7, all four scales agree with the
GPU within **0.000242** maximum absolute error in the named linear view, consistent with
the final half-float render target. Removing jitter from the replay produces mean view
errors of 0.008–0.101. Cut frames 16 and 17 are byte-identical to fresh rendering of the new
case at the same times, including truth and masks. Suppressing the reset changes 96.6–99.5%
of pixels by more than 2/255 in at least one named-view RGB channel on the cut frame.

A small-size native control found a real storage-binding bug: unpadded frames could start
between 256-byte boundaries. Raw frame strides now round up to eight 32-byte records.
The failed 12×12 control is retained; the corrected capture and independent replay pass at
all four scales, including padded 6×6 and 3×3 inputs. The pure alignment rule, kernel support,
image decoding and workspace allow checks pass. Raw records have semantic tags and geometry;
their finite-value validation is separate from display images' mandatory alpha=1 check.

This uncompressed instrument deliberately refuses output above 384×216. Its history alone
uses `16 × aligned(input_pixels,8) × 32` bytes: at hypothetical 4× 1080p it would be
63.28 MiB, and at 4K 253.13 MiB, before any other allocations; lower scale factors cost more.
Those sizes have not been run, and a large single storage binding would need a different
layout. No G6 timing or memory claim follows. Existing neighbourhood work is still executed;
this control is not a cost-optimized production path. No timing measured while the reference
job and other captures shared the GPU is used as performance evidence.

Reproduction:
`wrela tool reconstruct -- --scene edges --shot 5 --width 192 --height 108 --warmup 0 --frames 32 --decoder raw`;
repeat with decoder `production` for the paired baseline. The motion pair uses
`--shot 7 --cut-at 16`. Independent replay inputs are captured with
`--warmup 31 --frames 1 --decoder raw`. Sources, run IDs, fixture hashes and metric hashes
are recorded in `build/neural-evidence/raw-history-aligned/manifest.json`.
The earlier exact decoder sources are retained under `raw-history-v1` and
`raw-history-v2`; the final archive adds aligned buffer strides.
External measurement utilities are `replay_raw.py`, `compare_sequences.py` and
`check_raw_cut.py` in the established external state directory, each embedded in its results.
No G1–G7 gate has been satisfied or weakened.

### The 65,536-sample production-reference check completed

The pending phase64 clearing run `1793059465-54690141` completed successfully on the AMD.
Its fixed shot, simulation time, size and view transform are the same as the smaller runs.
Coarse/reference/repeat use 16,384 / 65,536 / 65,536 samples, with independent seeds 17 and 18
for the two fine estimates.

| Numerical check | Whole-image mean FLIP | Upper half | Lower half |
| --- | ---: | ---: | ---: |
| 16k versus 65k | 0.002115 | 0.001109 | 0.003121 |
| Independent 65k seeds | 0.000730 | 0.000555 | 0.000904 |

Both **whole-image necessary numerical checks pass 0.003**. The lower-half coarse/fine
diagnostic still exceeds that limit, so this is not a worst-region guarantee. Agreement
between these estimates cannot establish unbiased lighting: all use the production renderer.
One frozen 192×108 shot also cannot establish temporal, scene-population or sealed-test
reference validity. G1–G7 remain unmet.

Against this reference, production FLIP is 0.073293 / 0.096488 / 0.142255 / 0.183315 for
1× / 2× / 3× / 4×. All four candidate buffers are byte-identical to the 4k/16k phase64 run,
so the convergence experiment changed the reference estimate, not the reconstruction.
The run's `flip.json` embeds the scorer and records image hashes; `provenance.json` records
the exact execution-source archive, compiler and AMD driver. The active measurement record
outside the repository now marks this job completed. No elapsed capture time is used as a
frame-rate or latency claim.

### AMD browser budget baseline recorded

With all earlier GPU experiments completed, fixed-quality production fixtures were profiled
sequentially on the AMD. See the full conditions and evidence in
[the budget baseline](bench.md#2026-10-26-amd-neural-rendering-budget-baseline-a3ff61a4).
At 4×, browser GPU span is **21.696 / 24.226 ms median/p95 at 1080p** and
**47.487 / 49.936 ms at 4K**. Temporal reconstruction alone is **4.427 / 4.460 ms**
and **17.773 / 17.866 ms**, respectively. Clouds and bloom are additional large costs.
These fixed-pose, short profiles are diagnostic; they are not the 120Hz/20-minute G6 protocol.
They use the production heuristic decoder, not the raw control or the rejected neural model.

The next model must therefore improve both the information retained for quality and the
cost of reconstruction. A larger per-output-pixel residual MLP without a computation budget
would ignore the measured constraint. Spatial feature work at input resolution, a small
output decoder, and separately measured visibility/coverage handling are justified next
experiments. Cloud and presentation work also require attention for the complete-frame goal.
No claim is made that any proposed architecture will meet the gates.


### Input-resolution neural patch model: faster kernel, rejected quality

The next experiment is a **48→16→16→12** shared encoder evaluated once per input pixel.
Its twelve outputs describe three local bilinear RGB residual polynomials. Each output
pixel blends four overlapping polynomials over a bilinearly reconstructed current colour.
The baseline interpolation happens in scene-linear RGB; residuals use the fixed exposure-1
Reinhard linear view. The inverse view and final half-float target match the capture protocol.
There are **1,260 weights / 2,520 shipped half-weight bytes**. The model is fixture-only and
is not enabled in a game or the engine's production temporal path.

Inputs comprise a 3×3 current RGB neighbourhood, relative depth neighbourhood, motion,
jitter and scale, one reprojected previous-output RGB sample, validity, depth disagreement,
semantic tag and history age. No case identity, absolute image position, field parameters,
target or future frame enters inference. History is resampled down into the encoder: it
does **not** yet have a direct full-resolution history connection. Each encoded cell occupies
64 bytes; optional training observations occupy 208 bytes and are exported only when requested.
The fixture skips the production neighbourhood and resolve dispatches for this decoder,
but still allocates some unused production resources. This is not a production memory audit.

Training uses whole cases 0/3/5/7; validation uses disjoint cases 1/6. Each contributes
32 consecutive cold-start frames at 192×108 and all four scales, covering all 16 jitter phases
twice. This fixes a coverage weakness in the earlier RGB dataset's frames 7/31/55: modulo 16,
those cover only phases 7/15/7. The new set contains 512 training and 256 validation captures.
History features in this first round come from the zero-residual patch decoder, not from
the learned model's rollout. This distribution difference remains a limitation.

The Wrela trainer differentiates mean RGB squared error through all four overlapping encoders,
using Adam, batch 128, seed 3 and 2,000 steps. It trains unclamped predictions so out-of-range
outputs still receive gradients; validation uses deployed clamping and half-quantized weights.
Full-pixel one-step validation MSE improves **0.020631230 → 0.016081713 (22.05%)** relative
to zero residual. This is not a recurrent quality result.

Continuous AMD rollout of the learned model, paired against production with identical targets,
masks, timing and pinned metric versions, gives:

| Development case | Scale | FLIP change | LPIPS change | PSNR change, dB | Opaque temporal residual, candidate / production |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 1× | −35.81% | −48.27% | +2.769 | 5.056× |
| 1 | 2× | −5.96% | −32.09% | −0.021 | 12.634× |
| 1 | 3× | −4.04% | −2.65% | +0.256 | 11.175× |
| 1 | 4× | +1.61% | −11.22% | −0.457 | 12.058× |
| 6 | 1× | +11.46% | +1.27% | +0.927 | 2.196× |
| 6 | 2× | −9.71% | −71.91% | +0.983 | 4.410× |
| 6 | 3× | −14.14% | −41.50% | +0.220 | 7.586× |
| 6 | 4× | −8.82% | −18.37% | −0.646 | 8.872× |

Negative error changes are better. The temporal column is worse in every row. At 4×,
candidate mean FLIP is 0.365786 / 0.423756 for cases 1/6, far from G1. Both cases also lose
PSNR; newly exposed errors increase at 4×. The opaque temporal mask covers 58.11% / 61.81%
of pixels, with changed/mixed visibility scored separately. These 32-frame development clips
are not five-second acceptance clips, not a sealed population and not external comparators.
The model is **rejected for promotion**, despite its one-step loss and some perceptual gains.

Correctness and runtime evidence:

- The polynomial-coordinate test passes. Exact backpropagated derivatives match finite
  differences across twelve weights spanning the layers; deliberately zeroing the derivative
  makes the test fail, then restoring it passes.
- Independent CPU replay uses only captured causal inputs and packed weights. The largest
  coefficient difference is below 7.4e−7 and final named-view difference below 0.000243.
  Reversing the jitter sign gives mean errors of 0.069–0.108 on the learned-model probe.
- A fixed-shape vector kernel unpacks weights once into 315 vec4 values (5,040 bytes),
  uses constant indices and vector dot products, and retains the scalar implementation as
  a control. Across both 32-frame clips and all scales, final optimized versus scalar maximum
  named-view difference is 0.000242; mean differences are below 1.6e−8. Targets and masks
  remain byte-identical. This does not make floating-point execution bit-identical.
- A case 1→4 cut at frame 16 matches fresh case 4 rendering byte-for-byte on frames 16/17,
  at every scale. Suppressing reset changes 95.24–99.27% of cut-frame pixels by over 2/255
  in at least one named-view RGB channel. This checks reset causality, not all G3 artifacts.
- AMD Chrome encoder median drops from **10.605 to 0.814 ms at 4× 1080p**, and from
  **41.863 to 3.186 ms at 4× 4K**, with unchanged trained weights. The final output stage
  still costs **1.090 / 4.301 ms**. An explicit four-gather decoder rewrite does not materially
  improve its p95. See [conditions and all three kernels](bench.md#2026-10-26-amd-neural-patch-v1).
  These are reconstruction-fixture timings, not complete game-frame or G6 results.

Reproduction: capture with
`wrela tool reconstruct -- --scene edges --shot 1 --width 192 --height 108 --warmup 0 --frames 32 --decoder patch --patch-fast 1 --model out/models/patch-1793063898-289696714/weights.half`.
Use `--training-inputs 1` for causal per-frame inputs, and omit the model for the initial
zero-residual dataset. Train with
`wrela tool reconstruct --timeout 3600 --blob-budget 3221225472 -- patch-train out/datasets/patch-v1/manifest.txt 2000 3`.
`--patch-fast 0` retains scalar inference; both are explicitly experimental options.

All evidence remains on Pop!_OS under `build/neural-evidence/patch-v1/manifest.json`, including
the source archive hash, training source hash, exact model hashes, paired reports, CPU replay,
reset/negative control and all timing reports. Model: `patch-1793063898-289696714`;
packed weights SHA-256 `4b9b969a9fd191f25f69cdc6f348f4464f0b2bb7b9d6bd9be687b64bee62fe18`.
Production clips are `1793063890-927028191` / `1793063897-608218538`; scalar learned clips are
`1793063995-411379862` / `1793064001-819925230`; final optimized clips are
`1793065513-190478045` / `1793065519-546747103`.
The external `audit_patch_v1.py`, `replay_patch.py` and `summarize_patch_v1.py` scripts are
embedded with their hashes in their results. New package checks, the two focused pure tests,
workspace allow and changed-source formatting pass. Full workspace suite remains required
before a PR; it was not run for this development checkpoint.

The next experiment should preserve full-resolution temporal detail and explicitly optimize
motion consistency while retaining the measured small input-resolution encoder. On-policy
training and a direct, validity-controlled history connection are hypotheses to test;
neither is implemented or claimed to work here. No G1–G7 gate is satisfied or weakened.
