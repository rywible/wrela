# Sky research, implementation, and measured tradeoffs

This work follows the [earlier sky design review](world-class-sky-research-2026-09-22.md). That review predates several features already present at the start of this task: cloud types/fronts, ozone, moonlight, quality tiers, and shared sun optical depth. Those are existing foundations, not new work claimed here.

The implementation improves cloud contours, sampling stability, distance haze, and illumination reuse. It does **not** establish that the sky has passed a world-class art review or that performance generalizes to all ordinary hardware. Hardware evidence here is Chrome/WebGPU on an Apple M4, 16 GiB RAM, Metal 3.

## Research decisions

| Primary source | Practical finding and decision |
| --- | --- |
| [Hillaire, 2020](https://onlinelibrary.wiley.com/doi/10.1111/cgf.14050) | Compact, dynamic atmosphere lookups and a multiple-scattering approximation are a strong scalable foundation. Keep the existing architecture; replacing clear-air scattering would not address the dominant cloud defects. |
| [Bruneton, implementation and tests](https://ebruneton.github.io/precomputed_atmospheric_scattering/) | A useful independent clear-air reference and example of separating mathematical tests from image validation. Our high-sample cloud comparison is not a Bruneton or spectral-reference validation. |
| [Wilkie et al., 2021](https://cgg.mff.cuni.cz/publications/skymodel-2021/) | Fitted radiance/attenuation includes post-sunset, altitude, and finite-distance effects. Useful for a future twilight accuracy audit; not a solution to the current procedural cloud problem. |
| [Guerrilla, Nubis 2017](https://www.guerrilla-games.com/read/nubis-authoring-real-time-volumetric-cloudscapes-with-the-decima-engine) | Regional weather and procedural cloud shape must cooperate. Retain the existing authored coverage/front/development controls and improve their volumetric realization. |
| [Guerrilla, Nubis Evolved](https://www.guerrilla-games.com/read/nubis-evolved) | Near clouds and fast effects need different reconstruction tradeoffs. Do not add temporal accumulation without wind reprojection, disocclusion, camera-cut, and latency tests. This implementation improves the current-frame result. |
| [Guerrilla, Nubis³](https://www.guerrilla-games.com/read/nubis-cubed) | Compressed distance fields, voxel clouds, and accelerated light sampling are established techniques. Precomputation itself is not a novel Wrela contribution. Test which repeated work is actually expensive here before committing memory to it. |
| [Epic, volumetric cloud documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/volumetric-clouds?application_version=4.27) | Conservative density, separate reflection budgets, shared shadow products, atmospheric integration, and scalable reconstruction have practical production precedent. A conservative density must never reject nonempty cloud; ordinary sampled density does not provide that guarantee. |
| [Wolfe et al., spatiotemporal blue noise](https://research.nvidia.com/publication/2021-12_scalar-spatiotemporal-blue-noise-masks) | Good temporal sampling improves convergence when paired with temporal reconstruction. Pixel scrambling alone leaves grain in this renderer because it caches a single estimate rather than accumulating history. |

## What changed

- **Incident lighting is compiled into a shared spatial product.** The existing 128×128×16 optical-depth texture previously used only one channel. Its remaining channels now store atmospheric direct-light transmission. One additional RGBA16F volume stores incident diffuse radiance with the cloud attenuation approximation. The view and reflection marches sample these fields instead of repeating atmosphere-table interpolation at every occupied point. Added memory: **2 MiB**. There are no new external assets or runtime dependencies.
- **Scattering remains view-dependent.** The phase functions stay in the view march. Their fixed angular terms are evaluated outside the loop, and one exponential supplies three finite scattering orders. Solar intensity and color still apply at consumption, so changing intensity does not require recompiling a unit-light field. The secondary celestial source and samples outside the local grid retain procedural evaluation.
- **Cloud shape is less fog-like.** Cellular shape contributes more to both macro form and billows; development controls a denser upper profile. Coverage threshold was retuned alongside those changes. Increased core extinction/density and a revised diffuse floor make opaque masses and their shaded sides more legible. These are artistic density-model choices, not a fluid simulation or calibrated water-content model.
- **Ray length determines sampling work.** The approximately 274 m finest authored erosion period motivates a nominal 70 m interval. Short rays need fewer nodes, while grazing rays can use up to 64/128/192 samples on low/balanced/high. Paired deterministic quadrature replaces unaccumulated pixel noise. The old scrambled estimator remains available as a test control. This controls work; it is not an error certificate.
- **A derived support bound trims empty height.** The broad field lies in [0,1], so propagating that interval through the height profile gives an upper cloud boundary. The view march stops there. A small numerical margin is included. This is a bound on the authored height profile, not a claim that arbitrary warped density is an SDF.
- **Clouds receive foreground atmospheric perspective.** The march accumulates extinction-weighted distance, reuses the existing eight-segment camera aerial-perspective volume at that depth (six-segment direct fallback for reflections and distances beyond 20 km), and composites `cloudLight * airTransmission + airRadiance * cloudOpacity + clearSky * cloudTransmission`. This avoids adding foreground haze twice to the already integrated clear sky. A representative depth is still an approximation for several separated cloud layers.
- **The visible sun uses the same cloud transmission as the rendered sky.** Its previous one-column estimate could disagree with the visible cloud; the moon already used the visible product.
- **Lighting invalidation includes ground color and planetary origin.** Those inputs now affect the cached incident light. Resource lifetime and byte accounting cover the added volume.

## Compiler experiment that was rejected

The initial hypothesis was that repeated procedural density evaluation dominated cost. A GPU partial-evaluation prototype cached four immutable shape channels in material coordinates. Wind became a coordinate offset; cover, fronts, development, storminess, sun, and moon remained runtime inputs. The cache covered 16.384 km at 256×256×64 in RGBA8, with a procedural border and outside fallback.

It worked, but did not earn its cost. At equal 64-sample integration it saved roughly 3–8% in the stable cases, consumed **16 MiB**, and introduced interpolation error. Doubling the sample count still nearly doubled render time. Build cost was about 0.88 ms on this device. The prototype is **not in the shipped renderer**. Its [patch](sky-basis-cache-experiment.patch) and [measurements](sky-basis-cache-results.json) preserve the failed hypothesis and a way to reproduce it against the starting sources.

That result redirected work toward incident lighting and sampling allocation. This is an example of the field/compiler thesis helping identify reusable structure, but measurement rejecting one particular representation.

## What could be novel

The credible research opportunity is a compiler that automatically separates a semantic weather program into immutable shape, advected coordinates, bounded support, light-dependent transport, and view-dependent integration, then chooses representations using measured error and cost. A cloud field could eventually emit matching traversal, shadow, reflection, and temporal-motion products from one definition.

This change implements a small, explicit specialization of that decomposition in the renderer. It does **not** add a general cloud IR to the geometry compiler, automatically prove spatial emptiness, or establish a new rendering algorithm. The existing geometry IR's outside-distance contract cannot simply be applied to thresholded warped cloud noise. Density interval analysis, filter/threshold coverage preservation, and automatic representation selection remain research work.

## Validation and limits

`bun tools/sky-field-check.ts` compares the actual cloud-view kernel at 448×336 across broken cloud, overcast, sunset, zenith, moonlight, translated camera, and clear sky. It records finite/nonnegative output, RGBA RMS/max/bias, equal-sample cached-lighting/aerial-versus-procedural error, 256-versus-512 sample convergence, allocation size, and interleaved GPU timestamps. Empirical image gates fail the command on regression. Its 32-sample control uses the **new** density and atmospheric composition; it is not the old shipped renderer. The reference increases view samples only; shared light-grid integration and the finite scattering model remain approximate.

`bun tools/sky-replay.ts --bundle=<saved fixture.js>` replays the exact saved pre-change and final lookdev bundles, preserving bundle hashes and raw samples without rewriting the working tree. It measures complete 1024×768 frames during stillness, walking, turns, and changing weather. This separates equal-quality kernel savings from the total cost of the higher-quality default.

The production lookdev covers clear, clouded, clearing, shore/reflection, low sun, twilight, zenith, and sky views; separate checks cover authored environment integration, rebasing, atmospheric edge cases, low quality, and motion. The remaining limitations are the local upward-looking cloud slab, approximate multiple scattering, coarse shared shadow/light interpolation, finite-distance representative-depth compositing, limited macro art direction, and lack of wind-aware temporal reconstruction. No second GPU or mobile device was available for measurement.

## Final quality budgets

| Tier | Camera cloud product | Maximum view samples | Reflection product / maximum samples | Shared light samples |
| --- | --- | --- | --- | --- |
| Low | 320×240 | 64 | 256×128 / 48 | 6 |
| Balanced | 448×336 | 128 | 384×192 / 64 | 12 |
| High | 768×576 | 192 | 512×256 / 96 | 16 |

The maximum is reserved for long grazing rays; overhead rays typically use 32–40 samples. Reflections have a separate lower integration cap. Reducing the balanced view allocation partly offsets the new light volume: net product storage grows by **1.297 MiB**, not 16 MiB. Camera aerial reuse adds no texture allocation. The larger initial 512×384/128-sample default was rejected after whole-frame weather updates revealed excessive refresh cost.

The old atmosphere edge fixture used default temporal jitter while asserting pixel-identical exposure invariance. Its failed run was investigated rather than weakening the threshold: the fixture now explicitly uses spatial anti-aliasing so it compares the same rays. Exposure-relative HDR RMS is exactly zero in the corrected hardware test.

Final measured results and artifact paths follow.


## Measured results

The [raw report](sky-field-results-2026-09-22.json) includes every timing sample, reference errors, bundle hashes, and artifact directories. A single-host run cannot establish portable frame rates. GPU timestamps are quantized and the machine is not an isolated lab.

| Cloud-view case | 32-sample scrambled control RGBA RMS | Final adaptive RGBA RMS | Final kernel median, ms | Equal-64-sample cache speedup |
| --- | ---: | ---: | ---: | ---: |
| broken | 0.029970 | 0.001176 | 2.785 | 19.3% |
| overcast | 0.002519 | 0.000782 | 1.376 | 27.0% |
| sunset | 0.028530 | 0.000922 | 1.868 | 20.8% |
| zenith | 0.007105 | 0.001298 | 0.950 | 22.8% |
| night | 0.028361 | 0.000885 | 3.539 | 17.3% |
| translated-camera | 0.028661 | 0.001213 | 1.933 | 20.0% |

The reference's 256→512 sample RMS is at most 0.000251 across these cases. Joint light/aerial cache RMS versus direct evaluation is at most 0.000823 at equal sampling. The occupied light-volume build measured 0.786 ms median. The night timing showed substantial host variance; retain the raw samples rather than treating that one number as a stable hardware characteristic. Error is measured over scene-linear RGB **and cloud transmission together**; it is not a perceptual percentage or proof of physical accuracy.

Complete-frame comparison at 1024×768, 60 measured frames per scenario (same fixed camera, authored project, and quality tier; old and final saved bundles):

| Scenario | Old GPU p50 / p95, ms | Final GPU p50 / p95, ms | Old / final mean, ms |
| --- | ---: | ---: | ---: |
| static | 4.39 / 8.13 | 4.33 / 7.73 | 4.78 / 4.49 |
| walking | 4.33 / 8.78 | 4.26 / 9.70 | 5.05 / 4.52 |
| turning | 4.78 / 8.85 | 4.39 / 8.72 | 5.10 / 4.73 |
| weather | 4.39 / 11.08 | 4.46 / 13.96 | 5.21 / 6.14 |

**The higher-quality default is not a universal frame-time speedup.** Static and typical movement frames remain near the original cost, while weather refreshes have a higher tail: p95 rose from 11.08 to 13.96 ms. The maximum sampled final weather frame was 16.32 ms. This was deliberately recorded rather than replacing whole-frame evidence with the faster equal-quality cache kernel. No frame-rate guarantee follows from these short samples.

Validation passed: 46 targeted unit tests, both TypeScript configurations, workspace boundaries, formatting of changed TypeScript, all seven hardware field cases and image-error gates, eight balanced and eight low-tier scene captures, twelve motion captures, eight dawn-to-moonlight captures, authored environment/water integration, and eight atmosphere edge/exposure cases. Origin-rebase HDR RMS and exposure-invariance HDR RMS were both zero. The old procedural-versus-noise-texture harness was repaired to bind the current frame and lighting resources, and also passes.

Canonical full-scene captures: [before sky](../../output/browser-1790124740131-9483/environment-sky.png), [final sky](../../output/browser-1790126282680-11058/environment-sky.png), [final clear scene](../../output/browser-1790126282680-11058/environment-clear.png), [final low sun](../../output/browser-1790126282680-11058/environment-low-sun.png), [final zenith](../../output/browser-1790126282680-11058/environment-zenith.png). The scene still needs stronger macro art direction and broader hardware review before calling it world class.

Day/night integration captures: `output/browser-1790126416964-11312` (dawn, noon, storm, sunset, golden hour, night, stars, moon); all complete with no browser validation errors.
