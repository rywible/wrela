# Vesper · Processional Beast

Original creature and single-agent authoring workspace, 11 September 2026.

This is the first-iteration record. See [current tool iteration](VESPER_TOOLS.md)
for subsequent capabilities, measurements and current staging.

Soundstage is left on Sanctuary → Vesper → Performance, paused at Attend.
Use **Play phrase** to run the complete motion. The named checkpoint
**Vesper authored performance** restores the reviewed source and staging.

## Retained evidence

- [Twelve-second 1080p performance](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-performance-20260911-145358-73246a/performance.mp4), 30 fps offline renderer capture. Each of 360 frames has source, shader, camera and motion metadata in the adjacent `frames` directory.
- [Exact front/side motion study](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-final-motion-20260911-145126-8b57ec/report.json), 48 frames including both loop endpoints.
- [Dry lighting study](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-final-dry-20260911-145209-9ab625/report.json), 32 images.
- [Wet lighting study](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-wet-verified-20260911-145325-868d15/report.json), 32 images; captured metadata confirms wetness 1.
- [Working-reference style board](/Users/ryanwible/projects/wrela/.soundstage/studies/style-board-20260911-144015-25079a/index.html), inspected in Soundstage's native review window before the final jaw/pitch refinement. References are provisional.
- [Frozen construction baseline](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-before-refinement-20260911-142827-f28988/report.json).
- [Final saved study](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-final.json).

The dry and wet studies cover front, quarter, back and above under noon, golden
hour, sunset, afterglow, overcast, rain, softbox and room. Read the report/viewer
source, inspected every view in labeled contact sheets and inspected full-size
motion and material frames. A browser attempt to open the local motion HTML was
blocked by its URL policy; the local HTML and its actual PNGs were inspected
without that browser. No generated illustrations are renderer evidence.

## Visual assessment

The mask, four horns, layered mane, embroidered teal mantle, trim, beard and tail
are original procedural construction. Eight beats describe a measured rise,
coil, sweep and recovery with four timed steps. Side review caught and corrected
the signs of the jaw opening and forward pitch. The camera envelope includes the
rising pose. The final side frames show the lower jaw opening downward and
planted paws carrying the lean.

Silhouette, motif placement, two-sided mantle shading and material highlights
remain coherent in the inspected views. The low-light shared scene look leaves
the mantle and legs too dark in golden hour, overcast, rain and afterglow. No
asset-specific exposure compensation was applied. Wet cloth becomes visibly
shiny and loses some textile character at wetness 1. The mane still reads as
sculpted tapered locks, the limbs are segmented and the underside is simple.
This is a stylized authoring specimen, not production-quality Elden Ring fidelity.

## Verification

- Clean native build and renderer verification on Apple M4. Renderer diffuse error 0.006604582; no GPU errors in the performance protocol check.
- `swift test`: 69 tests, zero failures, 10.946 seconds.
- Creature tests cover 721 performance samples, finite transforms, analytic reach residual below 2 mm, planted-foot stability below 2 mm, actual transformed limb endpoint agreement below 0.1 mm, deterministic seeking and downward jaw opening.
- `validate-performance`: visible key edits, invalid inputs rejected atomically, exact future document and GPU replay, undo/redo, named beats, tempo rescaling and indexed rendering pass.
- `validate-creatures`, `validate-workshop`, `validate-authoring`, `validate-cave`, `validate-expedition` and `check-boundaries` passed. An initial expedition harness run was invalidated by concurrent source editing; its stable rerun passed.
- Native Motion frame stepping, Performance play/freeze and skeleton view, and Behavior visitor/ten-second controls were exercised with screenshots.

## Fingerprints

Movie frame 0 source digest: `4423a510085b4d27ea2ff3e8daee82fae8eea3c1f9ea363ce8b1169e392ebf32`.

Shader digest: `3fd50761fc3a086359cc13d36019daa02e1bdf7da8fc1d7b5f81a8dfcf4d9a06`.

Asset source: [vesper.json](/Users/ryanwible/projects/wrela/Games/Sanctuary/Authoring/Assets/vesper.json).
Compiler, geometry and score ownership: [workspace notes](/Users/ryanwible/projects/wrela/docs/CREATURE_WORKSPACE.md).

## Remaining scope

Authored linear-blend cloth/hair deformation has no collision solver. Base IK
assumes a flat floor; additive joint corrections require rendered contact review.
The game specimen is a temporary presentation preview, without creature collision,
combat, saved population or independent terrain contact for each foot. Skinned
surfaces currently use full detail and skip bind-pose culling; crowd optimization
and deformation LOD remain work. The editor exposes semantic anatomy and pose
corrections, while arbitrary new anatomy is authored in the project Swift recipe.

## Live studio measurement

[20.41-second report](/Users/ryanwible/projects/wrela/.soundstage/profiles/vesper-live-20260911-145612.json).
Native 1920×1080, 4× MSAA, M4, live performance with softbox lighting.
Submitted and presented rate: 59.93 fps. GPU median 6.32 ms, p95 7.49 ms,
p99 8.34 ms, maximum 10.74 ms. CPU frame-interval median 16.67 ms, p95
17.67 ms, p99 24.97 ms, maximum 42.75 ms; presentation timestamps include a
33.33 ms maximum interval. The average rate does not erase these delivery hitches.
The offline movie was captured separately and was not used for these measurements.

## Sanctuary integration

[Native game capture](/Users/ryanwible/projects/wrela/.build/vesper-integration-20260911-145648/runtime/captures/vesper-garden-final-52f56d7b.png). Tested in an owned temporary game workspace,
with a separate save directory, leaving the player's default save untouched.
The same source is uploaded through `AnimatedAsset` and evaluated through
`AssetSource.poseMatrices`; Soundstage is not a game dependency. Native movement
and live animation were inspected with actual app screenshots.

[20.41-second live garden report](/Users/ryanwible/projects/wrela/.build/vesper-integration-20260911-145648/runtime/profiles/vesper-garden-live-20260911-145808.json).
At 1920×1080, submitted rate was 60.02 fps and presented rate 59.98 fps. GPU
median 14.06 ms, p95 14.82 ms, p99 15.14 ms, maximum 15.91 ms. Frame-interval
p95 17.64 ms, maximum 21.75 ms. Presentation intervals stayed at approximately
16.67 ms. No GPU errors. Simulation and creature time advanced from 28.98 to
49.40 seconds. Normal atmosphere updates remained enabled: air, lighting and sky
phases were all sampled. Sky-update GPU maximum was 15.91 ms; air-update maximum
14.84 ms. These categories describe whole frames and are not additive intervals.

A sequential [garden-only comparison](/Users/ryanwible/projects/wrela/.build/vesper-integration-20260911-145648/runtime/profiles/garden-without-vesper-20260911-145851.json) measured 59.94 submitted
fps, GPU median 12.82 ms and p95 14.30 ms. The specimen run's median was about
1.25 ms higher. This is a short sequential observation with evolving live weather,
not a controlled claim that every creature costs exactly 1.25 ms. Neither run
excluded routine live cache updates. The isolated game process was closed after
verification; no temporary creature remains in the player's saved game.

The encoded movie was opened and played in QuickTime Player; its duration is
12 seconds and its decoded orientation matches the source PNGs.
