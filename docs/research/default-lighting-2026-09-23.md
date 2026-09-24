# Default lighting: cheaper sky visibility, without world GI

The shared player/editor path now adds cached local sky visibility to static mesh surfaces, attenuating diffuse sky and environment reflections in sheltered areas. This is automatic; no player or Studio toggle was added. The shader compiler removes unused probe/contact code from ordinary scenes and removes sky-visibility work from pipelines that cannot consume the product.

[Open the captured before/after comparison](default-lighting-2026-09-23/review.html).

## Measured outcome

Apple Metal-3, 960×720, balanced quality, spatial antialiasing. Whole-frame GPU medians in milliseconds; lower is better. Each final condition collects 64 timestamp samples in an A/B/C/C/B/A ordering, after warmup, using four-frame submission bursts. The geometry, environment, camera and exposure are identical between arms. “Optimization only” uses the new renderer with the original meshes; “Default” also uses the sky product.

| Winter run | Before | Optimization only | Default | Default reduction |
|---|---:|---:|---:|---:|
| trial-1 | 5.833 | 4.784 | 4.817 | 17.4% |
| trial-2 | 5.669 | 4.981 | 4.850 | 14.5% |
| trial-3 | 5.407 | 4.751 | 4.850 | 10.3% |
| acceptance | 5.374 | 4.784 | 4.784 | 11.0% |

The final acceptance run improves Winter from **5.37 to 4.78 ms (11.0% less GPU time)**. The four repetitions range from 10.3% to 17.4%. The optimization-only Winter control is pixel-identical in scene-linear RGB. Other fixtures differ by at most 0.0001221 in that control, consistent with shader arithmetic precision.

Small scenes did **not** give consistent performance results across repetitions. The last acceptance medians were:

| Fixture | Before | Default |
|---|---:|---:|
| alpine | 1.114 | 1.999 |
| room | 1.901 | 0.786 |
| closed | 2.425 | 1.049 |

In particular, alpine regressed in that run, while an earlier run changed from 2.032 to 1.114 ms in the opposite direction. These small timings are unstable under this harness/hardware scheduling. They do not justify a universal engine speedup claim. Winter is the sustained representative workload with a repeated measured improvement. This is one machine and one resolution, not a low-end/mobile or other-engine comparison.

## Visual checks

The closed-room image loses **93.7%** of its previous average scene-linear brightness. It still has a narrow direct-shadow leak along a wall seam; sky occlusion does not repair every shadow-map bias error. Independent aerial perspective also remains. No claim of zero light in every enclosed pixel is made.

Winter retains material detail on the rocks and adds local darkness to sheltered terrain. The before/after change is deliberately subtle outdoors. Open-surface numerical tests require full visibility on both sides, and the fully open bent normal equals the authored normal. Camera movement was captured after settling the new chunk membership. The generated comparison uses actual renderer captures, with identical display settings and no image retouching.

## Preparation and memory

The accepted initial Winter product has 28,870 vertices and 461,920 additional CPU attribute bytes (about 451 KiB). It fires 1,385,760 rays once, with no per-pixel geometry query in ordinary lighting. Its wall-clock build took 5.53 seconds; measured compilation slices totalled 1.70 seconds of active work, with a largest slice of 3.5 ms. Initial builds in the repeated captures took roughly 4–6.5 seconds. These are real costs, not “free baking.” Source-key/dependency analysis, temporary BVH/JS allocations and GPU mesh residency are additional to the retained attribute payload and ray-loop timing.

A 24-metre camera move changed world membership and rebuilt the cache. It reused **25,980/29,448 vertices (88.2%)**, traced 166,464 rays for the changed support, and settled compilation in 470 ms (150 ms active). New products fade in over 180 ms; that fade is additional to compilation time. Unchanged camera/lighting inputs reuse the existing product. CPU tests separately verify origin rebasing, nearby blocker invalidation, distant-source reuse, disposal/cancellation and whole-build refusal on source-triangle overflow.

## Compiler/runtime contract

