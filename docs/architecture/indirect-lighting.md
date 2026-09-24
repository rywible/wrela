# Lighting: one product, compiled transport

## Default system

Player and Studio use one automatic lighting path. Authors place geometry, materials and lights; interiors, caves, night scenes and outdoors do not require a lighting-mode switch. Engineering reference/ablation controls are separate.

`BrowserSceneHost.applyIndirectLighting` composes full-resident sky visibility with a bounded compiled radiance cache. The new default supplies two-bounce diffuse transport from physical sky, a low-order sun/moon approximation, up to eight point lights and static emissive materials. It also provides low-frequency local reflections and visibility-tested lighting for moving receivers. This is approximate GI, but it is not the previous expensive world-volume solver.

See [the latest lighting refinement evidence](../research/lighting-refinement-2026-09-23.md) and [the initial comprehensive lighting evidence](../research/comprehensive-lighting-2026-09-23.md) for captures, costs and limitations. The older solver below remains an engineering reference; its feature flags and grid/probe settings are not the default runtime contract.

## Compilation and relighting

The compiler builds a static BVH without silently dropping occluders (200,000-triangle cap). It places up to 192 paired surface-adjacent samples within 48 metres of a camera anchor quantized to 16 metres. Spatial distribution and a camera-distance priority determine placement; all resident source geometry participates in ray visibility. Each sample uses 64–256 sphere directions and up to two diffuse continuations. Small sample populations spend unused tracing capacity on more directions; the 192-sample world retains 64. Thin/transmitting, animated, windy and water geometry do not become opaque static bounce blockers.

Nine output SH coefficients carry transfer from nine direct-sky coefficients, nine bounced-sky coefficients, eight local lights and emitted radiance. Physical sky, sun/moon, cloud attenuation and point-light color/intensity relight this small field on the GPU. No geometry tracing runs per pixel. Sun/moon bounce is band-limited and approximate; direct illumination retains the shadow system. Point positions/ranges and static material/geometry edits rebuild the field. Moving-light and camera-anchor rebuilds coalesce while retaining still-valid sky/emission data; moved point-light coefficients are rejected until replaced. Continually moving emitters are not dynamic multi-bounce GI.

Dense static receivers store two visibility-tested sample IDs per side plus an encoded distance weight in otherwise unused rigid vertex lanes. Coarse rigid carriers use indexed subdivision with a half-metre target and at most twelve divisions, preserving their surface and interpolated color/material coordinates. Their lanes reference bounded four-sample mixture records; compact support reduces nearest-sample boundary discontinuities. Mixture records occupy the existing field storage binding. Exact finite visibility queries reject samples behind walls. Allocation is capped at 65,536 vertices overall and 12,288 refined vertices per surface. The compiler retains source identities/ranges so derived geometry is stripped before subsequent compilation. Draw ranges, source meshes and sky attributes are restored/composed explicitly.

Diffuse SH resolves per vertex. Partially admitted interpolation normalizes its radiance before blending with the fallback, avoiding double-darkened coverage boundaries. Alternate realizations take uniform bindings from exterior space above transformed bounds rather than an arbitrary, potentially buried source vertex. Local reflections use roughness-filtered SH. Certified flat, plain rigid surfaces interpolate low-frequency local reflections on either side; rough surfaces also interpolate their sky response. Glossy sky reflection and the full PBR response remain per pixel. Specialized pipelines remove the unused per-pixel local gather, and certified pair-only meshes eliminate mixture decoding/extra sample loops. Coated, textured, uncertain-normal, moving and alternate surfaces retain general evaluation. This is a controlled low-frequency approximation, not an exact mirror representation.

Moving receivers select up to four nearby samples with exact static segment tests and normalized smooth weights in object uniforms. Their selection is cached by absolute position; at most 16 queries and a 1.5 ms scheduling target are admitted per extraction, with remaining work spread over subsequent frames. A receiver awaiting admission uses the existing environment fallback. A receiver center does not certify direct-sun occlusion.

