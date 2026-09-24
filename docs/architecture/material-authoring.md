# Surface authoring

Materials retain their existing procedural base and optional creature appearance. `MaterialDefinition.appearance` adds a bounded surface recipe; old documents do not change. Model validation lives in `packages/model/src/surface-appearance.ts`, runtime response defaults and review guidance in `packages/runtime/src/surface-appearance.ts`, GPU packing in `packages/render-webgpu/src/surface-appearance.ts`, and shader helpers in the adjacent WGSL file. Studio's dedicated panel edits the same source through normal transactions.

## Authored intent and evaluation order

1. Evaluate the existing base pattern and legacy layers.
2. Apply up to four ordered coatings, with color, roughness, metallic fraction, coverage, and signed relief in metres. Each coating has a stable identity, an enabled flag, and a uniform, noise, slope, height-band, or combined mask. Combined masks intersect noise with independently weighted slope and height restrictions, so moss can grow on noisy upward surfaces within an authored elevation band. Mask threshold, softness, inversion, and frequency remain explicit and editable. Disabled coatings retain their order and authoring settings.
3. Apply weathering, accumulated dirt, exposed damage, and wetness. These are bounded procedural surface-history approximations. Dirt favors upward-facing surfaces; damage uses spatial breakup. Wetness darkens diffuse color, lowers roughness, and adds a Fresnel coat.
4. Evaluate the selected material response and existing scene lighting.

Local coordinates remain attached to the undeformed surface, including an existing anatomical frame. World coordinates share metre-scale patterns across separate objects. GPU noise origins are reduced in double precision before upload, and world height bands are rebased along with the scene. Detail noise is filtered by the pixel footprint to avoid unchecked distant noise frequencies. Threshold masks converge toward a bounded occupancy estimate as their noise becomes subpixel; rust and damage no longer disappear merely because the filtered noise mean falls below a threshold. The occupancy estimate assumes uniform noise and is an approximation, not an exact integration of the lattice noise distribution. World detail uses a common frequency lattice with separately packed integer and fractional origins, including sediment phase, to keep anisotropic grain stable across rebases.

## Substance responses

- Generic preserves the existing dielectric response.
- Skin reuses the creature wrapped-diffuse and thin-surface transmission model.
- Foliage uses a dedicated two-sided diffuse reflection/transmission budget with green scattering. Reflected and transmitted energy share the same albedo budget, and direct transmission respects full occlusion.
- Fabric reuses the cloth grazing-sheen response.
- Metal sets conductor behavior while allowing nonmetal coatings to cover the substrate.
- Glass uses an index-of-refraction controlled Fresnel response with tinted environment transmission.

Skin/foliage thickness and scattering tint, skin wrap, fabric sheen and anisotropy, and dielectric/conductor clearcoat are editable response controls. Defaults come from one model helper shared by Studio and runtime. Explicit authored creature optics take priority over family presets, and Studio disables the conflicting response controls with an explanation. Opaque coatings and dirt attenuate underlying skin transmission, fiber sheen and coat responses. Glass blends with opaque coatings instead of overwriting them. Existing creature anatomy frames, tangents, and groom controls are preserved.

Glass remains an opaque environment approximation: it does not refract other scene objects, sort transparent layers, or implement volumetric absorption. Skin and foliage do not model volumetric multiple scattering. Dirt and damage masks do not measure curvature, cavities, rain accumulation, or geometry loss. These limits are surfaced in review guidance; the controls are a procedural art-direction workflow, not a claim of measured material simulation.

## Rendering contract

The appearance block appends 124 floats after the established 200-float object block. Creature and water offsets remain unchanged. Seven header vec4s are followed by four coatings of six vec4s each. The overall buffer is 324 floats. Combined mask controls occupy the final reserved coating vec4. Three previously reserved header components store detail-origin fractions, with the sediment phase in the detail-origin w component. Absent appearance uploads zeros and bypasses the new shader branch. Batching includes the complete packed appearance block, so distinct masks, histories, and materials cannot incorrectly share uniforms. No textures, bind groups, or new render passes are needed.

