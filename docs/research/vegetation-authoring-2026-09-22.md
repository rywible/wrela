# Vegetation authoring and visual review — September 22, 2026

The requested vegetation authoring controls now operate through source documents, compilation, Studio, rendering and cooked delivery. This includes species and maturity, hierarchical growth, pruning and damage, leaf/needle density, branch sway, leaf flutter, and deterministic stand variants. The current alpine visual result **does not meet AAA acceptance**.

## Preserved image evidence

Three actual production-renderer experiments establish the remaining representation problem:

1. Broad fan geometry retained crown coverage but looked like large triangular broad leaves at close distance. Evidence: `output/authoring-lookdev/1790110916847-18149/source/output/browser-1790110922147-18241/plant-detail.png`.
2. Individual thin needles at 24 per shoot, 6.5 cm length and 2.5 mm width produced a nearly bare canopy with dashed/pinpoint sampling. Evidence: `output/lookdev-snapshots/1790111437792-20310/source/output/browser-1790111437899-20326/plant-detail.png`.
3. The preceding bounded triangle preset uses 48 needles per shoot, 8.5 cm length and 5.5 mm width. It restores some canopy coverage and makes the intended narrow needle shape visible, but close foliage remains jagged/noisy, and gameplay crowns remain sparse and irregularly sampled. The width is an explicit art-directed needle-tuft approximation, not measured botanical anatomy. Evidence: `output/lookdev-snapshots/1790112313983-22222/source/output/browser-1790112314078-22228/plant-detail.png`, `stand-gameplay.png`, `stand-far.png` and `vegetation-lookdev.json`.

More triangles alone did not resolve the mismatch between authored thin geometry and pixel coverage. The next useful experiment is a bounded comparison of filtered needle/bundle coverage against a high-sample reference. It should measure projected-area stability, close shape, shadows, motion and cost separately. Material transmission must remain separate from geometric coverage. Increasing needle counts or widths again is not the next step.

## Geometry and captured-frame cost

That triangle source has 1,201 branches. The review realization contains 49,765 needles, 199,060 foliage vertices and 90,924 wood vertices, totaling 205,119 triangles. The interactive realization contains 24,866 needles and 97,174 triangles. Both remain below their declared work budgets and produce no truncation diagnostic. Interactive sampling widens retained needles in proportion to the sampling stride; this preserves a simple projected-area estimate, not overlap, shadowing or a proven perceptual error bound.

The following measurements come from **individual captured frames**, at 1024 × 768 on the reported `apple · metal-3` adapter. They are observations, not a sustained-frame-time benchmark or broad-hardware performance acceptance.

| Capture | Submitted source triangles | Renderer triangles | Recorded GPU time | Distant surfaces |
| --- | ---: | ---: | ---: | ---: |
| Plant beauty | 205,121 | 205,121 | 3.015 ms | 0 |
| Plant close detail | 205,121 | 205,121 | 6.226 ms | 0 |
| Stand near | 2,165,465 | 1,923,801 | 14.352 ms | 0 |
| Stand gameplay | 2,161,369 | 1,934,041 | 17.367 ms | 0 |
| Original far stand | 2,161,369 | 1,958,617 | 13.763 ms | 0 |

The stand includes nine tree instances and streamed neutral ground, so the total is not nine times one tree alone. Gameplay cost remains material even though the crown occupies relatively few pixels. The 64 MiB CPU vegetation cache is bounded; final source sharing is tested, but a large forest still requires representation and residency budgeting.

## Corrected distant review

The original far camera at `[36, 7, 66]` did **not** select distant geometry. Its image cannot substantiate distant-representation acceptance. The review fixture now uses `[96, 12, 168]`, still inside the world's 240 m visual interest range, and fails the capture if `detailSurfaces` remains zero. This camera crosses the projected-size threshold with room for wind bounds and hysteresis.

The corrected GPU capture succeeds and selects **18 distant surfaces**, covering both woody and foliage surfaces of all nine trees. Evidence is saved in `output/authoring-lookdev/1790113223050-23493/source/output/browser-1790113227143-23583/stand-far.png` and `vegetation-lookdev.json`. The production CPU selector independently projected the actual stand bounds at 34.46–43.55 pixels and selected the same 18 surfaces.

