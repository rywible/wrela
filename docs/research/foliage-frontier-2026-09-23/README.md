# Foliage frontier and look development study

September 23, 2026. [Open the visual comparison board](index.html). [Measurements and provenance](evidence.json). [Compiler product census](census.json).

**The next milestone should be one convincing lodgepole shoot, branch, and tree, with the compiler preserving their appearance as detail becomes unresolved.** Our shaped pine is a better structural prototype, but its foliage still reads as sparse fuzz arranged in repeated tiers. More twigs are an expensive way to compensate for that. The most valuable compiler work is to derive several faithful representations of the *same* botanical source, share repeated geometry, and select detail at branch/shoot scale for both camera and shadows.

This study adds research, captures, a short benchmark, and a product census. It does not change the production renderer or declare the asset visually approved. The existing shape-first pivot remains appropriate; neither a new ecological simulator nor a complete Nanite implementation is a prerequisite.

## Evidence and comparison method

The inspected source is frozen at `output/vegetation-lookdev/1790184534521-43476/source`, fingerprint `c7003ec9e9e39c5c40e02c7804eb3063415d0bbcf9209d88d2ede45eb81c8d46`. The live workspace contains other ongoing changes. Findings below refer to this snapshot, not an assumed clean Git commit.

- Captured eight current views: isolated branch, whole tree, silhouette, close detail, backlight, near grove, gameplay grove, and distant grove. All completed on the Apple Metal hardware adapter at 1024 × 768 with MSAA. No GPU timing from these individual captures is used as a performance result.
- Added a separate static temporal comparison. The ordinary renderer capture API deliberately disables temporal reconstruction. The accepted temporal images therefore use browser compositor screenshots after 32 history frames, with active reconstruction checked. The initial attempt through the ordinary API is excluded from the comparison. Static images cannot establish moving-camera stability or wind quality.
- Visually inspected NPS lodgepole photographs and official images from **Alan Wake 2, Horizon Zero Dawn Remastered, Avatar: Frontiers of Pandora, and Ghost of Tsushima**. These are selected high-quality exemplars, not a ranking of every AAA game. The Witcher 4 technical demo is a technology reference, not shipped-game evidence.
- Comparisons are qualitative: species, age, field of view, weather, exposure, camera response, output resolution, and scene dressing differ. Photographs are morphology references, not calibrated reflectance measurements. A mature closed stand is not a target silhouette for every young open-grown tree. Alan Wake's conifers and Pandora's fictional flora are not lodgepole anatomical references.

The board keeps the original image aspect ratios and identifies the source and comparison limits. It includes native-resolution Wrela images; thumbnail attractiveness alone is not the acceptance test. Reference images remain research material with source credits, separate from game assets.

## What is visibly missing

These are judgments from the inspected images, followed by code-supported hypotheses. They are not measurements of error against registered photographs.

| Scale | Wrela observation | Reference observation | Action |
| --- | --- | --- | --- |
| Needle and shoot | Thin, comb-like strands and stippled sheets dominate the close view. The isolated branch is airy throughout rather than having distinct, substantial terminal tufts. | NPS images 13171 and 16971 show needles wrapping around shoots, dense local masses, overlapping tips, visible bare wood between tufts, and varied orientation. | Author a few reference-matched 3D shoots. Preserve paired attachments, curvature, taper, cohort placement, and actual gaps before reducing them to coverage fields. |
| Branch | Repeated secondary/twig arrangements and smooth tubes are prominent. Main limbs look inserted into a smooth trunk. | The NPS branch has uneven tuft spacing, local density changes, and rough, smaller woody connections. | Group variation at branch and cohort scale; improve collars, taper, bark plates, and twig-to-branch transitions. Random tint per tiny element is insufficient. |
| Whole tree | The central trunk remains very legible through many nearly horizontal tiers; foliage has limited medium-scale mass. | Real stand crowns form irregular clusters and openings; Alan Wake's forest also reads as larger masses broken by branches and gaps. | Tune crown lobes, branch reach and inclination together. Create open-grown, crowded, and damaged *authored* variants; do not simply increase all density parameters. |
| Light | In the backlit fixture, much of the crown becomes very dark. In daylight, needle sheets have limited separation and the trunk surface is smooth. | Real needles show directional highlights and self-shaded tufts; AAA scenes use sun/shade separation and nearby vegetation/ground to establish depth. | Validate shoot normals and optics in neutral front/back/grazing tests, then add local canopy visibility and scene bounce. A dark backlit tree alone does not prove a transmission bug. |
| Grove | Similar silhouettes repeat on a neutral floor. The fixture reveals structure but cannot demonstrate a finished forest. | Horizon integrates low plants, litter, moss, rocks, and readable clearings; Avatar ties plants to the surfaces they inhabit; Alan Wake layers trunks, crowns, undergrowth, and atmosphere. | After the specimen passes, author one small forest edge with a walkable clearing, understory, litter, exposed roots, and a distant backdrop. Use a restrained biome palette. |
| Image stability | Fine stochastic texture remains a visible concern. | Attractive still images do not certify stability in any engine. | Review active temporal output and a fixed camera/wind trajectory. Separate sampling noise, motion-vector errors, disocclusion, and LOD changes. |

