# Water implementation and acceptance record

23 September 2026 · [Visual review](implementation.html) · [Original research](README.md) · [Measured results](implementation-evidence/summary.json)

The water upgrade is implemented across source authoring, compilation, simulation, rendering and the player. **This is not an AAA visual sign-off.** The new systems are usable, but the integrated environment remains a diagnostic composition and the surf still exposes its sheet construction. GPU cost remains above the provisional water budget.


**Surface refinement follow-up:** [reduced dimples, matched before/after captures and verification](dimple-refinement.md).

## Implemented

| Area | Working behavior | Main implementation |
|---|---|---|
| Crest physics | Positive crests compress; horizontal orbital velocity, Jacobians, CPU inverse queries and GPU synthesis agree | `packages/compiler/src/water-spectrum.ts`, `packages/render-webgpu/src/water-spectrum-gpu.ts` |
| Sea-state authoring | Unsaturated artistic wind response; significant height, peak period, finite-depth spectrum weighting and independently directed swell | `packages/model/src/water-body.ts`, `apps/studio/src/environment-authoring-panel.tsx` |
| Ocean foam | Advected GPU history with emission at compressed positive crests, authored residence time, mip filtering and reset on seeks/source changes | `water-spectrum-gpu.ts`, `water-body.wgsl` |
| Creek foam and wetness | Conserved transported foam, effect emission, bank wetness and drying; checkpoint/replay includes wetness and event time | `packages/runtime/src/water-simulation.ts`, `session.ts` |
| Optical response | Separate absorption and scattering coefficients, directional scattering, crest transmission, directional unresolved slope statistics | `packages/render-webgpu/src/water-body.wgsl` |
| Caustics | Focusing modifies an estimated direct receiver term with visibility, rather than multiplying all received lighting | `water-body.wgsl` |
| Reflections | Conservative depth mip hierarchy, bounded screen-space traversal and roughness rejection; up to 16 source-derived off-screen appearance proxies | `water-transport.ts`, `water-transport.wgsl`, `packages/compiler/src/water-reflections.ts` |
| Bed and contact | Fine bed/contact product independent of the fluid grid, irregular obstacle profiles, local relief and bed wetness response | `packages/compiler/src/water-domain.ts`, `packages/runtime/src/scene-host.ts` |
| Local current detail | Smallest normal cascade follows the solved local current with crossfaded advection phases | `water-body.wgsl` |
| Breakers and waterfalls | Bounded compiled sheets with overturning lips, ballistic droplets, independently phased events, foam and run-up wetness | `packages/compiler/src/water-effects.ts`, `water-body.wgsl` |
| Thin-sheet transport | Sheets render after the base water, sampling a separate snapshot with a short optical thickness | `packages/render-webgpu/src/index.ts`, `water-body.wgsl` |
| Temporal separation | Opaque history resolves before water; a separate quarter-pixel radiance history applies a depth/identity-aware low-frequency correction without blurring full-resolution detail | `packages/render-webgpu/src/index.ts`, `water-temporal.ts`, `water-temporal.wgsl` |
| Authoring and review | Sea-state/swell, water clarity presets, foam lifetime, drying, bank detail and local-effect controls; creek, ocean, surf and valley player scenes | `environment-authoring-panel.tsx`, `water-lookdev.ts`, `water-valley.ts`, `apps/player/src/main.ts` |

The sea-state implementation is **finite-carrier quadrature**, with JONSWAP-shaped energy and a bounded shallow-depth suppression approximation. It is not a dense ocean FFT or an exact directional oceanographic model. Foam history is a visual residual; gameplay retains deterministic CPU water queries. Thin sheets and spray do not alter collision or add liquid volume.

## Compiler and performance changes

