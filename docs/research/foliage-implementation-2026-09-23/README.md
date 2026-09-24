# Foliage implementation and acceptance evidence

September 23, 2026. Implements the portable compiler and rendering work from the [frontier study](../foliage-frontier-2026-09-23/README.md). **This is a working implementation and review scene, not an AAA visual sign-off.** The new quality tests expose a remaining representation error; they do not certify the proxy.

## What works

The shaped pine now has a deterministic 3D source shoot: paired, curved needles with tapered triangular cross sections, short exposed bases, and two retention/color cohorts across four shape variants. Bounded branch references compile the complete selected source before applying geometry limits. Explicit needles and filtered shoots now derive from that same source, replacing the old independently invented comb pattern. Branch pitch and attachment variation, shorter tufts, collars, and filtered bark fissures improve the source structure.

Repeated shoots use shared geometry and compact instances. The compiler retains source identity, branch attachment, coherent motion, template bounds and material data. An edited curved twig retains the expanded realization instead of silently becoming straight. Straight shoots use an affine template transform, including explicit axial scaling. This restricts sharing to geometry it can actually reproduce.

The renderer selects explicit or projected detail separately for the camera and sunlight, with 15% hysteresis. Conservative motion bounds participate in selection and camera rejection. Offscreen shoots remain available to shadows. A group bound can prove that every shoot is distant before per-shoot traversal. Local lights conservatively keep explicit shadow detail. Crown aggregates remain unqualified, and the old crown builder refuses compact shoot instances it cannot interpret.

Immutable geometry streams share GPU storage across source/cooked/worker copies after verifying their actual attributes. Coverage textures have shared, reference-counted residency, including pending uploads. Instance packets survive camera and wind updates; root transforms, selection and history changes invalidate them. Newly visible occurrences invalidate their own temporal history without discarding the history of every neighboring shoot. Geometry, source metadata, transfers, serialized validation and installed-memory accounting include the new products. Picking returns the source shoot/needle identity.

Optional local canopy shade compiles a sparse projected-area field and samples upward escape directions through nearby source shoots. It attenuates ambient illumination, preserves direct shadows, and avoids multiplying visibility already represented by compiled GI. This is an isotropic rest-pose approximation, not calibrated scattering or a directional transport solution. Authoring controls expose close needle detail and canopy shade.

The forest edge is an editable project with four pine variants, a bounded understory population, moss/litter material layers, stones and a 24-second eye-height route. Placement uses crown-distance shade and a smooth moisture proxy, with explicit path clearance. Terrain is deliberately flat: its slope is known, but no general ecological placement or terrain-fitting claim is made. The scene is available at `/player?forest=1` with **Walk the trail**.

## Performance evidence

Apple M4 Air, 8 GPU cores, 16 GB, native 1920×1080 temporal reconstruction, nine-tree grove. A frozen source snapshot alternates expanded/shared/shared/expanded, using 240 identical warmup ticks and 1,200 measured ticks in each run. Local canopy shade is disabled in both halves to isolate representation/storage and selection changes. Full frame IDs, timings, resource counts and source manifests remain in the output directory recorded in [evidence.json](evidence.json).

| Matched path | GPU p95 | CPU p95 | Maximum resident GPU bytes | Typical submitted triangles |
| --- | ---: | ---: | ---: | ---: |
| Expanded, A1 | 7.864 ms | 1.7 ms | 249,031,425 | 728,002 |
| Shared, B1 | 6.160 ms | 1.8 ms | 228,779,001 | 463,018 |
| Shared, B2 | 6.095 ms | 1.9 ms | 228,779,001 | 463,018 |
| Expanded, A2 | 7.668 ms | 1.8 ms | 249,031,425 | 728,002 |

The shared path reduces GPU p95 by approximately 21–23%, resident memory by 8.1%, and typical submitted triangles by 36.4% in this controlled workload. It introduces independently selected explicit close detail, so this is not an assertion of pixel-identical rendering. The CPU target is met in these short trials; the 6 ms GPU target is narrowly missed. Pass intervals overlap and must not be summed.

The initial instancing implementation regressed CPU p95 to 9.7 ms because it rebuilt thousands of matrices and records per frame. That implementation was rejected. Immutable packet caching, conservative group decisions and shared stream storage brought the CPU cost back down. An earlier ABBA run also showed a large transient tail in one shared trial; the retained evidence is not limited to the best run.