| Final-suite capture | Submitted source triangles | Renderer triangles | Recorded GPU time | Distant surfaces |
| --- | ---: | ---: | ---: | ---: |
| Stand near | 2,165,465 | 1,923,801 | 14.942 ms | 0 |
| Stand gameplay | 2,161,369 | 1,934,041 | 18.481 ms | 0 |
| Corrected far stand | 2,163,417 | 248,690 | 2.884 ms | 18 |

The far frame renders 248,690 triangles versus gameplay's 1,934,041, approximately 87.1% fewer across the complete frame. Ground streaming and visibility also differ between those cameras, so this is not an isolated vegetation-LOD benchmark. Recorded GPU memory increases from 207,478,572 bytes at gameplay to 218,481,192 bytes at far distance; selecting cheaper geometry does not imply that previous buffers were evicted. GPU times remain single captured-frame observations on the same reported adapter, not a sustained performance result.

Visual inspection of the corrected far image confirms that the stand remains visible and the distant path is exercised. The crowns occupy only a small patch of the image and still read as sparse, stippled shapes. This verifies selection and a lower rendered triangle count; it does **not** establish preserved coverage, stable motion, matched silhouettes, or AAA visual quality. A full-versus-distant comparison at the same camera and a filtered-coverage experiment are still required for that acceptance.

## Functional verification

The focused tests cover all six species, stable subtree pruning, edited trunk/branch attachment, bare descendants, total leaf loss, conifer hierarchy and density, winding and shading agreement, branch/leaf motion bounds, source/cache identity, cooked motion round trips and rejection of malformed attributes, maximum cookable geometry budgets, deterministic mixed seed/maturity stands and actual Studio stand rendering. Twenty focused tests pass, along with both TypeScript configurations. These checks establish functional correctness for the covered cases; they do not establish artistic quality.


## Implemented filtered-coverage experiment

The subsequent implementation replaces individual needle triangles with three curved crossed ribbons per source shoot and a compiler-generated scalar coverage field. The field is derived from the same deterministic tapered needle source, including dimensions and damage. The current source narrows needles from 5.5 mm to 3.2 mm and raises semantic count from 48 to 144 per shoot; geometry no longer scales linearly with individual needle count. The measured primary review geometry is 47,562 triangles, versus the preceding 205,119 triangles. That is a geometry reduction, not a complete-frame speedup claim.

The first production image is preserved at `output/lookdev-snapshots/1790115406258-29686/source/output/browser-1790115406365-29696/plant-detail.png`. It preserves crown mass with fewer triangles but still has severe black stippling. It fails visual acceptance. The single-sample hashed mask is the principal visible sampling problem; lighting and needle albedo also require review under the current physical sky.

The corresponding hardware coverage probe is preserved at `output/lookdev-snapshots/1790115406258-29686/source/output/browser-1790115512935-30100/thin-coverage-lookdev.json`. It uses the production coverage shader and exact semantic triangles supersampled 16× in each image dimension. Across nominal needle widths 0.1, 0.25, 0.5, 1, 2 and 4 pixels, plus translated and oblique projections, absolute image-average coverage bias is at most 0.0038. At 0.5 pixels, mean absolute pixel error is 0.1210 and the image visibly breaks into speckles. Low mean bias is therefore insufficient for artistic or temporal acceptance.

The next implemented path uses four-sample alpha-to-coverage with the same scalar field. Its camera output must be compared against the hashed baseline and reference before acceptance. The shadow path remains a filtered single-sample hashed mask; this is not a multisample shadow-map or a continuous transmittance solution. The coverage probe now reports both methods. The production plant/stand recipe explicitly requests MSAA; the renderer default temporal policy is unchanged.

Generic grass, fern, oak, birch and shrub realizations now use species outlines and compiled scalar fields too. Their near/far realizations retain identical leaf populations and fields while reducing curvature. Held-out grass/fern/oak source variants are available through `--heldout`. Automated tests verify area mip conservation, perpendicular needle width, deterministic field damage, near/far coverage identity, cooked round trips and malformed data rejection. They do not certify motion stability, shadow noise, multi-layer overlap, species realism or AAA output.


## Four-sample coverage result

