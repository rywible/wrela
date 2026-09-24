# Sky reference audit — September 23, 2026

Wrela has a credible atmosphere renderer, but the current result does not yet meet the strongest AAA sky references. The largest remaining gap is the organization and variety of cloud forms, followed by edge character, depth through the cloudscape, and consistent twilight lighting. Adding more small noise or increasing saturation would not address those problems.

Open the [visual comparison board](sky-reference-audit-2026-09-23/index.html). It includes current production captures, downloaded references with attribution, and diagnostic comparisons. Images retain their original colors and proportions. This is a research audit; no production renderer or authored environment was changed.

## Evidence and comparison limits

I read the current shaders, atmosphere resource setup, authoring schema, runtime environment, and previous sky research. I rendered seven fresh 1440 × 900 high-quality views through the production renderer: side light, back light, front light, storm, golden hour, blue hour, and zenith. All completed on the Apple / Metal hardware adapter without recorded browser errors. Three additional balanced-quality views use the same frozen renderer bundle.

Two diagnostic checks narrow the causes:

- **Zenith with cloud history disabled:** its PNG is byte-identical to the original high-quality zenith capture. The softness in this static view is therefore present in fresh integration, not introduced by history reuse. This does not establish motion quality.
- **Blue-hour exposure sweep:** four full-redraw captures hold weather, sun, camera, and zero wind constant, varying exposure compensation from +0.8 to +4.5 EV. Lower exposure reduces the washed-out horizon but leaves very dark, nearly featureless cloud bodies. Exposure adjustment alone does not produce an acceptable twilight image.

The raw captures are visual evidence, not performance measurements; these runs may share the GPU. Unrelated files changed during capture, but the seven audited sky/schema/display/fixture files matched the initial source manifest afterward. The exact frozen bundle and hashes are recorded in [evidence.json](sky-reference-audit-2026-09-23/evidence.json).

The references are selected visual exemplars, not a claim that every frame in those games looks this good. Camera angle, weather, lens, exposure, and grading differ. NASA photographs include older scans and modest-resolution images; they establish morphology and layering, not a pixel-sharpness or absolute color target. The simple lookdev terrain is also not a fair competitor to a finished AAA landscape. Whole-scene integration still needs a subsequent playable-valley review with vegetation and water.

## References and what to study

