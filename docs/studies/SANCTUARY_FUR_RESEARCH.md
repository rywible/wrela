# Sanctuary short-coat experiment

Current decision: the actual 0306 edge-envelope and material-combination captures still read as spots over clay. The localized surface, sparse strands and raised-patch family remain **HOLD**; keep `furStudy=0` and `coatStudy=0`, preserving the liked base Sunhare. The bounded next design is [one continuous body-volume experiment](../../Tools/Experiments/SanctuaryFur/CONTINUOUS_VOLUME_DECISION.md), using connected shell/fin coverage from one authored density source. It is design only: no shell implementation, renderer interface or default promotion is authorized by the report. The exact A2C/optical-depth comparison, restricted chart assumption, source caps, native acceptance and already-over-budget populated performance are explicit. Earlier paragraphs below are chronological checkpoints, not current approval.

0120 compiler qualification rejected the coverage revision's fixed six-ring reduction above its1mm deviation limit. All four coat tests stopped at that guard; root reports the remaining124 tests passed. A narrow reducer repair now selects the deterministic minimum-worst-error six-ring partition from the actual25 rings, preserving source guides,352 strands, all budgets and the1mm rejection. It reports exact error/strand/rings on failure. This necessary compilation repair has not been tested or rendered by this author and is not a new appearance iteration. Updated source hashes are in `Tools/Experiments/SanctuaryFur/groom-source-checkpoint.json`.

The current review checklist is [352-strand acceptance](../../Tools/Experiments/SanctuaryFur/GROOM_352_ACCEPTANCE.md). It distinguishes actual0017 side-hop/wet/quarter evidence, source attachment/contact tolerances, visible fur identity and live cost. The [semantic material proposal](../../Tools/Experiments/SanctuaryFur/MATERIAL_INTERFACE_PROPOSAL.md) describes only existing coat consumers and explicitly unsupported membrane inputs; no new material foundation was implemented.

## What is already implemented

`Games/Sanctuary/Authoring/Materials.metal` kind9 evaluates bind-space filtered noise at roughly80/m plus9/m clumps, modulates color slightly, and supplies0.15/0.30mm relief with roughness0.9. `Engine/FieldEngine/Resources/Surface.metal` adds an ambient-only grazing term `base*ambient*.12*(1-NoV)^3*(1-wet)`. Generic wetness halves roughness and darkens color22%. Material overrides may supersede the recipe roughness. The mesh silhouette never changes. Kind13 already supports explicit groom geometry, coverage and guide-aligned anisotropic GGX, so a later geometry experiment need not invent another renderer.

The current small-ellipsoid normal cutoff is a separate confirmed defect. It must be repaired and matched reference captures retaken before attributing clay/plastic appearance to a missing coat. Adding bump or sheen cannot repair wrong base normals.

## Primary sources and the decisions they resolve

