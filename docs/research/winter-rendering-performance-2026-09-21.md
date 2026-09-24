# Winter rendering performance — September 21, 2026

The production renderer now uses one shading sample with temporal reconstruction. Three 121-frame native-1920×1080 trials on the Apple M4 produced GPU medians of **13.57, 11.80, and 11.47 ms**, with **14.68, 14.81, and 14.61 ms p95**. The maximum across all 363 production samples was **16.25 ms**. Browser frame pacing was approximately 60 Hz. **The requested 8 ms headroom target is not achieved.**

The earlier audit's four-sample renderer was approximately 40 ms. These are separately recorded runs, not a claim of a precise simultaneous speedup. The world recipe, camera trajectory, final 198,068 visible triangles, 115 draws, and 486 rendered / 22 culled surface identities match. Concurrent creature development changed some compiler and fixture code between snapshots. The dominant demonstrated gain remains eliminating expensive four-sample shading; additional optimizations should not each be credited with an independently established speedup.

Changes shipped:

- **Temporal reconstruction:** eight-frame projection jitter; previous camera, object, skin, procedural deformation, wind, and water positions; per-tap identity/depth rejection; neighborhood clipping; lower history weight for water; resets on camera cuts, lighting/source changes, rebasing, resize, diagnostics, and device recovery. Neighbor reads share a workgroup tile, including partial edge groups. Display does not apply another edge blur to reconstructed frames. Canonical captures remain deterministic, without accumulated history.
- **Reusable diffuse lighting:** the sky product is convolved into nine spherical-harmonic coefficients once per update. Surface shading evaluates those coefficients instead of four sky lookups. Unchanged atmosphere inputs reuse the existing products exactly.
- **Compiled water:** the compiler emits the independent two-wave slope map and inverse. The GPU locates local specular events from that map and integrates the sharp GGX denominator over the pixel using four boundary integrals. Wave carriers also serve the general source evaluator. This does not reduce the existing fallback sample ceiling.
- **Water validity checks:** the analytic path applies only within its tested roughness, phase-curvature, shutter, view, and numerical-conditioning domain. It currently excludes point lights. Constant shadow visibility requires checking every depth texel reachable by the footprint, PCF kernel, and normal offset. Mixed shadow regions retain correlated integration. Smooth sky terms still use quadrature; this is a local approximation, not an exact general lighting solver.
- **Less redundant work:** filtered procedural noise exits before hashing when fully filtered out; unchanged material/instance buffers skip uploads; GPU visibility scheduling includes full target area and MSAA traffic; geometry selection accounts for raster samples and actual scene resolution. Geometry cost observations use a new kernel version so old measurements are not reused.
- **Explicit resolution:** balanced/high default to one sample and a device pixel ratio capped at one. `RendererOptions.pixelRatio`, `resolutionScale`, and `antialiasing` expose deliberate choices. The reported production result renders at full 1920×1080. Resolution reduction was tested separately and is not used to claim the target.
- **Measurement:** reconstruction has its own timestamp range, uploads and allocations are accounted for, and benchmark runs reject invalid GPU timestamps or validation errors. GPU pass ranges overlap on this backend; their durations must not be added to estimate a frame.

Production owned GPU memory at the final frame is **175.42 MB**, versus **211.03 MB** in the earlier four-sample audit. The temporal histories account for much of the remaining allocation. Static final-frame upload traffic is approximately **18 KB**, down from **122 KB** in the audit.

Verification passed **319 compiler/renderer/runtime and acceptance tests**, both TypeScript configurations, workspace boundaries, formatting checks on changed source, and actual Chrome WebGPU checks. The independent water suite covers 120 queries: **0.543% relative RMS**, **1.234% maximum relative error**, and five confirmed analytic-path acceptances. Analytic-path coverage is narrow; the other cases exercise existing paths. The sky convolution matches its analytic reference over 128 directions with maximum absolute error **0.000536**. GPU reconstruction checks cover seven history/disocclusion cases on an odd-sized target. Shadow checks include a one-texel blocker that must reject reuse. GPU compaction checks validate every float of current and previous instance records at one and four samples.

Timing evidence is in [the recorded summary](winter-rendering-performance-2026-09-21.json). Raw outputs and the immutable tested checkout are under `output/rendering-performance-final/source/`. The measured implementation fingerprint is `aec58212ce9eb6e6d3ffb0aea469e34da4602997cb59972db2a994c10821e92d`; subsequent edits fix only verification fixtures (instance readback stride and the independent water bound's time dependence).

Reproduce from a stable checkout:

```sh
bun tools/rendering-compiler/performance-check.ts --full
bun tools/rendering-compiler/temporal-check.ts
bun tools/rendering-compiler/water-check.ts
bun tools/rendering-compiler/gpu-visibility-check.ts
bun test ./packages/compiler/src/*.test.ts ./packages/render-webgpu/src/*.test.ts ./packages/runtime/src/*.test.ts ./tools/rendering-compiler/*.test.ts
bun run check
```

The next performance milestone is widening useful compiled-water coverage and reducing reconstruction/scene cost while retaining these quality checks. Neither the analytic prototype nor lower resolution has established a reliable sub-8-ms production result.