Optical material edits retain the existing base geometry cache. The optional physical relief recipe creates a separately cached geometric realization after material binding; it does not overwrite the authored base mesh. See [physical surface relief](surface-relief.md) for compiler bounds, budget fallbacks and unsupported deformation cases.

## Review and verification

Studio provides close, grazing, low-sun transmission, and gameplay-distance review buttons through the shared preview service. They change review distance and sun elevation/azimuth while leaving source material unchanged. Camera orientation still determines whether an absolute sun direction is backlit. The standalone material review fixture uses a fixed camera and known light directions for reproducible comparisons. Existing base-color, roughness, metallic, normal, and clay views remain available.

`bun tools/surface-appearance-check.ts` creates a real WebGPU surface and compares linear HDR pixels against a dry control. It checks skin, foliage, fabric, metal, glass, all four history controls, and all five mask types. It fails on absent pixel effects, nonfinite output, incomplete rendering, or renderer/browser errors. The expanded hardware run on Apple Metal 3 passed 15 feature comparisons, including combined masks and legacy-layer preservation over surface detail. Three additional comparisons verified that fully opaque coatings over glass, skin, and fabric exactly match an ordinary opaque control (all three RMS errors were zero), with no renderer/browser errors. Evidence: `output/lookdev-snapshots/1790111257872-19828/source/output/browser-1790111257954-19845/surface-appearance.json`. This establishes executable integration, not artistic acceptance.

Unit tests cover legacy preservation, explicit creature precedence, substance defaults, source roundtripping and validation, mask intent and inversion, packed offsets, origin rebasing, disabled layers, and batching separation. GPU review and art-direction acceptance should use curved surfaces, multiple lighting rigs, animated characters, and gameplay-distance captures in the target game before treating a material as finished.


## Material family look development

`bun tools/material-lookdev.ts --families` captures six source materials (limestone, skin, foliage, fabric, bronze, glass) under neutral, grazing, and back lighting and at gameplay distance. It also captures the coated version in beauty, albedo, roughness, and metallic modes. Every capture checks renderer completeness and accumulated hardware diagnostics, including distant and diagnostic frames. The same tool without `--families` reviews the alpine palette. All captures require a real WebGPU adapter; they are not CPU render substitutes.

The initial family sheet exposed concentric moire on cloth. The authoring renderer now projects the weave onto three surface-facing coordinate planes and applies reconstruction filtering to subpixel thread frequencies. The exact affine box-integrated woven helper remains available for compiler verification; the additional authoring filter addresses its remaining reconstruction sidelobes. This is procedural triplanar cloth, not a garment UV weave with textile-specific fiber scattering.

Studio color pickers convert between sRGB display values and stored linear albedo. The panels are separated into material-level controls, mask editing, substance response, and reusable color/number controls. Palette recipes now distinguish mineral pores and bedding, exposed masonry deposits, damp moss at footings, wood grain, silvered exposed timber, and subdued pine needles. Visual review determines whether those choices succeed in the final scene; sphere sheets and numeric pixel changes alone do not establish AAA quality.

### Visual verdict, 2026-09-22

The final focused gateway detail capture (`output/lookdev-snapshots/1790111314756-19999/source/output/browser-1790111314910-20010/architecture-detail.png`) is an improvement over the clean baseline: timber grain is readable, stone has finer surface structure, and damp lower courses remain distinct. An intermediate coarse, square mottling result was rejected; the final recipe reduces that contrast and relief and moves mineral detail to a finer scale. The result is suitable for continued authoring iteration, but it is **not an AAA art approval**: masonry still reads as uniformly cut modular blocks with repeated procedural grain, and the scene lacks the varied edge erosion, localized surface history and richer lighting needed for a finished hero asset.

