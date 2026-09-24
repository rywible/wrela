# Compiled lighting implementation — 23 September 2026

The first implementation of the lighting audit's approach is in the engine: shared diffuse/reflection visibility, bounded static multi-bounce transport, compiler visibility regions, stackless fallback traversal, probe relocation, static sun contact queries, consistent water local lighting and environment-coat attenuation. **It fixes the reproduced enclosed-room failures. It does not pass the world-scale GI rollout gate.**

[Before/after review](compiled-lighting-2026-09-23/index.html) · [Measurements and source provenance](compiled-lighting-2026-09-23/evidence.json) · [Current architecture and options](../architecture/indirect-lighting.md) · [Original audit and research](lighting-audit-2026-09-23.md)

## What changed

| Product | Implemented behavior | Boundary |
|---|---|---|
| Static light transport | One to three diffuse surface bounces; runtime default two. Every bounce contributes material throughput to a reusable sky/sun transfer operator. | Geometry and reflectance are static. Changing sun direction still rebuilds. No bounced point lights or emission. |
| Shared surroundings | One visibility gather supplies diffuse, rough local reflection and clearcoat. Geometry radiance and directional sky visibility are separate products. | Nine SH coefficients describe broad radiance; no detailed mirrors, parallax-correct capture or general scene refraction. |
| Compiler visibility | Conservative receiver-space clear/blocked masks and bounded unresolved triangle lists. Exact numerical segment fallback remains. | Compiled proofs depend on their geometric/offset bounds; floating-point tests are not exact arithmetic. |
| Fallback traversal | Threaded BVH eliminates each pixel's traversal stack, preserving traversal order and the 128-node bound. | Exhaustion blocks light and can over-darken. |
| Probe placement | Nearby back faces trigger a bounded relocation with a clearance check. | A heuristic, not proof that every probe is outside a solid. Volume seams remain. |
| Contact shadows | Static segment queries repair the sealed-room sun seams missed by the shadow map. A dark-room hint selects a longer query; it never decides occlusion itself. | No cascades, general contact-hardening, or additional dynamic GI occluders. |
| Materials | Environment clearcoat now attenuates the underlying ambient/reflection response. Glass uses local context for its existing approximation. Spectrum water uses the shared light mask, finite range and local shadow functions. | GGX multiple-scattering compensation and a complete layered BSDF remain future work. |
| Surface-restricted compiler experiment | Optional `surfaceVisibility: true` compiles tighter visibility over actual receiver triangles. Exact immutable mesh/range/pose matching controls eligibility. | **Off by default.** Repeated measurements did not justify its extra build cost. Changed, displaced and moving surfaces retain general visibility. |

The plain renderer still supports GI-off. Player world GI remains explicitly opt-in with `?gi=1`. When GI is enabled, local reflection context, relocation and two diffuse bounces are the new defaults. This implementation does not make the GI-off environment path geometry-aware.

## Image and numerical results

The same closed room, metal material, camera and exposure now produce a nearly black metal block instead of reflecting the outdoor sky through the walls. Its central face falls from **0.0606319 to 0.00026723** mean scene-linear luminance: **99.56% lower**. The remainder includes beauty aerial perspective, which is not enclosure-aware. Comparing every RGB channel against the no-direct-light ablation gives **maximum difference zero**: the reproduced direct sun seams are removed. The open-room control remains lit, with central-patch luminance 0.284473.

This does not establish global leak freedom. The higher-resolution isolated diffuse diagnostic retains a rare boundary value of 0.03528, despite mean luminance only 9.75×10⁻⁸. A separate 96×72 closed-box regression is exactly zero. The acceptance board preserves both the dark beauty result and these limitations.

The open metal comparison keeps the same diffuse field and switches only between global sky reflection and local reflection context. Local context changes the broad reflected surroundings; it does not show a detailed reflected room. The ordinary open-room comparison includes two bounces and relocation, so it must not be attributed to a single change.

