# Sanctuary terrain recipe: minimum production plan

2026-09-12. Design only; no implementation, build or new experiment. **Decision: retain the current terrain for existing saves; develop constraint-grown terrain as an explicitly created, separate world. No in-place migration.**

## Evidence and present contract

The [retained research](SANCTUARY_CONSTRAINT_BIOMES_RESEARCH.md) supports global irregular drainage topology. I inspected its [geometry comparison](../../.build/sanctuary-native-20260912/constraint-biome-graph-study/delaunay/run-01/graph-comparison-research.png): Delaunay removes most long rectangular tributaries, but fine zigzags and fragmented potential-basin dots remain. Channel eighth moment fell from 1 to .0165, with zero uphill spill edges and exact same-environment replay. This establishes neither continuous banks nor lake occupancy. Soil colors must not be used as evidence of geometry quality.

Current contracts matter more than the experimental island outline:

- `Terrain.swift` has no world seed/version. It blends radial biome weights, analytic summits, bounded route grading and authored waterbeds, preserving the old cabin height inside its established rectangle.
- `BiomeGeography.swift` has geography version 2, eleven fixed landmark IDs/XZ coordinates, route IDs and 32 × 32 km bounds. Those are gameplay identities, not the save schema.
- `ExpeditionStore.swift` reads schemas 1–3, writes 3, limits documents to less than 1 MB and protects atomic primary/backup saves. `Expedition` has no terrain descriptor. Its seed initializes creatures, not terrain. Layout's placement seed 82317 is another independent contract.
- `SanctuaryWorld.init` currently constructs `SanctuaryLayout` before loading the controller. Layout constructs `Terrain()` internally. Restore validates camera support against that terrain. All three must resolve the selected source before any elevation reconciliation.
- Garden edits have stable patch IDs, ordered terrain operations and revisions; up to 128 contributions, 16 m radius and cumulative ±12 m height offset. Boulder displacements reference stable feature IDs. Neither may be reapplied over a silently changed base.

Historical v3 files contain no generator fingerprint: exact reconstruction of every earlier executable's terrain is impossible from those saves alone. Compatibility here means retaining the currently qualified v3 terrain/source behavior and all stored state, not claiming recovery of unidentified historical code.

## Minimal source and representation boundary

Keep `Terrain` as the game-facing facade. Proposed `SanctuaryTerrainRecipe` is immutable, Codable and validated: generator ID/version, independent terrain seed, bounds, constraint set/version and canonical content hash. Constraints comprise uplift bands, fixed outlets, protected landmark footprints/elevation intervals, route corridors and explicit retained lakes. Named geography remains a separate stable contract; generated suitability may vary continuously without renaming discovery IDs.

`SanctuaryTerrainCompiler.compile(recipe:) throws -> CompiledSanctuaryTerrain` runs at explicit world creation/loading, outside ticks and mesh streaming. The result is immutable packed arrays plus spatial indices. The facade supplies height/gradient and signed water fields to movement, layout, meshes and studies. No game state moves into FieldEngine or Soundstage. FieldCore owns reusable metre-valued interpolation, bounded curves and deterministic graph math; SanctuaryContent owns uplift/drainage rules, basin policy, recipes and persistence. Project owns mesh/material representation. Do not make SanctuaryContent import FieldCompiler just to obtain a graph.

The eventual compiler sequence is bounded: globally seeded sites with stable IDs and prescribed boundary outlets → quality-checked irregular triangulation → analytic uplift plus fixed-count drainage/erosion passes → route/landmark constraint qualification → continuous bed/channel reconstruction and water qualification. Any terrain correction that changes receivers reruns drainage before acceptance. No accepted pass may secretly lower a mountain through a narrow high-grade blend. Long-term moisture/suitability can follow this source later; live weather never regenerates its mountains or placement IDs.

A production planar Delaunay utility is currently absent. Choose a deterministic, tested implementation in the reusable mathematical layer before enabling the generated recipe. Do not ship Python/SciPy, imported research arrays, jittered D8 connectivity, or a novel large triangulator inside this first integration change. Pin tie-breaking and predicate behavior; reject invalid/over-budget topology rather than falling back to a different world.

## Continuous queries and explicit water policy

Triangular barycentric height is C0 and useful as a numerical starting representation, but its piecewise-constant gradient is not sufficient final bank/near-ground acceptance. Reconstruct channel centerlines as bounded curves that retain junction/outlet IDs, monotone downstream surface elevations and shared junction values. Cross-sections carve a continuous bed and signed horizontal coverage; constrain curvature/width so smoothing cannot cross divides or disconnect confluences. Join lake banks to a continuous terrain–water intersection, not individual flooded nodes. A canonical spatial index returns the same incident features on either side of a chunk boundary.

Keep bed, potential spill and occupied water level separate. First generated-world policy: only explicit authored lake constraints retain water; ordinary pits require bounded breaching and rejection if the breach budget fails. Closed basins are unsupported initially. Positive supply is an authored design constraint, not a claim of simulated lake balance. Stream surfaces cannot rise downstream; all supported water resolves through `SanctuaryWaterFieldSample` (positive coverage wet, zero bank, depth nonnegative). Add stable segment/basin IDs alongside the existing water-body category, without changing legacy precedence. No decorative second water surface.

