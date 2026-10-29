# Cloud source coordinates help reuse, but uniform optical-depth caches fail

2026-10-29 UTC. Parent91c97c8d35e43aaee861acd4dddc0736709f665d.

**Decision: retire both far-sunlight caches.** Caching the distant sunlight
quadrature reduces nominal cloud cost, but damages shading. Doubling all three
grid dimensions still leaves 4.36% of pixels outside the engine's existing 2/255
image guard, adds 64 MiB, and takes 23.20 ms to generate. The slight unpaired
whole-frame savings do not constitute a quality-matched rendering win.

First joint quality/runtime/residency assessment: **15.69 minutes**.
Decision: **22.58 minutes**, after **one** targeted optimization. No training,
new transport references or subagents. All work ran on Pop!_OS AMD; the Mac only
orchestrated SSH. Runtime is restored exactly to the parent, and all failed
implementations, executables and outputs are preserved. G1–G7 remain unchanged.

## Measure the work that actually runs

The preceding [probe-bound experiment](neural-probe-ceilings.md) proposed examining
repeated lighting queries during scattering. Source inspection shows that the
clearing's [Trace](../examples/clearing/stage.wrela) has bounced=0, so its leaf-bounce
probe lookups are disabled. The full-frame profile also has no fog injection stage.
Neither is an appropriate target for this scene.

The active [cloud march](../engine/sky.wrela) costs 6.23936 ms. Every occupied
view sample performs eight source-density queries along sunlight, using fine
near-surface intervals and progressively wider distant ones. A diagnostic removes
these queries and their downstream optical-depth-dependent math: cloud cost falls
to 4.40660 ms; the complete frame goes from 27.71420 to 26.01304 ms.

That ablation is invalid rendering: 10.10% of pixels differ by more than 2/255,
maximum 109/255. It is a causal cost diagnostic, not an optimization or an exact
accounting of isolated texture-query cost. The remaining 4.41 ms also shows that
removing sunlight work alone cannot deliver a tenfold cloud speedup.

## The source-aware representation and fair baseline

Wrela's cloud field is stationary in source coordinates. Wind moves the entire
field, so source coordinates can preserve a cache across wind updates. The same
weather, shape/detail noise, current sunlight and source functions are available
to the conventional baseline and the candidate. Neither receives reference data,
future state, reduced scene density or cheaper hidden fallback.

The candidate keeps the first two sunlight intervals exact. It caches the remaining
six density-integral terms in a 128×64×128 half-float texture, over a 65,536 m
horizontal window and the cloud layer's 4,000 m height. This is 8 MiB of actual
requested texture storage; unused RGBA channels are counted. The original
multiscattering/phase calculation reads the interpolated distant optical depth,
combined with the exact near contribution.

Lookup points outside the interior sample domain evaluate the original far
quadrature directly. The spatial window follows rounded source-space eye coordinates.
Any change of the exact sunlight vector, window, weather or noise invalidates the
cache. Rebuilding uses the same source evaluator and is included in cost/memory.
Wind alone does not invalidate it until the window needs to move.

The only followup doubles each dimension, to 256×128×256: eight times the samples
and storage, **64 MiB**. All near-surface samples and shading formulae remain unchanged.
This isolates whether inadequate lattice resolution explains the quality loss.
No second followup or model fitting was justified after this failed.

The original full source march is the retained control. The engine already contains
a cheaper half-dome option, but its documented cloud-edge errors exceed 2/255;
it remains an optional reduced-quality mode ([prior B5.4 evidence](bench.md)).
The failed candidates do not meet the full-quality control, so no tournament win
against reduced-quality alternatives is asserted.

## Shaded image quality and edge errors

Compare final 1920×1080 shaded RGBA after 660 frames at identical fixed workload
and resolution. The existing 2/255 engine image guard is a conservative rejection
screen, not a substitute for the unchanged G1–G7 thresholds. There is no independently
converged cloud-transport truth in this block, so even an exact match could only
establish sampler parity, not physical correctness or frontier quality.

| Method | Mean RGB byte error | Pixels >2/255 | P99 / max channel byte error | Contrast-band mean / p99 byte error |
|---|---:|---:|---:|---:|
| Original source march | 0.000000 | 0.000% | 0 / 0 | 0.000000 / 0 |
| 128×64×128 far cache | 0.518486 | 7.020% | 12 / 63 | 0.252530 / 7 |
| 256×128×256 far cache | 0.289159 | 4.365% | 7 / 57 | 0.178671 / 5 |

The contrast band contains 274,790 pixels: parent-image RGB jumps of at least
16 byte units, expanded one pixel. It detects errors near high-contrast image
edges, including visible cloud/foreground boundaries. It is not a semantic cloud
silhouette label or a G3 coverage test. Screen alpha is exact, but opaque screen
alpha alone cannot establish cloud coverage correctness.

The upper quarter's mean RGB byte error is 1.69537 / 0.88499 for coarse/fine.
Independent replay maximum differences are one byte for baseline, two for coarse
and one for fine, affecting respectively 1 / 10 / 3 channel bytes. The candidate
errors are much larger than that repeat variability.

Source marching/density and the cloud coverage formula are unchanged, but this
single-frame check cannot establish motion, disocclusion or changing-light quality.
The visible shading failure already rejects both candidates; a larger reference
suite or additional training would not be a proportional next verification step.