The independent one-bounce diffuse reference fixtures remain explicitly at one bounce. Their CPU/GPU maximum disagreement is **0.000179** (box) and **0.000122** (rock patch); world-origin rebasing differs by at most **0.000122**. Against independently sampled transport, RMS error is 0.010101 and 0.003668 respectively. Probe interpolation remains an approximation, not a ground-truth renderer.

## Performance: useful bounded savings, world rollout still fails

Apple / Metal, balanced quality, 640×480. Small fixtures use paired ABCCBA isolated scene-pass timestamps, 16 samples per condition. Winter uses ABBA whole-frame timestamps, 32 samples per condition. Build/upload settling is excluded. These are short local observations with noisy tails, not sustained performance or other-hardware guarantees.

| Matched comparison | Compiled path | Control | Image agreement |
|---|---:|---:|---|
| Open-box diffuse scene pass | 3.015 ms | 3.867 ms full BVH | Maximum difference 0; 22% lower median |
| Rock/ground diffuse scene pass | 2.949 ms | 4.915 ms full BVH | Maximum difference 0; 40% lower median |
| Winter beauty, receiver regions versus coarse cell lists | 20.316 ms | 20.120 ms | Maximum difference 0; no measured improvement |
| Winter beauty, upgraded GI versus GI-off | 19.726 ms | 2.818 ms | Different lighting; not an optimization equivalence test |

The small-fixture compiler path includes the conservative cell lists that existed before this implementation. Those percentages are **not** the incremental contribution of the new region trees alone. Separate earlier stack/traversal experiments suggested a useful stackless implementation, but cross-run timings do not establish a precise speedup for it.

Winter's upgraded GI has a 22.938 ms whole-frame p95, builds in 10.002 seconds and reaches a 4.2 ms maximum observed cooperative CPU slice. The scene contains 53,360 participating triangles and 405 excluded surfaces; 54 probes relocate. Its general visibility-cell payload is 834,704 bytes. Broad dark bands remain near terrain/water intersections and volume coverage. The old audit's 15.335 ms GI median used fewer lighting features and a separate run; this upgrade has **not** made world GI cheaper than the old implementation.

Removing visibility entirely measured about 5.96 ms in a diagnostic control, identifying query cost as a major remaining expense. That control leaks and is not an accepted renderer mode. The next compiler experiment should remove repeated traversal through a compact surface lighting cache with bounded boundary/dynamic correction, rather than continually enlarging these region trees.

## Experiments that did not earn a default rollout

- Deeper subdivision exhausted the memory allowance sooner and did not improve the paired median.
- Larger leaf/candidate budgets increased cost and memory; reverted.
- Vectorized slab traversal did not establish an overall improvement; reverted.
- Surface-restricted regions preserved images, but one run's 6% saving shrank to 0.65% in a repeat. Initial build time was 16.0 seconds; early-exit classification reduced it to 11.3 seconds. The path remains available behind `surfaceVisibility: true`, with the default off.

Frozen source locations, fingerprints, raw reports and these decisions are preserved in the evidence bundle. Two queued GPU checks initially timed out waiting for the exclusive GPU lease; both were rerun successfully against their existing frozen snapshots. Those lease failures were not shader failures.

## Validation and remaining milestone

46 focused tests pass, including transformed/rebased geometry, thin walls, perturbed normals, transport relighting at all three bounce budgets, placement, cache invalidation, dynamic exclusions, allocation refusal, packed updates, static receiver eligibility and host/UI integration. Full workspace TypeScript checking and package-boundary validation pass. Rendered diffuse/reference, sealed/open enclosure, material-family/coating and local-light checks pass without recorded GPU errors. The separate water check identifies water pixels before comparing light range and shadow changes, so the opaque blocker cannot make that test pass by itself.

The implementation does not yet deliver the audit's complete playable lighting valley. Moving-door GI, cascaded coverage, cached static sun depth, photometric calibration, richer smooth reflections, energy-compensated GGX and stable world-volume transitions remain. Those are explicit follow-on work, not capabilities implied by the new static transport. The compiler hypothesis is promising in bounded scenes; this evidence does not support enabling world GI by default or claiming AAA lighting parity.
