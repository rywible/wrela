**Water rendering research — September 23, 2026**

We have a useful rendering and authoring foundation, but the current water does not meet the intended visual bar. There is a credible route to substantially better oceans, lakes, rivers, and creeks with bounded cost. The strongest approach is to compile a semantic water description into several cooperating representations: large-scale waves, filtered surface appearance, bed and boundary data, and local dynamic simulation. Full 3D fluid simulation should serve specific interactions that need it.

This is an implementation audit and research proposal. No production code was changed. Proposed budgets and compiler benefits below are hypotheses to measure, not achieved performance.

**What exists today.** The table describes the working tree inspected for this audit; older research documents sometimes describe superseded defaults.

| Area | Implemented | Important limit |
|---|---|---|
| Surface source | Up to eight sinusoidal height waves; analytic normals and vertical velocity; current advection; weather-driven amplitude and roughness | One still-water elevation; no horizontal Gerstner displacement, spectral ocean, or fluid state. Wavelengths are restricted to 0.5–100 m |
| Open water | A camera-following 640 m grid with 256 subdivisions per axis in a world; smaller authoring preview | No ocean geometry clipmap, horizon-scale realization, or dedicated lake boundary model |
| River authoring | Joined channel ribbon, variable width/depth, bounded mesh generation, wet/dry queries, bank diagnostics | At most 32 centerline points; all water stays at one elevation. No downhill profile, channel junction solver, or waterfalls |
| Flow | Uniform current advects wave phases; interaction queries follow river tangents and slow near banks | The visible waves still use uniform world-space advection. No pressure, conserved discharge, obstacle response, eddies, or backwater computation |
| Optics | Fresnel, absorption and approximate scattering, rough sky reflection, screen-space opaque-scene reflection/refraction | Offscreen/hidden geometry cannot be recovered from the screen buffer; no caustics or complete underwater volume rendering |
| Foam | Bank coverage from ribbon vertex colors, with animated procedural breakup | No transported foam state, crest production, impact foam, bubbles, or spray |
| Physics | Height/normal/current queries; one-point buoyancy and velocity drag; weather/query consistency | Bodies do not displace water or generate wakes; no two-way fluid coupling or multi-point hydrostatics |
| Compiler | Wave phase carriers, geometry error bounds, correlated appearance filtering, coherent phase reductions, specialized two-wave glint integration | The strongest glint optimization has a narrow validity domain; it is not a general ocean shading solution |
| World integration | One optional water-document reference per world | Multiple lakes at different levels and connected river/ocean networks need a broader source model |

The main implementation references are [water source](../../packages/model/src/documents.ts), [flow authoring](../../packages/model/src/environment-authoring.ts), [water mesh and queries](../../packages/compiler/src/water.ts), [phase compilation](../../packages/compiler/src/phase.ts), [scene installation](../../packages/runtime/src/scene-host.ts), [buoyancy](../../packages/runtime/src/physics.ts), [water shading](../../packages/render-webgpu/src/water.wgsl), and [optical transport](../../packages/render-webgpu/src/water-transport.wgsl).

The authored river depth currently affects interaction queries. Optical depth comes from opaque-scene depth, and the river does not automatically carve its bed. Consequently, authored depth, visible bed, and physical depth can describe different water. A common bed/bank representation would fix a substantial authoring weakness.

**Fresh visual and verification evidence.** A frozen-source Chrome/Metal capture at 1024×768 rendered the creek close-up successfully, with complete content and no diagnostics. The source fingerprint remained unchanged throughout the capture. The image shows broad oval/marbled reflections, very straight banks, and an abrupt water/terrain composition. It is recognizably water, but does not convincingly depict a natural shallow creek. A still image cannot establish motion quality.

![Current creek close-up, September 23, 2026](/Users/ryanwible/projects/wrela/docs/research/water-current-shore-2026-09-23.png)

