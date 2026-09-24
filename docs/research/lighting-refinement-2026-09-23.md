# Lighting refinement — 23 September 2026

[Rendered comparison](lighting-refinement-2026-09-23/review.html) · [Raw full sweep](lighting-refinement-2026-09-23/environment-sweep.json) · [Final follow-up](lighting-refinement-2026-09-23/final-followup.json)

## Changes

- Coarse rigid receivers use half-metre target spacing with indexed shared vertices. Up to four visibility-tested samples blend with compact support; dense carriers retain a cheaper two-sample representation. Additional records use the existing storage binding and are bounded by the receiver budget.
- Flat geometric normals certify which side faces the camera. Low-frequency local reflection can be interpolated on either side, while glossy sky reflections and the material response remain per pixel. This removes first-vertex reflection patches. A lazily compiled material specialization removes the per-pixel local gather where the source proves it unnecessary. It withdraws for incomplete transitions, unsupported materials, uncertain normals and rejected GPU fields.
- Moving receivers use four visible samples with smooth weights in object uniforms. They remain excluded from static occlusion; this does not implement dynamic indirect shadow casting.
- Small static scenes fit the directional shadow volume to all caster bounds, in discrete bands, rather than a mandatory 28-metre width. The existing texture budget yields sharper window/contact shadows. Uncertain animated/displaced bounds and larger populations retain existing coverage. Thin window-edge leaks still exist.
- Local-shadow allocation redistributes the existing eight-light texel budget: one or two lights use twice the linear resolution; three or four use a smaller intermediate increase; five through eight retain the existing resolution. All active lights retain coverage. Reallocation invalidates the maps and refreshes the global binding/accounting; stationary frames and radiometric edits retain the cached maps. Sharper maps cost more to redraw when geometry or lights move.
- Camera anchors have hysteresis. Up to four resident region products fit within a conservative 32 MiB accounting budget (a single active product can exceed it). Compatible builds reuse static geometry, enclosure analysis and identical transport samples. Returning to a resident region restores its field and derived meshes. Geometry/material edits invalidate incompatible products. This is an in-memory cache, not cooked world streaming or persistent disk storage.
- Dense flat receivers get a separately compiled two-sample shader, eliminating four-sample/storage-record branches only where the mesh certifies their absence. Unknown or mixed receivers keep the general path. Receiver visibility uses a finite any-hit query, avoiding nearest-hit position/normal/material construction while preserving blocked/clear admission.
- Compilation skips absent radiometric columns without changing coefficient summation order. A frozen comparison verified 50,544 coefficients exactly equal. Small scenes spend unused sample capacity on more rays per sample, up to 256, while Winter retains 64.

## Measurement correction

The previous twelve-frame warmup did not reliably finish asynchronous material specialization. Some earlier timings mixed shader preparation and general fallback pipelines with steady-state rendering. The harness now explicitly waits for requested variants in **both** versions, then warms both before recording. Preparation time is retained separately. The earlier 1.57 → 3.31 ms interior observation is not a reliable steady-state regression estimate.

This comparison uses the prior **automatic** lighting system, not sky/direct lighting alone. Each version compiles its own geometry and transport. Full-frame GPU medians below are from a frozen 960 × 720 balanced/spatial run on Apple Metal, with 64 recorded frames per arm in A/B/B/A order. Small-fixture timing remains noisy; the initial outdoor regression triggered two follow-ups. These do not establish performance on ordinary hardware generally.

| Scene | Previous GPU ms | Refined GPU ms |
|---|---:|---:|
| Outdoor daylight | 1.049 | 1.901 |
| Dusk | 1.901 | 1.835 |
| Windowed interior | 1.573 | 1.245 |
| Open room | 1.114 | 0.983 |
| Sheltered entrance | 1.868 | 1.835 |
| Sealed room | 1.507 | 0.983 |
| Large sealed cave | 1.606 | 1.114 |
| Warm and cool lights | 2.490 | 2.163 |
| Emissive room | 2.556 | 1.212 |
| Night lighting | 1.901 | 1.638 |
| Eight shadowed lights | 3.375 | 2.589 |
| Moving receiver | 0.786 | 0.721 |
| Winter Valley | 6.455 | 6.259 |

## Follow-ups and accepted performance limits

The intermediate repeat still showed an outdoor regression (0.852 → 1.245 ms), prompting the pair-only specialization. The final six-scene follow-up includes that specialization, finite visibility queries and the shadow allocation change:

