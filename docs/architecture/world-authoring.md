# World and level authoring

World documents optionally own `composition`. Old documents retain their existing behavior. Semantic source is separated from realization:

- `model/world-authoring.ts`: bounded schemas, reference discovery, relationship and traversal-grade diagnostics.
- `model/world-path.ts` and `model/world-room.ts`: shared geometry and module fitting used by validation, runtime realization, and Studio.
- `authoring/world-authoring.ts`: immutable creation and duplication commands. Studio submits these through its ordinary validated transaction/undo path.
- `world/composition.ts`: deterministic realization and spatial queries, without importing runtime or Studio.
- `world/population.ts`: biome/clearance admission and stable source exceptions followed by runtime overrides.
- `world/session.ts`: nearby authored streaming interests enter the existing bounded planner.
- `world/spatial-review.ts`: actor clearance, terrain slope/step sampling, and static sightline diagnostics.
- `world/route-network.ts`: conservative route connectivity, entry access, and doorway/encounter approach diagnostics.
- `studio/world-spatial-panel.tsx`: editable top-down geometry map, review settings, and an instance exception picker.
- `studio/world-authoring-panel.tsx`: structured creation/editing controls for every composition collection.

## Supported source and realized behavior

Assemblies contain named definition references and local position, yaw, and uniform scale. Instances refer to the assembly and apply a parent transform. Duplicates share the same assembly, so edits propagate on recompilation. Generated identities use placement/member IDs and survive reordering. Optional placement grounding moves the whole group to the final terrain while preserving member heights and scale. Explicit member exceptions still own their final transform. Assembly nesting is intentionally absent.

Paths contain explicit elevation control points, optional corner rounding in meters, width, shoulder, grading flag, optional repeated surface object, spacing, and a maximum traversal grade. Zero rounding preserves the original straight-segment placement identities. Positive rounding replaces interior corners with bounded quadratic arcs, constrained to half the neighboring segment lengths and to the control-point convex hull. Arc-length sampling emits terrain flatten interventions and optional object instances; the exact same rounded polyline drives population clearance, traversal review, and the layout map. Both renderer and collision consume the resulting terrain. Grade warnings compare elevation rise to horizontal run. Corner rounding does not overshoot authored elevations. Population clearance follows the continuous corridor, independent of sampling or resident region boundaries.

Rooms contain a center, dimensions, yaw, wall module reference/width, and a door side/width. They emit wall instances with an entrance gap; procedural populations are excluded from the room footprint. The wall asset must actually have the authored module width. This is a layout generator, not an architectural solid modeler: it does not create floors, ceilings, trim, or boolean door cuts. Each wall span fits complete modules between its exact corners and doorway edges. Fractional spans overlap internal modules rather than enlarging the entrance or changing module heights. Modules wider than either doorway pier are rejected before commit. This is appropriate for plain wall modules; patterned modular walls still require dimensions chosen to avoid visible overlapping detail.

Biomes name existing population rule IDs, a radial extent, transition width, and density. Smoothstep falloff changes deterministic population admission. A rule assigned to biomes is excluded outside their union. Overlapping zones use the strongest weight, avoiding order dependence. Unassigned rules retain their original behavior.

Landmark and encounter spaces define radial population clearings and local members. Members become actual objects/characters with runtime collision and animation through normal world realization. Spaces can ground their center as a group, preserving member height relationships. Encounter spaces do not yet define enemy behavior, objectives, trigger logic, or combat pacing.

Streaming regions activate near any ordinary interest observer, deduplicate by ID, and compete by priority for the existing maximum of eight interest slots. Ordinary camera/player interests take precedence. Region radii are capped by the session terrain budget. These are terrain residency hints, not a general asset-streaming graph. Regions lose residency requests when the observer leaves.

Exceptions name an explicit instance, generated layout identity, or canonical population identity. They can remove, relocate, rotate, or rescale the placement. Runtime saved population changes supersede source exceptions. Exceptions to assemblies belong to the world placement, leaving the reusable assembly unchanged.

Explicit instances can author `grounding.offset` to follow the final terrain after composition paths have graded it. A source placement remains independent of changing landforms; an explicit position exception owns its elevation. Studio exposes this as **Follow terrain** and a height offset. This is useful for rocks, plants, and characters placed across a terrain iteration; manually fixed elevations remain available for elevated or embedded objects.

## Integration contract and budgets

Call `realizeWorldComposition(sourceWorld, sourceTerrain)` once before creating/replacing the world session and installing static instances. It returns derived world/terrain values and diagnostics without mutating source. Do not feed its derived world back into realization. Composition remains attached to the derived world so populations and streaming can read spatial intent.

Expansion rejects identity collisions and more than 256 combined static instances or terrain interventions. It does not silently truncate authored content. Room and path loops have explicit sample budgets. Relationship validation rejects missing assemblies/populations, duplicate identities, impossible door widths, invalid combined scales, and grading outside the supported terrain elevation range. It also counts generated instances, terrain grading samples, and removed exceptions before commit when given the source instance/intervention context. Asset references participate in project dependencies and validation.