| Reference | Why it belongs in the comparison |
| --- | --- |
| [Horizon Forbidden West: Burning Shores — official captures](https://blog.playstation.com/2023/03/29/pushing-the-envelope-achieving-next-level-clouds-in-horizon-forbidden-west-burning-shores/) | Distinct large silhouettes, illuminated gaps, and atmosphere that connects clouds to the landscape. The close-flight image also demonstrates that softness is appropriate in some views. |
| [Microsoft Flight Simulator 2024 — JohnnyT5000's captures](https://forums.flightsimulator.com/t/seriously-beautiful-clouds-in-msfs2024/767128/5) | An asymmetric hooked cloud and a sunset with multiple visible cloud textures. The author reports live weather without weather/cloud/atmosphere add-ons; this is a community reference, not independently verified capture configuration. |
| [Red Dead Redemption 2 — publisher Steam gallery](https://store.steampowered.com/app/1174180/Red_Dead_Redemption_2/) | The golden landscape shows sparse low clouds, fine upper patterning, illuminated haze, and coherent ground light. The moonlit scene supplies a separate night-atmosphere reference. Neither image proves Rockstar's internal algorithms. |
| [NASA S'COOL cumulus photographs](https://scool.larc.nasa.gov/GLOBE/cumulus.html) | Jeff Caplan's Rockies photograph, Kevin Larman's Colorado congestus, and Doug Stoddard's layered Puerto Rico sky distinguish vertical growth, silhouette variety, and different cloud families. |
| [NASA S'COOL cirrus at sunset](https://scool.larc.nasa.gov/GLOBE/cirrus.html) | Ed Donovan's photograph separates fine warm high clouds from a cooler sky. The straight lines are contrails; they are not the target for natural cirrus morphology. |
| [Lake Michigan crepuscular rays — Kurt Voigts / NASA APOD](https://apod.nasa.gov/apod/ap100811.html) | Clouds change the light in the air around them. Shadowed atmosphere is a distinct visual feature, beyond shadows on the ground. |

## What the research says to build

**Keep the scalable atmosphere foundation.** Hillaire's [2020 paper](https://sebh.github.io/publications/egsr2020.pdf) separates transmission, approximate multiple scattering, visible sky, and aerial perspective into compact products. Wrela already follows that broad architecture. Replacing it wholesale is not justified by these images. For twilight, compare against a converged reference and the [Wilkie et al. 2021 atmosphere model](https://cgg.mff.cuni.cz/publications/skymodel-2021/) before adding more color corrections.

**Separate cloud shape from cloud detail.** Guerrilla's [Nubis³ presentation](https://d3d3g8mu99pzk9.cloudfront.net/AndrewSchneider/Nubis%20Cubed.pdf), particularly PDF pages 79–125, models large volumes separately from billowy and wispy detail. Pages 129–149 cover shared light integration and approximations for internal glow and ambient illumination. The transferable lesson is deliberate structure at several scales with efficient shared transport. Its full voxel/simulation pipeline is a possible realization when needed, not a prerequisite for a ground-based Wrela scene.

**Treat the sky as part of the world's lighting.** Rockstar's [SIGGRAPH 2019 abstract](https://advances.realtimerendering.com/s2019/index.htm) explicitly connects cloud/fog rendering, volumetric effects, ambient lighting, and artistic control. Epic's [cloud documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/volumetric-cloud-component-in-unreal-engine) describes complementary roles for multiple-scattering approximations, cloud ambient occlusion, cloud shadows in the atmosphere, and sky-light capture. Their value comes from how the components agree in the final image.

My recommendation for Wrela is a small semantic palette of cloud formations, with shared lighting and conservative support bounds compiled from the same definitions. Keep ordinary-hardware cost visible throughout. A larger general cloud compiler or runtime weather simulation should earn its complexity after this small experiment improves the picture.

## What we already have

The September 22 research describes an older implementation. Its 32-step cloud limit, fixed 1,100 m base, and missing temporal reconstruction are **not current deficiencies**.

| Area | Current implementation |
| --- | --- |
| Clear air | Rayleigh/Mie scattering, ozone absorption, compiled optical-depth tables, approximate multiple scattering, sky-view and aerial-perspective products. |
| Low clouds | Procedural volume with weather organization, cellular updrafts, variable development, and bases varying from 850 to 1,950 m. Storminess expands the nominal top from 3,300 to 6,500 m. |
| Cloud light | Shared 128 × 128 × 16 light/ambient grids, cloud self-shadowing, local optical correction, phase functions, sky fill, and additional low-sun scattering terms. |
| Scene coupling | Cloud shadows, sky-derived diffuse irradiance, and filtered environment reflections are present. |
| Quality | Balanced cloud view: 768 × 512, up to 256 samples, 30 m target step. High: 1440 × 900, up to 384 samples, 20 m target. Actual sample count adapts. |
| Reuse | Wind-aware reprojection, support/depth checks, age limits, and full redraw on discontinuities. |
| Art controls | Cover, development, storminess, high-cloud cover, a weather front, time-of-day sequencing, tint, and exposure. |

These are useful capabilities to preserve. Feature presence alone does not establish visual acceptance.

## Where the current images fall short

### 1. Cloud identity and composition — highest visual priority

The daylight image has convincing volume, but its large foreground banks use a similar inflated, rounded vocabulary. Repeated shoulders and hanging lobes dominate; there are too few distinct growth forms, torn remnants, or quiet thin layers. The recent height variation is visible and useful, yet cloud identity remains constrained by one shared density recipe.

The code explains the limited control: [cloudscape schema](../../packages/model/src/environment-authoring.ts) lines 18–30 exposes scalar weather controls and one front; [cloud density](../../packages/render-webgpu/src/atmosphere-clouds.wgsl) lines 62–153 uses one mixture of weather noise, billows, cap, base, and erosion. There are no individually placeable semantic formations or independent layers in that schema.

**Next experiment:** author three recognizable forms—an asymmetric rising tower, a broad shallow bank, and a sheared dissipating fragment—using bounded envelopes plus the existing detail and transport. Compose a few groups with intentional gaps. Review silhouette first, then shade them. Vary base height within coherent weather groups; randomizing every cloud's height is not a substitute for meteorology.

### 2. Edge character and interior detail — highest visual priority

The overhead image is conspicuously smooth and stretched; daylight mixes soft large lobes with much busier small distant clouds. The contrast between actively growing edges and evaporating filaments is weak. Softness itself is not a defect: the problem is insufficient variation in the right places.

The full-redraw zenith check rules out history as the cause of this particular static softness. Balanced resolution adds another quality constraint, but the issue persists at native high resolution. The broad/fine noise organization and local optical approximation are candidates; this audit does not isolate their individual contributions.

**Next experiment:** condition billows, wisps, and erosion on formation type, maturity, and position within the volume. Keep dense interiors coherent. Run a small ablation of density detail, local lighting, and texture filtering before changing sample counts. Avoid global sharpening and stronger fine noise everywhere.

### 3. Distant depth and the horizon — high priority

The cloudscape becomes a narrow row of detailed small forms above a conspicuously clean horizon strip. Overlapping banks often read at similar contrast instead of receding through successive depths. The flat terrain exaggerates this, but there are also concrete renderer limitations.

[Cloud integration](../../packages/render-webgpu/src/atmosphere-clouds.wgsl) lines 275–285 uses a local horizontal slab, ignores near-horizontal/downward rays, limits the traversed path to 16 km, and rejects entries beyond 45 km. Lines 358–368 apply air transport once at extinction-weighted mean depth and fade toward clear sky between 22 and 45 km entry distance. The main-view air cache has a direct-integration fallback beyond 20 km; the cloud effect does not simply stop at the aerial cache boundary.

**Next experiment:** compare the current composite against air transport accumulated at several cloud depth intervals. Add an inexpensive distant layer or curved-shell realization from the same weather description, with continuous horizon coverage. Test a flat horizon and a mountain view. Establish whether each change improves depth before paying for full per-sample air integration.

### 4. Twilight, cloud illumination, and exposure — visible failure

Golden hour is a real improvement, with warm rims and readable bodies, but much of it stays in a narrow ochre/brown range. In the blue-hour fixture, dark cloud masses sit over an almost white horizon and unexpectedly bright ground. That is not a convincing blue-hour result.

The fixture requests **+4.5 EV** at sun elevation **−0.05 radians**. The static exposure sweep shows the failure is not solved by lowering that number: cloud bodies become nearly black. [Clear-sky integration](../../packages/render-webgpu/src/atmosphere-sky.wgsl) lines 139–147 also adds an explicit twilight residual to compensate for a 16-node integration. [Cloud lighting](../../packages/render-webgpu/src/atmosphere-clouds.wgsl) lines 298–343 uses angle-dependent fill and scattering approximations. These deserve a combined twilight audit; the evidence does not identify one coefficient as the sole cause.

**Next experiment:** render a fixed-exposure sunset sequence with clear sky, then one cloud, then the full scene. Inspect scene-linear radiance and the display result separately. Calibrate sky/cloud/ground brightness together, then set an exposure curve. A broader palette should follow altitude and illumination, rather than an indiscriminate pink grade.

### 5. Thin and upper cloud layers — medium priority, strong artistic payoff

High clouds exist, but [the current cirrus path](../../packages/render-webgpu/src/atmosphere-clouds.wgsl) lines 252–279 is a single sheet at 7,800 m with two oriented noise frequencies, an opacity cap, and simple sun/moon illumination. It cannot express the range of fine veils, fall streaks, and differently lit upper structures visible in the references. Mid-level cloud organization has no independent authoring control.

**Next experiment:** retain the cheap sheet realization, add directional wisps and broken patches with their own coverage/orientation, and shade them using atmospheric transmission at their altitude. Demonstrate a legible upper layer over sparse low clouds before adding more volume layers.

### 6. Shadows in the air and atmospheric drama — medium priority

Cloud-to-ground shadows are already implemented. However, [the atmosphere integration](../../packages/render-webgpu/src/atmosphere-sky.wgsl) lines 84–119 does not evaluate cloud visibility along its air samples. It therefore lacks cloud-shadowed atmospheric shafts in this path. That limits the sense that clouds inhabit and reshape the surrounding light.

**Next experiment:** sample the shared cloud light/visibility field in a bounded atmospheric pass. Test subtle shafts and cloud shadow volumes with modest haze. This should be driven by actual visibility, not a screen-space radial glow. Local fog and landscape lighting should be assessed in the playable world, not inferred from the bare lookdev hills.

### 7. Sampling artifacts and motion acceptance — quality gate

Front-lit and storm captures contain visible contour/stripe patterns in cloud bodies. Their appearance is consistent with integration or cached-light discretization, but the exact cause remains unproven. Global RMS against a denser rendering of the same model can pass while a localized artifact remains obvious.

**Next experiment:** hold the field fixed and independently vary view quadrature, light-grid sampling, and lighting approximation. Inspect crops at 100% and moving-camera/wind recordings. The current audit establishes still-image defects; it does not newly establish ghosting, wind evolution quality, or performance on low-end hardware.

## Recommended order

1. Establish the reference board as the visual gate and isolate the twilight imbalance and visible banding.
2. Build the three-form authoring experiment and condition edge detail on those forms. This is the largest likely improvement to everyday skies.
3. Improve cloud depth/horizon continuity and give thin upper clouds a distinct identity.
4. Integrate cloud shadows into the atmosphere and tune the entire playable valley under the same weather.
5. Measure complete-frame cost, update spikes, memory, and motion quality at balanced settings on ordinary target hardware. Use compiler-derived bounds and shared lighting to recover budget where measured.

Acceptance should include side/front/back light, horizon and overhead views, twilight at several solar angles, overcast, and camera/wind motion. Keep a dense integration control, but judge beauty against real skies and the game references. A more accurate integration of the current cloud recipe would still preserve its artistic limitations.
