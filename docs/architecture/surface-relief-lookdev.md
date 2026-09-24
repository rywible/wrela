# Physical surface relief review

`bun tools/surface-relief-lookdev.ts --revision=v1 --frames=12` captures the production `BrowserSceneHost` realization of two authored specimens: a vertical bark cylinder and a clipped angular stone. The source objects, camera and flat-color material recipes are identical in the smooth and relief variants; only `material.appearance.relief` changes. Shader normal strength and procedural color detail are disabled so this study cannot pass by painting noise onto a smooth surface.

The source model lives in `packages/model/src/surface-relief-lookdev.ts`, browser capture in `tools/fixtures/surface-relief-lookdev.ts`, and orchestration in `tools/surface-relief-lookdev.ts`. `MaterialReliefPanel` edits the same optional appearance recipe through complete parent replacement, so enabling relief never attempts to write into a missing nested object. Depth, groove/chip spacing and desired close-up detail spacing are presented in millimetres.

The tool writes nineteen images: paired near clay, separate bark/stone clay, separate bark/stone silhouettes, separate grazing-light views, far clay and far silhouettes, plus a forced-near control at the identical far camera. The forced control filters coarse render candidates without altering the authored material or near mesh. Source JSON is included for both variants.

`capture.json` records generated triangle/vertex counts, bounds, source compiler reviews and warnings, geometry candidates and their projected error bounds, selected representations, memory observations, frame completeness, hardware adapter and individual GPU pass timings when available. Silhouette images are measured as binary occupied pixels, recording the exact changed-pixel count against the matching smooth camera. `comparisons.json` summarizes pairs and the matched-view automatic-versus-forced-near choice.

Timing samples are short observations after residency warmup, not a throughput benchmark or proof of an optimal tradeoff. GPU-owned memory includes allocations retained by the renderer cache; selected geometry payload bytes are reported separately. A geometric displacement bound does not certify unchanged normals or radiance. Review the actual close-up silhouettes, grazing relief and matched far images before accepting visual quality. Budget-limited detail and curvature-limited depth remain visible in the compiler report rather than being presented as fully realized requested dimensions.

## Measured review — 2026-09-22

Evidence: `output/authoring-lookdev/1790113223050-23493/source/output/surface-relief-lookdev/v1/`. All nineteen captures completed on the reported `apple · metal-3` adapter without browser errors or incomplete frames. Clay and grazing comparisons show physical grooves and chipped surface variation; near silhouette masks change by 2,320 pixels for bark and 481 pixels for stone. Realized maximum depths are 20.54 mm and 12.05 mm respectively. Each near specimen uses 12,000 triangles. Both hit the detail budget: the requested maximum edge length is 22 mm, while achieved maxima are 206.55 mm for bark and 55.86 mm for stone. These results establish physical relief, not full realization of every requested detail dimension.

At the identical far camera, approximately 155 m from the specimens, automatic selection chooses both original `parametric-mesh` representations. Removing those candidates forces both displaced `direct-mesh` representations:

| Same far view | Automatic coarse | Forced near |
| --- | ---: | ---: |
| Selected triangles | 320 | 24,000 |
| Selected position/normal/index payload | 26,880 bytes | 2,016,000 bytes |
| Observed GPU median, 12 settled frames | 0.918 ms | 1.311 ms |
| Observed renderer-owned GPU allocation | 52,439,968 bytes | 55,533,120 bytes |

The coarse choice uses 75 times fewer triangles and geometry payload bytes. Projected silhouette bounds are 0.132 pixels for bark and 0.078 pixels for stone, within the 0.25-pixel allowance. The two far clay images appear similar at their tiny screen coverage; this is a distance-selection check, not proof of shading equivalence. The zero changed-pixel far silhouette result compares automatic coarse relief against the smooth baseline, not against forced near. Normal, radiance and temporal error remain unknown. The 0.393 ms observed median difference and cache-inclusive allocation figures are specific to this short capture, not a general performance or optimality claim.

