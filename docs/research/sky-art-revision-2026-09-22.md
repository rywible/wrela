# Sky and ground visual revision

The preceding implementation was rejected for weak clouds, weak light, softness and uniformity. This revision changes the authored field and lighting response, not just the screenshot size. It follows the primary-source research in [the sky field report](sky-field-implementation-2026-09-22.md). These results supersede that report's rendering budgets; they do not constitute an independent world-class art sign-off.

## Visible changes

- Two regional weather scales now organize cloud banks, gaps and convection. Development varies spatially, storm strength bends upper structures with height, and less developed regions retain flatter connected banks. Coverage no longer distributes nearly identical puffs across the entire sky.
- The height envelope subtracts from the cloud shape, producing actual irregular silhouettes instead of fading every cloud into a similar fog-like top. Delaying its upper taper gives the banks deeper forms. Billows, roughly 200 m edge erosion and roughly 80 m fine erosion have separate roles.
- Detail filtering follows the projected pixel footprint. Reducing march samples no longer silently removes the authored fine frequencies. Balanced camera clouds grew from 448×336 to 768×512; high quality is 1440×900. The old 25-tap smoothing pass is now a compact nine-tap resolve with a much stronger center weight.
- Four approximate scattering orders, directional detail response, blue clear-sky fill and denser cores produce brighter sunlit forms, cooler shadows and stronger golden-hour edges. The directional response differentiates two retained shape terms while reusing density intermediates. It is an artistic multiple-scattering closure, not a physically converged solution.
- The alpine lighting preset has stronger side light and less ambient fill. Cloudy weather keeps the same solar source intensity; cloud transport provides the attenuation. The dry state now contains substantial cloud structure.
- Soil had metre-scale coloration and extremely weak fine grains, with little between them. Decimetre clods, centimetre grains and finer grit now contribute color and surface normals. Sparse individually shaped grains add close detail; their support fits inside each cell, avoiding a neighbouring-cell search, and they converge toward their analytic area/height means when unresolved. Damp soil is rougher, reducing its smooth plastic appearance. This remains shader detail, not displaced silhouettes or new vegetation. Sharply thresholded clods and larger regularly spaced grains were rejected after visual inspection.

## Cost controls and field structure

A trial that ray-marched an additional 480 m toward the light at every occupied view sample created richer detail but unacceptable refresh costs. It was replaced by the two-term directional difference above: broad shadows still come from the shared light product; local detail comes from already evaluated field terms. No added light texture is needed for this revision.

The light grid now preserves 250 m cells in its inner 24 km and uses its border for distant illumination out to 64 km in each horizontal direction. This avoids repeatedly integrating light and atmosphere for distant primary-source samples outside the old 32 km box. Distant shadow interpolation is intentionally coarser; this is a lighting representation tradeoff, not a density LOD.

Regional cloud ceilings give a useful bounded skip. The quintic value basis and its quantized trilinear realization have bounded partial derivatives. The hardware test exhausts adjacent texel pairs: maximum measured partial **1.819608**, against the conservative **2.2** used to derive the ceiling slope. The march retains a full segment of slack and the quadrature pair phase when skipping above that ceiling. Fixed-step references do not skip. This is a specialized propagation through a known field, not a claim that arbitrary procedural clouds are distance fields.

The compiler opportunity is concrete: preserve semantic field terms and their bounds, reuse intermediates for directional response, and choose different spatial resolutions for near and distant lighting. This revision implements that decomposition explicitly in WGSL. It does not add an automatic general cloud compiler or claim a novel scattering algorithm.

## Quality budgets

| Tier | Camera product | Maximum samples | Nominal step |
| --- | --- | --- | --- |
| Low | 320×240 | 64 | 65 m |
| Balanced | 768×512 | 192 | 40 m |
| High | 1440×900 | 384 | 20 m |

Short paths spend fewer samples; empty supported regions can skip nodes; opaque paths terminate. Reflection products retain separate lower budgets. Balanced view storage grows by 3.703 MiB relative to the preceding revision. High quality is deliberately expensive and is used for native-resolution visual inspection.

## Evidence and limitations

[Raw results and artifact identities](sky-art-revision-results-2026-09-22.json) preserve successful and expensive trial measurements. The cloud-kernel fixture covers broken cloud, overcast, sunset, zenith, moonlight, translated cameras and clear sky. It now compares against 1024 and 2048 view samples because the sharper field had not converged sufficiently at 256 samples. Error gates were **not relaxed**. A final budget sweep retained 192 maximum samples at 40 m nominal spacing: balanced RGBA RMS is 0.00279–0.00373 for non-clear cases, below the existing 0.004 gate. Reference convergence is at most 0.000214. The 256-sample control remains available. Cached/procedural illumination differences remain below the existing 0.002 RMS gate. These comparisons hold the approximate light grid and scattering model fixed; they do not validate radiometric truth.

On this Apple M4, the isolated balanced camera cloud kernel costs about 3.38 ms looking overhead and 6.55–6.91 ms in the other occupied cases. Complete 1024×768 frame medians are 2.69–4.98 ms in the replay, but cloud refreshes still cause important spikes: walking p95 **27.46 ms**, turning **24.58 ms**, changing weather **27.66 ms**. The largest measured case is 33.69 ms. The isolated sample-budget improvement did not eliminate whole-frame spikes. This revision improves the picture at a higher refresh cost than the rejected soft version. It is not evidence of a locked 60 fps sky on broad hardware.

51 focused CPU tests pass, both TypeScript configurations pass, and workspace boundaries pass. GPU checks cover scene-linear exposure, atmospheric edges, authored weather, coating behavior, compiled noise, and exact render-origin invariance (rebase RMS 0). Soil was inspected in close and landscape captures. Full-scene, native 1440×900 sky, low-tier, day/night and motion captures were reviewed. No imported sky pictures or AI-generated sky assets are used.

Remaining visual/technical limits include the single local low-cloud slab, approximate incident lighting and scattering, coarser distant light cells, unchanged simple cirrus, and no wind-aware temporal reconstruction. Ground geometry and vegetation quality are separate from the soil-detail correction. Native captures and actual gameplay remain the artistic test; passing these checks does not settle that judgment.

## Native visual review

The sky study uses the production high-quality renderer at **1440×900**, with no image resizing or external sky assets. The playable scene below uses balanced quality at **1024×768**. These are distinct budgets; the high-quality stills do not demonstrate balanced performance.

- [Native daylight](../../output/browser-1790129588397-14481/cumulus-side.png)
- [Native golden hour](../../output/browser-1790129588397-14481/golden.png)
- [Native storm](../../output/browser-1790129588397-14481/storm.png)
- [Balanced full scene](../../output/browser-1790129827873-14942/landscape.png)
- [Balanced ground detail](../../output/browser-1790129776517-14885/environment-shore.png)

Reproduce the native sky with `bun tools/sky-art-review.ts --high`, balanced environment with `bun tools/environment-lookdev.ts`, and full scene with `bun tools/lookdev.ts --study=scene --frame=landscape`. Run `bun tools/sky-field-check.ts` for the fixed reference comparisons. Replay a capture's frozen `fixture.js` with `bun tools/sky-replay.ts --bundle=<path>` for complete-frame measurements.
