# Compiler frontiers: six experiments

Research performed on the winter renderer's Apple M4 / Chrome WebGPU environment. These are bounded prototypes; production rendering defaults were not changed.

**Decision:** advance joint procedural integration and explicit visibility events. Develop microgeometry aggregation for broad responses, with a separate treatment for sharp features. Do not build a universal branch-lighting table or deploy compiler light lists on the strength of these probes. Extend existing spatial dependencies when introducing cached transport. Use shared environmental causes as an authoring direction, with explicit artistic controls.

| Opportunity | Measured result | Decision |
| --- | --- | --- |
| Microgeometry → appearance | 8,192 normals → 233 groups: 1.51% relative L2 for roughness 0.45; sharp roughness 0.08 still 40.6% error | Useful for broad responses; normal grouping alone does not preserve glints |
| Whole-branch response | Rank 8: 1.52% without self-shadows, 48.3% with them; explicit events match 61,680 random comparisons | Compile discontinuities separately from smooth response |
| Eliminate irrelevant lighting | 834/2048 region/light pairs removed, identical GPU outputs; modest/variable benefit and no removals for above-ground lights | Not our next large performance investment |
| Integrate motion and correlated effects | Generated finite-Fourier shader: 4.0× / 29.9× faster than 64/512-sample quadrature in the isolated kernel | Strongest immediate compiler prototype; narrow function class |
| Compile the response to edits | 4/256 terrain patches rebuilt, 124,416 position/normal components match full rebuild; 260/303 shadow changes occur outside the local edit box | Geometry support already exists; transport dependency support is the extension |
| Shared environmental causes | 9,216 cells, six derived channels, temperature controls, mass-accounting residual 1.1e-13 | Useful authoring prototype; no production visual-quality or speed claim |

**1. Detail should sometimes compile to a response, but averaging is not enough.**

The reference integrates 8,192 deterministic surface normals. Snow material is correlated with upward orientation. Clustering uses only normal/material data; 160 view/light pairs are held out from that construction. The shader response is incident-cosine diffuse plus the same GGX form as the engine. There is no geometric visibility or multiple scattering in this probe.

At roughness 0.45, 74 occupied orientation/material groups achieve 4.38% relative L2 error; 233 achieve 1.51%. These reduce **lobe evaluations**, by approximately 111× and 35× respectively. They are not measured frame-time speedups. A single averaged normal/material produces 76.1% error in this fixture. At roughness 0.08, even 233 groups produce 40.6%; merely increasing this clustering resolution does not solve narrow glints.

Candidate packed scalar payloads are 1,480 and 4,660 bytes, excluding GPU alignment, metadata and spatial coverage. The current data is one authored distribution, not a validation set of production assets. Statistical response products need coverage, occlusion and temporal tests before they can replace geometry.

**2. Whole-branch compression fails at shadow events; explicit event compilation is much more promising.**

The fixture has 240 procedural flat needle plates, including 136 snow-bearing plates, viewed from three directions. Lighting follows a fixed-elevation azimuth orbit. There are 32 training light angles and 32 interleaved held-out angles. Reference shadowing is at each plate's centroid, not per-pixel visibility. Reflection is diffuse with limited back transmission; there is no trunk, multiple scattering, glossy response or volumetric snow. This is a research diagram, not a finished conifer.

A low-rank response plus angular Fourier interpolation works for the smooth unshadowed case. It fails with self-shadowing: the rank-8 held-out error is 48.3%; additional rank does not fix the angular interpolation. Interpolating between two poses also fails on an interior pose (30.9% at bend 0.03), and extrapolation is worse. Endpoint equality is merely a control, not evidence of generalization.

The alternative compiler solves when a ray enters or exits another needle rectangle. For this fixed orbit, boundaries reduce to equations of the form `A cos(theta) + B sin(theta) + C = 0`. It unions the resulting blocked intervals instead of fitting over their jumps.

It emits 3,331 intervals: 27,612 bytes for float32 endpoints plus offsets, about 27.0 KiB. Construction took approximately 1.76 seconds in this Python reference. The float32 table matched all 61,680 radiance comparisons at 257 independently randomized light angles. Its runtime work is interval lookup rather than an all-needle intersection query; this lookup has **not** been benchmarked in the production GPU renderer.

Boundary stress matters: 360 of 4,608 deliberately near-boundary queries disagree without handling precision. A measured ±2e-05 radian guard catches all observed mismatches and sends 2,561 adversarial queries to the original intersection test. Only 14/61,680 random queries touch that guard. This is measured evidence, not a general numerical certificate.

The animation below compares the geometric reference, the failed smooth fit, and the event lookup. All use the same fixed camera and moving light orbit. The still uses the worst held-out light for the fitted model (index 0).

![Fixed-branch relighting comparison](/Users/ryanwible/projects/wrela/docs/research/compiler-frontiers-2026-09-21.gif)

The event boundaries also allow piecewise integration of `visibility × diffuse response`. An antiderivative table for a uniform **angular line emitter** of width 0.12 radians produces 0.113% error against 256-sample geometric quadrature, versus 27.1% for one central light sample. The 128→256 reference convergence difference is 0.276%; the reported residual therefore includes reference quadrature error. This is not integration over the two-dimensional solar disk. The current float64 integral product is 294,480 bytes, substantially larger than the visibility-only table.

