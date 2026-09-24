# Comprehensive default lighting — 23 September 2026

Later work, corrected shader-preparation timing, and repeated performance results are recorded in [the lighting refinement report](lighting-refinement-2026-09-23.md). Treat this page as the earlier implementation checkpoint.


[Interactive capture review](comprehensive-lighting-2026-09-23/review.html) · [Raw sweep](comprehensive-lighting-2026-09-23/environment-sweep.json) · [Architecture](../architecture/indirect-lighting.md)

## Outcome

One automatic host path now serves daylight, dusk, night, windowed interiors, sheltered entrances and sealed caves. No authoring lighting-mode switch was added. The approach is bounded approximate GI compiled from geometry, with inexpensive runtime relighting and PBR shading; it does not enable the earlier expensive world-volume solver.

- Two diffuse bounces from sky, local point lights and static emissive materials; a low-order approximation of sun/moon bounce.
- Local low-frequency reflections, with roughness filtering. Moving objects select nearby visible lighting samples.
- Full-resident sky occlusion replaces the old short horizon, so a large cave is not treated as open sky.
- Closed convex receiver certificates suppress direct celestial-light leaks at shadow-map seams. Other receivers retain ordinary shadow maps.
- All eight authored point lights can cast shadows at each quality level. Cube faces cull irrelevant casters, cache independently, and share per-frame caster revisions. Texture memory is unchanged; face resolution is halved from the previous two-light allocation.
- Coarse rigid walls receive bounded lighting refinement. Existing dense carriers, semantic picking boundaries, texture coordinates and dynamic/windy geometry retain appropriate source handling.
- Materials expose emitted-light strength and color in Studio. The existing indirect-lighting and cache-coverage diagnostics show the automatic product.

## Validation

81 focused tests passed (28,352 assertions), including closed rooms/caves, cracked/open enclosures, geometric visibility, radiometric reuse, origin rebasing, moving receivers/lights, independent scene identities, cancellation, source-attribute retention, point-shadow coverage/caching, packing and existing assembly-motion behavior. The full repository suite then passed: 1,066 tests across 232 files, with 1,515,167 assertions. Both TypeScript checks and dependency-boundary checks passed.

A frozen-source hardware sweep rendered 13 environments at 960 × 720, balanced quality and spatial antialiasing on Apple Metal. It reported no GPU errors or incomplete draws. Interior relighting and the indirect-lighting/coverage diagnostic views were captured too. These are diagnostic scenes plus the actual Winter scene, not a matched AAA-game or real-world photographic validation.

## Performance and compiler costs

The comparison uses the same authored scene and full-resident sky visibility. The baseline renderer uses original carriers; the new renderer includes automatic lighting refinement. Timings include the frame's GPU work, with 12 warmup frames and 64 recorded frames per arm in A/B/B/A order, using four-frame bursts.

| Scene | Baseline median GPU ms | New median GPU ms |
|---|---:|---:|
| Outdoor daylight | 0.918 | 2.163 |
| Dusk | 1.999 | 1.901 |
| Open room | 1.540 | 0.918 |
| Windowed interior | 1.311 | 1.573 |
| Sheltered entrance | 1.999 | 2.294 |
| Sealed room | 1.278 | 1.180 |
| Large sealed cave | 1.311 | 1.278 |
| Warm and cool lights | 1.573 | 3.310 |
| Emissive room | 1.311 | 1.180 |
| Night lighting | 2.916 | 1.999 |
| Eight shadowed lights | 3.178 | 3.375 |
| Moving receiver | 1.343 | 1.507 |
| Winter Valley | 5.439 | 5.571 |

Small-fixture timings vary materially between repeated runs, often more than the measured difference. These numbers do not establish a universal speedup. Eight shadowed lights cost more than the previous two-shadow limit. Final Winter follow-ups measured 5.374 → 5.964 ms and 5.439 → 5.571 ms. These show a small-to-moderate added GPU cost on this adapter, rather than a demonstrated rendering speedup. The two-light interior repeatedly rose from about 1.57 to 3.3 ms; that regression needs targeted profiling even though other fixtures varied. CPU frame costs and other hardware still need broader measurement.

The first version compiled/refined too much receiver data: Winter needed 11.7 seconds wall time / 4.14 seconds active work, with a 20.2 ms maximum slice and 65,376 receiver vertices. Bounded nearest-sample selection and limiting refinement to genuinely coarse carriers reduced the measured build to 4.77 seconds / 1.62 seconds active, maximum slice 4.6 ms, and 28,870 vertices. The final receiver follow-up reported 4.78 seconds wall / 1.55 seconds active, maximum slice 4.3 ms. This is a scheduling target, not a deadline guarantee.

The field uses 192 samples in Winter. Retained CPU transport/geometry/receiver accounting is approximately 12.1 MiB; GPU SH/transfer payload is about 0.73 MiB, plus vertex streams. Admission bounds are explicit. 11,883 allocated Winter receiver vertices have no front-side binding and retain sky fallback; this is not complete world coverage. Deferred moving receivers are admitted over successive extractions to avoid a large one-frame search spike.

## What still prevents a 10/10 claim

1. Low-order sampled transport misses some small openings/emitters and cannot produce sharp parallax-correct local mirrors. Direct sun/shadow maps supply detail that the bounce field cannot. The review still shows faceted low-frequency bounce in the emissive box and shadow-map edge leakage around the windowed room; these are visible quality gaps, not merely theoretical limits.
2. Dynamic objects receive indirect light but do not cast dynamic indirect shadows or contribute arbitrary dynamic emissive bounce. A moved point light's old bounce coefficients are rejected; compatible sky/emission data survives its coalesced rebuild.
3. Initial preparation still takes seconds. Geometry streaming can invalidate transport and sky products; cooked chunk preparation and certified dependency reuse remain necessary for a robust shipping traversal experience.
4. Directional lighting still uses the existing single shadow map. More local lights share the old texture-memory budget at lower per-face resolution. Neither general contact hardening nor unrestricted many-light rendering is implemented; the authoring cap remains eight point lights.
5. Indoor volumetric illumination/fog occlusion and automatic exposure adaptation are not solved here. The sealed captures retain a small aerial-perspective floor. Existing authored exposure, atmosphere and zones remain available.
6. The color transport uses low-frequency source albedo and static opaque geometry. Water, transmitting/thin surfaces and foliage keep their separate material/closure approximations. The static cache does not replace authored canopy attenuation.

## Reproduction

`bun tools/comprehensive-lighting-check.ts --baseline=/absolute/path/to/frozen/baseline`

The gallery combines the initial 13-scene sweep with follow-up captures. Night/eight-light captures come from the canopy follow-up; Winter/windowed/interior captures come from the final receiver follow-up. The remaining scenes are from the full sweep. `capture-cohorts.json` records this mapping. Final receiver corrections normalize partially admitted diffuse interpolation and select alternate-realization bindings from exterior free space instead of a potentially buried first vertex.

Retained source fingerprints:

- Initial 13-scene sweep: `f2d3c8e66b1ff65bf739b3e0ba75e754cfcb34b913fc505c01c31fd60bffc525`.
- Canopy follow-up: `41ada50d0516b06d74287c91330b05d215ef2400fe6f03d5c9e2583a7161a035`.
- Final receiver follow-up: `8b27896dee3df13561813591968a659ce0f53a0863079e939197ce4108b4aa22`.

The command freezes working source, acquires the shared GPU lease, runs the environment sweep and writes captures plus raw timings. `--kinds=winter,cave,interior` runs a focused subset. The review directory retains baseline/sweep source manifests; the original frozen sources remain under `output/comprehensive-lighting/`.
