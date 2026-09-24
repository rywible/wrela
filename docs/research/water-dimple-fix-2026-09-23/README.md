# Short-wave dimpling correction

The fine ripple layer produced rows of similarly sized oval pits in the foreground reflection. Controlled captures disabling individual cascades isolated this pattern to the finest normal layer. Removing foam, Jacobian deformation, or local roughness variation did not remove it.

The compiler now distributes short-wave carriers across contiguous wavelength octaves and uses a narrower, wind-aligned directional distribution for the finest cascade. The broader wavelength distribution alone was insufficient; the combined change visibly reduces round cross-wave intersections. A stronger directional restriction was rejected because it looked too combed.

Only compiler coefficients change. The renderer retains 54 generated carriers, the same shader, dispatches, texture samples and allocations. The large-wave frequency/phase choices are retained; global normalization preserves requested height energy. The fine wave realization changes for existing seeds. This is a visual refinement of the finite-carrier model, not a new oceanographic model.

[Before](before.png) · [After](after.png) · [Next animation frame](after-motion.png) · [Sunset](sunset.png) · [Creek/pool](creek.png)

Validation: 24 focused tests passed, including a regression for wavelength coverage, directional energy and conserved height energy across five seeds. Both TypeScript configurations, workspace boundaries and production build passed. GPU checks cover analytic queries, filtered slopes, tile seams, origin rebasing, depth mips and history. Six ocean/creek motion cases passed with no GPU errors; maximum frozen-frame mean change was below 0.000017/255. Existing non-null-assertion lint warnings remain in the test file; no lint errors.

At 1920×1080 balanced/spatial on this Mac, water-on whole-frame GPU medians were 4.06–4.13 ms before and 4.06–4.59 ms after; p95 was 5.37–5.70 vs 5.64–6.29 ms. Spectrum median was 0.131 ms in every water-on leg. These runs have desktop-load variation and do not establish a speedup or precise cost difference. GPU memory is exactly unchanged at 7,791,116 bytes for this water scene. The change adds no runtime instructions or resources.

[Raw measurements and conformance](evidence.json) · [Motion checks](motion.json)

Reproduce with `bun tools/water-lookdev.ts --ocean --bench`, `bun tools/water-lookdev.ts --lake`, `bun tools/water-lookdev.ts --ocean --lighting=sunset`, and `bun tools/water-motion-check.ts`.