The six-family sheet (`output/authoring-lookdev/1790110916847-18149/source/output/browser-1790110929711-18328/materials-neutral.png`) confirms that the cloth rings seen in the first capture were eliminated; grazing and distant frames also remain free of those rings. Coating boundaries are softer and substance responses remain distinguishable. Glass continues to display the documented environment-only optical approximation. These observations establish specific visual improvements, not equivalence to production skin, foliage, textiles or refractive glass.


## Thin foliage review

`bun tools/foliage-lookdev.ts` replaces the foliage-sphere-only check with real specimens: a curved single lamina at 0.35 mm optical thickness, the same lamina at 4 mm, and a shoot extracted from the production conifer compiler. Seven frames cover front, back and grazing light at two distances plus the reverse face. A separate rendered probe compares an isolated backlit leaf against zero-transmission controls, with and without a larger external opaque blocker. Geometry is real mesh geometry; no alpha cutout textures or fabricated screenshots are involved.

`packages/render-webgpu/src/foliage.wgsl` defines the bounded diffuse closure, with an independent TypeScript audit counterpart and a 135-case hardware parity probe. The transmitted fraction is `transmission * exp(-thickness / 0.005)`. Reflection receives the remaining fraction; scattering tint can only reduce transmission. A 0.96 multiplier reserves 4% for a normal-incidence interface, and each channel's diffuse reflection plus transmission cannot exceed 96% of its authored albedo. The probe reports this **diffuse-only** bound, CPU/GPU agreement and exactly zero directly transmitted light at zero visibility. It does not certify conservation of the separate GGX specular term or the full renderer at grazing angles.

The former foliage response added back transmission on top of full reflected albedo and imposed a 25% shadow visibility floor. Both were removed. Shadow sampling now offsets thin foliage toward the incident side, avoiding the specific self-shadow caused by offsetting a backlit lamina toward the camera. Ambient sky response uses the same budget with irradiance from both sides; coatings continue to obscure the leaf response.

The 5 mm attenuation distance remains an empirical artistic scale. It is not a fitted pigment absorption coefficient. [PROSPECT's author documentation](https://jbferet.gitlab.io/prospect/articles/prospect1.html) describes a stronger biochemical/structural model for directional-hemispherical leaf reflectance and transmittance. [PBRT's layered transport chapter](https://pbr-book.org/4ed/Light_Transport_II_Volume_Rendering/Scattering_from_Layered_Materials) explains the broader multiple-scattering problem. This implementation does not reproduce those models, canopy multiple scattering, partially transmitting shadow maps, spectral leaf optics, or a complete measured leaf BSDF.

### Thin-foliage measured evidence, 2026-09-22

Hardware evidence is in `output/lookdev-snapshots/1790112162804-21863/source/output/browser-1790112162884-21875/foliage-lookdev.json` and its seven adjacent PNG captures. The Apple Metal 3 run completed without renderer/browser errors. Across 135 cases, the largest CPU/GPU discrepancy was `6.26975e-8`; the largest diffuse R+T/albedo ratio was `0.960000058` (float32 rounding around the 0.96 bound); and directly transmitted response at zero visibility was exactly zero. The rendered isolated-leaf probe covered 27,876 leaf pixels, produced a `0.0819992` linear backlight signal over its opaque control, and produced zero residual transmission signal with the external blocker present.

All seven images were inspected. The thinner leaf is visibly brighter than the thicker one under backlighting, while the thicker leaf retains more front reflection; grazing light falls off without the previous artificial transmission floor. The reverse face renders, and near/far captures preserve that ordering. The conifer shoot uses actual narrow needle geometry and becomes appropriately small at distance, but it is too small and dark in this diagnostic layout to approve detailed needle appearance. The laminae are deliberately simple green diagnostic shapes with weak venation, not finished botanical assets. Static captures also do not establish temporal stability, dense-canopy transport or AAA quality. The accepted result is the specific front/back, energy-budget and full-occlusion correction, with those limitations retained.