Closed convex opaque components can certify entire static triangles against celestial direct light. Exact edge welding declines cracks; non-convex or complex components decline the certificate. All triangle corners must map to the same certified component, and shared vertices retain a certificate only if all incident triangles qualify. This reduces shadow-map seam leaks without interpreting a dark sky sample as proof. Other receivers keep ordinary shadow-map visibility.

Both caches compile cooperatively with a 2 ms slice target and a 180 ms initial appearance transition. Compatible region replacements retain full lighting rather than fading it out again. Camera anchors have 12-metre per-axis hysteresis around their quantized centers. A four-region LRU retains products within a conservative 32 MiB accounting budget, except that the active product alone may exceed it. Compatible builds reuse geometry, enclosure analysis and identical samples; region hits restore derived mesh identities. Geometry/material edits clear incompatible regions. This is in-memory reuse, not cooked or disk-backed streaming. This is not a hard deadline; measured spikes are reported. Builds are cancelled on incompatible edits/disposal. Unique cache identities prevent unrelated scenes reusing GPU payloads. Absolute geometry survives origin rebasing; radiometric edits reuse the transport. CPU BVH/derived meshes and GPU fields are included in resource accounting. Initial compilation still takes seconds in the larger world, so cooked preparation and streaming reuse are remaining work.

## Persistent products and preparation ahead of travel

Automatic radiance products now have a versioned content key and a browser IndexedDB acceleration cache. The key hashes immutable geometry/receiver buffers, semantic IDs, absolute transforms, material transport, camera region and emitter positions/ranges. Sky and light color/intensity remain free relighting inputs. Compact Float64 query geometry preserves absolute-coordinate visibility; typed transport and receiver streams restore with fresh GPU identities and current source meshes. Invalid versions, malformed records, unavailable storage and quota failures fall back to compilation. The persistent store has conservative 128 MiB / 32-entry accounting and a 64 MiB entry cap. It is disposable cache data, not a release world package.

Observed camera translation schedules one neighboring region, retaining current lighting during preparation. Crossing promotes an existing job or consumes its finished product. Geometry/emitter changes cancel incompatible work. Incoming moving-receiver bindings are prepared before publication, keyed by captured absolute position; subsequent motion uses ordinary live admission. Background task scheduling is feature-detected with a timer fallback. Products above 16 MiB do not start speculative neighbors. Actual geometry streaming still invalidates the full product; sky persistence and local dependency invalidation remain unfinished. See the [active goal and retained evidence](../research/lighting-goal-2026-09-23.md).

## Sky fallback and direct shadows

The sky cache traces 24 cosine directions per side over the entire resident geometry. The former 9–12 metre fade was removed because large sealed spaces could receive exterior sky. Accordingly every admitted resident occluder participates in invalidation until directional dependency reuse is certified. This sacrifices the previous distant-streaming reuse optimization for enclosure correctness. The front stores bent direction times exposure; the back stores scalar exposure. A tangent inset and normal bias keep joined corners inside their receiver support.

All eight supported local lights can cast cached shadows at every quality level. The fixed eight-light texel budget is distributed over two, four or eight slots. One or two active lights receive twice the base linear resolution, three or four receive a bounded intermediate resolution, and five through eight use the base. Reallocation destroys the old texture, updates resource accounting/bindings and invalidates depth products. All active lights retain coverage; sharper maps cost more when refreshed. Each face rejects nonintersecting casters and has its own cache key. A per-frame caster revision table avoids rebuilding long geometry signatures for every light and face. Camera motion and color/intensity edits preserve cached maps. Source movement updates the relevant faces. Small static scenes conservatively fit the directional shadow volume to every caster in discrete bands, reusing the texture budget for finer detail; uncertain displaced/animated bounds and larger populations retain the existing minimum coverage. Directional bias is reduced; full directional cascades and general contact-hardening remain unimplemented.

## Present limits