Removing the foliage occupancy prepass is available as a matched diagnostic. The tested direct-shading path cost **23.92 ms GPU p95**, with a 21.43 ms opaque interval, and was rejected. The default retains the depth pass and a color shader without sample-mask output, allowing early depth rejection. An initial no-prepass capture incorrectly included an unwritten timestamp; that run is invalid, its evidence is retained, and the timing mask was fixed before rerunning.

The final default-path rerun, with canopy shading enabled, recorded 3,072 frames at 6.160 ms GPU p95 and 1.8 ms total CPU p95. Resident GPU memory was 232,945,913 bytes in that later source snapshot, which also includes concurrent renderer changes.

These are short diagnostic runs on AC, not sustained thermal or battery qualification. The existing benchmark supports 5-minute warmup plus 30-minute runs. Visual acceptance is already failing, so no sustained shipping pass is claimed.

## Quality gate: not passed

The new isolated-shoot fixture renders actual 3D needles and their proxy through the production renderer at matching views. Six initial cases cover requested footprints of 16, 64 and 128 pixels and two held-out azimuths. Coverage is compared after 4×/8× supersampling; scene-linear foreground energy is compared against the empty background at 4×.

Observed projected-shoot area deficit is **12.9–14.0%** versus the explicit reference. Foreground-energy differences are approximately **15.6–20.5%**. These exceed the proposed 2% area and 5% radiance budgets. Spatial coverage is also too different, especially as individual needles become resolved. Moreover, reference spatial L1 has not converged under the strict gate, so the numbers are diagnostic estimates rather than a proven error bound. No candidate is promoted by this tool.

The old comb representation is visibly less faithful to the actual shoot arrangement. The new projection improves correspondence but still discards depth and changes oblique occupancy. More atlas pixels alone will not restore that information. Explicit needles remain the resolved fallback. The projected path remains an authored approximation; its screen-size threshold is a heuristic, not a certified appearance bound.

The 20 native compositor images in the temporal route sequence exercise wind, camera motion, disocclusion and detail decisions after 32 history warmup frames. They are retained for inspection. They do **not** establish a quantitative ghosting/shadow budget. The forest still reads as sparse, regularly tiered trees on a simple floor; understory and ground detail remain below the AAA references. Improved metrics or implementation breadth do not constitute art approval.

## Reproduction and controls

Run from the repository root:

```sh
bun tools/shoot-lookdev.ts --matrix
bun tools/shoot-qualification.ts --matrix
bun tools/foliage-benchmark.ts
bun tools/forest-walk.ts
bun tools/vegetation-lookdev.ts --architecture --matrix
bun tools/vegetation-lookdev.ts --forest
bun tools/vegetation-benchmark.ts --architecture --moving --aa=temporal --thermal
bun tools/vegetation-benchmark.ts --architecture --moving --aa=temporal --no-thin-prepass
```

Each GPU tool freezes source bytes before capture and uses the cooperative hardware lease. The thermal command is deliberately long. For authoring comparisons, toggle `botanical.conifer.architecture.sharedShoots` or `canopyVisibility`; the former retains an expanded fallback. Native-only mesh shaders, neural transport, per-light directional shoot aggregates, GPU shoot compaction and a certified representation transition are not implemented or claimed.

## Validation and remaining work

207 tests passed across 50 files at the final integration checkpoint: source/template equality, cooked validation, transfer ownership, picking, sharing, culling, camera/light ownership, temporal history, and existing renderer/model regressions. Both TypeScript checks and the package-boundary check pass. The player opens on the hardware renderer and its trail button advances the eye-height camera with complete frames. Nine held-out seed/age captures completed; two inspected cases still show the sparse crown and legible tiers. The shared workspace contains concurrent water-rendering work. Its temporary resource-test dependency-boundary violation was resolved by that work; the package-boundary check now passes. Final test counts and player verification are recorded in the implementation log.

The next justified representation experiment is a directional/depth-preserving **single shoot**, tested against a converged reference before adding crown-scale aggregation. The next art pass should address radial tuft fullness, less legible tiers, branch/ground transitions and a denser but coherent floor. A broad terrain/streaming ladder and sustained shipping qualification should follow acceptance of that small complete scene.
