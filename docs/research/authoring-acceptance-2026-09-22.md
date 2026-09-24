# Authoring implementation and acceptance review

This is the earlier implementation baseline. Later compiler, construction, coverage, sky and bounded-GI work is recorded in [the priorities implementation review](aaa-priorities-2026-09-22.md); the results below retain their original source fingerprint and scope.

The eight authoring domains now have expanded source schemas, editing operations, compiler/runtime integration, focused Studio controls and executable review workflows. Ownership is mapped in [the architecture overview](../architecture/authoring-system.md). This is a functional implementation milestone. **The requested AAA visual bar is not met, and full delivery against that bar remains open.**

## Frozen rendered evidence

[Review gallery](../../output/authoring-lookdev/1790113223050-23493/index.html) · [manifest and verdicts](../../output/authoring-lookdev/1790113223050-23493/report.json)

The complete run contains 116 PNGs and a motion video across all eight categories, using source fingerprint `bf489b1eb58bab398e961af7396661bacf1e492d1c92443c2e49dd7eeb02b5d8`. Every category completed on Apple Metal 3. Actual images and motion contact sheets were inspected; all eight received a `needs-work` visual verdict. A subsequent cache-identity correction adds the configured relief triangle budget to product identity; it changes no default captured geometry and passes focused tests.

| Category | Verified advance | Remaining visual failure |
| --- | --- | --- |
| Creatures | Landmark-bound fitting, independent material ownership, coherent cloth during sampled poses | Primitive anatomy/face and unfinished garment construction |
| Assemblies | Dimensions, anchored sockets, joints, collision/save/replay and rendered gate motion | Uniform masonry, limited edge erosion and ground contact history |
| Vegetation | Growth/edit coherence, explicit needles, branch/leaf motion, seeded communities and actual distant selection | Sparse noisy coverage, costly near geometry and unverified temporal coverage |
| Geology | Directed landforms, protected routes, finite cave/overhang geometry and preview | Synthetic rounded shells, obvious strata seams and weak rock/terrain integration |
| Materials | Spatial coatings, bounded leaf diffuse transmission and actual geometric bark/stone relief | Repetitive grooves, under-resolved stone triangulation shading, insufficient fine detail and optical approximations |
| World | Rounded graded routes, grounded assemblies, access review, streaming and continuous traversal | Uniform ground, sparse habitat layers and unfinished roads/banks |
| Performance | Clip operations, transition/scrub, contacts, facial/events and recorded motion | Weight transfer and final character performance remain unfinished |
| Environment | Hillaire-style atmosphere, procedural volume clouds, weather/water coherence and rebase stability | Soft cloud structure, muddy low sun and stylized water reflections |

## Executable verification

- Full source-only test run: **776 pass, 0 fail**, 1,526,361 assertions across 157 files. Later nonvisual relief identity changes pass their focused tests.
- Studio, standalone player, worker and offline snapshot build successfully (14 assets). The 200-definition accepted-edit benchmark records p50/p95 15.47/16.10 ms; this measures accepted edits, not geometry compilation or full visual update latency.
- Both TypeScript targets and dependency boundaries pass. Formatting/lint check passes with 100 existing non-null-assertion warnings; it is not warning-free.
- Real Studio: twelve edit/undo/redo checks plus save/reopen pass. Relief selection exercises the mounted React change handler because Bun's browser interface does not expose native option selection; numeric edits and buttons use native input.
- Surface hardware probes: 15 visible feature responses and three opaque-coating invariants pass, with no renderer/browser errors.
- Thin foliage: 135 CPU/GPU comparisons, maximum discrepancy `6.27e-8`, bounded diffuse R+T and exactly zero occluded direct transmission. This does not certify the complete leaf BSDF or canopy transport.
- Environment hardware: rendered rebasing difference is zero; weather-driven waves and river geometry remain coherent.
- Continuous traversal: 24 complete settled frames, collision ready, zero blocked frames, and streaming activation. One initial incomplete frame and approximately 1.007 seconds total settling remain visible in the report. Captured GPU median/p95 are 13.763/25.100 ms at 1024×768; these are sampled observations, not an isolated sustained-frame-rate benchmark.
- Walk contact audit: maximum planted slip 1.613 mm; maximum contact residual 23.490 mm. Artistic gait quality remains a separate review.

## Compiler advantage, demonstrated within its domain

The physical-relief study generates close geometry and retains the original as a distant candidate. At the same distant camera, automatic selection uses **320 triangles** compared with **24,000** forced near. Projected correspondence bounds are 0.132 px for bark and 0.078 px for stone, under the 0.25 px geometric budget. Short twelve-frame GPU medians are 0.918 ms automatic and 1.311 ms forced near. The compiler reports unresolved detail rather than claiming the requested 22 mm tessellation spacing was achieved.

This demonstrates one useful representation choice. It does not establish a universal Pareto frontier: normal, radiance, coverage and temporal errors are not certified, timings depend on adapter/workload, and cached GPU allocations do not immediately shrink when distant geometry is selected. Near geometry can also win when matched measurements favor it; this behavior is tested.

[Thin-field research](thin-field-foliage-2026-09-22.md) and [Hillaire research](sky-lighting-research-2026-09-22.md) preserve primary references and measurable next experiments. Current lighting has atmospheric multiple scattering, sky illumination, direct shadows, supported analytic primitive intersections and screen-space water transport. General scene-bounce GI and hardware ray-traced lighting are absent. No improvement over published research or claim of world-leading quality/performance has been established.