This provides a common foundation across environments, not a claim that every authored scene has reached AAA final quality. Low-order/sample-limited transport can miss small openings and tiny emitters, and coarse interpolation can still flatten lighting. Dynamic objects receive cached indirect light but do not cast dynamic indirect shadows. Water/thin/transmitting geometry retain their separate transport models. Sharp local mirrors/parallax, arbitrary dynamic emissive bounce, indoor fog/atmospheric occlusion, automatic exposure adaptation and robust multi-region cooked streaming remain open work. Budget overflow has a visible diagnostic/fallback; it does not silently certify complete lighting.

## Historical diagnostic solver

The remaining sections describe the older explicit diagnostic field, not the automatic radiance cache above.

## Current static solver

The scene indirect-lighting path traces actual static triangles and compiles one to three diffuse surface bounces, with two by default in `IndirectLightingCache`. The low-level `compileIndirectProbe` retains a one-bounce default for the independent reference comparison. Transport is static: this is not general dynamic GI, ray-traced mirrors, or material transmission. The physical atmosphere is the default source. Nine incident-sky SH coefficients plus a visibility-tested directional sun relight both diffuse irradiance and local reflection context on the GPU. Explicit `lighting` options retain constant-source transport for numerical reference tests.

`IndirectLightingCache.update(scene, options)` builds the query hierarchy and irradiance field cooperatively. `waitReady()` supports deliberate review captures; interactive rendering uses completed probes as they become available. The host retains explicit `indirectLighting` settings, progress, source reports and diagnostic views for reference/lookdev tools. Product entry points do not configure this experimental world solver.

## Query and source contracts

`compiler/indirect-query.ts` extracts complete material draw ranges into world-space triangles, constructs a binary BVH, and intersects rays against the actual transformed triangles. Negative and nonuniform scale work through transformed geometric edges. Absolute origin offsets are added in JavaScript number precision, after the local matrix transform, rather than rounding a large translation into a Float32 matrix. Triangle extraction and BVH partitioning yield every 1,024 primitive visits.

The compiler refuses the whole query when its triangle budget is exceeded; it never silently omits arbitrary static occluders. The default cap is 100,000 triangles. Articulated rigid parts marked dynamic, skinned, deformed, windy, thin-coverage, foliage, glass, and water surfaces are explicitly excluded and listed in the report. Their absence is a limitation, not a proof that the remaining scene is complete. Surface reflectance currently uses representative triangle vertex color times base material color and nonmetallic weight, clamped below one. Procedural coatings, normal maps, emissive materials, point lights, and measured BSDFs are outside this source contract.

Sun radiance parameters describe normal-incidence irradiance; constant sky describes incoming radiance. Every diffuse hit contributes visibility-tested direct sun. Cosine-weighted paths either reach sky or continue to the configured bounce limit. Each continuation multiplies RGB material throughput, which is also stored in the relighting operator. Work grows linearly with the bounce budget, without branching into a new sample tree at each hit. Changing intensity or sky color does not retrace these paths.

## Probe transport cache