**Limits:** fixed geometry and a one-dimensional light path are essential to this result. Deformation, changed snow geometry, different light elevation and nearby blockers invalidate it. Per-needle centroid visibility is insufficient for close-up quality. Compilation considers needle pairs; unique shape variants and pose tables can cause prohibitive compilation and memory growth. Identical families can share products, but arbitrary procedural variation cannot safely share their visibility.

**3. Correctly deleting work does not guarantee a faster GPU program.**

The compiler constructs position balls and normal cones for 256 patches generated by the actual winter terrain compiler. It rejects a light only if every admitted normal points away from every admitted light direction. It disables this rule for transmission/two-sided materials and widens the cone for known motion. The production point-light attenuation has infinite support, so no invented distance cutoff is used.

The intentionally favorable mixed fixture puts four of eight lights below the terrain. It excludes 834/2048 pairs (40.7%) with zero false exclusions across 131,072 sample/light checks. All eight lights above the terrain exclude zero pairs. These bounds require the actual shading normals, including bump and deformation; unbounded normal perturbations must keep the general path.

GPU tests reuse the production GGX function and compare a generic eight-light loop, a compiled compact list, and generated programs grouped by identical active-light sets. All outputs match exactly. The mixed case requires 13 generated variants; the above-ground case requires one. Lists show a modest warmed benefit in the favorable case and a clear penalty where nothing can be removed. Extra dispatches/indirection often erase the theoretical saving. The generic shader already cheaply returns zero for back-facing GGX, which limits the available benefit.

These tests do not establish a substantial frame-time win. A sensible future selector must retain the generic program when specialization loses; many more lights would require a separate architectural investigation.

**4. Joint integration is a real compiler transformation, not just a handwritten shortcut.**

`fourier.ts` accepts a small expression tree of constants, integer-carrier cosines, addition and multiplication. It expands products by convolution, combines equal modes, preserves correlations and emits WGSL. Unsupported functions are outside this prototype; excessive symbolic expansion rejects the transform instead of truncating it. It uses the existing compiler phase-footprint machinery on the CPU.

The test expression multiplies two correlated signals: `(0.6 + 0.35 cos(phi)) × (0.5 + 0.45 cos(phi + delta))`. Compilation yields a constant and four conjugate mode pairs. Pixel and shutter integration are analytic for an affine phase footprint; the source expression itself has no fitted approximation.

Across 160 independent footprints, the compiled result differs from 48³ dense samples by 0.00966% relative L2. Independently filtering the factors gives 18%; a point sample gives 75.6%. The 24³→48³ convergence difference is 0.0299%. The correct joint average retains energy that independent filtering deletes.

The generated GPU program was checked at 65,536 inputs against an independent CPU closed-form expression; maximum absolute error is 1.59e-07. Its isolated median is 0.0184 ms, compared with 0.0737 ms for 64 samples and 0.5509 ms for 512. Those sampled GPU alternatives still have 8.27% and 0.801% error versus the analytic result.

This does **not** imply those speedups for water or the whole frame. GGX, arbitrary noise warps, thresholds, visibility and nonlinear motion do not generally belong to this finite function class. Deliberately applying an affine footprint to accelerated phase motion gives 35.1% error. The compiler must carry validity conditions or subdivide/fall back when the phase model fails.

**5. Spatial edit compilation already exists in part; transport is the missing relationship.**

`packages/world/src/session.ts` already filters terrain interventions into each patch's content key with a spatial margin. This experiment does not claim a newly invented terrain cache or an added production performance win.

The probe adds a local radius-1.2 m, height-4 m terrain intervention. Including the existing 5 cm finite-difference normal stencil selects 4 of 256 fixed patches. Rebuilding those and reusing the others exactly matches full regeneration across 124,416 float32 position/normal components. Timing is retained in the raw artifact, but rebuild counts and exact equality are the reliable evidence here.

The shadow counterexample uses a separate sampled height-field horizon reference, with the sun along +x. At elevation 0.12 radians, 303 receivers change shadow state, 260 outside the local edit box. A conservative downstream strip invalidates 1,474/65,536 receivers and misses none. Elevation 0.6 is also tested. This establishes why future compiled lighting products need transport-aware influence regions. Reflections, indirect illumination and changed water paths can require much broader invalidation than this one-direction shadow example.

**6. Shared causes can replace independent authoring channels, but the physical model still matters.**

A 96×96 toy terrain runs snowfall interception, conservative directional drift, temperature-controlled melt, downhill liquid routing, retained wetness and a later refreeze. Snow coverage, albedo, roughness, wetness, ice and an illustrative branch-load response derive from the same state. Cold and warm controls change melt and ice consistently; water-equivalent mass plus boundary export is conserved to floating-point precision.

![Shared environmental fields, not a production scene](/Users/ryanwible/projects/wrela/docs/research/compiler-frontiers-2026-09-21.weather.png)

