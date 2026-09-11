# Sanctuary: first playable build plan

Status: the field garden and inspection loop are implemented; later milestones remain planned. See README.md for the runnable scope and current limits. Working game label only; the repository remains `wrela`.

## Product contract

Build one first-person sanctuary game before extracting a reusable engine. The intended player enjoys exploring spacious, beautiful landscapes and finding rare creatures. Tracking and capturing brings those creatures home, where ecological interactions transform habitats and create reasons for further exploration.

The art direction is painterly and anime-inspired, with polished silhouettes, color, lighting, and animation. All authored visual content comes from Swift definitions and mathematical fields. Generated meshes, textures, rigs, sampled fields, collision proxies, and disk caches are allowed. Prerecorded audio is allowed.

Building is direct and first-person: point, dig, place, and walk through the result. Terrain edits and creature state persist. Rare encounters have more common, trustworthy evidence. Legendary creatures have deliberately authored mysteries embedded in the generated landscape; the first playable contains a hint, not a complete legendary encounter.

Examples such as the Winter Crown are illustrative, not final content requirements.

## Local baseline

Inspected September 10, 2026:

- Repository initially contains Git metadata, `.gitignore`, and `LICENSE`.
- MacBook Air, Apple M4, 8 GPU cores, 16 GB unified memory.
- Swift 6.2.4 and Xcode are installed; system reports Metal 4 support.
- Built-in display is 2560 x 1664. Benchmark rendering explicitly at 1920 x 1080; do not accidentally measure a larger Retina drawable.

Native 1080p at sustained 60 fps is the target, not an established result. MetalFX reconstruction is an allowed fallback and optional setting. Initial development targets this Mac and installed macOS; deployment to older OS versions is a later decision.

## First complete experience

Leave a small sanctuary, see a distant landmark, explore a composed valley, discover signs of one rare creature, track and capture it, return it to the sanctuary, and observe a local habitat change. Sculpt a small area of terrain. Save, close, and reopen with the changes intact. Include one environmental clue suggesting an extraordinary creature farther away.

Start with one connected valley whose traversal scale can be tuned during playtesting. Distant scenery should create a sense of a larger world without requiring us to simulate an entire continent. Expand traversable territory after the encounter and streaming work.

The first species should visibly influence a small patch of habitat. A second species later demonstrates symbiosis. Full water simulation is not required to test the first rescue; persistent water volume and flow become a distinct subsequent milestone.

## Initial code organization

Use Swift Package Manager for a small set of targets and a native macOS executable using AppKit and MetalKit. Add app bundling or an Xcode project when signing or platform integration requires it. A single local command should build and launch the development app.

Suggested boundaries, kept within one repository:

- `FieldCore`: typed graph, stable IDs, units, bounds, field semantics, CPU reference evaluation, and validation.
- `FieldCompiler`: graph simplification, generated Metal evaluation, mesh/sample generation, dependency tracking, and cache keys.
- `SanctuaryRuntime`: Metal rendering, spatial queries, chunk lifecycle, simulation scheduling, creature state, persistence, and input.
- `SanctuaryGame`: Swift world/species definitions, discovery rules, sanctuary gameplay, and the app entry point.

Start runtime subsystems as folders, not separate frameworks. Add a graph-export authoring executable only when incremental graph reload is needed. Do not build a universal plugin architecture or editor first.

AppKit owns the window and first-person input. MetalKit hosts rendering. Developer controls can use a small native panel. The game view and object studio use the same renderer and content pipeline.

## Field and compiler contract

Swift authoring produces a typed, inspectable graph; arbitrary Swift closures are not the GPU program. Begin with sphere, box, capsule, transforms, union, subtraction, a bounded smooth union, and simple material composition. Introduce displacement only with explicit amplitude/frequency limits and valid evaluation guarantees.

Distinguish shape, material, environmental, and influence fields. Distinguish exact signed distances, conservative distance bounds, and general implicit values. Do not march arbitrary functions as if they were distances. Unbounded terrain definitions must be clipped or evaluated within explicit finite compilation regions.

Use the CPU evaluator as a numerical reference. Generated Metal evaluation must agree within stated tolerances. Cache keys include graph content, compiler version, representation settings, and relevant target capabilities. Stable authoring identities are separate from content hashes, so changing a definition does not silently create a different creature in saved state.

