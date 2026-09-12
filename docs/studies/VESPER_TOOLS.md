# Vesper · agent authoring and contact rehearsal

Second autonomous iteration, 11 September 2026. This extends the original
[Vesper study](VESPER.md), whose measurements and limitations describe the first
iteration. One agent authored the tools and the creature.

## Outcome

The agent can query actual compiled surfaces, submit a revision-checked local
sculpt/animation/contact transaction, undo it in one step, and inspect the final
pose across four synchronized views. Seven local surface fields refine Vesper's
mask and mane; related eye geometry follows the mask edits. Continuous weighted
limbs with an editable muscle-volume parameter replace the previous segmented
construction. Five performance tracks add body lowering, lateral weight shift,
mask phrasing and overlapping mane movement. Arrival/departure tangents allow
smooth transitions instead of forcing a stop at every key.

A warm mask/eye revision took 60.98 ms: two draw batches rebuilt, twenty reused.
The preceding seven-field edit took 118.15 ms. These are individual observed
local edit times, not cold compile times or a percentile benchmark. Motion,
contact offsets and collider revisions can retain the compiled geometry.

## Physical rehearsal and limits

The final contact solver runs after additive body animation. Steps and slopes
are replayable study fixtures; Sanctuary samples the actual terrain under each
foot through the same evaluator. Native solver on/off inspection at 7.5 seconds
on a 0.3 m step with 0.1 cross slope exposed uncorrected errors of 216.5, 408.9,
103.9 and 105.5 mm. With correction enabled all four displayed 0.0 mm.

A 121-sample final flat-floor report measured maximum reach error below 0.001 mm,
maximum planted clearance below 0.001 mm and planted speed below 0.00001 m/s.
The retained step review also reports no sampled proxy penetrations. These
figures measure registered foot targets and four body capsules, not all visible
geometry. Discrete ground capsule queries use seventeen axial samples and can
miss thin features or between-frame collisions.

There is no mass, force, friction, balance, rigid-body collision response,
continuous collision detection, cloth collision or hair collision solver.
The contact system can expose unreachable targets but cannot invent a recovery
step. Contact solving overrides lower-chain pose intent to meet the authored
foot target; higher-level acting still needs visual judgment. Surface fields
preserve topology and weights; sampled positive Jacobians do not prove global
injectivity. Edited parts use full detail until deformation-aware LOD exists.

## Retained visual evidence

- [Updated twelve-second performance](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-tools-performance-20260911-162914-2cb374/performance.mp4): 360 exact-time 1080p renderer captures encoded at 30 fps. This is offline review evidence, not a live benchmark.

- [Final flat study](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-tools-final.json).
- [Stepped rehearsal study](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-tools-step.json).
- [Four-view contact review](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-contact-review-20260911-162637-bfacc0/index.html): 24 GPU captures at six times, synchronized front, quarter, right and back.
- [Dry lighting review](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-tools-dry-20260911-162723-264052/report.json): 32 GPU captures.
- [Wet lighting review](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-tools-wet-20260911-162803-9e0dac/report.json): 32 GPU captures.

The dry/wet reviews cover front, quarter, back and above under noon, golden
hour, sunset, afterglow, overcast, rain, softbox and room. Every frame was
inspected in labeled contact sheets, with full-size dry/wet quarter images and
side motion images also inspected. Native four-view playback was exercised;
all four views fit the review window. Native solver toggling and single-frame
stepping were exercised. The source has a named checkpoint, **Vesper contact
authoring**. Review captures include exact studies and source/shader fingerprints.

The result has fuller limbs and a more deliberate mask silhouette, with a
crouch that maintains its foot placements on uneven ground. It is still a
stylized procedural specimen. The mane is visibly a repeated arrangement of
sculpted locks; the cloth underside and leg anatomy remain simple. Wetness 1
makes cloth overly shiny. Dim weather loses leg and mantle readability under
the shared scene lighting. No per-object exposure compensation was introduced.
This is not yet an Elden Ring fidelity result.

## Verification

- Final native build succeeded.
- `swift test`: 73 tests, zero failures, 11.404 seconds.
- New tests cover local deformation derivatives and normals, missed-brush rejection,
  Hermite velocity, post-body contact correction, unreachable targets, segment
  endpoint agreement and capsule separation.
- `validate-creature-tools`: revision rejection, atomic undo/redo, actual surface
  probes, visible local edits, post-animation ground contacts, planted slip,
  disabled-solver penetration, exact future GPU replay, proxy overlap and tangent
  rescaling passed.
- `validate-performance` and `check-boundaries` passed. Game hosts have no editor
  dependency and no species branches were added to the shared authoring tool.

## Fingerprints

Contact-review frame zero source digest:
`79eafc222d6e6aa7014bb88f5e2545b9fda618c09fc51994de9ac650f5ff7179`.

Shader digest:
`3fd50761fc3a086359cc13d36019daa02e1bdf7da8fc1d7b5f81a8dfcf4d9a06`.

