# Neural rendering: frontier goals

2026-10-26. Benchmark charter and numerical stretch goals requested and authorized by Ryan.
Every numerical goal below is an aspiration chosen for this project, not a published industry
result, a feasibility estimate, or a measurement of Wrela. Ryan authorized autonomous implementation and full use of the Pop!_OS AMD machine.
All builds, training, and measurements run there; the Mac is occupied. No subagents are used.
No purchase of hardware or paid compute is authorized. Multiple cooperating models are allowed.
Exact SDKs, model presets, supported devices, and runtime versions must be pinned when a
comparison is run; the sources below are not a statement about which release is newest.

## The north star

Render only one sixteenth of the output's base-colour samples, yet reproduce the authored
world's converged full-resolution appearance in motion, within a complete real-time browser
budget. Beat the strongest publicly runnable reconstruction baselines on a sealed test suite.
Preserve authored detail and respond promptly to new visibility, animation, and lighting.

Four times the resolution on each axis means 480×270 → 1920×1080 or 960×540 → 3840×2160.
It does not mean sixteen times less total work: high-resolution depth, motion, field queries,
specialist models, cache updates, and reconstruction all count toward the measured budget.
No full-resolution colour reference, future frame, or reference-derived routing is available
at runtime. UI can be composited at native resolution in every compared pipeline, separately
from scored world rendering. Frame generation contributes no frames to these goals.

## Seven deliberately unreasonable goals

### G1 — Sixteen times fewer base-colour samples, reference-level appearance

At 4× linear reconstruction, reach all of these on the full-stack track:

- Relative to the converged source reference, the median per-clip mean FLIP error is at most
  0.03 under the locked SDR viewing configuration; the worst-decile clip mean is at most 0.06.
  Apply these limits separately to the complete world image and the predeclared detail ROIs,
  so sky/background area cannot conceal damaged geometry.
- In a blinded two-alternative test of reference versus approximation, observers identify
  the approximation with an upper 95% confidence bound no greater than 55%. Test full-speed
  motion, not only stills. The reference is the target, not a native TAA screenshot.
- Pass G3's artifact gates and the applicable G6 deployment profile simultaneously.

These absolute FLIP values are aggressive project targets, not universal visibility thresholds;
the human test and artifact tests are independent requirements. Establish 2× and 3× linear
milestones first, without weakening the eventual 4× target. Claims retain the exact operating
point; a 2× result cannot be reported as meeting the 4× goal.

### G2 — Win a fair upscaler tournament

At each supported common input/output resolution and declared cost budget, on the fixed-input
track, beat the frozen DLSS, FSR, XeSS, Wrela temporal, and strong conventional spatial/temporal
baselines. Select each baseline's best valid configuration using validation data before testing.
The target, against every supported baseline, is:

- At least 25% lower median per-clip mean FLIP error and 25% lower median per-clip LPIPS error.
- At least +3 dB median per-clip PSNR, in the fixed display-referred RGB transform/range.
- At least 50% lower median temporal residual error, as defined below.
- At least 75% preference for Wrela as the closer match to the reference, ties counted as
  half, with the paired/clustered 95% lower confidence bound above 60%.
- No artifact category's median error more than 5% worse on any primary error metric, and no
  clip more than 10% worse on mean FLIP or LPIPS. Separately publish tail errors and worst clips.
- No extra queued frame of algorithmic latency; causal history only.

Run at 2×, 3×, and 4× linear factors where the SDK supports them. Unsupported modes are marked
unavailable. Passing on one supported GPU or one scale does not establish universal leadership.
For timing comparisons use the same machine and supported execution paths; a quality-only replay
result is labeled quality-only. Publish browser-versus-native product timing separately.

### G3 — Preserve detail through the cases that normally break reconstruction

On predeclared instrumented tests, with independent object motion, camera motion, and lighting:

- The 99th percentile stale-image trail behind moving occlusion edges is at most one output
  pixel. A stale pixel has reference-relative displayed-luma error above 2/255 attributable to
  the previous surface; use instance/visibility truth to separate ordinary reconstruction error.
  Report the maximum as well. Cover edges moving 1–32 output pixels per frame.
- Newly exposed surfaces reach within 10% of their settled reconstruction error within two
  output frames for 99% of reveal events. Use the same revealed content/lighting for the settled
  comparator, an error floor of 0.005 FLIP, and the G1/G2 spatial gates to prevent a bad settled
  image from making this easy. Measure the first revealed frame too.
- At least 99% recall and at least 99% precision for reference-visible instrumented strands,
  branches, and holes at least one output pixel wide with displayed-luma contrast ≥0.1. Test
  subpixel features separately with coverage-error curves, without claiming exact recovery.