The actual four-sample comparison is saved at `output/lookdev-snapshots/1790116218493-32469/source/output/browser-1790116218585-32484/thin-coverage-lookdev.json`. All ten cases completed without GPU errors on the reported Apple adapter. Absolute mean coverage bias is at most 0.001032, compared with the hashed baseline maximum 0.003798. At 0.5-pixel needle width, mean absolute pixel error falls from 0.12103 to 0.05247, a 56.6% reduction. At 0.1 pixels it falls from 0.13794 to 0.06474; at 2 pixels, from 0.05162 to 0.02068. The translated and oblique cases improve too.

Visual inspection of the 0.5-pixel comparison confirms that four-sample coverage restores recognizable repeated needle-spray structure where the binary baseline looks like scattered dots. At 0.1 pixels the reference is already an aggregate; the four-sample result retains its vertical pattern with visible residual stipple. Severe average-opacity loss was not observed on this adapter. This is evidence for the bounded coverage change, not evidence that four samples eliminate all aliasing.

The test is a single scalar layer with subpixel translation or an oblique projection, not a full forest. It does not validate multi-layer sample correlation, real canopy scattering, shadow-map noise, frame-to-frame motion stability or ordinary-hardware performance. The production camera can use four samples; actual directional shadow maps still use one hashed sample. A reference column represents 16× supersampling per axis and is not a practical runtime cost target.


## Overlapping-canopy failure and production correction

The subsequent full production capture at `output/lookdev-snapshots/1790116291579-32648/source/output/browser-1790116337852-32737/` exposed what the single-layer probe missed. `plant-detail.png` shows a much thinner, skeletal crown despite recognizable fine needles. Hardware alpha-to-coverage masks can correlate across overlapping low-opacity layers; a one-layer area test cannot establish aggregate canopy correctness. The albedo and shading-normal diagnostic captures are also preserved. The compiler vertex colors are neutral multipliers in [0.88036, 0.999998], so no duplicate baked material albedo was found in this path.

The same run recorded 154.075 ms for plant detail and 345.440 ms for stand gameplay at 640×480, despite only 47,564 and 522,868 renderer triangles respectively. These are single capture measurements, not a sustained benchmark; they are nevertheless a severe regression requiring correction. Moving coverage discard after the expensive shade function to satisfy derivative uniformity made empty ribbon overdraw costly. Atmosphere cost changed concurrently and must be separated in the final timing analysis.

The production correction resolves occupancy in a cheap thin-only depth pass using independent source-local per-sample thresholds. The color pass uses exactly the same vertices/instances with equal-depth testing and no alpha discard, enabling rejection of hidden samples before material and lighting work. The opaque scene loads the coverage depth; holes still admit the background. The reference tool now includes eight overlapping sprays at 0.1, 0.5 and 2 pixels, with separate hardware-A2C and independent-mask columns. This correction still requires the complete frozen production rerun; no performance or visual acceptance is inferred from CPU tests.

Held-out captures at `output/lookdev-snapshots/1790116291579-32648/source/output/browser-1790116291689-32659/` show grass, fern and oak with readable green surfaces and actual cast shadows. Their morphology remains stylized: fern leaflets are broad and uniform, oak branching sparse and angular. These captures establish that the shared blade coverage path renders those species; they do not certify a reference-matched flora library.


## Final overlap evidence and production review

The final 13-case hardware probe is preserved at `output/lookdev-snapshots/1790117006674-34204/source/output/browser-1790117065187-34541/thin-coverage-lookdev.json`. It compares the identical semantic source and 16× supersampled triangle reference with one hashed sample, hardware four-sample alpha-to-coverage, and the production independent four-sample mask. All cases completed without reported GPU errors.

| Eight overlapping sprays: nominal needle width | Reference mean coverage | Hardware A2C mean coverage | Independent mean coverage | Hardware A2C pixel MAE | Independent pixel MAE |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0.1 px | 0.424311 | 0.152696 | 0.453443 | 0.283312 | 0.185772 |
| 0.5 px | 0.408776 | 0.154596 | 0.377211 | 0.263218 | 0.118643 |
| 2 px | 0.272802 | 0.210121 | 0.265609 | 0.079482 | 0.052654 |

