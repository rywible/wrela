# Water surface refinement — 23 September 2026

The pitted look had two visible contributors in controlled captures: circular foam breakup and discrete fine-wave normal highlights. Removing either in isolation showed its contribution.

Changes:
- Generated fine-cascade normal retention is now 1 / 0.7 / 0.25. Removed ensemble slope energy contributes to roughness; the compiler calculates per-band variance once. Wave heights, Jacobians, collision queries and fluid state are unchanged.
- Foam noise is aligned to authored current or wind and its contrast is reduced. Foam simulation, residence time and advection remain intact.
- Three variance values and foam direction occupy previously unused header lanes. No extra textures, texture samples or buffer allocation.

Matched 1280×720 ocean and quiet-pool captures at t=5 and t=5.35 confirm the visual change. The stronger initial smoothing and direction-only experiments were not used. The retained fraction is an artistic shading closure, not a claim of an exact optical solution.

12 targeted tests, both TypeScript configurations, workspace boundaries, production build and GPU conformance passed. GPU shading parity now checks the declared retained-band model; physical height/Jacobian checks remain against the original full spectrum. Snapshot timings are not a speedup claim.

[Before/after](dimple-refinement.html) · [GPU checks](dimple-refinement/conformance.json)