**Art verdict: physical relief works; these specimens do not meet the AAA appearance bar.** Bark reads as regular long flutes, without enough interrupted growth plates or transverse fissures. Stone remains a beveled polygon block, and obvious radial triangular shading wedges converge at the front cap center in clay and grazing views. The smooth control lacks those wedges. A bounded CPU investigation found matching normals at duplicate vertices; the original cap fan instead directs error from unresolved fine detail. Even at 24,000 triangles, its approximately 40.76 mm maximum edge remains too large for the approximately 14.1 mm grit wavelength. The next bounded review should match sampling density to authored relief frequency, introduce coherent broken bark plates, and repeat these same clay, silhouette and grazing comparisons within an explicit geometry budget. The current images remain preserved as the failed quality reference.

### Cap triangulation diagnosis

Independent CPU review found continuous generated cap normals across the original fan; the visible wedges are under-resolved shape detail, not separated normal seams. At 24,000 triangles, maximum edge spacing was 40.76 mm while the finest stone band varied around 14.1 mm. A sampled comparison with the authored depth gradient measured 6.20 degrees RMS normal error. Holding topology and budget fixed and increasing the pattern scale to 0.5 m and 1 m reduced RMS to 0.38 and 0.087 degrees. These diagnostic comparisons isolate sampling sensitivity; they are not a global normal-error certificate. The compiler remains unchanged and reports the unmet spacing budget.

### Multiscale CPU comparison

`bun tools/surface-relief-cpu.ts --revision=multiscale-v1` writes `output/surface-relief-cpu/multiscale-v1/report.json`. It invokes the unchanged production relief compiler for three stone recipes at 2,000, 12,000 and 60,000 triangles. The source is one closed angular extrusion with its original planar cap fan. Only its positive-Z cap receives relief. The same 31 × 31 cell-center locations in an interior 0.4 m × 0.5 m rectangle are sampled in every run; neither recipe, sample locations nor source topology change with budget.

The report measures barycentrically interpolated mesh normals and actual triangle normals against centered finite differences of the procedural depth field. It includes RMS, mean, 95th percentile and maximum angular error, depth error in metres, finite-difference half-step convergence, actual compiler costs and limitations, geometry/residual band weights, unchanged source and recipe keys, and source-file SHA-256 identities. One reference uses the bands selected for geometry; a second uses the entire authored spectrum, including unresolved bands assigned to appearance. Both are necessary: filtering away detail can reduce sampling artifacts without reproducing the complete authored surface.

The initial multiscale run measured the following interpolated-normal RMS errors in degrees. Each cell lists low / review / high mesh budgets:

| Unchanged recipe | Selected geometry spectrum | Complete authored spectrum |
| --- | ---: | ---: |
| Fine grain, 75 mm spacing | 0.029 / 0.143 / 1.398 | 8.027 / 7.950 / 5.638 |
| Weathered, 100 mm spacing | 0.139 / 0.783 / 1.045 | 8.017 / 7.230 / 5.174 |
| Broad chip, 160 mm spacing | 0.131 / 1.316 / 0.795 | 7.096 / 4.664 / 2.166 |

Selected-spectrum error need not fall monotonically because a larger budget admits additional spatial frequencies. Full-spectrum geometric error falls across these matched budgets but remains measurable. At review budget, fine grain realizes broad-band weight 0.970 and zero middle/fine-band weights; weathered realizes weights [1, 0.260, 0]; broad chip realizes [1, 0.844, 0]. The report preserves these omissions rather than equating the selected-spectrum score with complete geometric fidelity.

These are local, static CPU measurements of planar stone caps. They do not establish whole-mesh bounds or measure rendered residual shading, lighting, silhouettes, motion, bark, GPU cost, or visual acceptance. The earlier failed images remain the visual baseline; new production lookdev captures must judge the actual combined result. `tools/surface-relief-cpu.test.ts` validates the metric against an exact sloped plane and a known 10-degree normal perturbation, then checks unchanged source recipes and identical sample locations across compiled budgets.