NPS reference provenance: [Yellowstone pine collection](https://www.nps.gov/features/yell/slidefile/plants/conifers/pine/Page-3.htm): 13171, Jim Peaco, 1989; 16971, Jim Peaco, July 1998; 13170, credited “Bob Stevensoon,” October 27, 1988. The second image is a branch close-up despite its broad location caption. Do not treat it as a whole-crown reference.

## The implementation explains part of the gap

The earlier [thin-field research](../thin-field-foliage-2026-09-22.md) predates several fixes. Current code already has filtered coverage, independent sample masks, a thin depth pass, bounded diffuse reflection/transmission, deformed shading normals, shader specialization, and a candidate crown path. Recommending these as wholly absent would be incorrect.

| Current mechanism | Audited location | Implication |
| --- | --- | --- |
| Three curved ribbons per shoot, three longitudinal segments near and one far | `packages/compiler/src/needle-ribbons.ts:6`; `conifer-mesh.ts:218` | Cheap geometry, but a small fixed set of planes supplies the visible volume and normals. View-dependent planar appearance is a plausible contributor to the look. |
| Coverage patterns use mean cohort count and median twig length; four randomized patterns and half-retained variants | `conifer-mesh.ts:140`; `thin-coverage.ts:121` | These are synthesized 2D needle patterns, not baked projections of the explicit 3D needle realization. Shape error and sampling error are currently mixed. A scaled template is not an exact source reference. |
| Scalar needle coverage; orientation fields only in the separate crown candidate | `thin-coverage.ts:88`; `render-webgpu/src/scene.wgsl:246` | Current shoots shade using ribbon normals, even though many needle orientations contribute to their coverage. Better foliage optics cannot fully repair the wrong normal distribution. |
| Shaped twigs have cohort age 0 and retention 1, with mild tint variation | `packages/compiler/src/pine-architecture.ts:104` | Controlled architecture does not yet express multiple spatially distinct needle cohorts or their seasonal appearance. Add authored cohorts without reopening ecological simulation. |
| A whole-surface distant choice at 96 projected pixels, 15% hysteresis, unknown error | `packages/compiler/src/vegetation.ts:211`; `render-webgpu/src/detail.ts:26` | Whole-tree decisions preserve substantial unnecessary shoot detail at distance. The legacy coarse mesh is shared between camera and shadow use. The crown candidate has extra light-footprint admission and separate view ranges. |
| Near wood and foliage are expanded arrays | `conifer-mesh.ts`; `botanical-motion.ts` | Repeated ribbon topology and per-vertex wind attributes are materialization costs the compiler could replace with compact instance/curve records. Exact reuse must respect curvature and variation. |
| Camera coverage samples use a local-position salt; shadow thresholds use UV/layer | `render-webgpu/src/thin-coverage.wgsl:38–67` | Repeated modules may correlate differently in shadows and camera. This is a specific sampling hypothesis to test, not a demonstrated explanation for all grain. |
| Thin occupancy prepass and equality-depth shading already exist | `render-webgpu/src/index.ts:2153` | Profile and improve this path; do not propose adding it from scratch. Tightening empty proxy area may matter more than just removing triangles. |

The leaf closure is still empirical: a 5 mm artistic attenuation scale and a bounded diffuse reflection/transmission split. It is not measured lodgepole optics or a canopy multiple-scattering solver. The corrected normal deformation follows the proxy geometry, not the missing needle-level orientation distribution. The lookdev fixtures record no indirect-lighting cache; this is distinct from the engine's separately implemented bounded static diffuse GI.

The current crown product remains unqualified. Earlier 64-view and four-cluster experiments failed coverage cases; the frontier fixture explicitly tests the **compiled mesh silhouette**, not exact 3D needles, radiance, moving shadows, or temporal quality. Increasing atlas resolution cannot by itself fix a wrong source representation.

## Frontier techniques worth transferring

### Production lessons

**Geometry sharing, aggregation, and motion must be designed together.** Epic's current experimental Nanite Foliage combines part instances, triangle-to-voxel simplification, and skeletal motion. Voxels retain a normal distribution; transition decisions use simplification error. This is strong evidence for multiple representations of disconnected foliage. It does not establish that a WebGPU port is cheap or that its published demo costs transfer to this laptop. [Epic foliage documentation](https://dev.epicgames.com/documentation/unreal-engine/nanite-foliage).

Assemblies share repeated parts inside an asset and integrate them into its hierarchy. Epic reports major asset/streaming savings in its demo, but its current documentation also lists restrictions including a single instancing layer and bind-pose part behavior. Wrela should first test compact reusable shoot templates with controlled variation, rather than copy the entire system. [Epic assemblies](https://dev.epicgames.com/documentation/unreal-engine/nanite-assemblies).

**Fine culling and hierarchical wind are proven production directions.** Remedy describes meshlet culling, GPU-driven rendering, and GPU foliage rigs in Alan Wake 2. Its wind strength field uses SDF-defined regions. Our semantic branch graph could compile directly to bounded motion groups with current/previous transforms. The portable implementation should use compute, storage buffers, instancing, and indirect draws; WebGPU's core pipeline exposes vertex, fragment, and compute stages, not the mesh-shader pipeline used in these native examples. [Remedy](https://www.remedygames.com/article/how-northlight-makes-alan-wake-2-shine), [WebGPU specification](https://www.w3.org/TR/2026/CRD-webgpu-20260512/).

**Grass is a distinct realization problem.** Ghost of Tsushima generates individual grass blades on the GPU. Its shared gust fields coordinate trees, grass, cloth, and particles, with a separate character-displacement response. AMD's procedural grass example uses parametric curves, density LOD and width compensation. Wrela can test compute/vertex-generated blades per patch, while treating width compensation as an approximation that must preserve clumping, gaps, and shadow density. [GDC grass talk](https://www.gdcvault.com/play/1027033/Advanced-Graphics-Summit-Procedural-Grass), [Sucker Punch wind explanation](https://blog.playstation.com/?p=345372), [AMD grass example](https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/).

**Authorship remains directed.** Horizon's remaster improved geometry, materials, interaction, biome density, terrain integration, and lighting together. Massive describes parent-child placement rules followed by artist adjustments. Sucker Punch used real leaf scans and deliberately limited foliage variety/noise in individual biomes. These favor reference-grounded recipes with local overrides, not uncontrolled random scattering. [Horizon](https://blog.playstation.com/2024/10/17/horizon-zero-dawn-remastered-a-deep-dive-into-its-enhancements/), [Massive scattering](https://www.massive.se/article/crafting-pandoras-breathtaking-landscape-with-snowdrop/), [Tsushima environment art](https://blog.playstation.com/2020/07/09/crafting-the-world-of-tsushima/).

Epic's 5.8 release notes also describe experimental growth, skeletal extraction from meshes/images, grafting, avoidance, and trunk-material authoring. The useful lesson for us is editable construction with fast visual feedback. The existence of a growth solver elsewhere is not evidence that our rejected ecological model should resume. [Epic release notes](https://dev.epicgames.com/documentation/unreal-engine/unreal-engine-5-8-release-notes).

### Filtering and light transport

Hashed alpha testing addresses disappearing minified coverage by replacing a fixed threshold with stable stochastic thresholds, with residual noise. We already use related machinery. The remaining question is whether our coverage input, overlapping samples, shadows, and reconstruction agree with the true shoot. [Wyman and McGuire](https://research.nvidia.com/labs/rtr/publication/wyman2017hashed/).

SGGX offers a filterable directional microflake distribution parameterized by projected area. It is relevant when many needles are unresolved. It does not provide spatial gaps, clumping, or canopy illumination by itself. Our proposed aggregate must carry spatial support and directional extinction in addition to a normal distribution. [Heitz et al.](https://research.nvidia.com/publication/2015-08_sggx-microflake-distribution).

Massive describes the contribution of small details and dynamic scene interactions to Avatar's indirect lighting. That supports improving local canopy/ground lighting, but does not make hardware ray tracing the immediate requirement for Wrela. Start with measurable compiled local visibility and our existing GI framework. [Snowdrop lighting](https://www.massive.se/article/snowdrops-ray-tracing-shines-a-light-on-pandora/).

Correct deformed motion vectors matter for reconstruction. AMD's foliage guidance specifically addresses missing foliage velocity. We already evaluate previous motion; verify it through high-contrast moving shoots, history rejection, and disocclusions instead of assuming the presence of velocity data is sufficient. [AMD temporal foliage guidance](https://gpuopen.com/learn/fsr-2-1-unreal-engine-plugin-part2/).

**Research horizon, not a next sprint:** 8DNA, a SIGGRAPH 2026 paper, precomputes complex asset light transport into a neural representation including near-field illumination. It suggests a longer-term compiler opportunity to distill expensive local transport. Its abstract does not establish editable, wind-deformed forest rendering at our frame budget. First test small deterministic transfer tables; defer neural training/inference infrastructure. [Wu et al., 8DNA](https://research.nvidia.com/labs/rtr/publication/wu20268dna/).

## What our compiler should own

These are Wrela proposals, not features claimed to exist or measured speedups.

1. **One canonical shoot description.** Emit actual needle centerlines, widths, frames, attachment pairs, cohort/material IDs, and bounded deformation from the authoring graph. Generate the explicit reference and every optimized product from this record. Today the filtered and explicit paths use different constructions.
2. **A small reusable shoot vocabulary.** Separate topology/template geometry from instance curves, transforms, color/cohort variation, and source identity. Share coverage and material data by content key. Start with measured clusters of shape parameters; do not silently turn genuinely different shoots into identical copies. Report visible repetition as well as saved bytes.
3. **A branch/shoot hierarchy.** Each node owns children, conservative rest/motion bounds, source coverage, material statistics, render alternatives and fallbacks. Choose explicit needles for resolved hero detail, accurate shoot proxies for intermediate scale, and spatial directional aggregates only when their error is acceptable. Keep resolved wood explicit.
4. **Separate camera and light decisions.** A tiny camera footprint can still cast a resolved nearby shadow. Qualify sun/local-light products and off-camera casters independently. Avoid per-pass duplicate residency where a shared product works. Use hysteresis and test transition/history costs.
5. **Appearance-aware reduction.** Preserve occupancy, depth spread, orientation distribution, mean radiance and directional transmission. A small positional error does not certify a good leaf LOD. Extend the metric contract to cover fractional coverage and shadow error explicitly; do not insert unitless coverage error into a world-space silhouette bound.
6. **Compile away invariant work.** Existing exact foliage/bark specialization is valuable. Extend it with bounded feature masks, constant material terms, shared wind evaluation, and optional precomputed bark/detail tables when measured error permits. Keep invalidation/fallbacks when weather, layers, lights or source edits invalidate assumptions. Limit pipeline variants to avoid compile-time and cache explosion.
7. **Local invalidation.** A pruned branch should rebuild its affected products and ancestor bounds, not every tree or global atlas. Preserve stable source ownership, sparse edited occurrences, cooked/worker transfer, and memory accounting.

For directional canopy data, visibility and transmission must remain distinct. A useful first prototype stores local sky visibility and sun-direction optical depth for one static shoot. Combine it with external occlusion without counting the same self-shadow twice. A single homogeneous extinction coefficient cannot capture arbitrary clumping; validate both sparse and dense shoots. Retain a direct fallback outside the sampled light/motion domain.

## Performance evidence and priorities

The new measurement uses the same frozen source as the visual study: M4 MacBook Air, 8 GPU cores, 16 GB, AC power, native 1920 × 1080, temporal AA, moving camera, nine-tree shaped grove, no lighting ablations, shader specialization enabled. Five-second warmup, three consecutive 20-second trials, bounded queue depth 4, one game update per submitted frame. GPU samples match submitted frame identities; errors were empty. No claim is made that unrelated applications were absent.

| Trial | GPU p50 / p95 / p99, ms | Total CPU p95, ms | Completed frames/s |
| --- | --- | --- | --- |
| 1 | 6.88 / 8.19 / 9.18 | 1.60 | 124.0 |
| 2 | 6.42 / 7.73 / 8.06 | 1.60 | 128.8 |
| 3 | 6.62 / 7.86 / 8.26 | 1.70 | 126.9 |

Maximum tracked GPU allocation: 254,886,497 bytes (243.1 MiB). This is a short current baseline, not a controlled before/after improvement, sustained thermal pass, display presentation measurement, or forest/character/streaming qualification. **All three trials exceed the project's 6 ms GPU p95 target.** The prior sustained failure remains open. Different trial frame counts traverse different tick spans; use fixed-tick alternating runs for causal optimization comparisons.

Pass intervals suggest where to experiment: opaque p95 4.39–4.65 ms, thin-depth 2.95–3.15 ms, and shadow 2.36–2.95 ms. These intervals overlap on this GPU and must not be summed or treated as predicted savings.

The CPU product census is more directly actionable:

| Single seed-73 specimen, review quality | Near wood triangles | Near foliage triangles | Far total triangles |
| --- | ---: | ---: | ---: |
| Original | 14,466 | 19,098 | 7,536 |
| Shaped | 34,786 | 58,968 | 20,658 |

The shaped source has 3,326 leafy twigs and 471,744 reported semantic organs; these are **not** 471,744 rendered 3D needles. Near wood + foliage contain 116,793 vertices. Both filtered products report no truncation. The distant foliage still renders all retained shoots with reduced ribbon segmentation. This motivates hierarchical aggregation, but only after source fidelity is established.

In the far review capture, the renderer reports 313,466 selected camera triangles, including terrain, with 18 coarse surfaces. The fixture's 957,378 pre-selection triangle count is a different quantity. Neither is a fragment count. This distinction matters when evaluating LOD savings.

| Priority | Optimization experiment | Why it is plausible | What must be measured |
| --- | --- | --- | --- |
| First | Trim empty ribbon regions / split oversized proxies; compare depth prepass on/off using identical coverage | The current occupancy pass is substantial and every ribbon includes empty space | Whole-frame p95, fragment/overdraw proxy, coverage error; extra geometry can be worthwhile |
| First | Compact shoot instances and generated vertex attributes | Repeated topology, expanded positions/normals/wind, and many small twigs | GPU and CPU p95, resident/upload bytes, attribute-fetch/ALU trade, cold cook time |
| First | Branch/shoot LOD with camera and shadow footprint budgets | Whole-tree thresholds leave many tiny shoots expensive | Matched quality, silhouette/gap stability, off-camera shadows, transition spikes |
| Next | Coherent motion groups and projected-motion LOD | Reuse deformation and tighter bounds; tiny flutter may be below a pixel | Normals/velocities, culling correctness, phase continuity, update cost |
| Next | Static/dynamic shadow separation and bounded distant shadow refresh | Slowly changing or unresolved casters may be reusable | Shadow lag, low-sun detail, invalidation cost, and retained casters; no blanket distant-shadow removal |
| Next | Front-to-back cluster ordering and conservative hierarchical culling | Reduce occupancy/shading work and off-screen traversal | Total costs including sorting/compaction; never use sparse foliage as a solid occluder |
| After specimen | Patch-based grass/understory generation and biome-cell residency | Keep population size separate from visible work | Per-patch cost, density transitions, interaction, and unchanged-view population scaling |

GPU-driven visibility and indirect submission already exist in the renderer for eligible surfaces. The new opportunity is useful foliage granularity and conservative bounds, not simply adding an indirect draw call. Native mesh shaders, ray tracing, work graphs and neural upscaling should be capability-specific future options, not dependencies of the portable baseline. Dynamic resolution can be a shipping policy later, but cannot pass the current native-resolution target by changing its terms.

## A bounded implementation sequence

**1. Build the visual target at shoot scale.** One terminal shoot, an older shoot, and one branch with explicit untruncated needles. Compare front, back and grazing light on neutral dark/light backgrounds; three azimuths and several scales. Tune tuft placement, dense cores, larger gaps, bare wood, and restrained cohort variation. Inspect actual dimensions; do not derive calibrated color from archival JPEGs. Approve the branch and then three whole-tree seeds before expanding forest infrastructure.

**2. Run a three-way representation test.** Same canonical source: explicit needles, today's ribbons, and a compiler-generated coverage/depth/orientation candidate. Isolate a bounded branch so the explicit path cannot hit the whole-tree geometry cap. Sweep needle widths of 0.25–8 pixels, shoot footprints of 16–256 pixels, held-out camera/light angles, sparse/dense overlap, and wind. Separate source-proxy error from raster sampling error. If richer shoot data does not outperform simple triangles at matched quality/cost, keep triangles in that domain.

**3. Establish quality budgets before selecting products.** Initial proposed gates: converged expected coverage area bias ≤2%; normalized spatial coverage L1 ≤2%; mean scene-linear radiance change ≤5% across a representation switch. These are trial tolerances, not measured perceptual thresholds. Compare shadows on a neutral receiver independently. Set motion/disocclusion tolerances using an accepted dense reference and retain image sequences; a static aggregate cannot qualify motion. If the supersampled reference has not converged below the candidate budget, refine it before judging the candidate.

**4. Improve light and wood after the shape reads correctly.** Fit needle orientation/roughness and diffuse transmission in isolated tests, add local visibility with controlled external blockers, and improve collars/bark in the close camera. Distinguish needle self-occlusion, ground bounce and canopy multiple scattering. Evaluate overcast and bright sun without changing exposure to hide an error.

**5. Optimize the accepted branch and grove.** Alternate A/B/A/B at fixed camera/simulation ticks. Preserve resolution, light path, seed, completeness and quality. Rank by whole-frame GPU p95, CPU preparation/render time, resident/transient bytes and upload traffic. Then run the existing five-minute warmup / 30-minute sustained gate on AC and battery as required. No proxy is promoted solely because it is faster.

**6. Finish one forest edge.** Use the accepted pine with a few coherent variants and limited understory, litter, exposed ground and clearings. Derive placement from slope, shade, moisture proxies and neighbors, with local art overrides and gameplay clearance. Review a walking route at native resolution with moving sun/wind, rather than only an attractive overlook. Only then extend the population/streaming ladder.

The governing experiment is small: **can one compiler-authored shoot retain the volume, highlights, gaps, and motion of a convincing 3D source for less total frame cost?** A successful answer improves both the art and the scaling strategy. A failed answer tells us which representation to keep, without committing the engine to a premature forest-wide architecture.

## Reproduction and retained limits

Commands used from the workspace, with snapshot paths recorded in `evidence.json`:

```sh
bun tools/vegetation-lookdev.ts --architecture --frame=plant-beauty,plant-branch,plant-silhouette,plant-detail,plant-backlit,stand-near,stand-gameplay,stand-far
# From the resulting frozen source directory:
bun tools/vegetation-benchmark.ts --snapshot-run --architecture --aa=temporal --moving --duration=20 --warmup=5 --trials=3
```

The study-only census and temporal screenshot scripts are preserved under `output/foliage-frontier-study-2026-09-23/`; the source snapshot and browser bundles preserve the implementation used. The 11 MB raw benchmark is retained there; the report keeps compact summaries. The first capture attempt timed out on another process's cooperative GPU lease; the subsequent successful capture and benchmark used the lease normally. No production tests are claimed because no production code was changed.

Research limitations: selected official stills, not local builds of the comparison games; no objective cross-game benchmark; no calibrated real-world image dataset; no fresh moving-image acceptance or sustained thermal test; no implementation or measured speedup for the proposed compiler products. The deferred crown failures and the current visual rejection remain evidence, not tasks to mark complete.