This proves a dependency/coherence pattern, not realistic snow. Routing is grid-directed, canopy exposure and thermal coefficients are illustrative, and branch stiffness is an authored parameter. The visible grid artifacts are evidence that better transport/discretization is needed for an art-quality implementation. Artist overrides and deliberate exceptions remain necessary. No measured authoring-hours saving or rendering speedup is claimed.

**What I would build next**

1. Promote the finite expression/integration prototype into a narrowly supported appearance product, then validate an actual rendered procedural material across camera distance and motion. Retain joint carrier identities and generated shader source. Do not force arbitrary shaders into this representation.
2. Extend the branch experiment to spatially varying visibility and a small, explicitly supported deformation domain. Compare explicit events against a proper geometric reference and a broad-response aggregate. Test memory and recompilation cost across many unique branches before adopting it for forests.
3. Add transport influence/dependency metadata when a compiled lighting product first needs it, reusing existing product invalidation and terrain support machinery. Keep sharp events separate from smoothly approximated response.
4. Use a shared snow/wetness state when authoring the richer winter scene; treat it as a controllable content generator and validate the result visually.

The architectural implication is a small set of different realizations selected from authored structure: finite analytic integrals where algebra closes, explicit events where visibility changes, statistical aggregates where only the broad response survives, and ordinary rendering elsewhere. A universal cached response would hide exactly the difficult cases this investigation exposed.

**GPU methodology and evidence**

Three independent hardware Chrome runs, each with eight alternating-order trials per mode. Each timestamp spans 32 dispatches; results below divide by 32. The lighting workload evaluates 262,144 queries by repeating 16,384 terrain points 16 times. The integration workload evaluates 65,536 footprints. These are offscreen compute throughput tests, not scene frame times, display FPS or end-to-end game performance. Adapter clocks were not measured; run medians expose variation. No timings from overlapping renderer passes are added together.

| Workload | Program | Pooled median ms/query batch | Range of the three run medians |
| --- | --- | ---: | ---: |
| lights-mixed | generic | 0.1208 | 0.1065–0.1772 |
| lights-mixed | compiled | 0.1055 | 0.1004–0.1208 |
| lights-mixed | specialized | 0.1167 | 0.1106–0.1638 |
| lights-above | generic | 0.1526 | 0.1352–0.1618 |
| lights-above | compiled | 0.1946 | 0.1597–0.2109 |
| lights-above | specialized | 0.1341 | 0.1239–0.1597 |
| correlated-filtering | analytic | 0.0184 | 0.0184–0.0205 |
| correlated-filtering | grid4 | 0.0737 | 0.0737–0.0748 |
| correlated-filtering | grid8 | 0.5509 | 0.5468–0.5519 |

All three final browser manifests report `sourceStable: true`; their bundles and raw results are preserved at:

- /Users/ryanwible/projects/wrela/output/browser-1790038179840-80557
- /Users/ryanwible/projects/wrela/output/browser-1790038181465-80608
- /Users/ryanwible/projects/wrela/output/browser-1790038182895-80632

The CPU manifest is stable, with source fingerprint `78ca84fd02e55b4d36283c4e00387355451c52d1d6162e0d7cb9d4ccbd13cf28`. The appearance implementation SHA-256 is `8652b6e0642a9489c48ee93426f22c58a9c624692e750bf5cd69ae4f2b319c56`. Raw CPU/appearance data, emitted shadow-event arrays and references, and per-run GPU JSON live in `/Users/ryanwible/projects/wrela/output/compiler-frontiers`. The consolidated companion JSON preserves the numerical evidence.

Validation: 27 TypeScript tests passed (research plus existing phase and streaming tests), 5 Python invariant tests passed, both TypeScript configurations passed, workspace boundaries passed, and formatting checks passed for the six new TypeScript files. GPU runs reported no validation errors, exact lighting equality, and generated-integral agreement with the CPU reference. Production rendering was not modified, so no new winter scene frame-time improvement is claimed.

**Prior art and what is specific to Wrela**

Appearance-preserving geometry aggregation has substantial precedent: [Loubet and Neyret's hybrid mesh/volume LoDs](https://onlinelibrary.wiley.com/doi/abs/10.1111/cgf.13138) separate resolved surfaces from sub-resolution structure. [Yang and Barnes](https://yyuting.github.io/docs/eg_2018.html) demonstrate compiler-based smoothing of procedural programs. These precedents support investigating automatic transformations; they do not establish that our prototypes are equivalent or production-ready.

Visibility-event representations also predate this work; [Seales and Dyer's shaded polyhedral animation](https://graphicsinterface.org/wp-content/uploads/gi1990-21.pdf) exploits changes in visibility, while [coherent shadow maps](https://diglib.eg.org/server/api/core/bitstreams/1fb3a412-4a6a-496e-9140-c6592dd10252/content) compress precomputed visibility. [Fearing's snow model](https://www.cs.ubc.ca/labs/imager/tr/2000/fearing2000a/) treats accumulation and stability as causes of appearance.

Our opportunity is to derive suitable representations, validity conditions and dependencies directly from semantic authoring, with reproducible comparisons and automatic fallback. No claim of scientific novelty or exclusivity is made.
