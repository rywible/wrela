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


### Direct history and source-geometry ablations

The patch model now has an optional direct full-resolution history connection. Its 1,260
weights are **unchanged**: this round isolates the reconstruction architecture, not training.
The fixed-input path proposes motion from the nearest current low-resolution geometry sample,
checks previous depth and velocity, and blends the current neural prediction with reprojected
previous output in the named view. Its maximum history share is explicit; cold-start share is
bounded by age/(age+1), with age capped at 16. Both paths reset age on cuts and resize.
Default history share is zero, preserving the previous experiment.

Constant maximum shares 50% and 90% were tested on both 32-frame development clips at every
scale, with zero-residual/90% controls. A separate **full-stack** path evaluates the authored
analytic geometry at the output position and its previous position. It takes only each
frame's geometry parameters (period, width, offset, speed), with no field colours, integrated
coverage, reference data or future frame. There are two source-geometry queries per output
pixel, in addition to the same low-resolution base-colour samples. Their work is inside the
timed output pass. The fixed-input and source paths have separate shader entry points;
the fixed-input binding does not receive the source parameters.

This is the two-layer analytic field's visibility function, **not** a general 3D visibility
solution. Point visibility also cannot describe a pixel's mixture of moving foreground and
background. The broader authored-field advantage remains a hypothesis beyond this fixture.

| Case | 4× variant | Mean FLIP | Mean LPIPS | Temporal residual / production | Newly exposed error / production |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | 90% history, sampled geometry | 0.357252 | 0.479582 | 4.581× | 1.240× |
| 1 | 90% history, source geometry **(full stack)** | 0.332059 | 0.424967 | 1.356× | 1.252× |
| 1 | 90% history, zero neural residual | 0.472798 | 0.549568 | 10.071× | 1.540× |
| 6 | 90% history, sampled geometry | 0.425954 | 0.399452 | 1.870× | 1.027× |
| 6 | 90% history, source geometry **(full stack)** | 0.418722 | 0.439455 | 1.033× | 1.015× |
| 6 | 90% history, zero neural residual | 0.480028 | 0.566561 | 3.420× | 1.309× |

Ratios above one are worse. Production 4× FLIP/LPIPS are 0.359978/0.512431 (case 1) and
0.464741/0.556551 (case 6). Against the previous unblended neural model, sampled-geometry
90% history reduces the opaque temporal residual by **62.0% / 78.9%**. Source geometry then
reduces that residual another **70.4% / 44.8%**. Both source variants still trail production
temporally and lose PSNR (−0.216 / −0.236 dB). Case 1's newly exposed error remains 25.2%
worse than production. Source geometry also worsens case 6 LPIPS relative to sampled geometry.
No variant is promoted, and the extra-source result is not a G2 fixed-input tournament entry.

At 1×, the 90% source variant's FLIP is 0.2726 / 0.3508 versus production's 0.3184 / 0.2574:
the second case regresses substantially. More history is not a general quality improvement.
All four scales, both shares and the zero-network controls remain in the manifest. Tuning on
these clips makes them development feedback; none is a sealed-test observation.

The 18-frame case 1→4 reset probe accepts history at 63.14% of 4× pixels with sampled geometry
and 82.87% with source geometry. These include two cold/reset frames. In the complete uncut
source clips, mean acceptance is 90.10% / 86.85%. Acceptance is not proof of correct transport:
the quality and reveal metrics above remain the independent checks.

Independent CPU replay reconstructs the current spatial model, reprojects prior output from
captured geometry, and reproduces the blend. The source version reads the exact geometry-only
parameters exported for each frame; it does not reconstruct them from the target. Maximum
named-view disagreement is below **0.000244** across the cut probes and both complete source
clips, consistent with the final half target. Reversing motion produces a failing negative
control. Disabling the new option preserves the prior model byte-for-byte over 32 frames and
all scales. Separating the source pipeline also preserves both complete clips byte-for-byte.

For both geometry paths, reset frames 16/17 match fresh case 4 at the same times byte-for-byte.
Suppressing reset changes 95.63–98.91% of cut-frame pixels by over 2/255 in at least one
named-view channel. The geometry translation test passes, fails when motion's time sign is
reversed, and passes after restoration. The spatial-only trainer now explicitly rejects
history-enabled captures; that rejection was exercised. Its old feature-only datasets remain
compatible. Geometry/source-parameter readbacks run only in requested diagnostic captures.

An offline, **target-informed** best per-pixel blend of the same spatial prediction and
already-captured history gives 4× view MSE 0.018053 / 0.015803 versus the deployed constant
share's 0.025374 / 0.019828 (28.9% / 20.3% lower). This is a fixed-history headroom diagnostic,
not a runnable model, a rollout, a temporal guarantee, or an acceptance score. It justifies
testing learned confidence with causal inputs; it does not predict that training will realize
that bound. Repeated resampling and mixed-layer visibility still need architectural attention.