Initially choose representation policies explicitly. Automatic selection follows profiling evidence. Every compiled product records source revision and approximation settings.

Parameters that stay within compiled bounds can update without regeneration. Edits that change bounds or topology must invalidate dependent products. Replacements compile asynchronously; retain the previous valid version until a compatible replacement is ready.

## Rendering strategy and experiment

First establish a mesh-rendered baseline derived entirely from fields. Start with opaque painterly materials, a sky, directional lighting, shadows, atmospheric depth, generated vegetation, and procedural wind. Choose colors and large forms before adding fine noise.

In the object studio, implement a second path for bounded procedural ray intersections using Metal acceleration structures. Compare it with the mesh path on identical geometry, material, lighting, camera, and output resolution.

Measure field evaluations, GPU time, memory, silhouette quality, and update cost for isolated objects and repeated objects. Include a foliage-heavy valley and moving creatures before choosing a production policy. Hardware ray tracing accelerates spatial traversal; it does not make custom field evaluation free.

Mesh terrain and instanced generated vegetation are the initial production policy. Direct field rendering is a candidate for selected close objects and evolving surfaces. Rendering paths must share material semantics, depth conventions, shadows, and eventually motion vectors if temporal reconstruction is adopted.

## Spatial consistency and persistence

Terrain is partitioned into editable chunks with dependency overlap at boundaries. Rebuild only affected regions. Publish matching render, collision, and relevant query revisions together; define how navigation waits or invalidates paths during an edit. Track approximation tolerances explicitly. Use authoritative field refinement where required for close interactions.

Test chunk seams, subtraction at boundaries, edits under the player, and small features near the collision tolerance. Movement and terrain editing are production correctness concerns, not just visual demonstrations.

Save a versioned world seed and generator version, explicit creature records, ecological state, and compacted terrain edits or snapshots. Generated caches are disposable and are not the save's source of truth. Use atomic save replacement and a recoverable previous save. Schema changes must migrate or explicitly reject incompatible saves rather than silently changing the world.

Persist creature identity and approximate remote activity when regions unload. Materialize detailed simulation near the player while preserving position plausibility, needs, and discovery evidence. Defer offline progression until its desired behavior is defined.

## Ecology and animation

Use explicit creature entities for identity, behavior, capture state, and relationships. Environmental fields express suitability and interactions; they do not replace entity state.

Begin with one species, simple wandering/resting/drinking behavior, bounded procedural body variation, and an authored-in-code rig and gait. Animation quality is a separate task from generating shape. Simple ground adaptation can be added if visible sliding or floating undermines the experience.

Introduce one local environmental influence and a gradual visible response. Use persistence thresholds to prevent habitat flicker. Later add a second species that benefits from the changed habitat and introduces a manageable tradeoff.

Simulation uses fixed steps with separate cadences for movement, decisions, and ecological growth. Fine simulation is local; distant habitats use coarser updates. Tune rates from measurements. Seeded replay targets reproducibility within the same build and settings, not cross-device floating-point lockstep.

Water will need explicit conserved volume, connectivity, and flow state. A terrain trench supplies a boundary, not a fluid simulation. Start with surface channels and pools when that milestone begins; caves and stacked water layers require a separate design.

## Development and agent loop

Provide one executable with a playable scene and an object studio. The studio supports named lighting presets, orbit views, zoom, animation poses, and a fixed background. It is not a second renderer.

Add a thin debug command interface, backed by the same actions as the visible controls: load scenario, set seed/camera/time, pause, step, replay input, inspect state, capture image, and export timing. Prefer launch arguments and local command files initially; add a local service only if the workflow needs it. Computer use covers visible interaction and exploratory playtesting.

Keep image captures paired with scenario, revision, seed, camera, and lighting metadata. Use fixed input replays for regression checks, with human playtesting for enjoyment and discovery pacing.

Parameter edits should appear by the next few frames when no compilation is needed. Graph changes require incremental Swift authoring and derived-product work; keep the old scene usable while that completes. Measure actual reload latency. Engine-code changes may require rebuilding and restarting into a saved development scenario.

## Milestones and completion checks