The focused current-tree suite passed **26 tests, 2,672 assertions, zero failures**. It exercises wave/query agreement, river support and diagnostics, phase integration, glint math, packing, transport ownership, geometry bounds, and weather/buoyancy integration.

The independent hardware accuracy check **failed before evaluating accuracy**: its isolated shader harness still supplies `physicalSunTransmittance`, while the current water shader calls `physicalCachedSunTransmittance`. Inspection also found the missing `physicalSkyRoughReflection` stub. This is a harness integration failure, not evidence that the production water shader cannot render. Repair and rerun it before making a new numerical accuracy claim. Earlier successful accuracy numbers are historical.

A second landscape capture could not acquire the cooperative GPU lease because a concurrent vegetation benchmark owned it. That process was left alone. This audit does not provide a new sustained performance comparison. Capture provenance, commands, outcomes, and the raw frame report are saved in [the evidence record](water-rendering-2026-09-23.json).

**Where performance stands.** The September 21 [scalability study](winter-rendering-scalability-2026-09-21.md) estimated **1.86 ms incremental water shading/transport cost** on Apple M4 at native 1920×1080, using repeated-pass comparisons under sustained load. That is a useful historical anchor, not a current-source or cross-hardware guarantee. Backend timestamp intervals overlap and must not be added as exclusive costs. The fresh close-up's few timestamps are unsuitable for establishing sustained water cost.

The compiler's [two-wave glint path](../../packages/render-webgpu/src/water-glints.wgsl) requires exactly two suitable independent carriers, roughness between 0.06 and 0.12, no point lights, and additional view, shadow, curvature, shutter, and conditioning checks. **The current creek has roughness 0.22, so it cannot use this specialized path.** Other automatic filtering remains active. Ordinary unresolved sharp highlights can still require up to 64 lighting responses; coherent and reference variants have different limits.

There is also a substantial memory/bandwidth floor: [WaterTransportGpu](../../packages/render-webgpu/src/water-transport.ts) owns full-resolution RGBA16F color and R32F depth snapshots, totaling **23.73 MiB at 1080p**, before meshes, other renderer targets, or any new fluid simulation. A small creek still triggers full-frame capture. Improving the wave evaluator alone does not remove that cost.

**Research that changes the design.** These are primary papers, author resources, and current implementation documentation; the recommendations following them are our synthesis.