- Bed, contact and effect coefficients remain resident; only the current fluid/header prefix uploads on a tick. The previous fluid copy is no longer packed or uploaded. Foam and wetness share the fourth dynamic channel with tested precision limits.
- The base water, bed and effect draws share one body buffer. Equivalent spectrum instances share one source buffer and set of maps, with explicit acquisition/release and edit invalidation. Phase origin participates in the cache key.
- Bounded, non-choppy water uses three 128² moment layers, about **0.5 MiB**, versus the former six 256² layers, about **4 MiB**. Ocean maps plus persistent foam use about **5 MiB**. No unused identity-Jacobian layers are synthesized for bounded water.
- Fine bank geometry no longer multiplies the per-vertex wave evaluation. The free surface uses the fluid grid; the fine contact field cuts its visible shoreline in the fragment stage. The valley uses **64² fluid/surface samples and a 256² bed**.
- Fluid updates skip inactive tiles with a complete exchange halo. Initially resting, unforced lakes skip the solve. Active water remains governed by the same CFL and conservation rules.
- Balanced/low optical snapshots use half-width color and full-resolution depth. The conservative depth hierarchy includes odd trailing rows and columns. The 1080p allocation is **18.46 MiB**, including the hierarchy, versus **23.73 MiB** for the old color/depth snapshot. Open water without receivers uses a **12-byte placeholder**.
- Renderer measurements now expose water-owned transport, spectrum, body, geometry and object bytes. The ledger includes the rendered bed; generic scene color/depth/history, sky and shadow resources are excluded from the water subtotal.

The isolated, source-stable synthesis experiment rejected phase recurrence on the tested Apple/Metal GPU. Direct synthesis p50 was **0.156 ms ocean / 0.037 ms creek**; recurrence was **0.299 / 0.098 ms**. These are compute-only intervals, excluding foam and mip generation. The benchmark holds the cooperative GPU lease and batches 16 independently timestamped dispatches per interval. [Raw experiment](implementation-evidence/synthesis.json).

Direct carrier synthesis remains the default. FFT selection has not been implemented: the measured synthesis cost is a small fraction of the complete water path, and there is no equal-energy FFT result justifying a replacement. Regional classification, map specialization, sharing and upload reduction were implemented first.

## Compact water history follow-up

Default and temporal rendering now retain water history at half width and half height. Two radiance/depth/identity pairs plus a correction texture consume **19.78 MiB at 1080p**, including the uniform buffer. A depth-aware additive composite retains full-resolution detail; it never reads its active color attachment. Changed foam/glints, different identities and disoccluded depth reject stale radiance. Water sources, camera cuts, seeks, origin changes and capture transitions invalidate history through the renderer’s existing reset/identity rules.

The measured 640×384 integrated orbit owns **13.81 MiB**, including **2.34 MiB** of water history. Applying the exact allocation formula to the existing 1080p integrated fixture gives **47.51 MiB**, just below the provisional 48 MiB target; this is an allocation projection, not a new 1080p performance certification. High-quality transport, denser geometry or extra effects can exceed it. The table below remains the source-stable **spatial-mode** experiment; it does not include the new temporal work and must not be presented as default-mode timing.

## Spatial-mode measurements

Chrome 153, Apple/Metal hardware, 1920×1080, balanced/spatial. All three final cases use the same frozen source fingerprint, recorded with exact bundled fixtures. Four ABBA legs each contain 45 warmup and 120 measured moving frames. The off control keeps fluid simulation and the opaque bed/environment. Texture snapshots are now released when the control has no water.

| Scene | Whole-frame GPU p50, water on | Whole-frame GPU p95, water on | Fluid p95, water on | Upload median | Water-owned GPU memory |
|---|---:|---:|---:|---:|---:|
| Ocean | 4.00–4.46 ms | 5.18–6.16 ms | No local solver | 4,176 B | 7.43 MiB |
| Diagnostic creek, 128² | 13.80–14.16 ms | 15.34–16.12 ms | 1.9–2.2 ms | 268,304 B | 23.09 MiB |
| Integrated pool, 64² / 256² bed | 15.14–15.20 ms | 15.86–15.99 ms | 0.3 ms | 72,224 B | 27.73 MiB |

The 128² creek upload is **66.1% lower** than the earlier study's 790,784 B/frame; the grid payload itself falls exactly two thirds. The integrated scene demonstrates the further saving from choosing a smaller authoritative grid while retaining fine contact.