Overrides of sampled paths use sample identities: inserting control points or changing sample spacing can remap those identities. Use assembly member IDs for hand-authored objects that must remain fixed across path topology edits.

## Spatial review and editing

The layout map exposes path control points, assembly locations, room centers, biome centers, landmark/encounter centers, and streaming centers. Its extent includes zone radii and room footprints. Rounded centerlines and destination access markers show the same geometry that the world realizes. Select a point and click the map to move it in X/Z, use arrow keys for meter nudges, or edit its X/Y/Z values. Structured controls also expose widths, corner rounding, dimensions, radii, rotations, group grounding, and all individual path points. Adding a point extends the previous heading by five meters. Removing an assembly removes its dependent placements in the same undoable edit; removing an entry route clears that review reference. The instance picker creates an exception with the exact generated identity and current transform; removal/position/rotation/scale controls remain visible in the exception section.

`composition.review` stores actor radius/height, maximum step height, and a sightline's explicit endpoints. Review samples the actual graded terrain along each path, checks slope and lateral step change, and checks actor width/height against transformed authored object/character bounds. Assemblies use compiled bounds per part and respect parts marked noncolliding, so a portal's broad source envelope does not falsely block its opening. Sightlines intersect transformed instance bounds continuously and terrain at bounded sampling intervals; an authored endpoint inside the intended subject is allowed. Per-part bounds still conservatively approximate swept or hollow geometry. Terrain and traversal samples are bounded to 512 per route and expose their actual sample spacing; this is not a navmesh reachability guarantee. Procedural scattered vegetation is not included in the instance-bounds diagnostic.

The optional entry-route setting drives a conservative route graph. Interior centerline crossings and gaps of at most one actor diameter connect corridors only when both routes and the junction are clear. The graph reports disconnected routes and whether landmark/encounter boundaries are reachable. Room access is evaluated at the rotated doorway, checks actor width, and samples the actual connector from an accessible route; passing near the back wall does not count as entry. A blocked route is unavailable as a whole, so this can conservatively reject a usable subsection. The report describes authored corridor access and does not imply free-space reachability or dynamic enemy behavior.

Review settings are excluded from the procedural-save generator fingerprint. They express inspection state rather than a generation change.

## Verification and remaining acceptance

Composition tests exercise rounded-corridor consistency, precise doorway edges, group grounding, entry-network intersections, disconnected destinations, room entrance access, relationship-safe removal, transforms, reorder-stable identities, instance exceptions, actual terrain-height changes, corridor and room exclusion, biome blending, persistent override precedence, entrances, encounter placement, bounded expansion, traversal diagnostics, and streaming activation. Authoring tests check immutable, schema-valid edits. Host integration tests check composition-only asset compilation/rendering, transformed placements, geometry cache invalidation, and preservation of graded roads through live terrain edits. Spatial tests cover walls, overhead clearance, rotated/scaled bounds, terrain obstructions, removal overrides, and bounded review work.

The alpine lookdev scene adds a deliberately curved approach, a gate branch stopping at the closed door, terrain-grounded pines and actor, and small authored habitat patches along the bank and verge. Its route review reports zero blocked samples on both rounded paths, a clear authored sightline, and access from the entry route to the sentinel clearing and gate landmark. Rendered acceptance is separate: the habitat still reads as scattered clumps on broad uniform ground, and the distant stand lacks the layered density of a finished AAA environment. Remaining work includes direct 3D viewport path/zone manipulation, navmesh reachability and triangle-accurate visibility/clearance analysis, pattern-aware room fitting, continuous road surface meshes and intersection treatment, independently streaming object assemblies, integrated encounter behavior, and finished content playtests.

## Continuous world traversal evidence

`bun tools/world-traversal-lookdev.ts --frames=48` keeps one real `BrowserSceneHost`, physics world, and renderer alive while following the rounded approach and gate branch at three meters per second. Simulation advances at 60 Hz; saved images are evenly spaced samples along that continuous journey. `--small` selects 640×480, and `--include-stills` also captures the existing near/gameplay/landscape views for the category gallery.

The fixture queries installed terrain triangles, downward physics rays, and three overlap spheres above the allowed step height. It records every collision probe, initial renderer/streaming readiness, any initial incomplete screenshot, bounded settling time, final completeness, CPU/GPU measurements, host and terrain resource usage, and a contact sheet. A small authored gate streaming region activates during the route. Screenshots capture the live displayed canvas so the renderer's normal LOD history is retained. Missing residency, blocked clearance, incomplete final frames, missing region activation, or renderer errors fail the report after preserving evidence.

The report distinguishes initial presentation from settled screenshots: waiting for generated products is measured and cannot disappear into an apparently successful final image. Capture overhead and waiting are not claimed as realtime frame rate. The clearance probe approximates a standing capsule; it does not replace controllable-player collision/sliding tests or room-interior traversal.

The alpine paths now instantiate an editable reclaimed-flagstone assembly with layered earth/mineral appearance. Their placement uses the same rounded source route as terrain grading and clearance. The thin slabs are visual course markings; traversal still uses the graded terrain. This makes the route visible while leaving full continuous road-surface and intersection construction as a separate capability.
