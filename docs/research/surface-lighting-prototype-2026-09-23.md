# Compiler-built surface lighting prototype — 2026-09-23

> Subsequent product decision: surface caching is now automatic within the transport solver, and the player/editor experimental GI switches have been removed. This report preserves the prototype measurements and its then-current opt-in configuration. It does not establish production world GI. See the [current lighting contract](../architecture/indirect-lighting.md).

The first bounded surface cache is implemented and remains opt-in. The final matched trials reduce median observed frame time by **29.7–35.5% at 1280×960**, with small image differences. At 640×480 the saving is **4.6–19.8%**. These are three small scenes on Apple/Metal, not a world or cross-hardware result.

[Open the lookdev comparison](surface-lighting-2026-09-23/index.html) · [Evidence and provenance](surface-lighting-2026-09-23/evidence.json)

## Measurements

GPU frame medians below use the conventional median of 24 timestamp samples per condition in ABBA order. Geometry, lighting, camera, renderer settings and source coefficients match. Steady frames exclude uploads/builds; a second ABBA trial changes source intensity every frame and includes relighting.

| Scene | Image size | Existing GI | Surface cache | Saved | Added GPU buffer | Added cache build |
|---|---|---:|---:|---:|---:|---:|
| matte | 1280x960 | 9.110 ms | 6.226 ms | 31.7% | 2.20 MB | 2.16 s |
| metal | 1280x960 | 9.503 ms | 6.128 ms | 35.5% | 2.20 MB | 1.89 s |
| sealed | 1280x960 | 14.254 ms | 10.027 ms | 29.7% | 3.31 MB | 3.08 s |
| matte | 640x480 | 3.539 ms | 3.375 ms | 4.6% | 2.20 MB | 2.16 s |
| metal | 640x480 | 4.522 ms | 3.703 ms | 18.1% | 2.20 MB | 3.44 s |
| sealed | 640x480 | 5.964 ms | 4.784 ms | 19.8% | 3.31 MB | 4.03 s |

Full field-plus-cache builds take 3.5–5.8 seconds in the final large-image trial. Maximum cooperative CPU slices there were 2.1–2.2 ms; this is observed behavior, not a hard deadline. The GPU buffer totals are about 3.47 MB for the open rooms and 4.57 MB for the sealed room. CPU staging, JavaScript objects, shared geometry and driver/pipeline storage are additional.

The large-image trial resolves changed lighting in an approximately 0.066 ms combined probe/surface interval, near the timestamp granularity; the baseline probe relight often quantizes to zero. With intensity changing every frame, total frame medians remain lower: matte 9.929 → 7.504 ms, metal 9.994 → 7.340 ms, sealed 15.466 → 11.469 ms. No relight pass runs during the measured steady frames.

Diffuse-room maximum beauty error is 0.001709 scene-linear; rough metal is 0.005585, with mean absolute error 0.00003167 and p99 0.0007019. The sealed-room cached/control images match. All cases pass the predefined mean ≤0.002, p99 ≤0.01 and maximum ≤0.05 channel-error gates; the separate sealed-room indirect test requires brightness ≤0.0001. These are acceptance observations against current GI, not an error certificate or a comparison with ground truth.

Camera movement, lower source intensity, constant-source mode, origin rebasing and disabling/restoring the field also pass. The old GI has a rare rebase boundary artifact; the cache does not add to it. The shared historical room fixture points its walls outward; the new fixture explicitly authors inward-facing room walls. Earlier captures of outward-facing backs exercised fallback rather than the intended cache.

## What changed