- [Lengyel, Praun, Finkelstein and Hoppe, Real-Time Fur over Arbitrary Surfaces,2001](https://www.hhoppe.com/fur.pdf): shells sample a fur volume; fins repair the weak grazing silhouette. This supplies a real silhouette solution, but repeated transparent layers and fin management are material costs. Do not infer present M4 performance from an older desktop demonstration. Defer the shell/fin implementation here.
- [NVIDIA, Fur Shells and Fins](https://developer.download.nvidia.com/SDK/10/direct3d/Source/Fur/doc/FurShellsAndFins.pdf): explains the close-strand aliasing problem and shell transparency near silhouettes. Sparse cards/strips can carry a few deliberate guard-hair groups but cannot be assumed temporally stable merely because triangle counts are small. No full-body card or shell stack in this first experiment.
- [Khronos KHR_materials_sheen specification](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_sheen/README.md), based on [Conty/Kulla's2017 Imageworks presentation](https://fpsunflower.github.io/ckulla/data/s2017_pbs_imageworks_slides_v2.pdf): a Charlie sheen distribution with albedo scaling provides a light-dependent fibre-layer response rather than an unrestricted added rim. The experiment uses Charlie with the inexpensive Neubelt visibility approximation and a numerically generated directional-energy table. Fixed roughness0.70 keeps the first table/interface small.
- [Zeltner, Burley and Chiang, Practical Multiple-Scattering Sheen,2022](https://www.disneyanimation.com/publications/practical-volumetric-sheen/): describes a more capable volumetric fibre-surface BRDF. It is useful context for a future reusable material model; adopting the full model/LTC resources now would expand this bounded question unnecessarily.

These methods address different scales. Sheen and filtered nap can distinguish a dense short coat from clay on the interior surface; they cannot make a smooth outline fuzzy. If corrected-normal Sunhare still needs silhouette softness at normal interaction distance, the next bounded experiment should use the existing groom representation for sparse cheek/ear-edge/haunch tufts, not increase bump until the anatomy looks noisy.

## Executable source and result

Authoritative files are `Tools/Experiments/SanctuaryFur/experiment.py` and `ShortCoatCandidate.metal`. No production code imports them. The dependency-free script generates the small energy table, metrics, CSV and labeled numerical height swatches:

```sh
python3 Tools/Experiments/SanctuaryFur/experiment.py \
  --out .build/sanctuary-native-20260912/fur-surface-experiment
```

Seed37011; six fixed three-dimensional Fourier modes in bind metres. Two broad groups and four elongated fine groups produce at most0.34mm signed relief; they are nap clusters, not six literal hairs. Gaussian pixel filtering plus a conservative0.30–0.50cycles/pixel cutoff removes unresolved waves. The reference is deterministic and follows articulation through bind coordinates. No world-position or time noise is used. It adds no triangles, alpha passes or texture residency. The generated33-float table is132bytes; coefficients are120bytes. The Metal candidate is source-ready for integration review, **not compiled or GPU-validated**.

The single run took0.532s. Fixed-roughness sheen layered over a white Lambert reference reached maximum sampled directional reflectance1.000046, within the declared0.0005 quadrature tolerance; this is not an all-angle mathematical energy proof. Above-Nyquist retained amplitude was zero at the tested1080p/FOV1.05 footprints for0.5,1,2,4,8m. At4m, quarter-pixel translation changed height by15.76µm RMS versus48.11µm unfiltered; at8m,18.92µm versus85.18µm. This measures a signal, not rendered shimmer. Different swatch distances cover different physical extents, so their RMS values are not a same-patch appearance comparison.

Raw artifacts: `.build/sanctuary-native-20260912/fur-surface-experiment/{metrics.json,filter.csv,height-swatches.svg,ShortCoatLUT.generated.metal}`. The script records its source hash. The SVG is explicitly a magnified numerical height chart, **not a Sanctuary render**.

Wetness is an authored approximation: flatten relief70%, reduce sheen80%, retain source albedo darkening, and subtract0.12 from the resolved coat roughness without applying generic half-roughness again. The standalone recipe is0.90→0.78; actual Sunhare parts override the dry roughness to0.86, so the integrated candidate is0.86→0.74 when fully wet. The original part roughness remains unchanged for the matched A/B. Neither wet clumping nor water transport through fibres is simulated. Six Fourier modes can still look like repeated grain; native views must reject visible striping or a velvet/plastic substitute.

## Performance gate grounded in the existing run

Latest actual0017 populated eye-level evidence measures GPU median/p95/max25.576/32.937/37.668ms, presented interval p95/max33.333/33.333ms, RSS max788.563MiB and thermal state fair. This exceeds the1080p60 budget. Root's [profile analysis](../../.build/sanctuary-native-20260912/groom-parity-0017-populated-profile-analysis.md) preserves full workload/source/device evidence. The historical d15f profile measured13.824/15.199/16.021ms GPU, but its downward camera pitch−0.64 differs from0017's eye-level−0.08;0017 submits6.46× as many visible triangles. Different source/shaders, climate age and thermal state prevent treating these as a controlled groom A/B or attributing the slowdown to coat geometry. The old15.199ms p95 is not representative eye-level headroom. No current populated60Hz success or spare budget is established.

Provisional candidate allowance: **≤0.20ms incremental GPU p95**, ≤0.05ms incremental CPU update/encode p95, no extra draw/triangle count, ≤1MiB incremental resident material resources including implementation overhead. Six sine/filter evaluations plus one sheen distribution may still exceed this; instruction counts are not timing evidence. Warm both modes, then compare short matched live opening runs with clouds, wind, normal cache updates and the same actor count/time progression. Retain full p95/max and update hitches; do not subtract overlapping GPU intervals or hide cache frames. Record device,1080p,MSAA,model state and source/shader hashes. Repeat any noisy comparison before concluding; root owns execution.

## Next integration and acceptance

Root owns shared shader response and native/build gates; Astra owns the project recipe and semantic coat authoring; sky_cache owns Sunhare anatomy. The first subject must expose reference/candidate as an authored saved mode. Existing `projectSurface` has no mode or sheen output, so an explicit study-only material variant plus a small optional project response hook is preferable to hidden tint/roughness sentinels. Exact ownership/interface must be agreed before shared edits. Default Sunhare remains reference and no other species changes.

Coat zones: body, head away from muzzle/eyes, outer ear and upper limbs; exclude eyes, nose, muzzle, ear lining and sole bandY≤0.04m. Bind-space comb direction should follow body neck→haunch, head crown→down/back and ear root→tip. Do not add a per-frame mesh authoring path. Any mapping approximation must be visible in the report rather than described as a groom.

Root should inspect **front,quarter,side** at approximately0.7,1.5,3m under matched noon and neutral indoor light, then dry/wet. Repeat idle/blink, hop/contact and a slow approach/orbit. Check face identity and clean eyes, coat response to actual light, stable pattern during joint motion, no stripes, crawling highlights, halo or silhouette pop, and no wet plastic sheen. The silhouette should remain identical in this first A/B; call that limitation out. Preserve study/source fingerprints and actual PNGs without accepting a baseline. Only after isolated acceptance enable the same saved candidate source in the native opening, greet/hop near Sunhare and measure the matched incremental cost. If the surface looks soft but still visibly lacks necessary edge fibres, report that specific failure and use the second experiment allowance deliberately.

Reusable follow-up: coat nap, fibre sheen and membrane/leather are separate semantic recipes. Shared filtered bind-space fields and material-response hooks can support future bat wings, but a membrane needs its own broad roughness/crease/transmission reasoning; it must not inherit fur noise or sheen simply to reuse a texture. No bat-wing work is included in this experiment.

## Integration checkpoint

After the first experiment decision, root authorized the isolated project material patch. `Materials.metal` now contains opt-in kind15 and optional `PROJECT_COAT_RESPONSE` hooks; original kind9 behavior is retained. Root reports the shared wetness/lighting hook call sites are implemented in source. `Tools/Experiments/SanctuaryFur/INTEGRATION.md` records the exact interface/order. The Sunhare author was directly instructed to add the agreed saved `coatStudy` selector/cache key after that handshake. Its default is reference0; candidate1 changes only the agreed coat-part material classifications. The first candidate uses generic bindXYZ nap, not the proposed anatomical comb field. Source hashes are retained in the experiment output directory. Shared renderer/Cave verification, selector verification, native A/B images and incremental performance remain pending root's matching build; no adoption approval is implied.

The actual 2257 renderer review rejected the periodic nap. Inspected seven PNGs from `.build/sanctuary-native-20260912/normal-coat-2257-stage-review.json`: reference/candidate quarter dry, candidate quarter wet and left dry, the candidate hop apex, and Moonhart/CloudRay quarter. The dry candidate has coherent diagonal hatch across the forehead, cheek and flank; it reads as textile ribs. Wet relief suppression makes it smoother, without establishing convincing fur. The corrected mount eyes/noses are dark and focused; the ray still has a visibly raised eyelid, deferred from this material-only change. No baseline was accepted.

The quarter A/B capture metadata match: resolved camera distance2.2761991m, extent1.3264897m, time0 paused, softbox intensity16/fill0.18/size0.18/warmth0.25, identical scene look. Source digest is `cdfc2ca7806804f06a942317d74b2eb7a487d08c4d1462b3064334fdeaf4b15d`, shader digest `673f7613b313b6c5fa31fb5ff399a54583a8c72567cbd536055093a996a4e979`. Exact inspected paths and metadata are retained in `.build/sanctuary-native-20260912/fur-localized-nap-experiment/inspected-capture-evidence.json`. These are actual renderer observations; the following numerical images are separate evidence.

One bounded revision replaces only the opt-in nap with [LocalizedNap.metal](../../Tools/Experiments/SanctuaryFur/LocalizedNap.metal), mirrored in the project material and standalone candidate. Two seeded3D value fields use quintic interpolation, with approximate cross-cluster spacing6.9/4.2mm and length16.1/9.7mm. Six global wave directions are removed. The height bound decreases0.34→0.24mm and albedo modulation becomes±1.8%. A derivative-dependent whole-cell fade and Gaussian attenuation suppress unresolved clusters. The original periodic candidate is retained as `ShortCoatPeriodicReference.metal`. All code from `shortCoatE` onward—including sheen energy response, wet finish and every reference material branch—is byte-identical to2257; `unchanged-reference-response.json` records that check. No geometry, texture, draw, semantic parameter or default material changed.

Replay the numerical comparison with:

```sh
/Users/ryanwible/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3 Tools/Experiments/SanctuaryFur/localized_nap.py --out .build/sanctuary-native-20260912/fur-localized-nap-experiment
```

The single seeded NumPy2.3.5 run took0.635s. On matched0.5m source patches, the eight strongest windowed Fourier bins held39.98%→1.23% of height energy in the head-front plane and39.68%→2.53% in the flank plane. Raw RMS relief changed108→78µm and108→72µm respectively. Across2048 sampled cell faces, the maximum value jump was1.98e-14 and first-derivative jump3.52e-9; quintic interpolation supplies continuous derivatives at those boundaries. At2m in the head-front plane, quarter-pixel height change fell18.89→5.60µm. Both revised layers fade completely at the tested4/8m footprints, leaving the retained sheen response. Full metrics, source hashes and labelled rawPGM maps are in the output directory; these maps show scalar height, not lighting or the Wrela renderer.

The stochastic field is not strictly band-limited: the sampled2m head-front above-Nyquist energy fraction is5.70e-5, while the periodic reference has5.24e-9 including window leakage. This small residual is retained rather than reported as zero. Filtering is an approximation, not exact pixel integration. Native motion remains necessary to reject crawling detail, pebbled skin, visible cell orientation or an excessively smooth return to clay. Shader work changes from six sine/exp waves to sixteen integer hashes, interpolation and two exponentials; no timing equivalence is claimed. The original incremental GPU p95≤0.20ms and memory/cost gates remain in force, with actual native cost still unmeasured.

The Sunhare anatomy owner agreed to a future bounded bind-space groom interface without changing ABI or geometry: head weight `smoothstep(.58,.74,y)*(1-smoothstep(-.16,.02,z))`, ear weight `smoothstep(.95,1.035,y)` with priority, and approximate low-limb weight `1-smoothstep(.27,.43,y)`. Suggested comb directions are body(0,-.25,1), head(0,-.35,1), ears(normalized(sign(x)*.225,1,0)) and limbs(0,-1,0). These are source-coordinate approximations: limb/belly overlap and8…25degree ear spread are not exactly classified. Bind direction must never be projected against a posed/world-space normal. Exact part zones could later use project material subclasses through the existing integer interface. The current revision deliberately keeps generic localized nap so the next matched A/B isolates removal of repetition; no grooming-zone code is adopted yet.

Decision: revised surface source is ready for root's same-view dry/wet and hop review, followed by incremental native cost only if the appearance passes. Production fur remains HOLD; this surface-only technique cannot soften silhouettes or cast fibre shadows. No additional literature survey, shell stack or second renderer was introduced.

## Actual2337 verdict and smallest next test

All16 coat PNGs in `.build/sanctuary-native-20260912/local-nap-2337-stage-review.json` were inspected: matched front/quarter/left dry, quarter wet, six hop samples and exact study before/reload. The two unrelated grass captures were outside this review. The diagonal woven hatch is gone. The candidate now has faint, irregular relief on forehead/flank but the same uninterrupted smooth contour; at this framing it still reads as clay, not a short furry coat. Wet candidate avoids the strong plastic glints of the reference but remains smooth. The hop strip does not expose new conspicuous material bands or detached features; six stills cannot certify absence of live shimmer. The study reload looks the same. This is a specific appearance failure, not a reason to intensify noise until anatomical shape is obscured.

Root's actual live pair is retained at `.build/sanctuary-native-20260912/local-nap-2337-coat-profiles.json`: the same1920×1080 quarter idle softbox workload for8.4s per mode, with normal cloud/wind/cache updates. Reference GPU median/p95/max was5.519/6.134/9.562ms; localized5.709/6.287/9.461ms. The observed p95 difference is+0.153ms; median+0.190ms and max-0.101ms. RSS maximum changed303.563→303.641MiB. A single sequential short pair has timing noise and ordering sensitivity; the negative max difference is not evidence of an optimization. This is an isolated studio measurement, not the populated native game or thermal/traversal qualification. It satisfies the provisional+0.20ms p95 gate for this pair only. Appearance still fails, so there is no default promotion.

The smallest useful next production test is sparse, anatomically combed silhouette fibres on Sunhare, using `CraftGroom.compile` and existing kind13. The [Khronos sheen model](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_sheen/README.md) supplies aggregate surface reflection but does not create geometry or a hairy outline. [Lengyel et al.2001](https://hhoppe.com/fur.pdf) identify silhouette cues as important to perceiving fur and use fins to repair the grazing failure of shells. This supports testing the missing contour cue; it does not establish that our sparse geometry will succeed or transfer historical performance to the M4. Increasing directed bump alone would retain the observed smooth contour. A shell stack adds repeated surface coverage and a larger acceptance problem, so it is outside this bounded test.

Proposed authoring contract, pending root's explicit new-file grant:

- Astra owns NEW `Games/Sanctuary/Project/SanctuarySunhareCoatDesign.swift`, returning additional `PartDesign` values from resolved base parts: `static func parts(base: [LivingWorldPresentation.PartDesign], parameters: [String:Float]) throws -> [LivingWorldPresentation.PartDesign]`. Passing base parts avoids recursive recipe/cache calls and gives access to exact authored shapes, colors and parent IDs. No mount, collision, renderer or physics edits.
- The Sunhare owner retains `SanctuarySunhareDesign.swift` and adds an optional saved `furStudy`0/1 switch to its bounded recipe cache. Default0 produces identical geometry/materials. Test `furStudy=1, coatStudy=0` first so the missing silhouette cue is isolated against the original material; the existing localized mode remains separately inspectable. Source controls and saved IDs are preserved.
- Compile64 strands total, distributed asymmetrically over cheek/crown, haunch/back and outer-ear regions, as four batches parented to the actual head/body/ear joints. Nominal length12–22mm, radius0.7–1.0mm, and narrow irregular groups; these are stylized visible guard-hair groups, not millions of individual undercoat hairs. Roots begin1–2mm inside the actual parent surface. Eye/muzzle/nose/lining/contact bands stay excluded. Comb along the established anatomy, with no camera-facing guide placement.
- Reuse `CraftGroom`'s parallel-transported guide frames and deterministic clump/length variation; compile without an envelope. Its current fixed24segments×4sides gives12,288 triangles and8,000 vertices for64 strands. At the current64-byte vertex layout plus UInt32 indices, raw mesh storage is659,456bytes (0.629MiB), before small descriptors. Discard unneeded temporary guide skin weights: each batch follows one existing parent, with no new runtime secondary simulation. Target≤1MiB per recipe and≤4MiB for six cached variants, with no per-frame compilation. These are source-derived budgets, not measured runtime allocations.
- Kind13 already handles the groom coordinates and guide-aligned anisotropic GGX approximation. It is not a full hair scattering model. No new shader/ABI is needed. Base creature shadow remains; initial sparse strands need not cast separate shadows, a disclosed approximation. Wetness uses existing material response and does not simulate physical wet clumping.

Success requires the same native front/quarter/left and0.5–4m views to read as short fur without a spiky/quilled or pasted-tuft appearance; clear eyes/muzzle; modest broken silhouette rather than a uniform halo; roots stay seated through blink, hop and ear/head acting; no thin-line sparkle during live motion or camera travel. Numerical checks should sample roots against actual parent shapes, exclusion clearance, finite geometry/normals, triangle/storage caps and inherited animated transforms. Root then measures an identical short live A/B, retains all cache updates/p95/max and repeats only if the result is close to the budget/noise floor. The same provisional incremental GPU p95≤0.20ms, CPU update≤0.05ms and explicit cache budget apply as ceilings only; isolated surface timing does not supersede the latest0017 populated eye-level GPU p95 of32.937ms or authorize promotion into that over-budget scene. If sparse fibres do not establish identity, record that failure before considering any larger coat system.

At that review checkpoint the material and anatomy source were unchanged after2337. Root subsequently authorized the following bounded groom experiment; the surface-only result remains unpromoted.

## Opt-in64-strand source checkpoint

Root transferred Sunhare source ownership to Astra and authorized `SanctuarySunhareCoatDesign.swift`, the narrow `SanctuarySunhareDesign.swift` selector/cache adapter, and `ProjectTests/SunhareCoatGeometryTests.swift`. Existing `coatStudy` remains separate. New saved `furStudy` accepts0 or1, defaults0, and adds four coat batches only at1. The original `build(_:)` function through end-of-file is byte-identical to the immutable2337 source: SHA256 `fb2982dc853bf34f39e9b339698201204ccaa605eb24f4c7483d08e20bcdf42d`. This is source comparison evidence; the added tests must still establish compiled equality.

The actual source uses16 deterministic guides: six upper/rear body, six crown/rear head, and two per outer ear. Each guide compiles four strands through existing `CraftGroom`, totaling64. Cheeks were deliberately omitted to preserve the inspected face. Body/head guide roots are projected onto their resolved parent fields after semantic controls. Ear roots are selected on actual outer-ear mesh triangles, because the outer and lining descriptors share one whole-ear field. Root groups start2.2mm beneath those surfaces; authored lengths are14–22mm, radius0.7mm and group width1.2mm, with bounded seeded length/clump/curl variation. Body/head comb toward+Z; outer ears comb toward their tips. No camera-dependent placement or runtime strand simulation is introduced.

The compiler enforces the current12,288-triangle/8,000-vertex topology, finite vertices/normals and buried actual root rings. Raw additional mesh storage is659,456bytes with the current64-byte vertex layout; maximum1MiB per recipe and4MiB across at most six cached variants. A topology change throws until its root-index and budget contract is reviewed. Cached success and failure are both explicit `Result` values. Nonthrowing catalog/pose metadata returns four conservative descriptors without compiling. Root wired the generator's throwing compile path to `SanctuarySunhareDesign.validatedParts`; compilation errors propagate to the existing source rejection boundary. No `try?`, partial coat or invisible fallback is used. Cache eviction may compile again on an authored parameter change; steady posing must not compile. Cache byte accounting covers coat vertex/index buffers, not all object overhead or native GPU residency.

The four batches retain exact head/body/outer-ear parents and use existing opaque kind13 with groom coordinates. They initially cast no separate shadows; the original creature shadow remains. The shader is the existing anisotropic GGX approximation, not physical fibre multiple scattering. Wetness does not reclump strand geometry. These are sparse guard-hair groups, not full undercoat coverage; their visibility, possible quilled appearance and thin-line sparkle are the main native risks. No shader change or default material promotion accompanies this source.

Four new test cases exercise the registered compiler, exact omitted/off/on base vertex/index/normal/color/groom preservation and independent saved study selectors; actual compiled root-to-parent ray exits plus field/face/lining/sole exclusions; deterministic control-corner compilation and cache bounds/rejections; and every generated strand vertex following actual parent matrices across idle/hop while the base rig remains unchanged. They are source-ready and **not run by this author**. Root owns compilation and execution, then matched native front/quarter/left dry/wet, study replay, hop/ear motion and live incremental cost. Use `furStudy=1, coatStudy=0` first against both0 to isolate the missing silhouette cue. Retain the existing+0.20ms GPU p95 and0.05ms CPU update allowance, with normal clouds/wind/cache updates and populated-game follow-up. No fur identity, contact, performance or baseline approval is inferred from this checkpoint.

## Actual0017 groom failure and one coverage revision

Inspected all30 actual PNGs and metadata from `.build/sanctuary-native-20260912/groom-parity-0017-stage-held/.soundstage/captures`, indexed by `groom-parity-0017-stage-replay.json`. The four coat CPU tests passed in root's immutable0017 focused gate:0 failures in13.718s (`groom-parity-0017-focused-tests.log`,635–645). This establishes the tested root attachments, exclusions, deterministic base geometry and inherited matrices; it did not establish perceptible fur. Both controls remain default0.

Across front/quarter/side, wet/dry, blink/hop and exact study reload, candidate1 is nearly identical clay with a few tiny dark marks on crown, back and ear edges. The marks move with their parents; no new detached attachment is apparent. The confirmed blink at2.68s stays unobstructed. The silhouette has almost no furry breakup, and wet0/1 both retain the reference material's smooth plastic highlights because `coatStudy=0`. Appearance therefore fails even though geometry/contact tests pass. Source SHA/capture paths and all30 inspected PNG hashes are archived before editing at `.build/sanctuary-native-20260912/groom-0017-review/review.json`, alongside the exact0017 coat/design/test source copies.

The actual candidate reports90,952 source triangles, versus78,664 off: the expected12,288 groom triangles are present. Both quarter views retain2.2761991m camera and1.3264897m extent at1920×1080/FOV60.1605682°. At that camera scale, the original maximum1.4mm root diameter projects to0.573pixels before tapering/foreshortening, and most geometry narrows further. Six body and six head guide sites occupy mainly rearward surfaces; four ear sites add only a few tiny edge cues. This is a sampling/coverage failure with weakly exposed contour placement. The actual buried-length fraction of0017 was not dumped, so it is not claimed to be the main cause; passing buried-root tests says nothing about the rest of each strand. The source's maximum normal lift was only5.6–8.8mm. Twenty-four longitudinal segments per strand mostly subdivided geometry already narrower than a pixel.

One bounded revision reuses the same CraftGroom and kind13 pipeline. It distributes44 body,32 crown and six guides per outer ear by deterministic farthest-point spacing on eligible resolved surfaces; four fibres each total352. The coverage extends across upper flanks and the crown aboveY.855m, while explicitly excluding eyes/muzzle/nose, the front ear lining and low limbs. Curved groups bend along the surface comb instead of standing straight outward:18–24mm ears,20–26mm crown and26–32mm body;1.7mm/2.2mm maximum root diameters;4.2mm/8mm group widths. The wider entire root group is buried4mm/6.2mm, then curves outward and back toward the surface. All compiled vertices are checked against face/contact exclusions, and every root ring remains inside its parent. These are stylized short guard groups, not a dense physical undercoat.

To spend the geometry budget on coverage, the cached representation retains six of CraftGroom's25 longitudinal rings, preserving sampled positions, normals and groom coordinates. Every omitted vertex is compared with the corresponding coarse interval; deviation above1mm throws. At the matched camera1mm is~0.41pixels, although close-up views still need inspection. This yields14,080 triangles (+14.6% over0017),10,560 vertices and844,800 raw bytes across the same four batches. Maximum1MiB per recipe remains; cache capacity decreases to four recipes (≤3,379,200 raw coat bytes, capped at4MiB). Temporary compilation works in≤16-guide chunks. There is no new shader, alpha shell stack, asset cache format or per-frame authoring.

The revised compiler also rejects a coat when≤55% of retained vertices sit more than1mm above their parent field, and records that fraction plus maximum reduction deviation. This is a source-field exposure diagnostic, not camera visibility or physically measured fibre length. Updated tests print those actual compiled values and retain parent-mesh ray checks, control-corner cases and all parent-motion tests. Root must compile/run them; no updated test or renderer result is available from this author. The same30-view replay and cost gates apply, with expected triangle delta14,080. A denser set of still-dark marks, obvious quills, plastic wires, regular rows or sparkle remains a failure; no default promotion is justified until actual identity and cost pass.


0124 reducer follow-up: the optimal six-ring partition still failed on strand40 at1.0156704mm (rings0,3,10,13,19,24). Root authorized a modest triangle increase within the original memory cap. Current source retains seven adaptive rings, preserving original guide controls/root/tip/placements and352 strands. Exact topology is12,320 vertices /16,896 triangles /1,013,760 raw bytes, below1MiB per recipe and4MiB for four cached variants. Only topology assertions changed; the1mm reduction, root/exclusion/contact/parent and control-corner gates remain. A transient intermediate-guide edit was fully reverted before this final boundary. Root must rerun the four affected tests and review actual native appearance/cost; this is not a passed qualification.


0126 actual compiler receipt corrects the prior hand byte estimate: **991,232 raw bytes**, exposure fraction0.71428573 and maximum reduction deviation0.6238807mm. Three tests passed (default roots/exclusions, animated parent inheritance, registered/base parity); the semantic-corner test rejected a compiled fibre entering the ear-lining margin. The source fix restricts six guide seats per ear to actual broad back-lamina triangles with centroid back distance>26mm and normal.z>0.8. All-vertex6mm lining rejection, root attachment,1mm reduction and cache limits remain unchanged. Errors now report parent/vertex/margin/bounds and the test reports exact parameter corner. The new seats await root tests/native images; default grooming remains off.


## Actual0130 complete native review

All30 actual PNGs from `ear-groom-0130-stage-replay.json` were inspected:12 dry/wet front/quarter/left bind views,12 side-hop comparisons, four idle/blink views and two study/reload views. Exact file/source/metadata hashes and gate decisions are retained in `.build/sanctuary-native-20260912/groom-0130-review/review.json`, with the three authored Swift/test sources archived before any next revision. Source digest `d4dcee44f7554bab4096f997820d6b927cddabdbc5a35a15003a5e37c7b15eca`; shader `2d3f3d0debd4e192e4df828a4f0720cd5fd86a4f4e3d00f249e4352689a3ed05`. All captures1920×1080,4×MSAA, same exposure1/shared look/resolved light. Within each fur0/1 pair framing matches; actual distance varies by view: quarter2.2761991m, front2.1923375m, left1.8288466m. Do not report all views as2.276m.

**Fur identity FAIL, dry and wet.** More groups are now visible, but as separated curled stubble across forehead/haunch and at the outline. Large smooth areas dominate. Wet broad specular highlights remain plastic, and short bright lines make the groom look wiry. Front eyes/nose/muzzle/ear lining remain clear, including the closed-blink pair. Sampled head/ear/body anatomy and motion show no new open attachment gap; existing shoulder/hip creases persist. Paw poses show no new apparent difference against reference; exact ground-contact clearance cannot be qualified from the soft shadow or empty `contacts` arrays. The hop apex is intentionally airborne. Study-before/reloaded images visually match. Still frames cannot certify live shimmer.

Root independently reached the same identity rejection and deferred incremental cost profiling; no baseline or default promotion. Next bounded question is continuous undercoat coverage with existing groom coverage, not more isolated tube groups. Source-space coverage is an approximation to unresolved fibres, not volumetric fur transport; its success still needs the same interaction framing, dry/wet and motion review.


## One bounded undercoat revision after0130

The repeated failure is sparse representation, not an insufficiently strong surface noise recipe. The next opt-in source replaces the352 standalone bristle tubes with352 overlapping conformal patches:220 body,112 head,10 on each actual outer-ear back. Each patch follows the authoritative parent field across its width and begins with a buried root edge. Body spans55×67mm, head35×43mm, ears13×23mm. Height remains millimetric; the previous centimetric curled groups are removed. This uses the existing project-owned mesh authoring and `GroomCoverage`/kind13 renderer path, without shader, ABI, exposure, roughness or secondary-simulation changes.

The primary-source distinction still applies: sheen affects aggregate surface response, while silhouette/coverage geometry carries unresolved fibres. Here the existing analytic pulse-footprint coverage supplies many unresolved fibre intervals across each surface patch (0.8mm period,0.94 root duty,0.18 tip variation). It is a polygonal coverage approximation, not a full shell-volume or hair multiple-scattering implementation. Overlapping short patches are intended to remove the wide bald spaces shown in0130; native images must reject scale/feather/roof-tile borders or a new plastic coating.

The same4semantic parents, exact base geometry and defaultoff source remain. Whole-patch face8mm, lining6mm and soleY>.31m exclusions, actual buried-root/rendered-parent checks and inherited posed-vertex equality remain. A denser25×9 source stencil validates the actual7×5 triangular interpolation to1mm. Projection/normal/accuracy errors throw; deliberately excluded unsafe candidates must be replaced from the bounded deterministic source candidate set until exact region counts are met, otherwise compilation fails. Same predicted topology12,320 vertices /16,896 triangles /991,232 raw bytes and1MiB/four-recipe caps; these are source-derived for the new patches until the next compiler receipt. No per-frame mesh work. Four affected CPU tests and the same30-image native replay are the next gates; no build/GPU was run by this author.

## Actual0212 compiled qualification

The current adaptive source passed all four production coat tests in32.455s;
the focused run passed21 tests with zero failures. Actual default retained-vertex
exposure is0.79492533, maximum sampled reduction error0.0009913478m, raw mesh
bytes484,448. All three owned source/test hashes exactly match immutable0212
and the checkpoint. Receipt, copied source and log SHA are retained in
`.build/sanctuary-native-20260912/undercoat-0212-qualification/compiled-receipt.json`.
This supersedes earlier uncompiled/estimated-count status below. Exact adaptive
vertex/triangle counts were not printed; do not infer them from raw bytes. Native
images, material identity and live cost remain unqualified; default remains0.


## Actual0212 native rejection

All30 matching PNGs inspected: rectangular raised tiles/basket pattern remains
over smooth skin, with stronger wet plastic highlights. Face/lining and sampled
parent motion remain readable, but fur identity fails. Default stays off; no cost
profile or baseline promotion. Hash archive:
`.build/sanctuary-native-20260912/undercoat-0212-review/review.json`.
Next diagnostic and exact scalar replay are in
`Tools/Experiments/SanctuaryFur/COVERAGE_DIAGNOSIS.md`; no geometry revision follows
from the CPU pass.


## Actual0237 calibrated coverage and next source

All23 actual calibration PNGs reviewed. Native unresolved density .25/.5/.75/.94
yields estimated flat coverage .256/.505/.744/.937 and lifted .265/.500/.738/.937.
These are qualified display inversions, not HDR/sample-mask measurements.
Coverage works at interaction distance; raised hard side boundaries remain a
representation defect. The separate4m front planar disappearance occurs even
opaque and is unqualified. Exact evidence, limits and one isolated per-vertex
transverse duty correction are in `Tools/Experiments/SanctuaryFur/COVERAGE_0237_DECISION.md`.
Current source changes only groom.z and actual edge/center test probes; unchanged
geometry/exclusions/parents and defaultoff. New compilation and same30native
images remain required; previous0212 metrics are not a new-source test pass.

## Actual0306 edge-envelope review: identity still rejected

All30 native PNGs from `bridge-review-0306-groom-replay.json` were inspected.
All14 reference/candidate pairs match camera, physical distance, resolved light,
rig, look, dimensions,4×MSAA, wetness, time, seed and source/shader digests.
Study-before/reloaded PNG pixels match exactly. Archive with all image/metadata
hashes, copied owned sources and prior compiler log:
`.build/sanctuary-native-20260912/groom-0306-review/review.json`.

The four production coat tests passed32.167s in0300, including the new actual
edge/center coverage assertions and all original root/exclusion/parent/default,
1mm and memory gates. Main/0300/0306 owned source SHA values match exactly.
Actual exposure .79492533, maximum sampled deviation .0009913478m and484,448raw
bytes are retained.0300 had unrelated failing player/animal fixtures; this is
only its qualified coat-suite evidence, reused under0306's explicit source delta.

**Fur identity remains FAIL dry and wet.** Side coverage attenuation softens the
square borders, but replaces tiles with separated raised spots and dash-shaped
highlights. Most broad body/head surfaces remain smooth, and wet highlights
remain plastic. The contour has sparse tiny marks rather than a coherent soft
coat. Front eyes, nose, muzzle, lining and the closed blink remain unobstructed.
No new detached anatomy or exposed root is visible in these sampled frames.
Coat marks follow the sampled parent poses. Paw shapes/poses show no apparent
candidate/reference difference, but exact ground clearance is not established
by soft shadows; the hop apex is airborne. Still frames do not qualify shimmer.
No baseline/default promotion or new cost profile follows this appearance failure.

No further density, lift, phase or patch-shape adjustment is justified by this
set. The calibration already establishes working coverage at interaction scale;
strong macro highlights still expose the chosen raised-patch representation.
The smallest remaining discriminating test, if continued, uses **existing**
`coatStudy=1` with `furStudy=1` against `coatStudy=0,furStudy=1`, quarter dry/wet,
with no new source. It would isolate the untouched smooth base material's role
before considering another implementation. Previous localized material alone
also failed identity, so combining them is an unqualified test, not a proposed
production cure. If both still read spotted clay, hold this patch family rather
than repeating small uniform changes. Default base Sunhare stays unchanged.

## Actual0306 combined material closure

All four actual `combo0306` PNGs were inspected: existing localized material
`coatStudy=0/1`, each with `furStudy=1`, quarter dry/wet. The combined material
retains separated pale/dark marks on smooth skin; wet highlights remain plastic.
It does not establish fur identity at the retained interaction framing. Face
and anatomy remain clear in these views. Receipt and image/metadata SHA values
are retained in `.build/sanctuary-native-20260912/groom-0306-review/combo-review.json`.

**HOLD this raised-patch representation family.** No more local density, lift,
patch-shape or material adjustments are justified. Default fur remains off.
Any future coat research must establish a distinct continuous representation
and its visible coverage before production changes; the successful coverage
calibration and CPU approximation guards do not certify fur identity. No new
cost measurement was requested after this repeated visual failure.


## One continuous-volume CPU gate

The authorized [CPU experiment](../../Tools/Experiments/SanctuaryFur/CONTINUOUS_VOLUME_CPU_RESULT.md) ran once in1.198s (seed37011). Its2,048-triangle chart fails1mm sampled accuracy at18.458mm because independent ring centers jump near the chest/neck. Eight/twelve unfiltered layers also fail grazing coverage convergence. The same-mask A2C arithmetic counterexample is not a Metal measurement. Exact source/metrics/mesh arrays remain in `.build/sanctuary-native-20260912/fur-continuous-volume-cpu/`. This is a distinct representation risk result, not native fur evidence. No production adoption or another density/material tweak follows; the liked base and both study defaults remain off.
