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

A synchronized linear-HDR accumulator and first real-renderer development instrument now run
on the hardware AMD GPU. Extend them to perceptual metrics and difficult scenes, then establish
the training/validation corpus before model search. Keep capture time fixed while spatial
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