One immutable source serves all global XZ queries; chunks never solve independent watersheds. Normals derive from the same reconstructed bed, not separately smoothed pictures. Near-player terrain/water mesh deviation must be ≤0.1 m at banks and travel surfaces, including between vertices; adaptive representation refines or rejects where that bound fails. C0 across source/mesh boundaries is mandatory; continuous normals at reconstructed bank joins are a separate checked requirement.

## Identity, edits and migration choice

Missing recipe on schemas 1–3 selects `legacy-sanctuary-v3` explicitly. Freeze that branch's height, water, geography/placement constants and RNG order. Continue writing those worlds as schema 3; do not insert a new generator into a v3 document that old binaries would silently ignore. Generated worlds require schema 4 and a validated descriptor. Unknown recipe/schema must refuse loading and preserve primary and backup, following the existing unsupported-version policy.

A separate world ID namespaces terrain point/channel/basin IDs. IDs derive from recipe version, seed and canonical source IDs, never triangle array order, render chunk or current habitat winner. Keep old landmark/feature IDs/XZ, buildings, actors and edits untouched in legacy worlds. A future new generated world uses the eleven existing landmark coordinates as protected constraints; reject an incompatible candidate rather than moving them after persistence.

Cache keys include complete recipe hash, compiler version and numeric representation version. Cache corruption causes reconstruction; caches are never save truth. Save the compact recipe and ordered edits. Apply garden edits exactly once above the immutable base. First experimental worlds reject edits intersecting protected channel/basin dependency corridors until a transactional hydrology-update contract exists; legacy edit behavior stays unchanged. Such a refusal must be exposed in production command validation, before saving or animating. Macro regeneration creates another world; it is never a garden operation.

## Budgets and the next executable chunk

Provisional ceilings, to measure rather than claim achieved: ≤40,000 macro sites, ≤80,000 triangles, ≤120,000 undirected edges; ≤8 solver passes; ≤32 MiB resident compiled fields/index and ≤96 MiB compiler scratch; ≤64 KiB recipe inside the existing 1 MB save limit. Bound point separation/triangle conditioning and enforce ≤400 m macro edges away from protected features; add constrained refinement within the same cap or reject. This is macro sampling, not metre-scale shoreline resolution. Initial compile target ≤5 s on the target M4, with zero graph solving during ticks/stream steps. Report cold compile separately from steady frame and chunk compilation costs.

**Implement next: recipe selection and legacy compatibility, not the full generator.** This is a small independently mergeable safety foundation:

1. Add `Content/SanctuaryTerrainRecipe.swift` with the validated descriptor, explicit legacy identity and canonical hashing. Reserve the generated ID but reject its construction until its compiler is available; no new-world UI yet.
2. Adapt `Terrain`/`SanctuaryLayout` initializers to receive a resolved immutable source, defaulting to legacy. Preserve legacy numerical expression order and all placement RNG consumption. Load controller state before creating layout/camera in `SanctuaryWorld`; restore resolves/validates identity before touching live state. Native Project world assembly consumes that same resolved source instead of another default `Terrain()`.
3. Add optional recipe state to `Expedition` and controller checkpoints. Preserve v1–v3 reads/writes; establish strict schema-4 decoding/rejection without allowing an unusable generated world to be saved. Include recipe identity in canonical simulation observations/checkpoints.
4. Add focused `TerrainRecipeCompatibilityTests` and production scenario coverage below. Stop after these pass: existing native worlds should be visually unchanged. The following implementation chunk can own the deterministic graph/compiler vertical slice and its Soundstage landform subject without mixing persistence work with new terrain art.

## Required acceptance tests

- Legacy fixtures from v1/v2/v3, including garden raise/lower/smooth order, shallow water, moved boulders, construction, journey, actor state and saved eye elevation: read/save/reload preserves semantic state and IDs. Compare legacy height/water/placement samples before and after at cabin, all landmarks, route verges and chunk edges; preserve floating-point results on the qualified platform.
- Unknown schema/recipe/hash mismatch, malformed constraints, over-budget recipe and corrupted cache: deterministic refusal/rebuild as appropriate; failed load/restore/save leaves live state, primary and last-valid backup unchanged. New-format refusal must not recover a different old world silently.
- Later compiler tests: independently compiled same-recipe canonical hashes; acyclic receivers, outlet conservation, zero uphill spill edges, explicit retained-basin policy, continuous confluences/banks, and coordinate queries independent of chunk order/cache eviction. Test degenerate points and insufficient refinement budget.
- Later geometry/travel gates: matched source/mesh probes across every seam and bank; route longitudinal grade ≤.55 and full-vector grade ≤.75 over the usable corridor, while checking approach eye clearance and retaining steep summits elsewhere. These are provisional design bounds, not universal guarantees of playability. Test all eleven destinations with real production walking/riding/flying, not fixture teleports as movement evidence.
- Execute `scripts/test quick --game sanctuary`, focused production scenarios and boundary checks for the first chunk. Before enabling generated worlds, additionally run all `exploration` CPU/native routes, matched Soundstage landform/water views and short live streaming measurements. No current research result satisfies those production gates.
