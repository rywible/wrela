# Current sky: comparison with AAA games and nature

23 September 2026. This audit follows the implementation and art iteration already in the workspace.

**Wrela has a substantial atmosphere renderer, but its cloudscape still looks constructed. The largest gap is the range and organization of cloud shapes, followed by upper-cloud structure, depth, and the balance of light inside the clouds.** More sampling alone will not close that gap.

The [comparison board](sky-current-audit-2026-09-23/index.html) contains **21 fresh production renders and 12 credited references**. It opens on Wrela versus a real growing cumulus cloud. Presets compare background banks, upper clouds, sunset, atmosphere, the alpine world, and High versus Balanced. Full-size originals and source links are available for every image.

## Evidence

I inspected the current atmosphere, cloud density, cloud lighting, reconstruction, display transform, environment authoring, and scene-lighting code. I captured a frozen copy of the current working source: eleven High sky views, eight Balanced views, and two Balanced alpine-world cameras, all at 1440 × 900 output. Cloud history was disabled for these stills. All captures completed on the Apple / Metal hardware adapter with no recorded browser errors. The nine audited source files still matched the snapshot after capture. The eight files identified by the previous art-iteration evidence also matched that iteration.

The sky studies use the production renderer over simple diagnostic terrain. The alpine captures use the current authored project and its existing cameras, with the same renderer bundle. See [capture evidence](sky-current-audit-2026-09-23/evidence.json) and [image provenance](sky-current-audit-2026-09-23/sources.json). The reference collection already existed in the workspace; I inspected its images, checked the recorded image hashes, and researched the original sources again. No production renderer or environment definition was changed for this audit.

These are visual comparisons, not matched weather, exposure, lens, or altitude tests. NASA's older photographs are useful for morphology, not calibrated color or sharpness. The MSFS images are community captures; their author reports live weather without atmosphere add-ons. The game references are selected exemplars, not representative performance or average-quality measurements. Fresh capture timings are deliberately excluded because these were visual-review runs.

## The reference targets