## Complete runtime and actual resident memory

Ryzen 5 5500U; AMD RADV RENOIR PCI1002:164c; Pop!_OS; Mesa26.1.6;
Chrome154.0.8037.57; Wrela0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. One worker, 1920×1080,
60 measured frames after 600 warmup. As in the previous block, both methods disable
measured adaptive scheduling and pacing, retaining fixed resolution and recurring
probe updates. All other clearing passes remain enabled.

| Method | Complete GPU median / p95 ms | Clouds median / p95 ms | Paced complete p95 / p99 ms |
|---|---:|---:|---:|
| Original source march | 27.71420 / 29.40692 | 6.23936 / 6.93252 | 29.62749 / 30.19913 |
| 128×64×128 far cache | 27.06012 / 28.84800 | 5.52452 / 6.30872 | 28.88872 / 29.74425 |
| 256×128×256 far cache | 27.18664 / 28.86080 | 5.67980 / 6.46024 | 28.65976 / 29.55769 |

The complete GPU span includes the full scene: vegetation/wood, visibility,
sorting/culling where used, terrain, probes, cloud march, temporal resolve and
composition. All original field textures and fallback data remain resident.
Cloud cache generation is separately labelled and measured during startup.

| Method | Host call median / p95 ms | Invocation interval median / p95 ms | Observed cache generation ms |
|---|---:|---:|---:|
| Original source march | 1.730 / 1.995 | 29.810 / 56.630 | — |
| 128×64×128 far cache | 1.800 / 2.060 | 29.010 / 56.005 | 4.48328 |
| 256×128×256 far cache | 1.745 / 2.040 | 28.670 / 56.765 | 23.19544 |

Cache generation occurs at frame18 in these runs. Its 4.48328 / 23.19544 ms cost
must be paid again after invalidation. These are observed cold kernel costs, not
a measured full steady-state refresh frame or a bound on all changing-light events.
The static-light experiment does not qualify dynamic updates. Host calls and
invocation intervals are not CPU-to-GPU completion or presentation latency.

| Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Original source march | 405,005,112 / 582,183,284 | 552,005,632 / 20,262,912 | 801,325,056 |
| 128×64×128 far cache | 413,393,720 / 590,571,892 | 560,386,048 / 20,180,992 | 809,989,120 |
| 256×128×256 far cache | 472,113,976 / 649,292,148 | 619,085,824 / 20,180,992 | 803,874,816 |

All instrumented requested live/peak allocations include textures, caches, scratch,
inactive resources and fallback data. Actual residency is deduplicated by DRM
client/device, with browser-tree PSS recorded separately after 660 paced frames.
These are whole-scene snapshots, not physical peaks or per-asset attribution.
Do not sum VRAM/GTT/PSS on UMA. Actual VRAM rises by approximately the added 8/64 MiB;
there is no memory reduction.

Runs are unpaired and power/thermals are not locked. Nominal complete-frame savings
are small and not confidence-qualified. Since both candidates already fail quality,
additional paired performance runs would not change the retirement decision.

## Decision and next bet

Retire uniform distant optical-depth caching for this objective. The representation
smears spatial shadow variation; eight times the storage only reduces mean error
by about 44%, and maximum error remains 57/255. No default, model, reference
pipeline or production runtime change is retained.

Next investigate source-derived empty-space bounds for the cloud view march.
Keep authored density and lighting exact; use known weather and noise bounds to
skip only samples proved empty. This attacks the substantial work that remains
after the sunlight ablation and avoids approximating the shadow signal.
Whether bounds are selective and cheap enough is unproven and must be measured
against the unchanged source evaluator, including bound generation and refresh.

No G1–G7 qualification follows from this experiment. The next goal remains a
quality-matched full-stack advantage, not a faster isolated approximation.

## Evidence

Pop!_OS:
`/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/cloud-light-cost-v1`.
Manifest SHA-256: `db21fabf7e17197019355ad0f3d0121bfab3b1bd0a0b9bb7657a1765c80e49c0`.

plan.json records the scope, source finding, first candidate and sole refinement.
baseline/, no-sun-path/, source-far-cache/ and source-far-fine/ preserve source
archives, commands, compiled builds, full-resolution images and GPU/host timings.
The three valid measurement methods also have exact-build allocation replays.
decision.json joins quality, complete cost, actual residency and limitations.
parent-source.tar.gz plus retired-fine-source.tar.gz preserve retained/failed source.
external-scripts.tar.gz contains the orchestration/scoring scripts and dependencies.

Each profile batches its frames in one build; residency reuses the copied executable.
Source cases still compile separately. One quiet AMD queue runs before CPU scoring;
there is no claimed workflow or rendering speedup from unmeasured batching.
Pre-run syntax/type repairs and their preservation limits are recorded. No failed
rendered result was deleted. Runtime restoration is exact; no repeated testing
of unchanged engine machinery is necessary.

Archive metadata correction: execution-repairs.json names
cache-initial-fmt-failed.log as the initial typecheck artifact, but that file is
empty: it copied the formatting stream. The E0323 checker error (constructing
vec3 from GlobalId instead of its numeric components) was visible in the tool
transcript and was fixed, but its raw checker log was overwritten by the successful
check. This note corrects that preservation claim without modifying sealed
evidence. All rendered failures and measurement data are preserved.
