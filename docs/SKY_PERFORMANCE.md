# Sky, lighting and weather pass — September 11, 2026

Measured on the Apple M4 MacBook Air at native 1920×1080 with four-sample MSAA.
Only one game/soundstage application ran at a time. Other desktop apps remained
open. Final frame-budget measurements disabled GPU
counters and included ordinary live cloud, lighting and wind updates.

| Workload | Submitted FPS | GPU median | GPU p95 | GPU p99 |
| --- | ---: | ---: | ---: | ---: |
| Earlier sky study | — | 5.03 ms | 12.98 ms | 14.16 ms |
| Earlier optimized sunset sky study, 30 s | 59.99 | 3.51 ms | 4.72 ms | 5.03 ms |
| Final sunset sky study, 30 s | 59.96 | 6.45 ms | 14.41 ms | 15.00 ms |
| Earlier garden | 59.59 | 16.50 ms | 21.66 ms | 26.96 ms |
| Final garden, noon, 30 s | 59.97 | 13.18 ms | 13.97 ms | 14.41 ms |
| Final garden, sunset, 30 s | 60.03 | 12.84 ms | 13.67 ms | 14.10 ms |

The final garden leaves about 3.5–3.8 ms of median GPU time against a 16.67 ms
frame budget. This is meaningful room, not a guarantee for arbitrary additional
content. The noon run's maximum GPU duration was 18.01 ms; sunset's was 15.17 ms.
Frame callback intervals and submitted FPS are recorded separately from GPU time;
there is no display-present telemetry. These are short checks, not a sustained
thermal guarantee. The final garden runs reported thermal state `nominal`, with low
power mode off. Allocated Metal memory was approximately 695 MB for the garden
and 668 MB for the soundstage, after removing 96 MiB of cloud-depth textures.

The earlier and final versions have different cloud structure and tuned coverage
presets. These comparisons describe the complete delivered versions, not an
identical-image microbenchmark. Noon was measured immediately before moving
solar-disk transmittance into the
per-frame light state. A same-time comparison confirmed that final optimization
produced identical pixels in the sunset view.

Raw reports, relative to the workspace:

- `.soundstage/final-live-performance.json` — earlier sky
- `.sanctuary/sky-integration-performance.json` — earlier garden
- `.soundstage/profiles/final-sky-sunset-20260911-015232.json` — earlier optimized run
- `.soundstage/profiles/final-sky-continuous-solar-20260911-021058.json` — final sky
- `.sanctuary/profiles/continuous-garden-noon-20260911-020734.json`
- `.sanctuary/profiles/final-garden-sunset-20260911-020927.json`

Isolated-sky timings were less repeatable than the garden measurements. The final
sky run was 6.45 ms median / 14.41 ms p95; an earlier optimized run was 3.51 /
4.72 ms. Another post-reprojection run was 5.59 / 14.14 ms. Diagnostic timestamps
showed variation even in unchanged display and blur passes, and other desktop
applications were active. Shared scheduling/power state is a plausible
contributor, but the cause was not isolated. The versions and simulation times
also differ. Do not claim a guaranteed 3.5 ms sky cost or attribute all variation
to background activity. Whole-garden measurements are the useful current
headroom result; more controlled profiling remains warranted.

## What changed

- Separate smooth atmospheric scattering from high-resolution cloud integration.
- Reuse the old sky allocation for an 8192×1024 upper hemisphere, doubling
  horizontal angular detail.
- Strengthen connected cloud billows, refine local extinction, and erode edges
  without introducing isolated sparkling density samples.
- Replace sinusoidal cloud strain with conservative moist-air transport,
  saturation adjustment, latent heat, buoyancy and cooling in a column model.
- Compile max-density bounds and conservatively account for the deformation
  Jacobian when skipping empty regions.
- Bound per-frame cloud work and continuously reproject wind motion on a
  representative spherical cloud layer. Fresh shape and lighting snapshots blend
  for at most 1.5 s. A depth-refinement experiment was removed after isolated
  aged-snapshot comparisons revealed torn and outlined cloud silhouettes.
- Specialize material shaders, evaluate global sunlight/exposure and solar-disk
  attenuation once per frame, pair shadow and bloom filter lookups, and select shadow LODs in shadow texels.

The attempted large RGB sunlight cache was slower and was removed. Compact
R16F optical depth remains. Indexed drawing was also slower than mesh shaders
in the tested garden; mesh shaders remain the default.

## Validation and appearance

- 19 Swift tests passed, including water balance, stable moist-column evolution
  and replay independent of frame cadence.
- 15 soundstage protocol checks and nine garden checks passed, including exact
  repeated paused PNGs, invalid input rejection and GPU field evaluation.
- Three evolved cases (seeds 4, 17 and 83, wind multipliers 1 and 2) tested
  1,333,575 density samples inside certified skips, with zero violations.
  `.soundstage/sky-traversal-validation.json` records the individual cases.
- Inspected 18 sky views and 28 stone views across the lighting presets, plus
  native wet/dry, indoor/outdoor and sky-angle controls. Exercised garden walking
  and the meadow view through computer use. No GPU command errors were recorded.
- Final isolated motion checks compare an 18.37-second-old sunset snapshot and a
  9.22-second-old overhead noon snapshot at double wind speed against fresh
  references. Reference delays were 0.15 and 0.12 seconds. The continuous mapping
  removes outlined silhouettes; layer-height differences still cause gradual
  displacement. Sunset mean absolute RGB difference was 1.17% of full scale,
  with a 12.94% 99th-percentile channel difference. This is visual acceptance of
  an approximation, not a universal reconstruction error bound.
  `.soundstage/final-sunset-motion.json` and `final-zenith-motion.json` record it.
- The solar-disk optimization produced byte-identical same-time captures:
  `.soundstage/solar-comparison.json`. The final shader/resource layout compiled
  successfully after removing the depth textures and extending the light buffer.

Final studies are `.soundstage/final-sky-study.json` and
`.soundstage/final-material-study.json`, with corresponding contact sheets.
Every source PNG has exact-frame settings and source/shader fingerprints.

Remaining limits are visible and intentional work for later passes: overlapping
cloud layers and disocclusion still make reprojection approximate; deep cloud
interiors and overcast skies lack richer diffuse transport; shaded objects can
remain too dark against the sky. The weather model is a two-dimensional column
approximation, not a full three-dimensional atmosphere. Rain is not yet rendered
precipitation or terrain hydrology. See `ATMOSPHERE.md` for the representation,
timing and simulation assumptions, and `SOUNDSTAGE.md` for the profiling tools.
