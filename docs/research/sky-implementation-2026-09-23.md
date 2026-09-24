# Sky implementation — September 23, 2026

This implements the main experiments recommended by the [reference audit](sky-reference-audit-2026-09-23.md). The [comparison board](sky-reference-audit-2026-09-23/index.html) retains the original captures and reference photographs and adds implementation captures. The result is a stronger procedural sky, not a claim that the remaining gap to the best AAA reference art is closed.

## What changed

- **Cloud composition:** environments can now author rising towers, shallow banks, and dissipating wisps. Each has position, base altitude, dimensions, rotation, density, edge erosion, and a variation seed. Broad envelopes establish the silhouette; different detail distributions describe each family. Background weather coverage is independent of these forms. The alpine environment uses all three.
- **Authoring and animation:** Studio exposes these controls in the base cloudscape and weather keyframes. Four formations per authored key can crossfade into up to eight runtime forms. Matching identities interpolate position, dimensions, density, and rotation; new or removed forms fade. A seed change is an explicit topology cut. Old documents keep their existing background behavior.
- **Distance:** a curved cloud shell replaces the flat intersection used by view rays. Rays can enter the cloud layer out to 100 km, with a bounded 32 km integration path. Three depth intervals apply atmospheric transmission and scattered light separately, improving overlap between near and distant bodies.
- **Upper clouds:** a separate, inexpensive sheet at 9.8 km combines broken coverage, directional fibers, and fine detail. Its rotation is authorable. Illumination uses atmospheric transmission at the sheet's altitude, allowing a warmer upper layer over darker low clouds after sunset.
- **Twilight:** visible sky integration uses 32 nodes, with denser angular integration for diffuse atmospheric illumination. The artificial horizon twilight residual is removed. Ground bounce now follows integrated sky energy instead of retaining a fixed fill after sunset. This corrects the bright green ground under almost-black clouds in the original blue-hour study.
- **Light in the air:** the same cloud visibility field used for cloud and terrain lighting now attenuates direct sunlight along atmospheric rays. A shell-boundary rounding fix prevents valid shadows from being discarded. Diffuse air illumination remains approximate and unshadowed.
- **Stability and cost:** the GPU reads formation data directly from uniforms rather than copying arrays through every atmosphere function. Conservative envelope bounds skip empty regions. Atmosphere updates follow changes in the shared shadow field, while view clouds retain a finer update schedule. Large upper-cloud wind jumps and formation edits invalidate history.

## Quality and cost decisions

The original 256-sample view cap failed the unchanged 0.004 RMS image-error gate after extending the cloud path. Raising only resolution would not fix that. Balanced and High now allow 384 samples for background weather, or 512 when explicit formations are present, with a 15 m target interval and bounded adaptive allocation. Balanced retains its 768 × 512 cloud view; High uses 1440 × 900. Low retains its smaller view and 64-sample budget. No claim is made that Low matches the dense reference gate.

A 60 km integration-path experiment was reduced to 32 km to bound work and improve convergence. The far entry range remains 100 km. More visible distance is useful only if it can be integrated reliably. Earlier experiments that stretched procedural noise or added strong upper-sheet streaks were rejected after visual inspection.

The dense reference checks compare integrations of the same artistic model. They establish numerical behavior for the listed views, not physical correctness of cloud scattering or photographic realism. Atmospheric multi-scattering, cloud illumination, and the upper sheet remain approximations.

## Remaining visual and hardware limits

Cloud identity is now authorable, but convincing sky composition still depends on the placement and scale of those forms. Large overhead masses can remain soft; fine distant weather can still look busy. The upper layer is a thin sheet, not a separately simulated volume. Atmospheric shafts use the bounded shared visibility field and do not cover every sun angle or moonlight condition. Cloud-flight interiors, independent middle-altitude weather, precipitation, and fluid simulation are outside this implementation.

Measurements are from the available Apple / Metal adapter. Broader ordinary-hardware performance remains to be established; the increased cloud sample budget has a real cost. Complete-frame results and the final visual evidence are recorded below.