- No detectable old-scene image after a declared camera-cut reset, using the same 2/255
  stale-content threshold. Resolution changes and streaming arrivals must satisfy the reveal
  gate. No future-frame lookahead or delayed presentation to meet it.

Temporal residual error: let e_t = candidate_t − reference_t in the fixed displayed linear-RGB
space. On opaque surfaces visible in both frames, measure mean absolute
e_t − warp(e_(t−1)) using reference geometry's correspondence. Measure visibility changes in
their own masks; do not discard their errors. For transparent/reflected contributions without
valid surface correspondence, report unwarped reference-subtracted temporal differences and
ColorVideoVDP separately. Average clips and categories equally. Low spatial error is required
alongside low temporal error, so a frozen or blurred prediction cannot qualify.

### G4 — Sixteen times less foliage/material representation memory at matched quality

For an authored foliage family, reduce resident appearance-representation bytes by at least
16× versus the strongest quality-matched conventional atlas/volume representation. Stay within
1.1× its complete foliage-pass GPU time at both p95 and p99. Count latents, weights, shared-model
allocation, decoded pages, fallback atlases, update scratch, and metadata. Declare exactly which
geometry/common buffers are excluded and keep those buffers identical between methods.

Matched quality means no more than 0.25 dB PSNR loss, no more than 5% relative increase in FLIP
or LPIPS (floor 1e-4 for ratios), and no regression on G3's applicable silhouette/motion gates.
Measure coverage, normals, depth, and directional transmission separately as diagnostics.
The test includes unseen views, lights, individuals, footprint changes, and crowns spanning
8–512 output pixels. Train-time asset specialization is a declared track, not zero-shot evidence.

As a richness demonstration, also render source crowns with 10× the leaf primitive count at
fixed canopy bounds and total leaf area. Require ≤10% additional distant-pass cost relative to
the same method's simpler crown, at matched reference quality. Repeat with the conventional
method: this scaling demonstration alone does not establish a neural advantage.

### G5 — Four times cheaper dynamic indirect lighting at matched reference error

On a fixed scene suite with independently converged light-transport references, achieve the
strongest conventional baseline's spatial/temporal error at one quarter of its complete GI GPU
cost, including feature extraction, rays, training/updates, cache refresh, and denoising. Also
target p95 ≤2 ms for the complete GI subsystem at 1920×1080 on the AMD Pop!_OS machine. A victory
requires both the ratio and the absolute budget; quality comparisons use the G4 tolerances.

After a light toggle or moving-occluder event, 99% of designated receiver regions must reach
within 10% of the converged post-change irradiance within four frames at a 60 Hz evaluation
cadence. Use absolute error ≤1% of the scene's declared illumination scale near zero. In analytic
sealed-wall tests with zero expected transport to a receiver, leakage must be below 1% of the
adjacent lit receiver's irradiance. Test colour bleeding, small emitters, emissive changes,
thin occluders, and the removal of geometry. Include all refresh spikes in the timing.

Current high-settings probes/cascades cannot serve as an independent truth for this goal.
Start with small controlled transport scenes, then expand to authored production content.

### G6 — Ship the complete cooperating stack within the AMD browser budget

Ryan selected one measurement machine: Pop!_OS, AMD Ryzen 5 5500U (6 cores / 12 threads),
AMD Radeon Graphics (RADV RENOIR, device 1002:164c), about 13 GiB usable system RAM, Mesa
26.1.6. Freeze the exact browser, kernel, power/thermal conditions, compiler, and adapter in
all records. The Mac is occupied; no RTX machine is available. Neither is a required validation
host for this effort, and no performance result is extrapolated to either.

| Profile on this same AMD machine | Actual output | Full-frame throughput stretch target | Reconstruction subtotal, p95 / p99 |
|---|---|---|---|
| Main | 1920×1080 | CPU critical path and GPU frame span p99 ≤8.33 ms | ≤1.0 / 1.5 ms |
| High resolution | 3840×2160 | CPU critical path and GPU frame span p99 ≤8.33 ms | ≤2.0 / 3.0 ms |

Both are deliberately aggressive 120 rendered frames/s targets, not estimates of feasibility.
A first deployment milestone is 1080p at 60 rendered frames/s (p99 ≤16.67 ms); it does not meet
this final goal. A 60 Hz panel or headless timer cannot prove 120 Hz presentation: report
rendering throughput separately. Presentation/latency claims require a measured display path
at its actual cadence. In paced runs target fewer than 0.1% missed presentation deadlines
attributable to the renderer over twenty minutes after thermal settling. Separately retain
cold/startup/cut/streaming measurements. Do not hide asynchronous work outside a reconstruction
label, change scene density, or count generated frames. Comparators unsupported on this AMD
adapter are unavailable, never scored as losses or used to support a victory claim.