### 1. Walkable field garden

Deliver a native app, first-person camera and basic grounded movement, a small field-generated terrain patch, rocks and one tree form, painterly lighting, and a switchable object studio. Include the minimal field graph and timing/capture controls.

Done when the app launches from one documented command, the user can walk the patch without falling through it, one definition drives its visible shape and spatial query, and a parameter change updates correctly. Capture repeatable views at 1920 x 1080. Establish a baseline rather than claiming the empty scene proves final performance.

### 2. Representative valley and rendering decision

Compose a valley with a visible destination, dense vegetation patches, shaded areas, and long views. Add chunk visibility/detail management. Compare bounded field tracing and generated meshes in the studio and representative scene.

Done when the landscape is worth exploring visually, a repeatable camera route produces timing and memory data, and representation decisions are recorded with evidence. Choose the next art or performance work from this result.

### 3. One expedition

Add one distinctive animated species, grounded tracking signs, approach/capture, return to sanctuary, and a visible local habitat response. Add a distant legendary hint. Keep creature location fixed or seeded during initial playtests so iteration is interpretable.

Done when a new player can understand and complete the loop, signs lead to a real encounter, the creature feels worth finding, and movement/search pacing has been playtested by the intended player. Adjust valley size and encounter spacing from that feedback.

### 4. A sanctuary that remembers

Add first-person terrain sculpting, compatible chunk replacement, save/load, and persistent creature/ecological state. Prove changed terrain affects movement and creature queries. Add unload/reload behavior for a remote creature.

Done when digging across a chunk boundary and saving/reopening preserves shape and collision, captured creatures are not duplicated, remote identity remains intact, and interrupted saves do not destroy the last valid world.

### 5. Ecological depth and expansion

Add a second species with a readable symbiosis, surface water if required by the interaction, more habitats, and larger exploration routes. Expand legendary discovery sequences only after ordinary discoveries work. Introduce bounded evolution when it contributes to attachment or habitat decisions.

Done when the player can explain why species interact, arrange habitats intentionally, and find a reason for another expedition. Grow world scale and species count against sustained performance measurements.

## Performance protocol

The frame interval at 60 Hz is approximately 16.67 ms. Initial planning envelopes are GPU work around 12 ms and CPU critical-path work around 4 ms, leaving room for variance. CPU and GPU overlap; these are independent diagnostic budgets, not numbers to add into a frame-time proof.

Use a provisional game memory ceiling of 6 GB on the 16 GB machine, then tune from measured residency and system pressure. Unified allocations must not be double-counted. Development tools also consume memory and compute.

Measure a repeatable route at explicit 1920 x 1080, with median, p95, p99, missed presentation intervals, memory, and edit/streaming spikes. The user requested skipping a 15-minute benchmark for the initial milestone; use a short baseline and leave prolonged thermal validation for later. Record power mode and whether compilation or agent work is active. Normal-play and concurrent-development runs answer different questions. Average FPS alone is insufficient.

Use correctness tests for CPU/GPU field agreement, bounds, chunk seams, save round trips, and entity lifecycle. Visual inspection covers art and animation. Do not treat screenshot similarity as a measure of fun.

## Immediate implementation order

1. Create the Swift package and native MetalKit window with reliable first-person input and fixed render resolution controls.
2. Implement the small field graph and CPU reference evaluation with semantic validation.
3. Generate an initial mesh and render it with a simple painterly material; add grounded movement and shared queries.
4. Add a small terrain composition, tree, rocks, lighting, and the object studio.
5. Add reproducible capture, timing, and parameter reload; inspect and measure before expanding content.

## References

- [Apple Metal samples](https://developer.apple.com/metal/sample-code/): rendering, synchronization, frame pacing, and acceleration-structure examples.
- [MetalKit MTKView](https://developer.apple.com/documentation/metalkit/mtkview): native Metal view.
- [Apple GPU feature tables](https://developer.apple.com/metal/capabilities/): M4 family and MetalFX support.
- [Apple hardware ray-tracing explanation](https://developer.apple.com/videos/play/tech-talks/111375/): traversal acceleration and procedural intersection work.

External references establish API capabilities, not this game's future performance. All proposed budgets and milestones above require implementation and measurement.