## Final verification

The renderer and authoring tests passed: **58 tests**, plus both TypeScript configurations. The GPU runs recorded no browser/GPU validation errors. Source manifests, frozen bundle hashes, reports, and image hashes are linked in [implementation-evidence.json](sky-reference-audit-2026-09-23/implementation-evidence.json).

| Check | Result | Gate |
| --- | --- | --- |
| Nine dense integration views, including authored towers and sunset | Worst RMS **0.003020** | ≤ 0.004 |
| 1024 versus 2048 sample reference convergence | Worst RMS **0.000877** | ≤ 0.001 |
| Cached versus direct lighting/air comparison | Worst RMS **0.001337** | ≤ 0.002 |
| Sixteen motion/weather/history scenarios | Worst RMS **0.003873** | ≤ 0.004 |
| Render-origin shift with authored clouds and water | RMS **0.000000373** | ≤ 0.005 |

Motion coverage includes camera movement and turning, wind jumps, cover changes, sunset lighting, upper clouds, clear/cloud transitions, sun/moon source switching, cuts, rebasing, formation edits, animated background coverage, and rebasing with an explicit formation. The original 256-sample control remains in the dense report; the acceptance gate now evaluates the shipping 384/512-sample policy. Thresholds were not relaxed.

The field run completed against a stable working source tree. Other files changed during an initial motion run, so the final motion checks, measurements, and artwork captures use a frozen copy of the completed source. The original audit remains separately preserved.

## Complete-frame measurements

These are GPU timestamps for complete submitted frames, not an estimate made by adding overlapping pass durations. Each moving/weather study has 60 samples at 1024 × 768 output with Balanced clouds.

| Study | GPU median | GPU p95 | Maximum |
| --- | --- | --- | --- |
| Moving camera, fixed weather | 14.61 ms | 29.88 ms | 31.46 ms |
| Advancing weather | 17.76 ms | 26.61 ms | 29.10 ms |

The static alpine world completed all three 1920 × 1080 views, with materials, vegetation, creature, and water. Its GPU p95 was **20.19–22.22 ms**, exceeding the existing **12 ms** target. Extraction/submission CPU p95 was **7.8–8.4 ms**, exceeding **4 ms**. No atmosphere rebuild occurred in those measured static frames; these misses cannot be attributed to repeated cloud generation. No controlled before/after whole-world benchmark was performed, so this is not a claim of a world-performance regression or improvement.

The correct conclusion is that the sky changes are implemented and pass the listed numerical checks, while the full world's performance target remains unmet. Cold builds, abrupt formation edits, and moving-cloud redraws cost more than cached static views. Broader hardware validation and further scene optimization remain necessary.

## Visual review

All seven High views were reviewed alongside the original audit: side, back, and front light; storm; golden hour; blue hour; and zenith. Three Balanced images and two controls using the original cloud parameters are included on the board. The unchanged +4.5 EV blue-hour control is especially useful: the bright horizon and green terrain imbalance are corrected without hiding the issue through exposure reduction.

The new tower provides a distinct focal silhouette. Fine detail is strongest on that form and can still appear granular compared with the softer background banks. Upper clouds are now separately visible and illuminated, but their procedural directional pattern remains recognizable. Zenith softness remains unresolved. The alpine captures demonstrate integration with the actual world; they do not establish finished AAA art quality for the surrounding environment.

## Editor acceptance

Six mounted Studio checks passed: adding all three cloud kinds, changing upper-cloud rotation, removal, undo, redo, and editing a weather key without modifying the base cloudscape. The check exposed and fixed a missing transaction allowlist for optional cloud properties on old documents. Starting a cloudscape on an old environment also initializes visible coverage; base cloud cover is now explicit in the panel. Two new transaction tests and sixteen existing transaction checks passed. These authoring-path fixes happened after the frozen visual capture and do not change its renderer. See [editor evidence](sky-reference-audit-2026-09-23/implementation-cloud-authoring.json).
