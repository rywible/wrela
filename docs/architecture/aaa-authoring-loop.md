# AAA authoring development loop

The alpine crossing is the shared integration scene. The river-bend shelter is a second composition made through ordinary authoring transactions over the same construction system. Both must survive source edits, saving, compiled delivery, continuous traversal, and rendered review. A successful capture is evidence that the system ran; it does not confer artistic approval.

## Construction and editing

The [construction/history system](construction-history.md) builds dimensioned masonry, arches, damaged profiles, surface palettes, deposition debris, and plant communities from shared age, moisture, exposure, contact, and damage controls. The [creature construction system](creature-construction.md) exposes anatomy, fitted clothing, and contact-aware motion recipes. Geological profiles and botanical hierarchies retain their own semantic controls rather than being reduced to anonymous meshes.

The [domain variant workflow](domain-variants.md) uses the existing candidate registry and transaction engine. A constrained request creates alternatives from one inspected revision. Review includes source differences, retained constraints, matched images, and request-to-complete-render time. Adoption is the normal undoable edit, and stale alternatives cannot overwrite newer work. Unmeasured agent token cost, manual intervention, and artistic acceptance remain unknown rather than being counted as zero.

## Realization and lighting

- Physical relief carries geometry that can be sampled within the triangle budget. Complementary shader bands carry resolved residual normals; estimated unresolved slope variance broadens roughness. Exact rest positions and normals keep the patterns attached across realization changes. Optional reference streams avoid imposing that memory cost on ordinary geometry.
- Thin foliage compiles authored needles and blade outlines into filtered occupancy fields and curved carriers. Coverage is separate from transmission. The camera and shadow paths query the same field; their sample patterns and limitations are disclosed in the thin-foliage research notes.
- Static diffuse GI uses compiled triangle queries, progressively built irradiance probes, and bounded receiver-to-probe visibility. Studio's **Bounce lighting** control enables the preview. This is one geometry bounce with a constant authored sky surrogate. Moving bodies, water, glass, and thin transmission remain outside that transport model; it is not a complete path tracer.
- The atmosphere preserves Hillaire-style transport tables, adds ozone extinction, and separates low-frequency spherical sky lighting from a camera-resolved cloud product. Procedural cloud sampling and optical approximations retain explicit quality/performance evidence.

The measured rendering-frontier module compares representations only within matching source, reference, camera, lighting, motion, resolution, adapter, and kernel domains. Missing errors or cost uncertainty cannot be silently converted into a quality certificate. A frontier from sampled views describes those views, not all possible future gameplay.

## Review and budgets

`bun tools/alpine-slice-lookdev.ts --composition=primary --profile=balanced` retains a source project, actual authoring operations, three converged live views, frame-tagged GPU measurements, residency, and budget verdicts. Repeat with `--composition=river-bend` and `--profile=portable`. The fixtures keep one renderer alive and preserve temporal history; screenshots and readback time are not mislabeled as frame rate.

The provisional portable contract is 1280×720 output, the low rendering profile, GPU p95 ≤26 ms, CPU render p95 ≤5 ms, and 128 MiB owned GPU allocation. The balanced contract is 1920×1080 output, GPU p95 ≤12 ms, CPU render p95 ≤4 ms, and 256 MiB owned GPU allocation. Both allow 192 MiB installed binary scene products. Internal rendering resolution is reported independently. Passing on one adapter does not establish broad hardware compatibility, and these budgets are development targets rather than claims about current performance.

Use the source-freezing wrapper for retained evidence:

```sh
bun tools/lookdev-snapshot.ts --capture=alpine --composition=river-bend --profile=balanced
bun tools/lookdev-snapshot.ts --capture=variants --small
bun tools/lookdev-snapshot.ts --capture=gi
bun tools/lookdev-snapshot.ts --capture=thin-coverage
bun tools/lookdev-snapshot.ts --capture=relief-probe
bun tools/lookdev-snapshot.ts --capture=scene --study=scene --composition=river-bend --small
bun run author:lookdev
```

The final art review spans silhouettes, close-ups, motion, backlighting, material relief, shores, sky transitions, and both world compositions. A game-ready verdict also requires continuous traversal and actual player interaction. Preserve failed renders and measurements: they identify where the next implementation must improve.

The algebraic, rendered and Studio evidence from this implementation pass is retained in [the priorities review](../research/aaa-priorities-2026-09-22.md). Scene previews and target-resolution budget captures are distinct: lowering preview resolution does not satisfy a runtime contract. The budget runner records `captureComplete` separately from `budgetMet`, preserves all completed frames, and exits nonzero when any target is missed.