The 0.5-pixel image shows why the production change matters: hardware A2C leaves the aggregate much too transparent, while independent sample masks restore most of its visual mass. It is not lossless. At that width, independent coverage remains about 7.7% below the reference; at 0.1 pixels it is about 6.9% above. Single-layer independent masks are noisier than hardware A2C: 0.5-pixel MAE is 0.06960 versus 0.05247, and the 0.25-pixel independent mean bias is −0.00807. The selected production method trades some one-layer noise for substantially better overlapping-canopy behavior.

The corresponding complete production depth-pass capture is preserved at `output/lookdev-snapshots/1790116760986-33845/source/output/browser-1790116761105-33857/`. All eleven frames rendered completely. The close-up restores fuller foliage around the woody shoots but still shows strong stochastic stipple. Gameplay stand silhouettes are coherent; this does not establish AAA close-up quality.

| Depth-pass production capture, 640×480 | Renderer triangles | Recorded GPU time | Distant surfaces |
| --- | ---: | ---: | ---: |
| Plant beauty | 47,564 | 15.466 ms | 0 |
| Plant detail | 47,564 | 49.742 ms | 0 |
| Stand near | 512,628 | 97.976 ms | 0 |
| Stand gameplay | 522,868 | 40.894 ms | 0 |
| Stand far | 250,220 | 13.566 ms | 18 |

These are individual captured-frame observations on the reported Apple Metal adapter, not a stable benchmark. They improve substantially over the failed A2C production run, but do not satisfy ordinary-hardware performance acceptance. The snapshot predates the final removal of obsolete coverage work from ordinary opaque fragments. Atmosphere work changed concurrently, so the timing delta cannot be attributed solely to the foliage depth pass.

The production recipe explicitly uses four samples and captures isolated wind times. Neither these images nor the overlap probe establishes continuous-motion temporal stability, convergence of the renderer's default temporal accumulation, cross-hardware mask behavior, reference-matched flora, or AAA quality. Those remain explicit acceptance limits; no unobserved temporal improvement is claimed.

## Final coarse-footprint correctness fix

The final production-shader rerun and its measurements are preserved in the [coverage probe report](../../output/lookdev-snapshots/1790117683430-38033/source/output/browser-1790117743594-38298/thin-coverage-lookdev.json). All thirteen image cases and nine coarse-footprint compute cases completed with no reported GPU errors.

The previous footprint blend could hash the same spatial cell at both adjacent levels when the footprint exceeded the entire coverage field. Applying the independent-uniform CDF to these identical values produced a biased threshold distribution; the focused regression demonstrates acceptance above 0.21 for requested coverage 0.1. Hashes now include their absolute footprint level. The upper hash of one level remains the lower hash of its neighbor, preserving continuity while removing that deterministic correlation.

The GPU compute probe directly exercises the production hash and CDF at footprints 256, 1,024 and 65,536, each with blends 0.25, 0.5 and 0.75. It evaluates 65,536 deterministic source salts per case and coverage thresholds 0.01, 0.1, 0.5 and 0.9. Maximum absolute coverage bias is **0.003662**; sampled CPU/GPU threshold disagreement is at most **1.02 × 10⁻⁷**, and adjacent-level endpoint disagreement is at most **5.96 × 10⁻⁸**. These measurements validate the covered hash distribution and continuity cases, not whole-canopy rasterization or temporal stability.

| Final eight-layer image case | Reference mean coverage | Independent mean coverage | Independent pixel MAE |
| --- | ---: | ---: | ---: |
| 0.1 px | 0.424311 | 0.458732 | 0.185314 |
| 0.5 px | 0.408776 | 0.379274 | 0.123384 |
| 2 px | 0.272802 | 0.266238 | 0.051597 |

The final 0.5-pixel aggregate retains about 92.8% of reference coverage, compared with hardware A2C's 37.8%; its pixel MAE remains lower than hardware A2C's 0.263218. Residual error is substantial: the 0.1-pixel aggregate exceeds reference coverage by about 8.1%, and single-layer 0.5-pixel independent MAE is 0.074034 versus hardware A2C's 0.052471. This fix removes a concrete coarse-footprint correctness bug; it does not establish AAA visual acceptance, continuous-wind convergence, sustained performance, or cross-hardware behavior. The earlier full plant/stand images and timing tables precede this hash correction and remain labeled observations of those snapshots.
