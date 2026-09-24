# Authoring system and visual acceptance

Wrela's eight authoring domains share versioned documents, validated transactions, undo/redo, persistence, compiler products and the Studio/runtime renderer. Domain schemas and mathematical invariants live in `packages/model`; editing operations live in `packages/authoring`; geometry realization lives in `packages/compiler`; playback and collision live in `packages/runtime`; focused Studio panels compose those services. Review tools use those same products rather than illustrative substitutes.

## Domain map

| Domain | Maintained workflow | Detailed contract |
| --- | --- | --- |
| Creatures and characters | Anatomy, measured proportions, landmark-bound clothing and mounts, grooming, appearance, expression and motion review. Landmark edits refit dependent geometry atomically; independent material ownership is preserved. | [Creature coherence](creature-coherence.md), [implementation](creature-authoring-implementation.md) |
| Props, architecture, machinery | Dimensioned profiles and sweeps, bevels, repeats, socket constraints, hierarchical joints and transmissions, wear and geometric clearance review. Recorded joint commands participate in collision, replay and saved state. | [Assembly authoring](assembly-authoring.md) |
| Vegetation | Species, age, growth hierarchy, pruning, damage, canopy and explicit needle/leaf geometry. Independent branch sway and leaf flutter carry through cooking, picking and conservative visibility bounds. Seeded mixed-age stands have close/distant reviews. | [Vegetation authoring](vegetation-authoring.md) |
| Terrain and geology | Ridges, directed drainage, talus erosion, strata, cliffs and finite cave/overhang outcrops. Authored route elevation profiles protect body-width traversal; publication and transient preview remain explicit. | [Geology authoring](geology-authoring.md) |
| Materials | Layered spatial masks, substance parameters, weathering, wetness, dirt, damage and filtered detail, physical bark grooves and stone chips. Coatings obscure substrate optical effects; appearance remains stable across rebases. | [Material authoring](material-authoring.md), [physical relief](surface-relief.md) |
| World composition | Assemblies, curved graded paths, exact room entrances, biome boundaries, landmarks, encounters, streaming and local exceptions. Grounding preserves group relationships; connectivity and clearance review preserve player routes. | [World authoring](world-authoring.md) |
| Animation and behavior | Duplicate/trim/retime across body, face, contacts and events; transition audition, locomotion, interaction alignment and full-window contact diagnostics. Explicit zero-blend scrubbing samples the requested clip exactly. | [Performance authoring](performance-authoring.md) |
| Lighting, atmosphere, water | Weather/time sequences, room-shaped lighting zones, exposure/color, procedural atmosphere/clouds, river geometry, shore response and weather-driven waves shared by rendering and physics. | [Environment authoring](environment-authoring.md), [Hillaire research audit](../research/sky-lighting-research-2026-09-22.md) |

The detailed contracts document bounded algorithms and their limitations. A convex sweep is not a general CAD modeler; a finite cave outcrop is not infinite volumetric terrain; talus sculpting is not hydraulic sediment simulation; environment-only glass is not scene refraction. These distinctions matter when choosing the representation for a game.

## Reproducible look development

Run `bun run author:lookdev` to capture all eight categories sequentially on a hardware WebGPU adapter. The command writes `output/authoring-lookdev/<run>/index.html` and `report.json`, plus an exact source snapshot, source fingerprints, authored projects, per-category logs, capture metadata, still images and motion evidence. Opening the HTML gives a review gallery with category criteria and full-resolution frames. It keeps technical completion separate from visual acceptance. A successful render never grants artistic approval.

Use `--study=vegetation,materials` for a focused iteration. Partial runs identify missing categories. `bun tools/lookdev-snapshot.ts --capture=studio` runs the real Studio edit/undo/redo/save/reopen check on frozen source. Named snapshot captures also include `character`, `terrain`, `materials`, `surfaces`, `foliage`, `relief`, `vegetation`, `environment`, `traversal` and `performance`. Material review includes thin-leaf optical probes and matched smooth/physical-relief captures. A machine-wide GPU lease prevents overlapping Wrela verification runs.

CPU checks use explicit paths (`bun test ./packages ./apps ./tools`) so historical `output/*/source` snapshots are not accidentally selected as tests. `bun run check` verifies both TypeScript targets, dependency boundaries and formatting. `bun run perf:author` measures accepted-edit latency on the 200-definition fixture; capture timing is not a substitute for sustained gameplay performance on target hardware.

## Acceptance bar

Review each subject in silhouette/clay, beauty, close view, gameplay distance and motion where appropriate. Review the composed world as well as isolated assets. Failures found in rendered output must become concrete source/compiler/runtime changes and then be recaptured. Continuous traversal must check residency, collision, visible transitions and frame time in addition to still images.

The current tools have executable source-to-runtime workflows across all eight domains. The current authored content has **not passed the AAA visual bar**. Anatomy refinement, natural rock integration, dense plant communities, surface history, cloud detail and sustained gameplay performance require continued scene-specific review. Hillaire is the existing atmosphere foundation and a reference for measured improvements, not a claim that this stack exceeds published research or other engines.

## Compiler-selected representations

Authored intent remains independent of its realization. Physical surface relief supplies a near mesh and the immutable original coarse mesh; the renderer can choose between them under a projected geometric error budget and comparable adapter-specific cost evidence. Without comparable timings it reports a work estimate. Wind-driven relief uses a separately labeled size heuristic, while vegetation retains its own distant representation. Unknown optical and temporal errors remain unknown: a geometric bound is not a bound on normal variation, leaf coverage, transmitted light or image appearance.

The [thin-field research audit](../research/thin-field-foliage-2026-09-22.md) preserves coverage experiments and their limits. Current lighting includes atmospheric multiple scattering, sky illumination, direct shadows, analytic primitive intersections, screen-space water transport, and an optional bounded static diffuse bounce cache using compiled triangle queries. The bounce cache excludes moving geometry and transmissive transport; it is not full scene path tracing or hardware ray tracing. The [AAA development loop](aaa-authoring-loop.md) defines the shared compositions, agent variant workflow, measured representation frontier and hardware budgets. Any claim of a Pareto improvement must state its scene, quality metrics, hardware, timing distribution and memory budget; there is no established universal optimum.