These results **do not establish an overall GPU speedup**. The ocean is more expensive than the original study's 1.57–2.10 ms median. Persistent foam and richer response add work; a separate foam-shading ablation reduced ocean whole-frame medians from roughly 3.7–3.8 to 2.95 ms in the earlier implementation snapshot. It retained foam synthesis and storage, so it does not measure total foam cost. Background sky work also changed in the shared workspace between the research baseline and implementation. Control legs show desktop load variation. The provisional **2 ms p95 water GPU** budget is not met; the **0.5 ms CPU-fluid** and **48 MiB water-owned memory** targets are met by the integrated fixture on this adapter. No modest discrete-GPU or low-end device certification was performed.

Pass timestamps can overlap on this tiled GPU. Do not add individual pass durations or label a difference of whole-frame medians as a water p95. Use the raw ABBA records and their manifests for comparison.

## Verification and known limits

75 targeted tests passed, covering single-wave physics, inverse queries, variance, exact fine contact vertices, fluid conservation, interaction/replay, source validation, shared resources, packing precision, transport allocation, temporal infrastructure and renderer residency. Subsequent changes also passed the affected suites. Both TypeScript configurations, workspace boundaries and the production build passed. Hardware conformance checks validate CPU/GPU spectra, origin rebasing, filtered moments and conservative depth mips at odd dimensions. Surf was checked with spatial and MSAA rendering.

The original spatial-water orbit is retained as evidence. The subsequent compact-history acceptance run passed all six ocean/creek policies: default and requested temporal modes used independent water history; spatial used none. Maximum frozen-frame mean change was 0.0000068/255 for ocean and 0.0000190/255 for creek. All continued to animate without GPU errors. The new ten-second orbit presented 596 frames at 640×384, included a ripple with exactly zero volume addition and reported no GPU errors. GPU conformance independently proves history acceptance, depth/identity rejection, opaque exclusion, reset, frozen invariance and retained detail at odd dimensions. [Compact-history acceptance record](implementation-evidence/compact-history.json).

Remaining acceptance work is explicit:

- **The surf is a prototype.** Its repeating curved sheet and foam patterns remain visible; it needs a better authored shore, breakup, event variation and more convincing spray before it is a hero shot. Waterfall support is present but has not received an equivalent art pass.
- **The valley is not a finished environment.** Banks, rounded obstacles, material hierarchy, terrain edges and vegetation placement remain below the reference bar. Fine contact and reflected surroundings improve the foundation without solving composition or stone authoring.
- **Temporal supersampling remains open.** Water now has independent radiance history, depth/identity rejection, reactive color rejection and reset on discontinuities. It uses camera reprojection and conservatively rejects changing surfaces; it does not yet track individual wave/foam trajectories or accumulate reflection and refraction as separate signals. Projection jitter remains disabled in compiled-water scenes. Airborne sheets stay spatial.
- Off-screen ellipsoids are deliberately bounded appearance approximations, not geometry-accurate visibility, planar reflections or ray tracing. They can still lose silhouettes. Caustics remain an approximate direct-light redistribution.
- Fine visual bathymetry and the sampled solver bed are not identical between grid nodes. There is no continuous certified error bound or adaptive shoreline tessellation yet.
- Per-cascade update scheduling, a measured sparse-carrier/FFT selector, optical-thickness tile scheduling and full 3D/FLIP/neural fluid realizations are not implemented. The research discussed the last group as conditional future experiments, not browser-runtime dependencies.

## Reproduce

```sh
PORT=4188 bun tools/dev.ts
# /player?water=valley, /player?water=ocean, /player?water=surf
bun tools/water-lookdev.ts --integrated --view=pool --bench
bun tools/water-lookdev.ts --ocean --bench
bun tools/water-lookdev.ts --surf --msaa
bun tools/water-motion-check.ts
bun tools/water-orbit-check.ts
bun tools/water-synthesis-check.ts
```

The benchmark source copy is retained under `output/water-frontier-v2/source`; the isolated synthesis copy is under `output/water-frontier-synthesis/source`. Raw result directories retain bundled programs, hardware context and manifests. A source copy prevents unrelated ongoing edits from invalidating a run; the cooperative lease cannot exclude every other desktop GPU consumer.