- The compiler recognizes static two-triangle rectangular receivers with constant geometric normals. It creates bounded world-space charts, geometry-only normalized probe weights, and optional GPU-resolved diffuse/reflection storage.
- Whole-tile triangle shadow cones establish visibility constancy. Probe-cell boundaries, ambiguous visibility, numerical margins, proof budgets and rapidly changing sampled weights retain the original GI path. Midpoint checks bound observed interpolation quality only; they do not prove visibility.
- Relighting resolves diffuse irradiance and nine reflected SH vectors per admitted sample. Shading interpolates four cached surface samples instead of repeating eight receiver-to-probe queries. Diffuse keeps the original per-probe nonnegative clamp. Reflection blends SH before its angular clamp, which adds a measured approximation.
- `surfaceCache: { radiance: false }` retains the weights-only experimental control. It avoids visibility/moment work while preserving the original per-probe reflection clamp. It was not the final performance candidate.
- Exact immutable positions, normals, indices, draw range and absolute pose govern receiver eligibility. Changed, dynamic, displaced, alternate or unsupported receivers fall back. Per-chart batch separation prevents instances sharing geometry from consuming another receiver’s lighting.
- Storage allocations, samples, patch count, proof work and GPU addresses are checked. Compiler construction remains cooperative and cancellable. Sky/source intensity and color reuse transport; changing sun direction still rebuilds the original field.
- Cache-specific opaque pipeline variants keep cache code out of the normal lighting kernel. The profiler now includes indirect relight work in the frame span. Re-enabling a previously used field after an upload now forces a fresh relight instead of reusing a stale key.

## Rejected intermediate result

The initial shared shader slowed the uncached GI control from 3.998 to 4.915 ms in its trial. Keeping that shader would exaggerate the apparent cache gain. The final shader specialization control is 3.506 → 3.408 ms, with identical image channels. GI-off also matches exactly and shows no observed slowdown. Short-trial timing noise prevents claiming that the ordinary path improved. Both raw controls are preserved in the evidence folder.

## Scope and use

Enable the prototype through `IndirectLightingCache.update(scene, { surfaceCache: {} })` (or the host’s corresponding indirect-lighting options). Defaults: spacing one eighth of the smallest probe spacing, 16,384 samples, 64 patches, sampled weight L1 tolerance 0.04. The hard bounds are 65,536 samples and 256 patches. `spacing`, `maxSamples`, `maxPatches`, `maxWeightError` and `radiance` are explicit controls. The lookdev uses spacing 0.04 m with 10,404 samples for each open room, 15,606 for the sealed room.

Use the `indirect-cache` diagnostic: green uses a chart; magenta retains queries. About 57% of visible open-room surface pixels and 49% of sealed-room pixels use the cache in the final larger view. This first product caches the authored front side; back faces fall back.

**This is a surface-cache prototype, not the completed cheaper world-lighting system.** It still builds and retains the original probe field and visibility hierarchy. It does not yet accelerate curved terrain, rock meshes, procedural normals, foliage, moving occluders, water or glass. It does not add local-light/emissive bounce or detailed mirrors, and it does not fix the world volume/banding problem. Neither world GI nor the surface cache is enabled by default.

The next useful step is compiler-generated receiver charts for a representative terrain/rock scene, with a memory cap and persisted compiled products. That should be measured before extending this prototype across the world. The longer-term alternative is compiling transport directly for those receiver charts so the old probe-volume build is no longer a prerequisite.

## Verification and provenance

- 46 focused tests pass; they cover dense independent interpolation samples, a blocker between samples, budgets, malformed payloads, immutable receiver changes, rebasing at ten million metres, cancellation, batching, lifecycle and timestamp decoding.
- The hardware fixture renders diffuse, rough-metal and sealed scenes at both sizes, including coverage images, moving-camera/relight/rebase/off-on checks and matched timing trials. All final browser diagnostics and acceptance errors are empty.
- TypeScript (workspace and browser), dependency boundaries and checks on the new implementation files pass. The paired pre-change renderer control checks both GI-off and ordinary GI images.

Reproduce with `bun tools/surface-lighting-check.ts` and `bun tools/surface-lighting-check.ts --large`; run the GPU tools sequentially under the shared hardware lease. For the kernel control, use `bun tools/surface-lighting-kernel-check.ts --baseline=<frozen pre-change source directory>`. Freeze source with `tools/source-snapshot.ts` before comparative runs in an active shared checkout.

Tested source fingerprint: `91134a292deadecb24b0d07ffd5248514742994f7933d2502560fcb99c2f3c3f`. Pre-change control: `94a5a3c4ae30f41b9270b71206349ef21b97895d293aabf8ffb3f01174d37d59`. The frozen source and full reports remain under `output/surface-lighting-prototype/`. Final housekeeping after the capture only changes formatting, comments, an equivalent optional-chain expression, test coverage and reporting metadata.