| Scene | Previous GPU ms | Final GPU ms |
|---|---:|---:|
| Outdoor daylight | 1.901 | 1.901 |
| Warm and cool lights | 1.704 | 2.032 |
| Moving receiver | 2.163 | 1.835 |
| Furnished room, day | 2.359 | 2.490 |
| Furnished room, night | 2.294 | 2.130 |
| Winter Valley | 6.160 | 6.226 |

**The evidence does not support a universal GPU speedup.** Earlier indoor gains do not reproduce uniformly; small scenes vary substantially between runs, and the furnished daytime room costs more in this capture. Winter is approximately neutral across the final repeat. We retain the visible quality improvements and exact compiler shortcuts, but GPU timing needs repeated acceptance on multiple devices before a general performance claim. These runs use static warmed shadow maps; the cost of refreshing the higher-resolution maps under moving lights has not been characterized.

A resident-geometry traversal moves the camera 32 metres, waits for a neighboring region, and returns. The final neighbor reused the BVH/enclosure analysis and 6 of 192 samples, prepared in **2.386 seconds wall / 0.726 seconds active CPU**, and reported a maximum 2.7 ms cooperative slice. The intermediate follow-up before the finite visibility optimization took 7.479 seconds wall / 2.651 seconds active CPU. These are observed whole-build results, not an isolated CPU microbenchmark; other repository CPU checks ran during part of the final GPU sweep. The final initial Winter build took 3.230 seconds wall / 0.975 seconds active CPU. A return hit restored the prior field in **0.70 ms without a rebuild**. This excludes real world streaming and the next renderer submission. The neighbor still had 388 deferred moving-receiver admissions immediately after construction and 18,913 unmapped static vertices. It is not seamless traversal yet.

The gallery uses final follow-up captures for its six scenes and the initial full sweep for the other nine. [Capture cohorts](lighting-refinement-2026-09-23/capture-cohorts.json) map every image to its retained source manifest. No screenshot is synthesized. In the furnished night room, ceiling bounce and the glossy box lose the original triangle-sized discontinuities; residual coarse local shadows and some wall patchiness remain.

## Validation and remaining work

The final full repository suite passed 1,083 tests across 234 files (1,621,215 assertions). Both TypeScript configurations and dependency-boundary checks passed. The thirteen-scene sweep and six-scene final follow-up reported no GPU errors or incomplete draws. Focused tests cover interpolation continuity/wall rejection, transport reuse, bounded region retention, source-attribute preservation, shader preparation barriers, specialization withdrawal, stable shadow slots, shadow allocation transitions, memory ownership and shadow coverage.

This is a measured improvement toward the target, not a 10/10 claim. Static sampled bounce remains approximate. Full dynamic GI, accurate local mirrors, interior volumetric transport, automatic exposure transitions, incremental geometry dependency invalidation and cooked streaming are unfinished. Camera-region reuse currently requires identical resident static geometry/materials; edits and streaming can still invalidate the whole product. Sampling remains bounded near the camera, and 11,883 Winter receiver vertices retain fallback lighting. No cross-vendor hardware acceptance or matched AAA-game comparison has been performed.

## Next acceptance milestones

1. Persist compiler products with regional dependency keys and prepare neighboring regions before entry. Reject whole-world invalidation on unrelated streaming edits; measure time-to-correct-light during actual traversal.
2. Add moving-occluder corrections with a bounded update budget. Test doors, characters, flickering/moving lights and emissive objects, not only static final frames.
3. Improve local reflection detail and parallax where the low-order field fails, selecting the representation from material/geometry semantics. Keep the cheap path for rough surfaces.
4. Integrate enclosed-space fog and exposure transitions; test walking between bright exterior, dark cave and lit room.
5. Establish matched reference renders, motion sequences and ordinary-hardware budgets. A quality score should follow those comparisons, not feature count.

## Reproduction

`bun tools/comprehensive-lighting-check.ts --baseline=/absolute/path/to/previous/source --automatic-baseline`

The baseline is frozen at `output/lighting-refinement/baseline`. Both source manifests and raw timings are retained with the review. Diagnostic `--ablation=reflection|indirect|shadow|direct` isolates work; these are engineering controls, not authoring modes. `--kinds=furnished,furnished-night,winter` selects the broader room and resident-region traversal checks.
