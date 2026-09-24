# Ozone and view-resolved cloud implementation

This is an implementation improvement and a measured-work proposal, not a claim of a new atmospheric model, AAA acceptance, or a performance lead.

## Observed problem

The `environment-clouded`, `environment-low-sun`, and `environment-clearing` captures in `output/authoring-lookdev/1790113223050-23493/source/output/browser-1790113274736-23932` were inspected. Clouds formed soft, visibly coarse sheets and low sun appeared muddy. A 512-wide full-sphere sky assigns only about 68–85 horizontal texels to a 48–60 degree camera. Upsampling those texels to 1024 pixels limits cloud detail regardless of density-field quality.

## Established research used

Hillaire separates atmospheric transmittance, diffuse multiple scattering, sky view, and aerial perspective into reusable products. Our existing atmosphere already follows this broad design. The improvement retains it. [Hillaire, EGSR 2020](https://sebh.github.io/publications/egsr2020.pdf).

The ozone parameters come from the author's Earth setup: a triangular density layer at 10/25/40 km and RGB absorption `[0.000650, 0.001881, 0.000085]` per kilometre. Absorption is distinct from scattering. [Pinned author implementation](https://github.com/sebh/UnrealEngineSkyAtmosphere/blob/183ead5bdacc701b3b626347a680a2f3cd3d4fbd/Application/SkyAtmosphereCommon.cpp).

Guerrilla's Nubis work establishes procedural density, regional weather, cloud transitions, and atmospheric integration as a practical authoring approach. Our small density/lighting implementation is not a reproduction of Nubis, its noise assets, or its reported performance. [Guerrilla's 2017 author presentation](https://www.guerrilla-games.com/read/nubis-authoring-real-time-volumetric-cloudscapes-with-the-decima-engine).

## Delivered changes

### Ozone

`atmosphere-ozone.ts` integrates the piecewise-linear altitude profile by splitting each spherical ray at density-layer shell crossings. Each interval is affine in spherical radius and has an analytic integral. This formula is exact in real arithmetic; its JavaScript evaluation and FP32 storage are not directed-rounding certificates. Independent 131,072-interval Simpson integration checks upward, downward, tangent, and multiple-shell rays. The vertical density column is 15 km; starting at the 25 km peak leaves 7.5 km.

Scattering table V3 preserves the existing four-vector header and cell offset. The fourth cell lane contains ozone density column. Two trailing vectors hold absorption RGB and the layer altitudes, adding 32 bytes to the previous authored table. V2 remains accepted with zero ozone. V3 validates its layer and cells before upload. The authored default is regenerated at 49,248 bytes.

Ozone contributes only extinction. It attenuates direct sunlight, clear-air view transmission, aerial perspective, multiple-scattering construction, and solar cloud illumination through the shared medium functions. Molecular and particle scattering terms remain separate. Existing real-arithmetic exponential integration evidence, unknown FP32 error, unknown table interpolation error, and finite 16-order diffuse approximation remain explicit.

### Cloud resolution and cache

| Product | Size | Use |
| --- | --- | --- |
| Clear-air sky | 512×256 RGBA16F | Reusable smooth atmosphere, sampled by cloud builders |
| Clouded spherical sky | 512×256 RGBA16F | Existing reflection and sky-irradiance source |
| Clouded camera sky | 512×384 RGBA16F | Background detail over the actual camera frustum |

The two extra textures add 2,621,440 bytes (2.5 MiB). The camera product is half resolution at the 1024×768 review size, with fixed storage/work bounds on larger displays. Cloud marching runs in compute, never per background fragment. The camera product needs camera orientation/FOV; the spherical product does not. Neither cloud drift nor orientation invalidates the clear-air sky. An idle clock does not rebuild clouds when the field is stationary: cache identity uses actual FP32 wind advection, and cloud-free frames omit cloud time entirely. Tests exercise rebuild dependencies, timestamp boundaries, byte accounting, and disposal.

### Density, lighting, and weather

Density uses a rotated, warped noise domain with quintic interpolation, broad masses, smaller billows, and edge erosion. Increasing cover also transitions toward a lower stratiform deck. Advection and all sample positions retain planet-relative coordinates, preserving the prior render-origin behavior. Camera clouds, spherical clouds, and sun shadows query one density definition.

The cloud view integrates 32 segments and up to three solar-density samples at each occupied segment. Illumination uses a normalized forward/backward phase mixture and a three-term empirical approximation for higher scattering orders. Solar color and planet occlusion come from the atmosphere; the diffuse atmosphere supplies ambient radiance. The previous arbitrary added clear-sky tint is removed. This is not a converged cloud transport reference or an energy certificate.

The clear, clouded, and clearing review states now separate aerosol haze from cloud cover. A dense cloud deck no longer automatically implies very high aerosol turbidity. Review tints are neutral so atmospheric hue is visible. Low-sun and twilight views retain explicit exposure controls and use lower mist rather than masking color errors with a warm tint.

## Validation and limitations

Twenty focused CPU tests pass, including analytic ozone against independent quadrature, the vertical column, color-dependent absorption ordering, V3 validation/V2 compatibility, cache dependencies, and previous atmospheric invariants. TypeScript and workspace boundary checks pass. Root reported that the complete renderer and new cloud module compile on the hardware GPU in frozen snapshot `output/lookdev-snapshots/1790115406258-29686/source`; this agent launched no GPU process.

Visual acceptance and timing still require the root's serialized environment captures. The cloud builder has more work than the previous single-density solar approximation. In the densest case it evaluates up to 128 density queries per output texel; early termination and empty segments reduce actual work. The polynomial hash also replaces transcendental sine hashes, but this does not establish a net speedup. Measure cold, moving-camera, animated-cloud, and stationary reuse cases with the same sky coverage and output size.

The cloud layer is a local horizontal slab, not a planetary cloud shell. Surface cloud shadows still use a cheap representative column rather than the view's solar quadrature. Ground geometry does not cast volumetric shadows into the atmosphere. Reflections and diffuse irradiance retain the coarse spherical cloud product. Strong cloud motion has no temporal reconstruction. These are known approximation boundaries, not completed research improvements.

## First hardware review and follow-up

All eight environment frames completed on `apple · metal-3` at 1024×768 in `output/authoring-lookdev/1790115555797-30193/source/output/browser-1790115630103-30658`. The clear, storm, low-sun, sky-only, zenith, and twilight images were inspected. Color separation and distinct cloud masses improved, but the result failed artistic acceptance: cloud surfaces showed repeated horizontal ripples, zenith masses remained too soft, and twilight was nearly black except for a red horizon. Increased angular resolution made a remaining sampling problem visible; it did not establish AAA quality.

The saved measurements report whole-frame GPU values of 1.18–11.27 ms and CPU values of 1.0–1.9 ms. Frame-to-complete wall time was 270–499 ms except the shore frame at 87 ms. They do not contain an atmosphere pass timing, so they cannot establish cloud construction cost or a speedup. The source manifest freezes the implementation before the following noise/filtering changes.

The follow-up retains a periodic analytic noise function as a reference and adds a compiler-generated GPU realization: a 66³ RGBA8 texture, containing a 64³ interior plus a one-voxel periodic border. It occupies 1,149,984 bytes and is generated once per renderer, independently of weather. The same existing sampler can address the padded tile without seams on clamped axes. The total additional texture allocation for view clouds, clear sky, and noise is now 3,771,424 bytes. This is a procedural product; there is no downloaded or hand-painted texture.

The field repeats every eight lattice cells; rotated octave coordinates and a lower-frequency weather field vary the combined density. The new periodic field is an authored source change from the previous unbounded hash, not just an interchangeable realization of that old source. The `cloudNoise: "reference"` renderer option retains evaluation of the new periodic source without texture approximation. `"compiled"` is the default. This allows the actual representation trade to be compared without changing the source field.

The cloud view and solar quadrature now attenuate noise octaves whose wavelength is unresolved by the integration segment. This is a frequency-filtering heuristic, not a coverage bound for the nonlinear density threshold. It targets the coherent sampling bands; a subsequent image review must confirm whether it succeeds.

`tools/cloud-noise-check.ts` provides a separate hardware experiment: 4096 analytic-versus-compiled field points, periodicity, a homogeneous transmission proxy, and alternating batched GPU timings for field sampling and initial construction. Its output must not be reported as a whole-frame performance result or cloud-image error certificate. This agent did not launch the experiment; execution belongs to the root's serialized GPU validation.


## Failed periodicity experiment and precision follow-up

The first completed component probe in frozen source `1790117138605-36189` failed its original translated-coordinate periodicity gate. Its evidence is retained in `cloud-noise-failed-2026-09-22.json`: density RMS 0.0040440, maximum 0.0164630, bias 0.00006789; translated sample difference 0.000245094 exceeded 0.0001. Density approximation passed its separate 0.035 maximum gate. Timing was never reached, so this run establishes no performance result.

The original test added integer periods to arbitrary f32 points. Those additions need not preserve the fractional phase: CPU emulation of the probe changes 1,317 coordinate-axis phases, up to 2^-23 of a period (2^-17 interior texels). The old assertion therefore conflated field periodicity with coordinate and filtering precision. The revised probe keeps that measurement, adds a strict equal-phase periodicity check on exactly representable binary-grid positions, and compares hardware sampling with explicit f32 trilinear texture loads. The density gate and 0.0001 equal-phase gate remain. Filtering deviation and shifted-coordinate drift are reported independently; no portable texture-filter precision bound is assumed. This follows the distinction between rounded f32 arithmetic and texture sampling in the [WGSL accuracy rules](https://www.w3.org/TR/WGSL/#floating-point-accuracy). The runner now saves the report, including failed assertions and device errors, before throwing.

The later formations review (`1790117165394-36437`) still shows visible horizontal cloud ripples. Octave footprint attenuation did **not** establish a fix. Inspection found a separate definite cancellation problem: cloud coordinates computed `(world - planetCenter) - radius`, introducing the 6.36-million-metre planetary term before cancelling it. Near Earth radius f32 spacing is 0.5 m. The bounded correction computes `world - (planetCenter + radius)` so local samples retain their available precision. It also applies to surface-shadow density, and changes neither the density source nor sample count. This is a numerical correction, not proof that it causes every visible band. Planet-center input quantization, coherent 32-segment quadrature, coarse illumination products, and soft density structure remain review limits. The next serialized image capture must establish whether the visible artifact improves; artistic acceptance is still failed at this point.


The next bounded quadrature change scrambles each texel's segment phase with an integer hash and rotates that phase by the golden-ratio conjugate between segments. There is still one density node inside each of the same 32 strata, with the original full segment length used for extinction and radiance integration. It adds no cloud-density or solar-density queries. This removes the exact midpoint alignment shared by image rows; it trades coherent quadrature error for spatial integration noise and is **not** a proof of lower radiance error. The per-texel phase is independent of time, so an unchanged cached view is stable. The root's next isolated capture must check both cloud contour bands and new grain before accepting this trade; there is no unmeasured speedup claim.


## Final bounded comparison and visual verdict

The full production-kernel experiment passed on `apple · metal-3` in source snapshot `1790117908531-38749`, fingerprint `cec523cf6d9418e4a68addaf34835a8ff249e59dd8d8179142035cc53359fdf7`. The complete raw result is preserved beside this note in `cloud-noise-measured-2026-09-22.json`; its original is `output/lookdev-snapshots/1790117908531-38749/source/output/browser-1790117908630-38762/cloud-noise-check.json`.

| Matched 512×384 production cloud-view case | Analytic median | Compiled median | Analytic / compiled | Linear HDR RGB RMS error | Maximum error |
| --- | ---: | ---: | ---: | ---: | ---: |
| Thin, cover 0.25 | 3.670016 ms | 1.409024 ms | 2.60× | 0.00185926 | 0.01968384 |
| Overcast, cover 0.92 | 4.718592 ms | 1.671168 ms | 2.82× | 0.00028662 | 0.00793457 |

Both variants execute the actual 32-segment cloud-view shader over the same generated clear-air and diffuse products, frame, output dimensions, and deterministic quadrature. Error compares compiled-noise radiance to analytic-noise radiance, not to converged cloud transport. Nine measured batches alternate variant order after three warmup rounds. Each batch includes two dispatches. Timings exclude LUT construction, image readback, scene geometry, AA, and display. These are not whole-frame or cross-device speedups.

The isolated random-coordinate field benchmark still favors analytic evaluation: this run measured 0.003072 ms analytic versus 0.004096 ms compiled per 4096-query dispatch (the previous run measured 0.002048 versus 0.004096 ms). Those small timings are visibly quantized. The actual cloud kernel reverses that result. This evidence supports retaining compiled noise as the current default on the tested backend, with `cloudNoise: "reference"` available for comparison. The trade remains 1,149,984 bytes and nonzero field/radiance approximation error. One-time tile construction measured 0.065536 ms median in this run.

Exact binary-grid periodicity error is zero. The original translated-coordinate discrepancy remains 0.000245094; manual f32 trilinear translation drift is 0.000001431, and hardware-versus-manual filtering deviation is 0.000483215. The separately compiled GPU phase-difference expression reports zero changed phases, whereas CPU f32 emulation reported changes. Compiler reassociation and the separately compiled diagnostic prevent assigning an exact GPU phase-error bound from that CPU result. The readbacks establish a measured filtering-path discrepancy, not a portable guarantee about its implementation or precision.

The final low-sun, clearing, and zenith images in `output/lookdev-snapshots/1790117683430-38033/source/output/browser-1790117724868-38212` were inspected. Horizontal contour bands are reduced, but visible grain replaces them along cloud edges; low-sun and clearing are conspicuously noisy, and zenith cloud shapes remain too soft and featureless. **Artistic acceptance remains failed.** The faster noise representation does not resolve insufficient cloud structure or quadrature/reconstruction quality. No additional cloud feature work follows this bounded experiment, and no claim of AAA output, new atmospheric research, or world-leading performance is supported by this result.