[AMD browser measurements](bench.md#2026-10-26-amd-direct-history-and-source-geometry)
include the source queries. At 1080p, the sampled/source output stages cost 1.937/1.494 ms
median; at 4K, 7.531/5.640 ms. Source queries are cheaper than the sampled-buffer variant in
this small analytic case, but the full reconstruction budget is still missed. No claim is made
that source queries are cheap for arbitrary 3D geometry or that this is a complete frame.

Reproduce a fixed-input capture with the prior patch command plus `--patch-history 900`;
add `--patch-source-geometry 1` for the explicitly separate full-stack path. Use
`--training-inputs 1` to retain causal features, raw point geometry, and (for source mode)
the exact two geometry parameter vectors. Sequence metadata declares the input track.

Evidence: `build/neural-evidence/patch-history-v1/manifest.json` on Pop!_OS, with all capture
IDs, source/weight/build hashes, complete comparisons, CPU checkers, cut controls, negative
test logs and timing reports. Final source clips with diagnostic inputs are
`1793067234-323531082` / `1793067245-571211036`; cut probes are
`1793066205-146124167` (sampled) and `1793066780-629644561` (source).
The final source archive SHA-256 is
`59311b9573be49442090c04ab11622f65b3bc28abf4a6a2319f83e873dda7f5d`.
Package checks, three focused patch tests, allow, changed-source formatting and diff checks
pass. No full suite/PR or G1–G7 acceptance is claimed at this checkpoint.


### Learned history confidence: one-step gains, recurrent rejection

A separate 13-parameter logistic model now predicts history confidence: current and previous
named-view RGB, their absolute difference, inverse render divisor, history age and motion
magnitude (12 features plus bias). Its sigmoid multiplies the existing geometric eligibility
and age-bounded 90% maximum. The 1,260-weight spatial model is frozen. The gate occupies
64 bytes (13 f32 parameters and three checked zero-padding values); inference uses no truth,
integrated coverage, future frame or case identity. It is an optional experiment, disabled
by default. Source-geometry runs remain **full-stack** entries, not fixed-input G2 entries.

The diagnostic capture exports the exact causal inputs before reconstruction; targets remain
a separate later pass. The Wrela trainer samples 512 pixels per frame/scale from four complete
cold-start cases (0/3/5/7), and checks every pixel of validation cases 1/6. Each clip has 32
frames at 192×108, all four scales, and every jitter phase twice: 262,144 training rows and
5,308,416 validation rows. These cases are development data, not a sealed test.

Training uses Adam, seed 3, 4,000 steps, batches of 512 and rate 0.02 decaying toward 0.002.
The loss is mean squared RGB error in the named view plus a weighted warped previous-error
consistency term on eligible history. That term has different masks and a different norm
from the acceptance evaluator; it is **not** the acceptance temporal metric. Inputs and
previous errors come from the constant-history control, rather than the learned model's
rollout. The three temporal coefficients are 0, 0.25 and 1.

| Temporal coefficient | One-step validation view MSE | One-step warped-error MSE |
| --- | ---: | ---: |
| Constant-history control | 0.018003379 | 0.004445967 |
| 0 | 0.015357259 | 0.005375419 |
| 0.25 | 0.015550814 | 0.004945132 |
| 1 | 0.016063972 | 0.004481103 |

The best one-step image improvement is 14.7%, but complete recurrent AMD captures reject
**all three models**. The following uses the same two complete clips, exact reference/mask
hashes, exposure-1 view, FLIP 67 ppd and LPIPS AlexNet v0.1 as the preceding experiment.
Temporal and reveal ratios are relative to that experiment's 90% source-geometry control.

| Case | Coefficient | 4× FLIP | 4× LPIPS | Opaque temporal ratio | Newly exposed error ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 0 | 0.320997 | 0.514685 | 2.216× | 0.942× |
| 1 | 0.25 | 0.317203 | 0.512338 | 1.960× | 0.946× |
| 1 | 1 | 0.311932 | 0.493491 | 1.655× | 0.959× |
| 6 | 0 | 0.415276 | 0.422297 | 1.521× | 0.994× |
| 6 | 0.25 | 0.415408 | 0.426585 | 1.431× | 0.995× |
| 6 | 1 | 0.415843 | 0.430968 | 1.352× | 0.996× |

Control FLIP/LPIPS are 0.332059/0.424967 and 0.418722/0.439455. At 4× the gates reduce
FLIP by 0.7–6.1%, while case 1 LPIPS rises 16–21% and opaque temporal error rises 35–122%.
At 1×, FLIP improves 11–27% and LPIPS improves 31–51%, yet temporal error rises 66–175%.
All four scales and comparisons against both the constant-history and production controls
are retained. Single-image fitting is insufficient here. Off-policy inputs and the loss's
different temporal definition are hypotheses to investigate, not established sole causes.

Correctness controls distinguish training failure from an obvious execution mismatch:

- The analytic gradient matches finite differences for all 13 active parameters; invalid
  history is inert. Deliberately zeroing the derivative fails the test, then restoration
  passes. The first test probe was too close to stationary to witness a broken gradient;
  a nonstationary probe fixed the test without relaxing its error tolerance.
- Disabling the gate preserves both full 32-frame controls byte-for-byte at every scale.
- A CPU replay of only the captured causal gate inputs and weights matches the GPU within
  0.000244 in the named view; bias perturbation provides a failing negative control.
  A separate replay reconstructs the spatial network, source geometry, reprojection and
  gate independently, with maximum final-view disagreement 0.000244141.
- The full replay initially exceeded a 4e-6 bound on the **pre-quantization spatial
  intermediate** at 3× (observed 1.049798e-5). Investigation is retained; the exact
  arithmetic cause is not established. Its retained intermediate bound is 2e-5, while
  independent geometry/history bounds and the final-view 0.00035 bound are unchanged.
  This numerical probe does not alter any G1–G7 threshold or rescue the rejected models.
- Reset frames 16/17 match fresh case 4 at the same times byte-for-byte. Suppressing reset
  changes 96.92–98.83% of first-cut pixels by more than 2/255 in a view channel.

The new offline oracle diagnostic respects the actual age/eligibility cap. The preceding
experiment's target-informed oracle allowed a maximum of 0.9 even during early cold start;
neither oracle was used for inference or constitutes a realizable-model result.

The coefficient-1 model was selected only for execution and runtime checks, not promotion.
[AMD Chrome timing](bench.md#2026-10-26-amd-learned-history-confidence) measures its complete
source-geometry output stage at 1.470/5.730 ms median (1080p/4K), compared with 1.451/5.632 ms
for the contemporaneous disabled-gate control. Short unpaired timings do not establish a
precise overhead bound. Both still miss the reconstruction budget.

Reproduce capture with the preceding source-geometry command and `--training-inputs 1`.
Train with `wrela tool reconstruct --timeout 3600 --blob-budget 3221225472 -- gate-train out/datasets/gate-v1/manifest.txt 4000 3 1000`
(change the last value to 0 or 250 for the other objectives). Capture the recurrent candidate
with `--gate-model out/models/gate-1793068787-531418279/weights.f32`. The exported gate-input
format is version 1. Training and validation clips must have disjoint case identities.

Evidence: `build/neural-evidence/gate-v1/manifest.json` on Pop!_OS embeds source/version/
weight hashes, capture IDs, all paired comparisons, replay code and results, cut controls,
negative-test logs and timings. Dataset IDs and the manifest are also under
`build/reconstruct/datasets/gate-v1`. Model directories and 64-byte weight hashes:

| Coefficient | Model directory | SHA-256 |
| --- | --- | --- |
| 0 | `gate-1793068769-126677887` | `920f33c0e0924d75ba4e1fa598907070decc565e8ca124dcf03ca16775dd9958` |
| 0.25 | `gate-1793068778-327079336` | `31f475866b44b9f7884b478eb03589f801f45f067a71085d1a4c142b2d8cf376` |
| 1 | `gate-1793068787-531418279` | `1742bb47661055d0f53c48fc3b7445d28cc442b76951947f877c5bdd9f413495` |

Training/profile source archive SHA-256:
`4b4a798e869a65707547b63081508a87f50fd551eb45f880776e09ad0e39f503`.
Capture source archive SHA-256:
`e1d2867af32346ab33a1d6ad35e1bd10378ce7c52adb66812358df971415c77b`.
The CPU trainer took 11.25/9.19/9.17 seconds including tool startup, loading and validation;
cache conditions were not controlled. Dataset capture/readback/file work took approximately
12.0–12.4 seconds per clip from run timestamp to final gate file, excluding teardown.
These small-fixture costs do not establish G7's source-to-validated-asset requirement.

Package checks, the focused gradient test, allow, changed-source formatting and diff checks
pass. No full suite/PR, model promotion or G1–G7 acceptance is claimed at this checkpoint.


### Full recurrent training: improved tradeoff, still rejected

The confidence gate now has an externally trained comparison that follows its **own outputs
through all 32 frames**, including their effect on both the next spatial encoder and the final
history blend. The spatial model's weights remain frozen. Captured prior RGB features are
explicitly discarded and recomputed from the candidate's output. Current source samples,
point geometry, jitter and age are causal inputs; truth and integrated coverage are used only
by the offline loss. The source-geometry variant retains its separate full-stack scope.

The external prototype uses PyTorch 2.5.1+cpu on Pop!_OS, outside the Wrela repository; the
deployed candidate still uses the Wrela gate and renderer. [PyTorch's reverse differentiation
description](https://raw.githubusercontent.com/pytorch/pytorch/v2.5.1/docs/source/notes/autograd.rst)
describes the graph/chain-rule mechanism used here. A double-precision six-frame finite-
difference probe checks all 13 gate parameters: maximum gradient disagreement is 2.26e-13.
Detaching frame history produces 9.92e-5 disagreement and fails the same 2e-6 bound. This
checks the smooth recurrent surrogate, not a derivative through discrete target quantization.

The initial forward replay failed its numerical checks. Inspection found two device behaviors:

- RADV RENOIR reports `subTexelPrecisionBits = 8`. Rounding bilinear fractional weights to
  that precision reduces maximum encoder-history disagreement from 0.000599 to 0.00000358.
  Six- and ten-bit alternatives do worse; the probe and device report are retained.
- The measured positive half-float render target behaves like rounding toward zero. On the
  cold-frame probe, that conversion matches 99.30–100% of channels, versus 46.4–54.4% for
  round-to-nearest. Remaining spatial arithmetic differences are retained separately.

These are measured properties of this AMD/compiler/driver path, not portable WebGPU promises.
The calibrated full recurrent replay has p99 channel error zero on all four new validation
captures, with maximum view error 0.000376 and worst mean error 7.19e-7. Teacher-forced
intermediate checks also pass. Initial failures remain in evidence; no acceptance threshold
was changed. Training uses f32 recurrent targets, while audit/evaluation applies the measured
half conversion. Actual GPU rollout remains the quality authority.

Both candidates start from the earlier coefficient-1 gate and train for 256 steps (16 epochs)
over complete cases 0/3/5/7 at all four scales. Each update uses every pixel of one 32-frame
clip; seeded case/scale shuffling and Adam rate 0.03→0.003 are recorded. Validation cases 1/6
are excluded from updates. Loss is named-view RGB MSE plus either 0.25 or 1 times the
conservative opaque temporal MAE used by the independent evaluator. Its masks come from
offline reference visibility. Replayed loss values agree with independently scored GPU
captures within 4.56e-8. This is still only one temporal diagnostic, not G3's full requirements.

| Case | Temporal coefficient | 4× FLIP | 4× LPIPS | Temporal / constant history | Newly exposed error / constant history |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.25 | 0.314339 | 0.456450 | 1.254× | 0.980× |
| 1 | 1 | 0.321247 | 0.439038 | 1.118× | 0.992× |
| 6 | 0.25 | 0.417438 | 0.437736 | 1.121× | 0.998× |
| 6 | 1 | 0.418061 | 0.438783 | 1.055× | 0.999× |

Compared with its initial one-step gate, the coefficient-1 candidate reduces 4× opaque
temporal error by 32.4% / 22.0%. Both new candidates nevertheless regress that metric against
the constant-history control, and case 1 LPIPS remains worse by 3.3–7.4%. All scales show a
temporal regression against that control. **Neither is promoted.** The loss definition and
training regime both changed, so the comparison does not isolate the causal effect of
backpropagating through time.

At 4×, the stronger penalty uses 97.15% / 99.10% of the eligible history cap on average.
It approaches the constant control while giving back some image improvement. These results
support shifting the next experiment toward the spatial representation and its interaction
with feedback, plus broader production content. They do not establish that confidence
learning cannot help with a different representation or training set.

[Training costs](bench.md#2026-10-26-pop-os-recurrent-confidence-training) are 139.84 / 137.68
seconds after data loading, with 4,402,096 KiB process peak RSS. No new GPU speedup or G7
claim follows: this reuses the frozen model and captured toy data, and quality rejects the
outputs. Runtime code and default behavior are unchanged in this experiment.

Evidence on Pop!_OS: `build/neural-evidence/recurrent-gate-v1/manifest.json`, including both
training programs' source, all input/weight hashes, original and corrected numerical probes,
gradient controls, all four-scale comparisons, independent loss parity and gate-use summaries.
The source bundle `external-sources.tar.gz` contains the external replay/trainer/evaluators
for reproduction outside the repository. Its SHA-256 is `a63a738de1a554fb755893c89d2de4ea681c230c3852a26d1f85b60255d42080`.
Renderer/capture source SHA-256 is
`4b4a798e869a65707547b63081508a87f50fd551eb45f880776e09ad0e39f503`.

| Coefficient | Model directory | Weight SHA-256 | Validation run IDs (cases 1 / 6) |
| --- | --- | --- | --- |
| 0.25 | `recurrent-gate-1793071668-176717405` | `c9c4ccccf9a348cd9fdb58a3e8b44b41e62823804ef1e9a8bc8f88ea0abbe4df` | `1793071973-417518141` / `1793071987-469352288` |
| 1 | `recurrent-gate-1793071808-886661632` | `3cab57a84072d3289cc5ada7e5336a99d47631eab78ae56e908bc2fa00cbf60f` | `1793072001-624444023` / `1793072015-927421509` |

Reproduce using the bundled `train_recurrent_gate.py` with the pinned CPU environment and
the recorded gate-v1 dataset, then `evaluate_recurrent_gate.py`; both create unique model/run
directories. Renderer invocation is unchanged except for the new `--gate-model` weight path.
This remains development work. No sealed test, full suite/PR or G1–G7 acceptance is claimed.


### Joint spatial/history training and a broader diagnostic catalog

The next experiment trains all 1,260 spatial parameters and all 13 confidence parameters
together through their own 32-frame output history. It uses the preceding calibrated CPU
replay, the same four training cases and all four scales, with every output pixel participating.
The two candidates start from patch-v1 and the preceding recurrent coefficient-1 gate.
Their objectives are view MSE plus 0.25 or 1 times conservative opaque temporal MAE.
Adam uses spatial rate 0.001→0.0001 and gate rate 0.01→0.001, seed 3, 256 updates;
the declared combined gradient-norm cap of 1 clips no update. Validation remains cases 1/6,
excluded from fitting. This is development data, not a sealed test.

The external prototype follows the same [PyTorch reverse differentiation mechanism](https://raw.githubusercontent.com/pytorch/pytorch/v2.5.1/docs/source/notes/autograd.rst)
as the preceding experiment. Parameter packing reproduces the old replay exactly. Double-
precision finite differences check 31 coordinates (including all gate parameters, spatial
layer boundaries/biases and history coefficients) and 16 seeded directions spanning every
parameter. Maximum disagreement is 2.77e-9. Detaching frame feedback instead produces
0.02518 maximum gradient disagreement and fails the 2e-6 check.

Training uses f32 weights/history, then packs spatial weights to f16 and retains a padded
f32 gate. Shipped model data would still be 2,520 + 64 bytes, with the existing architecture
and inference stages. This is not a complete GPU memory or runtime measurement.
[CPU fitting costs](bench.md#2026-10-26-pop-os-joint-spatial-and-history-training) are about
145 seconds per candidate after loading.

**Both candidates are rejected.** Lower training loss does not transfer into a better complete
validation tradeoff. The table compares actual AMD GPU output with the constant 90% source-
geometry control; ratios above one are worse. These remain full-stack experiments.

| Case | Temporal coefficient | 4× FLIP | 4× LPIPS | Opaque temporal ratio | Newly exposed error ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.25 | 0.356660 | 0.433309 | 1.405× | 1.108× |
| 1 | 1 | 0.346174 | 0.424700 | 1.213× | 1.071× |
| 6 | 0.25 | 0.421716 | 0.477091 | 1.209× | 1.038× |
| 6 | 1 | 0.429820 | 0.467764 | 1.074× | 1.005× |

At 1×, case 6 LPIPS regresses 75–89% against that control. All four-scale comparisons against
the constant control, production and the initial recurrent stack are retained, including
visibility-change and unmatched masks. The unchanged independent scorer verifies the same
target/mask hashes and viewing conditions for each pair.

The full recurrent CPU replay of the **packed** models matches GPU output with maximum
named-view disagreement 0.000411 and worst mean disagreement 7.57e-7. Replayed MSE/opaque
temporal MAE agree with independent GPU scoring within 5.24e-8. A separate packing-only probe
compares f32 and f16 spatial weights under the same recurrent target conversion. Across all
scales, its maximum mean view difference is 8.35e-5, maximum MSE change 7.39e-6 and maximum
temporal-MAE change 4.16e-6. Large quality regressions remain in the deployed packed output;
they are not rescued by the small packing differences. Frame-31 inspection montages are
retained as visual debugging, with no motion or human-acceptance claim.

| Coefficient | Model directory | Spatial half SHA-256 | Gate SHA-256 |
| --- | --- | --- | --- |
| 0.25 | `joint-1793072611-692335964` | `422760a06fbf0f59dc1c4cac0278d946279311bffd45029eb4cd3b2c3534720d` | `a8456ac573c38a656c9b31c1a928fc9e072fd0e6e518209c05484d80a5f9b22e` |
| 1 | `joint-1793072756-960751169` | `1d64233f6bb17681cdaa3e4eac4147a04d3437451a08599ceedbc2b93e7e7d04` | `614d55fadbc2fb0b14265b55774cf3046b3b71481f98b7112d0622fe704d43cc` |

Validation runs are `1793072939-167539238` / `1793072953-323341949` for coefficient 0.25
and `1793072967-325207231` / `1793072981-502237001` for coefficient 1 (cases 1 / 6).
Use `--model out/models/<directory>/weights.half --gate-model out/models/<directory>/gate.f32`
with the existing source-history capture configuration. No new runtime model is enabled
by default.

Evidence: `build/neural-evidence/joint-v1/manifest.json` on Pop!_OS embeds training source,
dataset/weight hashes, curves, numerical/gradient checks, quantization probes and complete
paired comparisons. Its external-source bundle SHA-256 is `fece003dafaaa0c0bfab4e5a658236263f967c8884a50002a9cf8804b51e8d12`. Renderer/capture
source SHA-256 is `4b4a798e869a65707547b63081508a87f50fd551eb45f880776e09ad0e39f503`.
The external trainer/evaluator and their sources remain outside the tracked repository.

### Additional colour, footprint and fast-motion development cases

The analytic catalog now preserves cases 0–7 and adds cases 8–71, with deterministic variation
in foreground/background colour, spacing, width, checker footprint and signed speed. The new
geometry parameters are dyadic fractions: periods 2.5–14.4375 pixels, widths 0.125–3 pixels
(clipped below the period), checker cells 0.875–8.3125 pixels, and speeds from ±0.25 to ±32
pixels/frame. Colours vary independently, with foreground radiance reaching 4.046875.
The same independent box integral remains the target. No target, integrated coverage, colour
parameter or future frame is added to inference.

These are additional parameter cases of the **same analytic family**. They are neither
production-content evidence, whole-generator holdouts, the sealed release suite, nor a
complete G3 test. Existing Wrela trainers retain their legacy 0–7 case restriction;
the broader catalog is initially an evaluation instrument.

Before viewing these results, the stress plan selected cases 8/10/13/18/21/23, covering
−32/−8/−1/+1/+8/+32 pixels/frame. Each is 32 frames at 192×108, all four scales. Three methods
were captured: production, the original spatial model with constant source history, and the
joint coefficient-1 candidate selected on the preceding legacy results. No fitting used
these six extra cases. Subsequent use makes them development feedback, not unseen release data.

| Method | Median clip-mean 4× FLIP | Median clip-mean 4× LPIPS | Median clip opaque temporal MAE |
| --- | ---: | ---: | ---: |
| Production | 0.487061 | 0.482253 | 0.022780 |
| Constant source history | 0.454154 | 0.370478 | 0.021027 |
| Joint coefficient 1 | 0.448415 | 0.358328 | 0.025932 |

Each entry is the median of six clip scores, not a median ratio or a population estimate.
The joint model has worse opaque temporal error than the constant control in **every** clip.
At case 8 (−32 px/frame), that error is 5.101× larger and FLIP is 9.2% worse; at case 18,
LPIPS is 7.3% worse. Per-clip/all-scale scores, mask populations, changed/unmatched regions,
reveal errors, and production comparisons are retained. Absolute spatial error is far above
G1; small aggregate image gains cannot override the motion/tail failures.

Catalog checks cover all new cases' integral-domain bounds and fast-motion endpoints. Dense
128×128 independent point quadrature agrees with the analytic integral within 1e-4 on every
scored stress case plus six additional cases, at three times and four pixel locations,
including negative coordinates. The dyadic boundaries align with this quadrature's cells.
A deliberate 10% bias in the bright-background integral fails the new test; restoration
passes. This is a reference check for this analytic family, not independent light transport.

A fresh 32-frame legacy case-1 capture has all 384 image/target/mask files byte-identical to
the earlier control across all four scales. Case 71 captures successfully; case 72 is rejected
with usage before capture. Sequence metadata now names `legacy8_and_stress64_v1`.
Reproduce a production stress clip with
`wrela tool reconstruct -- --scene edges --shot 8 --width 192 --height 108 --warmup 0 --frames 32`;
add the documented patch/history/model arguments for the other methods.

Evidence: `build/neural-evidence/stress-v1/manifest.json`, with the pre-capture plan, all 18
scored run IDs, comparisons, legacy parity and CLI/test controls. Capture-source SHA-256:
`3a6c463c5b519e17807e210c4469b4c6b5e6404a82000793d9f6a84041ef112a`.
The later tested-source archive, which expands only the quadrature test's case list, has
SHA-256 `6d672b52c1972a90f971206ce6aafa885ec797a8bf7c7db8f23462e79ced75a1`. The external bundle is shared with joint-v1 above.

Both training attempts remain rejected. The broader catalog and the packing/replay checks
make representation and generalization the next useful focus: preserve observed colour and
surface information while reconstructing detail, and test beyond the original small case set.
No G1–G7 threshold has changed or been met. Focused reference/gradient checks, package checks,
allow and changed-source formatting pass; no full suite/PR is claimed at this checkpoint.


## Raw-sample kernels: clearer detail, rejected quality and cost


At base `46f9c968`, a new full-stack control preserves sixteen raw renderer frames and gathers both authored layer trajectories. It rejects currently occluded sample points, then normalizes positive scene-linear colour weights. Extra source input is geometry only (slat period, width, offset and speed); no checker frequency, source colour, analytic coverage, reference or future frame reaches inference. This remains a two-layer analytic diagnostic, not general 3D or a fixed-input upscaler result.

The [design and primary sources](neural-kernel-reconstruction.md) describe the colour-independent 8→8→1 MLP: 81 f32 parameters, 336 deployment bytes including padding. It modulates tent weights using geometry and sample age. Four whole cases (0/3/5/7), all four scales and 32 cold frames supply 64 seeded pixels/frame, 32,768 current training positions. Two fixed 2,048-step Adam runs use named-view MSE plus opaque temporal MAE coefficients 0 and 0.25. Both final models are measured on legacy cases 1/6 and all six previously declared stress cases, without fitting those cases.

### Legacy 4× results


All values below are means over 32 cold-start frames at 192×108. Lower is better. Temporal error is reference-subtracted opaque correspondence MAE; it is not trail length or a complete G3 test.


| Case / method | FLIP | LPIPS | Opaque temporal MAE |
| --- | ---: | ---: | ---: |
| 1 / old raw | 0.512591 | 0.387900 | 0.077566 |
| 1 / source raw, fixed | 0.255558 | 0.197783 | 0.025210 |
| 1 / learned λ0 | 0.221607 | 0.146160 | 0.016287 |
| 1 / learned λ0.25 | 0.231822 | 0.153543 | 0.015530 |
| 6 / old raw | 0.485842 | 0.334271 | 0.034208 |
| 6 / source raw, fixed | 0.251275 | 0.180354 | 0.016725 |
| 6 / learned λ0 | 0.236069 | 0.138334 | 0.013055 |
| 6 / learned λ0.25 | 0.238755 | 0.139881 | 0.012341 |


The new correspondence control improves detail over the old raw method. Learning further improves legacy FLIP/LPIPS and motion error. It still loses motion stability to the earlier smoother source-geometry constant-history baseline, and absolute FLIP remains far above G1. Every paired comparison verifies identical reference/mask hashes and metric settings.

### Six-case stress results


These are the same public diagnostic cases 8/10/13/18/21/23, with signed speeds 1/8/32 pixels per frame and changed colours/geometry. They are not a sealed suite or an independent scene generator. Relative changes below compare each trained kernel against the new fixed raw-source control at 4×. Negative FLIP/LPIPS change and temporal ratio below one are better.


| Case | λ0 FLIP Δ | λ0 LPIPS Δ | λ0 temporal ratio | λ0.25 FLIP Δ | λ0.25 LPIPS Δ | λ0.25 temporal ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | +2.85% | -3.38% | 0.956× | +17.73% | +1.65% | 1.323× |
| 10 | -2.93% | -10.98% | 0.884× | +2.17% | -14.08% | 0.850× |
| 13 | +1.84% | -5.11% | 1.014× | +2.97% | -8.34% | 0.973× |
| 18 | -4.20% | -18.51% | 0.726× | -5.22% | -19.53% | 0.687× |
| 21 | -4.82% | -16.58% | 0.756× | +5.06% | -11.14% | 0.594× |
| 23 | -1.16% | -3.27% | 0.786× | +23.06% | -0.18% | 0.680× |


**Neither model is promoted.** The spatial-only model still regresses FLIP on cases 8/13 and temporal error on 13. The temporal-loss model has larger fast-motion regressions, including case 8. All-scale, reveal, mixed/changed-pixel and worst-frame results remain in the manifest; this table does not replace them. Fixed-view frame-31 inspections of cases 6/13 show more structure but obvious incorrect high-frequency patterns. These still images are debugging aids, not blinded human or motion evidence.

### Controls and limits


- An independent CPU replay checks all 32 frames and all scales of both fixed-control legacy clips; maximum named-view channel difference is below 0.000244. The learned audit samples 257 seeded pixels in every frame/scale of both models and both legacy clips, with maximum difference below 0.000243. Wrong jitter and zero-model negatives are retained.

- All 81 parameter finite differences pass (maximum error 4.41e-13); detaching the normalization denominator fails (gradient difference 0.00420). Constant and nonnegative affine RGB transforms pass in float64 before deployment rounding/clipping. This property follows from geometry-only weights, not from kernel prediction in general.

- A zero head reproduces 640 captured image/input/reference/mask files byte for byte. Fixed and trained cut captures each match all eight fresh-start images across four scales; suppressing the reset produces a measurable error. Non-finite, wrong-length and nonzero-padding model files are rejected.

- The visibility test passes; deliberately accepting the wrong layer makes it fail. Package checks, changed-source formatting and the workspace allow-list check pass. No full-suite or PR readiness claim is made.

- Browser latency and 4K allocation measurements fail G6; [the benchmark record](bench.md#2026-10-26-amd-retained-sample-kernels) includes the 120-second profiler failure and its explicitly shorter-warmup diagnostic. The 336-byte network does not make its history buffers or sample search inexpensive.


The next implementation opportunity is to avoid reading samples that cannot contribute, then investigate a compact cache and broader whole-case training. Preserving colours has helped, but it has not solved sparse support, mixed coverage, general geometry, population quality or deployment cost. G1–G7 remain unchanged and unmet.


Evidence is on Pop!_OS at `build/neural-evidence/raw-source-v1/manifest.json`. Source archives distinguish the initial fixed control, learned captures and timing (which adds a separate retain label). The original missing export-directory attempt and corrected model relocation are retained; binary weights were unchanged.


Learned capture source SHA-256: `be16076190ee333584d739b77b8bdaf123c8beaca4bf4c5fbce62986124381e7`. Timing source: `14deba433e93ca9cd0b745b17af7d62348aea09e6792acb587a15758a0ecb45c`. External scripts/models bundle: `17f238073e0cd7b062338349582b38b9e801bf51d85abaeda8d8d2a191bc44e1`.


| Objective | Model directory | Weights SHA-256 |
| --- | --- | --- |
| λ0.0 | `raw-kernel-1793075359061891164` | `168d11903b4bedc831ca3816275338ae0d96fa17d308389f7454bc406c1921bc` |
| λ0.25 | `raw-kernel-1793075364934390340` | `16d099f82d3dda4e447bbdf8668d4060bff3276a2562acb28cfbadf9ab9999d6` |


## Sample-search cost and lossless observation packing

Starting at `0cc8b17c`, two measured implementation stages reduce sample-search
cost without retraining or promoting either raw-kernel model. Stage A rejects
candidates before loading their observations when even the accepted motion error
cannot give them positive support. Stage B bounds the existing candidate loops
and replaces 32-byte records with 20-byte records. Colour was already Rgba16Float;
motion and depth remain f32. The horizon stays sixteen frames, source inputs stay
geometry-only, and the original 32-byte diagnostic export remains available.

Stage A reproduces 27 clips / 9,672 image, reference and mask files byte for byte,
including cuts and the no-reset negative. It also passes four 32-frame high-resolution
command/pipeline/final-display comparisons against immutable original builds.

Stage B's first strict HDR comparison **failed** on learned output. This failure is
retained. Packed-only and bounded-loop-only isolation each changes learned rounding;
repeating Stage A remains exact. The raw cache export is byte-identical across all
four scales, and an AMD GPU probe round-trips all 63,488 finite half-float patterns.
The bound tests fail when their accumulated motion margin is removed; the precision
test fails when motion is incorrectly rounded to half. These controls separate stored
information from arithmetic/output rounding.

Before completing the audit, the revised diagnostic plan requires at most one
positive finite half-float code step per reconstructed RGB channel, exact alpha,
references and masks, and fresh perceptual/temporal comparisons. It does not alter
any G1–G7 gate. All 27 clips / 9,672 files meet it; 7,522 files are byte-identical.
The fixed-filter clips are exact. Cut/fresh images match exactly across all four
scales, while suppressing the reset changes all eight corresponding images.

Both models are re-scored on every frame/scale of the eight evaluation cases.
Maximum per-frame absolute score changes are FLIP
0.00004244, LPIPS
0.00004542, and PSNR
0.00015775 dB.
Maximum aggregate opaque temporal MAE change is
0.000000579.
The declared diagnostic limits are 0.0001 FLIP/LPIPS, 0.01 dB PSNR, and
0.00001 opaque temporal MAE. Reference/mask hashes, metric definitions and learned
LPIPS weights match. All other temporal categories remain in the record.

The final displayed frames at 1080p and 4K match exactly for fixed and learned
timing fixtures. The whole `wrela same` result is false because cache layout and
pipelines change; only its final screen comparison is used here. This complements
the continuous 192×108 HDR audit and is not a full-resolution sequence audit.

[Browser measurements](bench.md#2026-10-26-amd-sample-pruning-and-packed-history)
show useful implementation gains, but latency remains far beyond G6. The 4K fixture's
reported allocations now fit below 384 MiB; complete browser/driver memory and
production performance remain unproven. Packing existing observations is not G4's
16× learned appearance compression. No new quality, population, G3, G5 or G7 success
is implied. Both models remain rejected and G1–G7 remain unchanged and unmet.

The remaining search still considers sixteen frames for each layer. The next
architectural experiment should test a compact cache of observations in surface
coordinates, with explicit collisions, age and visibility accounting, or amortized
filter evaluation. Broader training alone cannot remove this measured cost.

Evidence on Pop!_OS: `build/neural-evidence/raw-kernel-prune-v1/manifest.json` and
`build/neural-evidence/raw-kernel-packed-v1/manifest.json`. Packed capture source
SHA-256 `77db68f801b6de876cbcf1a45821ec9827b41eed48271b0567e63c66b41ff9a9`; timing source
`5673e31d9f1fc73787d260582c712a2bd265b03144cbc2390f14596d96894a89`; external scripts bundle
`62469e6afbbd6dbb12603b0adee2bcf76cf51010b515744674ea9ad65cbee8c3`.
Focused checks, changed-source formatting and allow-list results are retained.
This is an implementation checkpoint, not full-suite or PR readiness.