| Reference | What it contributes | Relevance to Wrela |
|---|---|---|
| [Tessendorf, Simulating Ocean Water](https://people.computing.clemson.edu/~jtessen/reports/papers_files/simdoc.pdf) | Spectral ocean synthesis | Benchmark a multiscale spectral realization against a compact explicit wave sum |
| [Bruneton, Neyret, Holzschuch, ocean geometry-to-BRDF hierarchy](https://evasion.inrialpes.fr/~Eric.Bruneton/) | Transfers unresolved wave detail from geometry into normals and reflectance | Strong precedent for compiling wavelength bands into the representation that can actually resolve them |
| [Crest, SIGGRAPH 2019](https://advances.realtimerendering.com/s2019/index.htm) | Multiresolution data shared by displacement, dynamic waves, foam, and flow | A practical reference for concentrating detailed work near the viewer |
| [Yu et al., Scalable Real-Time Animation of Rivers](https://evasion.imag.fr/Publications/2009/YNBH09/riversEG09.pdf) | Boundary-aware procedural velocity and advected surface detail | A useful inexpensive river baseline, distinct from a conservation-law fluid solver |
| [Jeschke and Wojtan, Water Wave Packets](https://research-explorer.ista.ac.at/record/470) | Particles represent groups of wave trains | A compiler-friendly research candidate for localized, controllable waves; implementation is available from the authors |
| [Jeschke and Wojtan, dispersive shallow water, 2023](https://research.nvidia.com/labs/prl/shallow-water-simulation/files/Dispersive_Waves_in_SWE.pdf) | Separates bulk flow and dispersive surface waves | Explains why ordinary shallow-water dynamics alone are insufficient for realistic deep-water wakes |
| [Crest shallow-water implementation](https://docs.crest.waveharmonic.com/Components/Inputs/ShallowWaterSimulation.html) | Local domains, fixed/moving placement, cached beds, simulation limits, and baked flow/foam outputs | Concrete evidence that local simulation and reusable static products are useful together |
| [Macklin and Müller, Position Based Fluids](https://mmacklin.com/pbf_sig_preprint.pdf) | Iterative particle density constraints for interactive liquid simulation | A candidate for bounded 3D splash/pouring experiments, with quality and incompressibility costs to measure |
| [Clawpack shallow-water solvers](https://www.clawpack.org/master/riemann/Shallow_water_Riemann_solvers.html) | Conserved depth/momentum, bathymetry, and wet/dry solver choices | A numerical reference for validating the local fluid solver |

The 2023 dispersive method is particularly interesting, but its published 512×512 CUDA experiment on an RTX 2080 Max-Q ran the simulation at about 100 fps, approximately 10 ms, with 87% spent on decomposition. Its heightfield model also has steep-wave and thin-boundary limitations. It is evidence for the physical decomposition, not evidence that we can fit that implementation into a submillisecond WebGPU budget. Start with simpler local dynamics and introduce dispersive coupling only when a shot or interaction requires it. [Paper, results and limitations](https://research.nvidia.com/labs/prl/shallow-water-simulation/files/Dispersive_Waves_in_SWE.pdf).

**Recommended water architecture.** Preserve semantic authoring and choose realizations according to the phenomenon:

| Experience | Baseline realization | Local dynamic addition |
|---|---|---|
| Ocean | Directional multiscale wave spectrum, choppy displacement, geometry LOD, filtered slopes, crest foam | Boat wakes and shoreline interaction where relevant |
| Lake | Bounded basin, wind/fetch-dependent waves, strong reflection and depth composition | Ripples, wakes, inlet flow, changing water level if gameplay needs it |
| River | Downhill surface/bed profile, continuous flow direction, obstacles, wet banks | Conserved depth/momentum for rapids, eddies, obstructions, and transient flow |
| Creek | Detailed shallow bed, restrained highlights, small ripples, convincing rock contact and caustics | Small high-resolution domains around stones, feet, and other interactions |
| Waterfall/splash | Authored falling sheets and bounded particles for predictable flows | A small 3D fluid domain where changing topology is essential |

Shallow-water equations are real fluid dynamics: they evolve depth and horizontal momentum over the bed. They discard vertical structure. A heightfield cannot fold over into a breaking barrel or describe airborne spray. Particle or volumetric methods address those cases, with additional neighbor-search, collision, surface reconstruction, and rendering costs. Treat that extra capability as a local requirement.

Separate bulk water level/current from wind waves, dispersive interactions, and appearance detail. Give each layer an explicit frequency range and coupling policy so adding a local solver does not double the wave energy. Keep gameplay-relevant simulation active beyond the camera when its downstream effects matter; visual invisibility alone is not a valid reason to stop conserved flow.

**Where the compiler can earn its cost.** The following are proposed experiments, informed by the source audit and references above.

1. **Compile the bed and boundaries once.** Derive bed elevation, shore distance, obstacle masks, local flow coordinates, bounds, and optical material regions from the same semantic source. Reuse them for geometry, wet/dry queries, fluid boundaries, foam, and caustics. Rebuild affected tiles after an edit; dynamic obstacles still need runtime updates.
2. **Compile waves by scale.** Emit displacement for resolvable geometry, slopes for resolvable shading, and statistical reflectance data for smaller detail. Precompute fixed spectral coefficients, dispersion factors, and conservative bounds. A GPU texture or FFT product generated from source is a valid compiled realization; it does not require manually authored textures.
3. **Compile static flow plus a dynamic residual.** For a fixed channel and specified inflow/outflow, solve a useful equilibrium during preparation and reuse it. Local runtime perturbations must preserve boundary flux and total volume. A painted or fitted current field alone is not proof of physical conservation.
4. **Select specialized optics.** Clear shallow water needs bed transmission and caustics; turbid deep water can terminate transmission sooner; distant water mainly needs stable reflected energy. Remove expensive paths only when the source or an error bound justifies doing so. Dynamic lighting and unseen reflected objects cannot generally be compiled away.
5. **Replace repeated micro-lighting when justified.** Keep explicit coherent waves where phase matters; investigate slope distributions for unresolved stochastic bands. Preserve the existing guarded glint path where valid. Summing independently shaded wave pairs is not a correct extension to eight waves: the reflectance of the combined normal is nonlinear.
6. **Bound simulation and transport work.** Experiment with 128² and 256² local grids, fixed-step substepping, screen-coverage-aware shading, and lower-resolution transport with depth-aware reconstruction. Reflection rays can hit outside the visible water rectangle, so naive cropping of the source snapshot is unsafe.

These optimizations can remove redundant evaluation and unnecessary resolution. They cannot precompute unpredictable object impacts, replace every nonlinear fluid interaction with a closed form, or establish low-end performance without hardware measurements.

**The first coherent milestone.** Build a short rocky creek that flows downhill into a clear lake. The user should be able to disturb the water and watch ripples and foam move around stones. Develop an isolated ocean horizon/storm benchmark alongside it to test spectral scaling without making the creek wait on a complete ocean system.

| Experiment | Concrete deliverable | Acceptance evidence |
|---|---|---|
| Restore the baseline | Updated independent GPU harness; camera paths through creek, lake, and open-water views | Frozen source, clean rendering, numeric reference results, moving footage, sustained baseline |
| Unify water and terrain | Bed/profile source, multiple bounded bodies, continuous sloping river support | Rendered shore, queried depth, collision, and current agree; no water beyond the basin |
| Establish visual quality | Multiscale detail, coherent reflections, believable transmission, advected foam; shallow caustics where useful | Review noon, low sun, overcast, near shore, underwater transition, and camera motion against reference footage |
| Add local real dynamics | One small shallow-water domain with bed/obstacles and interaction forcing | Still lake remains still; nonnegative depth; closed-domain volume accounting; inflow/outflow balance; obstacle response; stable wet/dry transitions |
| Prove compiler savings | Static-product and specialization variants against the same visual source | Matched image/motion quality, path coverage, compile/invalidation cost, GPU p50/p95, memory, and bandwidth accounting |
| Expand selectively | Spectral ocean, dispersive wakes, then one bounded 3D fluid experiment | Each addition earns its whole-frame cost and preserves lower-tier behavior |

Start the solver at fixed spatial bounds. Moving domains, boundary remapping, and multiresolution flux exchange introduce correctness issues before they provide a demonstrated benefit. Use solver-appropriate stability limits and retain enough numerical precision for depth and momentum. Large steps or aggressive damping can look stable while erasing the behavior we wanted.

A reasonable **initial performance hypothesis** is total water at or below 2 ms p95 on the M4 at 1080p, with no more than 0.5 ms of that spent on local simulation, and at most 48 MiB of water-owned resources including the existing snapshots. These are aggressive experiment gates, not forecasts. Also measure a reduced profile on an actual weaker integrated GPU; lowering quality on the M4 is not a substitute. Include full-screen water, a narrow creek, grazing sun, storm foam, point lights, and moving interaction workloads. Report normal presentation cadence separately from saturated throughput.

For fluid/physics integration, avoid a synchronous GPU readback every frame. Compare coarse CPU queries and bounded asynchronous GPU sampling, document their latency/error, and share the same base wave source. Render/physics disagreement will be visible immediately when a floating object intersects the surface.

The first artistic priority is believable contact between water, bed, and banks, followed by multiscale motion and stable reflected light. The current capture already shows that improving only the highlight integration would leave the largest visual weaknesses intact.