Across all cooperating models, target ≤8 MiB compressed shipped weights, and ≤128 MiB total
model/feature/history/cache/scratch GPU allocation at 1080p or ≤384 MiB at 4K. Count buffers even
when shared with a conventional path; additionally report whole-renderer peak memory and all
asset-specific latent bytes/downloads. These are shared caps, not a fresh allowance per model.
Compare against an ablation with every learned contribution disabled and quality held fixed.
G1 quality and G3 motion must hold in each profile for the full north-star claim; reduced-quality
fallbacks are useful deployment results but do not satisfy that claim.

### G7 — New authored content teaches its own affordable representation

Across at least twenty held-out field assets/edits in declared supported families, an author
changes source parameters and invokes the same build operation. At least nineteen must produce
a representation passing the existing quality gates in ≤5 minutes each on a frozen training
machine, without asset-specific trainer/decoder code edits or manual hyperparameter tuning.
All must either validate successfully or select a measured conventional fallback; no silent
quality failures. Separate exact-source hashing, data generation, fitting, validation, and
installation times. Include every failed attempt in the reported success rate and cost.

For at least one genuinely expensive family of material/field evaluations, target ≥10× lower
GPU evaluation cost at matched quality versus the best conventional compiled/cached alternative
under the same memory cap. Show whole-frame benefit too. A 10× microbenchmark victory that is
irrelevant to the actual frame does not meet the demonstration. Shared pretraining time, data
generation cost, and its contribution to held-out knowledge must be disclosed separately from
the five-minute specialization target.

## A score an agent can actually hill-climb

Use a development set for debugging, a fixed validation suite for candidate selection, and a
sealed release suite of at least 100 independent five-second clips across at least ten artifact
categories. Run separately at 60 and 120 Hz simulation/output cadence where claimed. Scene,
asset-family, parameter, light, animation, and trajectory splits are explicit. Whole-generator
holdouts belong to the generalization track; per-asset trained representations have a separate
specialization track. Reference frames and release seeds cannot enter training or routing.
For a general upscaler leadership claim, at least thirty release clips must come from at least
two independently implemented non-Wrela renderers, with valid reference/input metadata and
rights to evaluate and publish the captures. The Wrela-specific full-stack track is reported
separately. A result confined to Wrela content is labeled accordingly.

Freeze baselines on validation data. For each candidate at a given operating point:

1. Reject invalid inputs, causality violations, degraded negative controls, or a violation of
   the current milestone's motion/memory/latency caps. Mark other candidates infeasible; retain
   their cost/error points for diagnosis rather than calling them winners.
2. For feasible candidates, minimize the geometric mean of FLIP, LPIPS, and temporal residual
   error ratios to a frozen baseline, with equal clip/category weighting and denominator floor
   1e-4. Keep all three scores and their tails visible. The scalar is a search aid; it cannot
   override the separate G1–G7 gates or human judgments.
3. Promote a candidate only for ≥1% lower aggregate error with a paired 95% confidence interval
   supporting improvement, no category median primary error worsening >5%, and no clip spatial
   error worsening >10%. Use clips, not correlated pixels, as resampling units. A lower-cost
   candidate at matched quality may also enter the Pareto frontier instead of replacing the
   quality winner. Keep best-under-budget checkpoints for each device/scale.
4. Test the sealed release suite only after checkpoint/configuration freeze. Publish complete
   per-clip scores and failures. After a release has influenced development, replace its sealed
   audit portion with fresh cases. Any world-leading claim names the frozen competitors, device,
   scale, date, and test distribution.

For blinded release studies use at least thirty observers and one thousand balanced clip
judgments per main comparison, with clip and observer clustering in the uncertainty estimate;
increase sample size if the confidence criterion remains unresolved. Include full-speed clips,
not solely crops. No agent judge substitutes for this human gate.

Initial sequence: establish synchronized HDR truth and negative controls; build the fixed-input
replay/benchmark; reach a credible 2× reconstruction result; hill-climb through 3× and 4×; then
add field-aware appearance and lighting specialists when their ablations improve the stack.
The seven stretch goals remain fixed. Early milestones measure progress and do not count as
completion of the north star. Model architecture, number of models, training curriculum, and
cache placement remain open experimental choices.

## Define the claims before optimizing