Each probe stores nine real spherical-harmonic coefficients of irradiance divided by pi and six directional first/second distance moments. Eight surrounding probes share one visibility-weighted gather for diffuse light, base reflection and clearcoat. The moment estimate follows the general approach of [Majercik et al.](https://jcgt.org/published/0008/02/01/), with only six directional lobes here. Moment filtering alone previously leaked through closed-box corners; it is always guarded by geometry visibility.

The compiler first bounds every receiver-to-corner segment in an interpolation cell, including displaced probes and normal-dependent numerical offsets. A cell stores at most 48 triangle candidates; larger cells retain the hierarchy. Conservative triangle shadow half-spaces subdivide receiver space into clear/blocked probe masks and short lists of unresolved triangle/corner pairs. No sampled clear ray becomes a visibility proof. Trees have at most two subdivisions, 32 candidates per leaf, 512 candidate triangles per cell, and a roughly 2 MiB shared extra-payload cap; overflow falls back to the hierarchy. The packed hierarchy has two vec4s per node, with an escape index and packed triangle range. Threaded traversal eliminates the old 32-entry per-pixel stack while preserving its right-first order and 128-node limit. Exhaustion returns blocked, which can over-darken. FP32 tests and fixed offsets remain numerical approximations.

`surfaceVisibility: true` additionally restricts leaf proofs to clipped static receiver triangles. Only the exact immutable mesh buffers, index range and absolute pose can use that product after renderer realization. Skinned, windy, displaced, dynamic, changed or alternate-representation receivers use the general path. This experimental product is **off by default**: repeated Winter captures did not show a consistent frame-time benefit sufficient to justify its build cost. The renderer clears stale eligibility after source removal. In-place mesh-buffer mutation remains outside the immutable-source contract.

Bounded probe relocation is on by default and can be disabled with `relocate: false`. Six axis queries detect a nearby back face and move the probe just beyond it, limited to 48% of the smallest grid spacing. A clearance check rejects questionable moves. This is a placement heuristic, not proof of open space; actual receiver visibility still guards the result. It does not solve volume boundaries or every concave/closed solid.

Local reflections store nine unconvolved SH vectors: RGB contains incoming radiance from geometry and W contains directional sky visibility. Roughness-dependent GGX convolution factors occupy the previously unused Z/W channels of the existing integration LUT. The result combines filtered geometry radiance with visibility-weighted physical-sky reflection. This removes the sealed-room sky-reflection failure, but low-order SH cannot resolve mirrors, parallax or small openings reliably. Glass uses the local context for its existing transmission approximation; it still does not show general objects through glass.

Static contact segments complement the sun shadow map. A dark enclosure estimate only chooses a longer query distance; the actual compiled triangle query determines blocking. These tests use admitted static geometry, while the shadow map continues to cover moving casters. They do not add directional cascades, dynamic GI occluders, or general contact-hardening shadows. Beauty aerial perspective is still independent of enclosed-space visibility.

Interpolation coordinates move outward along the receiver normal by 2% of the smallest probe spacing, while geometric visibility starts from the actual receiver with its small numerical offset. This avoids a vanishing-weight discontinuity when a receiver lies exactly on a probe plane and facing weights eliminate the coplanar probes. A hardware comparison exposed that discontinuity as a 0.165161 CPU/GPU discrepancy on 22 box pixels. The bias is an interpolation heuristic, not a claim of unbiased transport. Focused tests perturb that receiver by ±0.1 micrometers and require stable illumination.

The cache holds one field with at most 2,048 probes. Each probe uses 60 diffuse/moment floats, including relocation in previously unused lanes 38, 39 and 42; local reflections add 36 floats. Physical relighting adds 360 transfer floats for diffuse and another 360 for reflections. At the limit the combined probe/transfer payload is 6.375 MiB, plus a 64-byte header, packed BVH (32 bytes/node, 48 bytes/triangle) and visibility cells. Temporary compiler objects and immutable receiver references are additional CPU memory. `reflections: false` omits the local-reflection product.

Static visibility uploads once per field. Progressive revisions upload the header, diffuse/reflection coefficients and both transfer products; relighting runs only when its inputs change. CPU coefficients are the initial seed; the GPU is authoritative after physical relighting. Construction yields cooperatively with a shared target of 2 ms. This is not a deadline: JavaScript work and collection can exceed it, and `maxSliceMs` records the largest observed slice. `report.rays` counts primary probe rays, not secondary sun/continuation work. Each first hit can add one sun query and `skySamples` paths with up to `bounces` continuation steps.

Geometry identity, transforms, source albedo, query membership, probe settings, and lighting determine invalidation. Excluded moving objects contribute only their identity and exclusion reason. Camera motion, time alone, and a change of render origin do not invalidate identical absolute static geometry. Physical-mode sky color, sun color, and intensity edits reuse transport; changing sun direction still rebuilds it. This is not suitable for continuously moving sunlight without a further directional-transfer extension. Sky transfer is low order; sun extinction and cloud attenuation are sampled at the probe rather than every secondary hit. An edit cancels the previous build and removes its field immediately. Refusal remains visible in `cache.report`; stale light is not kept as though it matched the new source. Renderer allocation failure emits a diagnostic and uses the environment fallback.

## Automatic surface-cache optimization

The transport solver automatically compiles an additional product for static,
two-triangle rectangular receivers with constant geometric normals. No cache
enable option is needed. `surfaceCache: false` is an uncached diagnostic control;
`surfaceCache: { ... }` overrides compiler budgets for engineering experiments.
It is separate from `surfaceVisibility`. Products with no admitted tiles are
discarded so they add no shader, relighting or retained buffer cost. This first
product caches the authored front side; back faces, changed mesh/normal buffers,
changed ranges or absolute poses, dynamic/displaced geometry, procedural normals,
material appearances and alternate realizations retain ordinary GI queries.

The compiler stores normalized probe blend weights at surface grid points. Whole
tiles must stay in one interpolation cell and have conservative visibility
classification to every potentially contributing probe. Triangle shadow cones,
with numerical margins, establish clear/blocked regions; ambiguous tiles fall
back. Broad-phase proof work is capped at 512 hierarchy visits and 256 triangle
candidates per probe/tile. Five additional interpolation samples reject rapid
weight changes, but this sampled quality gate is not a mathematical error bound.

After probe relighting, a compute pass resolves one diffuse vector and nine
reflection SH vectors per surface sample. A cached pixel interpolates four surface
samples. Diffuse retains the original nonnegative per-probe clamp; reflected SH
is blended before its angular clamp and introduces an additional measured
approximation. `surfaceCache: { radiance: false }` keeps the original angular
clamps and caches only weights for numerical/control experiments. The physical
sky, material BRDF, direct shadows and unsupported receivers keep their existing
paths. Separate shader variants remove cache logic from uncached opaque draws.

Default spacing is one eighth of the smallest probe spacing, with 16,384 samples,
64 patches and sampled weight L1 tolerance 0.04. Options expose `spacing`,
`maxSamples`, `maxPatches`, `maxWeightError` and `radiance`. Hard allocation caps
are 65,536 samples and 256 patches. Budget overflow excludes the affected receiver
from the surface product, preserving its existing lighting. The renderer checks
payload addresses and includes the product in GPU memory admission. It occupies
the field buffer after probe/reflection coefficients and before visibility data;
chart coordinates are relative to the field origin. Receiver chart identity is
part of batching compatibility.

Source color/intensity changes reuse the surface product and resolve it again;
changing sun direction still rebuilds the original field. Uploads, including
disable/re-enable transitions, invalidate the relight result. GPU frame timing
now includes the indirect relight interval. The `indirect-cache` view marks
cached pixels green and fallback pixels magenta.

This prototype still builds and retains the old probe field and hierarchy. It is
not a replacement world-GI solver, a new lighting-quality tier, or a world rollout.
See the [surface-lighting report](../research/surface-lighting-prototype-2026-09-23.md)
for matched captures, memory/build costs and the rejected shared-shader regression.

## Curved receiver visibility

After the rectangular product, the solver automatically attempts a bounded triangle visibility product. It preserves indexed geometry and assigns each provoking vertex a certificate covering every triangle that shares it. Conservative shadow half-spaces include a 60-degree normal cone; the shader checks a slightly narrower cone and the matching probe cell before consuming clear/blocked masks or short exact blocker lists. Other pixels retain ordinary queries. This supports procedural shading normals without freezing their lighting response, but does not cache their final radiance.

Immutable positions, indices, normals, colors, material coordinates, picking identities, draw range and absolute pose govern eligibility. Copied or retained realized scenes discard stale compiler data without reverting user edits. Dynamic, displaced, thin/transmitting and alternate realizations retain general queries. Proof streams occupy unused rigid skin-weight attributes; separate opaque shader variants isolate their cost. CPU source arrays and index order remain unchanged.

The compiler caps receiver triangles at 65,536, groups at 32 triangles, broad-phase work at 512 nodes/256 candidates, final lists at 48 blockers, appended regions at 4 MiB and vertex proofs at 4 MiB. A 16,384-entry temporary FIFO reuses shadow cones. Scheduling estimates skip already-cheap regions; their predictions are not guarantees of GPU savings. Empty products are discarded. The existing `surfaceCache: false` reference control disables both optimizations.

This remains part of the bounded lookdev solver. Winter's additional build took 5.77 seconds in the final capture, and its whole-frame performance/coverage gates still reject a world rollout. See [the measured results and rejected approaches](../research/triangle-lighting-2026-09-23.md).

## Reference and evidence

`compiler/indirect-reference.ts` is a slower, independently sampled cosine-weighted reference over the same query mesh. It exports linear RGB and portable float maps. Sharing the query mesh means this is a transport/interpolation comparison, not independent evidence that triangle extraction matches an authored field.

`bun tools/gi-lookdev.ts` captures an open colored box and an alpine rock/ground patch, with cached lighting disabled and enabled. It also renders the isolated indirect-lighting diagnostic and compares scene-linear GPU values against a 256-sample reference with 16 secondary sky samples. The fixture writes unclipped reference/cache PFM images, display PNGs, RMS/mean/maximum error, CPU/GPU interpolation disagreement, cache dimensions and bytes, build duration, maximum CPU slice, frame completeness, and observed GPU timing. It fails when CPU/GPU interpolation or a rendered nonzero-origin rebase differs by more than 0.002 linear radiance. The CPU rebase regression also starts at an absolute origin of ten million meters and verifies the packed BVH rejects the same wall-crossing segment after rebasing. A closed-box capture fails when residual light exceeds 0.0001 linear radiance against the zero-light expectation; these targeted behavioral gates are not general error certificates. These reported errors are observations, not certificates or AAA art approval.

The same fixture times guarded and unguarded interpolation with identical probe coefficients, camera, geometry, and lighting at 640×480 in the isolated indirect pass. It compares compiled visibility, full BVH visibility, and an unguarded diagnostic control in ABCCBA order, two unmeasured settling frames per group, and sixteen scene-pass GPU timestamp samples per condition. Uploads and field builds are outside those measurements. It also checks rendered agreement between compiled and full-BVH visibility. The reported median differences measure the saved and residual numerical segment-query cost on these scenes; it is not a general rendering benchmark or a justification to silently disable occlusion.

Focused CPU tests cover exact constant-sky diffuse response, blocked external lighting, colored wall bounce, affine/rebased triangle queries, immutable source reuse, source edits, exclusions, cancellation, memory refusal, and GPU buffer packing. Hardware validation and image inspection remain separate gates. The initial open-box capture measured RMS 0.01357 against mean reference radiance 0.06507; the alpine patch measured RMS 0.004966 against mean 0.0586. These are pre-segment-visibility observations and must not be presented as final acceptance. The corrected hardware capture is recorded separately after review.


## Runtime integration boundary

The player URL flag and Studio bounce-lighting button have been removed. The host's optional transport configuration is retained for engineering fixtures, not offered as a product-quality choice. `cameraVolume` and explicit `bounds` remain useful diagnostic controls. Winter applies any diagnostic field after assembling the actual render scene. Decorative lantern emitter proxies are explicitly nonoccluding; their posts and roofs retain shadows.

Budget refusal or incomplete construction uses the environment fallback. Animated geometry, foliage, water and glass remain excluded from static transport and listed in the report. Point lights cast direct shadows but do not contribute bounced light to this field. These are open engineering gaps in the unified target, not features users should be asked to enable.

## Enclosure and material regression gates

`bun tools/lighting-audit.ts` freezes source and captures the same sealed metal room, reflection/direct ablations and open room. It requires the sealed central face to retain less than 1% of the unoccluded fallback brightness, no direct-light contribution above 0.0001 in any scene-linear channel, and a lit open-room control. Residual aerial perspective is disclosed, not counted as transported light.

`bun tools/lighting-water-check.ts` renders production spectrum water under an out-of-range point light, an admitted light and a shadow caster. The out-of-range frame must match the unlit control; the admitted and shadowed cases must visibly differ. `bun tools/surface-appearance-check.ts` retains the complete family/coating checks. These complement the diffuse reference comparison and do not establish AAA visual approval.