| Reference | What makes it a useful target |
| --- | --- |
| [Horizon Forbidden West: Burning Shores](https://blog.playstation.com/2023/03/29/pushing-the-envelope-achieving-next-level-clouds-in-horizon-forbidden-west-burning-shores/) — three official captures | Different large silhouettes, cavities and openings, luminous thin edges, and cloud masses that recede into atmospheric depth. Its close-flight reference is soft: sharpness everywhere is not the goal. |
| [Microsoft Flight Simulator 2024](https://forums.flightsimulator.com/t/seriously-beautiful-clouds-in-msfs2024/767128) — two JohnnyT5000 captures | An asymmetric hooked cloud, torn edges, and an evening with several cloud textures and warm/cool transitions. |
| [Red Dead Redemption 2](https://store.steampowered.com/app/1174180/Red_Dead_Redemption_2/) — two publisher-gallery captures | Golden light and moonlit fog connect the sky, air, surfaces, and reflections. The sky can be relatively simple while the complete atmosphere is convincing. |
| [NASA S'COOL cumulus](https://scool.larc.nasa.gov/GLOBE/cumulus.html) — three photographs | Kevin Larman's Colorado tower, Jeff Caplan's Rockies cumulus, and Doug Stoddard's layered tropical sky show distinct growth, irregular silhouette, and contrasting cloud families. |
| [NASA S'COOL sunset cirrus](https://scool.larc.nasa.gov/GLOBE/cirrus.html) — Ed Donovan | Broken, fine, interwoven high cloud with varied opacity. Straight contrails in this photo should not be copied as natural cirrus. |
| [Lake Michigan rays](https://apod.nasa.gov/apod/ap100811.html) — Kurt Voigts / NASA APOD | Long cloud shadows change the atmosphere around the cloud, not just the appearance of its body. |

## What the research implies

**Preserve the clear-air foundation.** Hillaire's [production atmosphere reference implementation](https://github.com/sebh/UnrealEngineSkyAtmosphere) includes both the scalable method and a path-traced comparison. Wrela already separates transmittance, multiple scattering, sky view, and aerial perspective. My recommendation is to validate that foundation against an independent reference for difficult lighting, rather than rebuild it on visual suspicion alone.

**Give clouds a richer large-scale structure.** Guerrilla's [Nubis³](https://www.guerrilla-games.com/read/nubis-cubed) describes a voxel-based modeling pipeline, simulation-derived shapes, compressed distance fields for skipping empty regions, accelerated lighting, and approximations for inner glow and dark edges. The useful lesson for Wrela is that the cloud model, its illumination, and its acceleration structure need to be designed together. The full cloud-flight pipeline is not required to improve our ground-based scene.

**Judge illumination as a connected system.** Epic's [volumetric cloud documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/volumetric-cloud-component-in-unreal-engine) separates approximate multiple scattering, ambient occlusion, direct shadows, and sky-light capture. These control different visual effects. Rockstar's [2019 atmosphere presentation abstract](https://advances.realtimerendering.com/s2019/index.htm) explicitly joins clouds, fog, volumetric effects, and ambient lighting with artistic control. My interpretation is that coherent sky/air/ground lighting matters more than adding another isolated effect.

**Use meteorology to decide which shapes belong together.** The [WMO description of cumulus congestus](https://cloudatlas.wmo.int/en/species-cumulus-congestus-cu-con.html) distinguishes active vertical growth from detached, disintegrating tops. That is a useful authoring distinction: developing tips, mature interiors, and evaporating edges should not all use the same detail distribution. This does not require a real-time weather simulation.

## What is already implemented

The original audit's missing features are largely addressed. The current code has Rayleigh/Mie scattering and ozone, approximate atmospheric multiple scattering, cloud self-shadowing, a shared light grid, cloud shadows in the air and on terrain, sky-derived diffuse light and reflections, temporal reconstruction, a curved low-cloud shell, and separate upper clouds. Authoring supports towers, banks, and wisps. Twilight no longer has the original near-white horizon and bright green ground beneath black clouds.

Current quality limits are also substantially higher than the original audit: **High allows 1,024 view samples at 1440 × 900; Balanced uses 768 × 512 clouds with 384 background / 512 formation samples.** These are ceilings, not samples necessarily taken on every ray. Low uses 64 samples at 320 × 240. See [quality policy](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-gpu.ts:25) and [formation overrides](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-gpu.ts:421).

## The remaining gaps, in priority order

### 1. Cloud families are still too repetitive

**Visible:** the main tower has a convincing broad silhouette, but the views away from it are dominated by similarly flattened banks. Small clouds often look like shorter pieces of the same larger shape. The central tower is comparatively dense and compact; the NASA and MSFS references have stronger asymmetry, gaps, detached growth, and partially dissolving regions. Flat cumulus bases are physically plausible. The problem is the repeated shape of the entire body, not flatness itself.

**Code evidence:** [formation geometry](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:196) uses a fixed six-lobe tower and four-lobe bank. The seed changes noise coordinates, but not the broad lobe arrangement. The [schema](/Users/ryanwible/projects/wrela/packages/model/src/environment-authoring.ts:18) exposes three kinds and up to four authored formations per key; it has no explicit growth direction, branching arrangement, maturity, or per-region erosion. Background weather still uses one combined density recipe. This is a capability limitation; four forms alone is not a proven bottleneck.

**First experiment:** keep the existing renderer and transport, but generate several bounded growth structures per family. Vary unequal updraft branches, detached tips, a lateral spreading region, and a dissipating margin. Compare silhouettes across seeds before adding detail. Compose a weather group with shared base conditions and deliberate gaps. Do not simply randomize every base height or increase the formation count.

### 2. The transition from solid body to air is too uniform

**Visible:** the nearby bank in the west view looks inflated and smooth; distant clouds look much more densely textured. Storm undersides repeat large rounded lobes. The references mix crisp growing tips with semi-transparent fraying edges and quiet interior regions. Our clouds can read like shaded solid objects instead of suspended droplets.

**Code evidence:** [density shaping](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:139) uses shared thresholds and clamped density; [authored detail](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:233) uses fixed frequency mixes; [local lighting](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:249) estimates nearby optical depth from a directional field difference. These are plausible contributors, not individually isolated causes. The artifacts remain in High full-redraw stills, so temporal accumulation is not required to produce them.

**Experiment:** freeze one formation and compare density alone, direct self-shadowing, diffuse fill, and the combined image. Vary erosion and opacity by growth region and life stage. Preserve coherent interiors while making selected boundaries porous. Judge front, side, and back light together; global sharpening is unlikely to help.

### 3. Upper clouds are visibly procedural, and layer variety is limited

**Visible:** east-facing wisps look like isolated brush strokes; the zenith and west views expose repeated curved ribbons inside the feathers. Natural cirrus has less uniform filament widths, broken overlapping fans, and translucent veils. MSFS's layered sunset also has a cloud texture between the thick low bodies and fine upper sky that our default composition lacks.

**Code evidence:** [upper-cloud generation](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:403) uses a single plane at 9.8 km, seeded patch envelopes, one ribbon construction, and fixed filament frequencies. It uses a planar intersection even though low clouds use a spherical shell. At the study's 7 m camera altitude, its 90–120 km distance fade removes upper clouds between approximately 6.25° and 4.68° above the horizon; below that they are absent. This is a geometric deduction from the code, not an isolated measurement of its visual impact. Authored low formations are constrained below 6.5 km, and there is no independently authorable middle-cloud field. The current high layer therefore supplies only a narrow range of appearances.

**Experiment:** improve the inexpensive sheet first: overlapping filament bundles with several scales, broken veils, and less regular internal spacing. Test a curved upper-layer intersection and continuous distant coverage. Add one independent shallow middle layer only after it improves a specific reference composition. Keep a ground-camera quality target; volumetric cirrus is not automatically necessary.

### 4. The cloudscape has less convincing distance and scale

**Visible:** the east and opposite views resolve into a narrow collection of separate bodies above a clean horizon strip. Nearby and distant shapes do not always establish a continuous progression of apparent scale and contrast. The alpine camera makes the large tower read better in context, but the distant world is too simple to establish the expanse seen in the references.

**Code evidence:** [low-cloud integration](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:463) supports shell entry out to 100 km but integrates at most 32 km after entry. [Air composition](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:541) groups cloud contributions into three distance intervals. These limits may affect overlap and far coverage; this audit does not prove either is the sole cause. The earlier flat-slab diagnosis is obsolete.

**Experiment:** author explicit near, middle, and far groups and compare their distance/opacity with a diagnostic depth view. Then test a cheap distant weather representation from the same authored field. Use both a flat horizon and the alpine scene to separate rendering limits from unfinished landscape composition.

### 5. Lighting has drama, but its internal range is still narrow

**Visible:** golden hour has good bright rims, but the bodies tend toward uniform brown-grey. Just after sunset, many clouds sit in a similar orange-red range. Blue hour now looks like twilight, although low-cloud interiors are mostly silhouettes. The Horizon and MSFS examples separate luminous gaps, thinner illuminated regions, cool shadow, and distant haze more clearly. Dark silhouettes are possible in real twilight; their presence alone is not a physical error.

**Code evidence:** [cloud transport](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:482) combines approximate scattering orders, a density-dependent sky fill, a low-sun floor, and a local optical correction that changes with sun elevation. [Cloud ambient visibility](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:388) is tied to the solar optical column. [Air shadowing](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/atmosphere-clouds.wgsl:319) excludes very low sun and moon-dominant states and leaves diffuse air illumination unshadowed. The [display transform](/Users/ryanwible/projects/wrela/packages/render-webgpu/src/display.wgsl:10) is a compact per-channel filmic curve, so final color judgments also include its response.

**Experiment:** hold the cloud model and exposure fixed, sweep the sun across the horizon, and compare scene-linear direct light, diffuse light, and the display result separately against an independent converged reference. Test directional diffuse visibility before adding another color or fill correction. Integrate one successful state with ground haze and reflections in the actual game camera.

### 6. The quality must survive ordinary-hardware budgets

Balanced loses small edge structure and makes upper wisps smoother. High still has the shape issues above despite its larger sampling budget. The [previous art-iteration benchmark](/Users/ryanwible/projects/wrela/docs/research/sky-art-iteration-2026-09-23.md) reported complete-frame moving-camera p95 of 38.27 ms for Balanced and 44.76 ms for High on its Apple/Metal run; these are **historical whole-frame results, not fresh timings or isolated sky costs**. Ordinary-hardware coverage remains unproven.

A suitable next performance experiment is to compile more useful occupancy/support information from the richer formations and measure the resulting redraw cost. Preserve silhouette and transmission at lower resolution. Retain the existing integration and motion error gates, but add visual acceptance: agreement with a denser rendering of the same model does not establish that the model looks natural.

## Recommended next work

Start with one excellent daylight weather group: an asymmetric growing tower, a broad shallow bank, torn remnants, and broken high wisps. Match a real-cloud composition and check the existing alpine cameras. Keep the current transport while settling silhouette and edge character. Then calibrate cloud lighting across the sun-angle sweep and integrate the result with distant air and terrain. Measure Balanced redraw cost alongside each accepted visual change.

Use the same reference board after every iteration. Require recognizably different forms at a distance, a convincing upper layer at zenith, coherent sunset lighting, and acceptable results when the camera turns away from the main cloud. A fresh temporal-motion review, night-sky review, and broader hardware test are still needed before claiming a complete AAA sky experience.