- The compiler traces 24 cosine-distributed samples per side per vertex against admitted static triangles. A distance-only early-exit query avoids building shaded ray-hit objects. It is equivalent to the original nearest-hit visibility after the same 9–12 m fade, tested including the fade annulus.
- Shared source arrays/topology remain unchanged. Four unused rigid vertex lanes carry front bent-direction × visibility plus scalar back visibility; the vertex stride does not grow.
- Source/pose/material eligibility determines reuse. Per-receiver dependency bounds are conservative for the local horizon. Surface color and sunlight changes do not invalidate geometric visibility.
- The shader specializes away absent GI and absent sky-visibility work. Direct lights retain their existing shadowing. Reflection occlusion is an approximation; it is not a local reflection capture.
- Dynamic, skinned, windy, foliage, glass, water, displaced and unsupported alternate receivers retain their existing lighting. Dynamic, transmitting and water geometry are excluded as blockers. Displaced/alternate surfaces can still contribute their underlying static source triangles to the query; those triangles do not reproduce every displaced silhouette. The renderer specifically guards alternate realizations from consuming mesh-only sky data.
- Limits are 2,048 static sources, 200,000 source triangles and 65,536 allocated receiver vertices, with a separate 4 MiB attribute ceiling. Source-budget failure exposes an error and publishes no incomplete product. Receiver-budget exclusions are reported. Temporary compiler data is not included in the attribute limit.

This system has a 12 m local horizon and mesh-vertex resolution. It does not deliver world GI, color bleeding, emissive bounce, moving-object indirect shadows, mirrors, or uniformly detailed contact in coarse meshes. More work is needed on cooked preparation, small-scene timing, moving objects and shadow-map contacts.

## Rejected experiments and provenance

The first four-read PCF experiment reproduced the original nine-read separable filter algebraically, but did not establish a reliable additional runtime improvement. The production shader retains the original filter. A first shader failed WebGPU's fragment-input limit; packing two existing scalar varyings into one slot fixed it. Another early capture exposed black analytic rocks consuming zero-initialized mesh visibility; the alternate-realization packing guard fixes that and has a regression test. These failures remain in `output/default-lighting/first.log`, `interstage.log`, `captures.log`, and their frozen source directories.

The workspace had concurrent renderer refactoring. Early comparisons to the original whole-renderer snapshot also used isolated single-frame submissions and were unsuitable for attributing cost to this change. Final controls copy the current frozen source, substitute only the two pre-upgrade lighting shader sources, and supply inert override declarations for the current pipeline descriptors. Both arms therefore retain the same unrelated renderer work. Original mesh controls isolate shading; the default Winter case exercises `BrowserSceneHost`'s automatic path, not a manually attached field. The other three small fixtures use the same cache directly.

Accepted source fingerprint: `bd2f50e17f8b4aa4954f15eb0a764fad6b3845c895978ba0aede318f75910b1f`. Counterfactual control fingerprint: `fc54405283e5bb235f19039baaa83f7b48e4c6c1ca8ed117175e3ee84f94a809`. Full manifests, all four raw timing reports, summary data and images are saved in [the evidence directory](default-lighting-2026-09-23/summary.json). A later allocation-budget accounting fix counts unused vertices in partial draw ranges; it changes no captured Winter inputs or shader code and is covered by an additional CPU regression test.

Reproduce with:

```sh
bun tools/default-lighting-check.ts --matched-control --baseline=output/default-lighting/baseline
```

The baseline is the frozen pre-upgrade source, fingerprint `2081da95e2b89a52259ab4ed00c89fa050b3f20177bffd33ff3f5df421f873f6`. `--matched-control` creates a fresh documented counterfactual beside each frozen run. Use `--winter` for the representative world or `--quick` for alpine. GPU harnesses must run serially under the shared lease.

## Validation

- 34 focused tests passed across eight compiler/runtime/renderer test files (572 assertions). The final cache-only rerun passed all eight cache/visibility tests after the last test type-narrowing cleanup.
- Repository TypeScript and browser TypeScript checks passed. Workspace public API/dependency boundary verification passed.
- Changed-file formatting/lint passed with one existing non-null-assertion warning in the renderer; whitespace checks passed.
- The frozen GPU acceptance run completed all four scenes with no GPU diagnostics. Closed-room darkness, optimization-control precision and lighting-edit reuse have executable gates in the capture fixture. Runtime tests cover unchanged open surfaces, both sides of sealed geometry, packing guards, streaming invalidation/reuse, allocated-vertex budgets and whole-build refusal.