See [authoring reference](../SOUNDSTAGE.md#agent-native-local-authorship-and-physical-rehearsal)
and [workspace capabilities](../CREATURE_WORKSPACE.md) for command contracts and
ownership. The new mathematical source remains in FieldCore, compiled surface
transforms in FieldCompiler, reusable runtime contact/skin evaluation in
FieldEngine, fixtures and editor state in SoundstageKit, and Vesper's anatomy,
acting and registration in Sanctuary.

## Live studio measurement

[20.39-second live report](/Users/ryanwible/projects/wrela/.soundstage/profiles/vesper-tools-live-20260911-163124.json).
1920×1080, 4× MSAA, Apple M4, live Vesper phrase with softbox lighting.
Submitted rate 59.97 fps; presented rate 60.02 fps. GPU median 4.69 ms,
p95 7.01 ms, p99 8.14 ms, maximum 12.03 ms. CPU frame-interval p95 17.64 ms,
p99 23.71 ms, maximum 35.39 ms. Available presentation intervals stayed near
16.67 ms. No GPU errors. These delivery metrics are reported separately;
a nominal 60 fps average does not erase CPU submission hitches. No capture,
compilation or movie playback ran during this measurement.

The movie was opened and played in QuickTime Player. Its twelve-second duration,
orientation and framing match the actual renderer captures. The low sweep and
planted limbs were inspected during playback.

The final [working-reference style board](/Users/ryanwible/projects/wrela/.soundstage/studies/style-board-20260911-162844-60353d/index.html)
was inspected in Soundstage's native review window; its reference art remains
provisional. [Authoring check results](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-contact-review-20260911-162637-bfacc0/creature-tools-validation.json)
retain all 20 checks; the adjacent performance report retains 15 checks.

## Existing-stack regression checks

Renderer verification passed on Apple M4, with diffuse irradiance maximum error
0.006604582. Isolated native suites passed for
[creatures](/Users/ryanwible/projects/wrela/.build/test-runs/20260911-163202-authoring-c676d/index.html),
[workshop](/Users/ryanwible/projects/wrela/.build/test-runs/20260911-163214-authoring-8e1d7/index.html),
[authoring](/Users/ryanwible/projects/wrela/.build/test-runs/20260911-163222-authoring-2ff22/index.html),
[Cave](/Users/ryanwible/projects/wrela/.build/test-runs/20260911-163235-host-c2148/index.html) and
[Sanctuary](/Users/ryanwible/projects/wrela/.build/test-runs/20260911-163245-host-a8c39/index.html).
Source remained stable throughout these runs. The native game integration used
an owned temporary workspace and save directory, preserving the player's slot.

## Sanctuary live integration

[Final paused garden capture](/Users/ryanwible/projects/wrela/.build/vesper-integration-20260911-163320/runtime/captures/vesper-contact-garden-df0b991e.png).
Native movement and live animation were exercised in the owned Sanctuary build.
An app-name lookup initially opened a second installed preview; it was closed
before profiling, and the intended build was selected by its complete path.

[20.33-second live garden report](/Users/ryanwible/projects/wrela/.build/vesper-integration-20260911-163320/runtime/profiles/vesper-tools-garden-20260911-163524.json):
1920×1080 with Vesper, submitted/presented rate 59.03 fps. GPU median 17.83 ms,
p95 28.67 ms, p99 32.38 ms, maximum 36.62 ms. CPU frame-interval maximum
36.53 ms; presentation intervals reached 33.33 ms. No GPU errors. Thermal state
was 1. Normal air, lighting and sky updates remained enabled; sky-phase whole
frames reached 35.02 ms, air 17.14 ms and lighting 18.37 ms. These categories
are not additive timing intervals. Simulation and specimen time advanced from
31.40 to 51.70 seconds during the measurement.

A sequential [garden without Vesper](/Users/ryanwible/projects/wrela/.build/vesper-integration-20260911-163320/runtime/profiles/garden-tools-without-vesper-20260911-163606.json)
measured 59.82 submitted fps, GPU median 16.40 ms, p95 25.72 ms and maximum
38.69 ms. The specimen run's median was about 1.43 ms higher. These are short
sequential observations under evolving live weather and thermal conditions,
not a controlled per-creature cost. Both runs retain normal cache work. The
live garden does not have reliable 60 Hz margin in this state; that remains a
performance limitation rather than an excluded update cost.

## Ready state

Soundstage is left on Sanctuary → Vesper → Rehearsal, paused at 7.5 seconds on
the raised cross-slope fixture. **Solve contacts** compares the correction;
**Review four views** produces a synchronized review. **Vesper stepped rehearsal**
restores this staging, while **Vesper contact authoring** restores the flat studio
setup used for the performance film. The [ready study](/Users/ryanwible/projects/wrela/.soundstage/studies/vesper-tools-ready.json)
preserves this exact view. Published source matches the reviewed local fields,
performance tracks and contact offsets.

Native Motion advanced one frame from 7.500 to 7.517 seconds. Native Behavior's
visitor scenario advanced to PERFORMING with attention 1.00. The combined rig
and contact view displayed collision proxy guides and the constrained skeleton.
The isolated game process was closed after verification.