Keep three results separate:

1. **Reconstruction:** how well an upscaler reconstructs a reference from the same rendered
   signals at explicit input and output dimensions.
2. **Whole rendering stack:** how well each complete pipeline renders the same authored scene
   at the same measured end-to-end budget, allowing Wrela-specific fields and extra signals.
3. **Deployment:** what the actual browser build costs on each supported device, including
   frame-time tails, memory, startup, downloads, and model/cache refreshes.

A quality win with more features is evidence for the whole stack, not for a superior
equal-input upscaler. A native vendor SDK's time on a different GPU is not a browser speed
comparison. “World-leading” needs a bounded, published meaning: which released systems,
devices, scenes, quality dimensions, and budgets were tested.

## Public competitors and integration controls

| Primary source | What it establishes | Comparison consequence |
|---|---|---|
| [NVIDIA Streamline DLSS programming guide](https://github.com/NVIDIA-RTX/Streamline/blob/main/docs/ProgrammingGuideDLSS.md) | Required depth, motion, input/output colour; exposure handling; camera constants; feature-support query; D3D/Vulkan integration | Validate support and buffer conventions before scoring; record preset/model/runtime, not only a marketing quality label |
| [AMD FSR Super Resolution manual](https://gpuopen.com/manuals/fsr_sdk/techniques/super-resolution-upscaler/) | Temporal reconstruction inputs and explicit reactive/transparency information | Give a comparison its documented masks; missing masks can penalize transparency unfairly |
| [AMD ML upscaling manual](https://gpuopen.com/manuals/fsr_sdk/techniques/super-resolution-ml/) | Jitter, motion, mip bias, history reset, HDR input, exposure, and performance configuration | Record actual dimensions, mip bias, sharpening, and reset behavior; vendor example timings are not Wrela measurements |
| [Intel XeSS-SR developer guide](https://github.com/intel/xess/blob/main/doc/xess_sr_developer_guide_english.md) | High-resolution motion input or low-resolution motion plus depth; jitter, exposure, history reset; API/device paths | Declare the selected implementation and its support; do not silently compare distinct acceleration paths as one implementation |

The guides describe native SDK integrations; they do not establish interchangeable WebGPU
drop-ins. A common native evaluation harness can replay Wrela's rendered signals to supported
SDKs. Actual browser performance remains a separate measured result. On a device where a
competitor cannot run, mark it unavailable rather than inventing a timing or treating it as a
loss. Evaluate super resolution separately from frame generation and ray reconstruction.

For fixed-input comparisons, render identical scene states at identical resolutions and use
the same semantic signals, with the conversion/dilation each SDK requires. Correct jitter
phase, motion direction/units, exposure, depth, material animation, and reset flags are part
of a valid integration. Also provide a documented best-integration track if SDK-recommended
jitter or texture filtering differs. Do not force a deliberately poor configuration to make
buffers byte-identical. Count all additional signal-generation and conversion passes.

## Replace the reference before treating it as an oracle

The existing [lookdev protocol](lookdev.md) freezes time, averages 64 reference samples, and
tests a camera dolly. [`quality::Driven::screen`](../studio/lookbook/quality.wrela) captures
RGBA8 screen output; [`measures::Mean`](../studio/lookbook/measures.wrela) averages those bytes.
This is useful for its stated static regression tests, but averaging display-developed,
quantized images is not equivalent to accumulating linear radiance and developing once.
The documentation itself says higher-settings probes/fog/cascades test convergence, not
algorithmic bias.

Proposed reference requirements:

- Render a synchronized reference at every evaluation time from a deterministic simulation
  snapshot. Accumulate spatial samples in floating-point linear HDR, then apply the same
  exposure and display transform once. Advance simulation between output times, not between
  spatial samples of one frozen-time reference image.
- Declare pixel reconstruction filter, shutter interval, motion blur, and depth-of-field
  settings. If testing shutter-integrated motion blur, integrate the intended shutter with
  corresponding scene states rather than accidentally averaging successive playback frames.
- Establish convergence by independent sample sets and a tighter reference. Estimate remaining
  reference noise; a candidate cannot reliably “win” below that uncertainty. Require the
  independent/doubled-sample reference discrepancy to be at most one tenth of the relevant
  allowed candidate error, measured in the same metric and predeclared region. Raise the sample
  count or mark that region unresolved when it fails; a fixed sample count alone is not proof
  of convergence.
- For upscaler isolation, preserve the same underlying lighting/material function while
  replacing only reconstruction. For whole-stack truth, add small analytic or independently
  converged light-transport scenes that can reveal bias shared by play and high-settings mode.
- Export depth, material/instance/part IDs, visibility changes, and reliable surface motion
  beside colour. Transparent layers and moving reflections need their own treatment; a single
  opaque-surface vector is not an exact correspondence for every visible contribution.

Sampling, filtering, and radiance integration determine the target image; more samples alone
do not define the target. [PBRT, Sampling and Reconstruction](https://pbr-book.org/4ed/Sampling_and_Reconstruction).

## Use complementary scores and targeted artifact measurements

| Measure | Useful role | Limitation to preserve in the charter |
|---|---|---|
| PSNR/SSIM in a named, fixed colour space | Reproducible numerical regression and structural checks | Do not silently change exposure, range, transfer function, or crop; neither is sufficient for temporal or perceptual ranking |
| [LDR/HDR-FLIP](https://github.com/NVlabs/flip/) | Spatial perceptual error maps; HDR evaluation across exposure/tone-mapping conditions | Image metric, not a temporal-artifact detector; pin viewing parameters and HDR configuration |
| [LPIPS](https://github.com/richzhang/PerceptualSimilarity) | Complementary learned image-patch perceptual difference | Pin backbone/calibration and normalized RGB preprocessing; framewise LPIPS is not temporal or absolute-luminance HDR assessment |
| [ColorVideoVDP](https://github.com/gfxdisp/ColorVideoVDP) | Colour and spatiotemporal visible-difference prediction for SDR/HDR sequences | Requires declared frame rate, display geometry, photometry, colour space and viewing conditions; raw scene-linear EXR values are not automatically display luminance |

Add proposed diagnostic scores with exact definitions and saved masks: error on newly exposed
surfaces; persistence of stale detail after a moving occluder; silhouette coverage and thin-line
survival; time to recover after a cut or resolution change; highlights/reflections; transparency;
and motion-compensated error changes on surfaces visible in both frames. Compare temporal
changes with the synchronized reference's changes, so real animation is not counted as
flicker. Measure newly visible regions separately rather than hiding them in a warp mask.

Retain negative controls for blur, ghosting, missing detail, over-sharpening/ringing, delayed
response, and fabricated detail. Any metric that rewards the degraded control needs a
complementary gate. Use blinded human comparisons of lossless real-time clips, alongside crops
and slow motion for diagnosis; an agent's preference or one aggregate score is not conclusive.
Report per-scene and per-artifact results, tail errors and worst clips, not only a mean.

## Make hill-climbing survive an unseen test

These are proposed evaluation rules, not industry measurement claims:

- Split by scene, procedural family/asset, seed, camera path, animation, lighting and weather
  changes; nearby frames from one trajectory must not straddle training and test. Include
  withheld combinations and out-of-distribution scenes, not only unseen random pixels.
- Keep a development set for repeated hill-climbing and a sealed final test for locked
  checkpoints. Regenerate a separate audit set after major architecture decisions. Otherwise
  repeated checkpoint selection turns the test into training feedback.
- Multiple models, specialist branches and routing are allowed, but the route must be causal
  and available at runtime. Count all resident weights, features, history, cache, selection,
  switching and refresh costs. Oracle routing using the reference or future frames is a
  separate non-deployable upper bound.
- Log data-generation cost, training compute/time, seeds, optimizer and checkpoint selection,
  model/weight hashes, compiler options, quantization and calibration data. Keep training and
  inference budgets distinct.
- Cover static detail, camera translation/rotation, independent object/deformation motion,
  foliage/water/particles, disocclusion, specular motion, exposure changes, camera cuts,
  dynamic resolution, LOD transitions and streaming arrivals. Include cold starts and
  history resets instead of discarding all difficult warmup frames.
- Use paired repeated runs on the same device; state power/thermal conditions and warmup.
  Record GPU pass time, end-to-end frame distributions, missed deadlines, memory and latency.
  Separate model time from necessary preparation, history, postprocessing and asynchronous
  work. An inexpensive inference pass can still increase frame tails or total memory.
- Define budgets in milliseconds, bytes and actual input/output pixels, not “Quality” or
  “Performance” labels. Fix sharpening and display processing, or account for a separately
  tuned track. Show quality-versus-cost curves rather than one favorable operating point.
- Use paired uncertainty estimates with scenes/clips as statistical units. Millions of
  correlated pixels are not millions of independent trials. Publish the failing cases and
  reproducible input captures as well as the strongest result.

The numerical stretch goals above remain aspirations until the locked test and measured
deployment satisfy them. Source updates may motivate a new benchmark version; never change a
published test or its thresholds after seeing a candidate's result and retain its old label.
