# Wrela architecture

Wrela is one Swift package with three applications: Sanctuary, The Last Survey
(Cave), and Soundstage. Games are top-level projects depending on a common engine.
Soundstage loads their registrations; game executables do not link the editor or
each other. `scripts/check-boundaries` checks imports and the product dependency graph.

## Ownership

| Location | Owns |
| --- | --- |
| `Engine/FieldCore` | Shapes, field bounds, metre units, wind, attachment transforms, 3D visibility and capsule queries |
| `Engine/FieldCompiler` | Surface extraction, simplification, meshlets and numerical Metal field verification |
| `Engine/FieldEngine` | Metal renderer, atmosphere, PBR, frame composition, procedural spatial audio, project/asset contracts |
| `Engine/GameHost` | Native game window, input, fixed ticks, HUD presentation and command transport |
| `Games/Sanctuary/Content` | Terrain, ecology/expedition rules, Frostling motion and versioned persistence |
| `Games/Sanctuary/Project` | Field recipes, registrations and Sanctuary scene assembly |
| `Games/Cave/Content` | Volumetric cave field and deterministic encounter/objective rules |
| `Games/Cave/Project` | Watcher recipe/motion, registrations and Cave scene assembly |
| `Games/*/Authoring` | Each game's published asset recipes, art direction and procedural surface materials |
| `Tools/SoundstageKit` | Project-independent editing, undo, capture review, animation and behavior inspection |
| `Tools/Soundstage` | Application composition: registers the two projects |
| `Tools/AgentTools` | Shared command client used by stagectl, gamectl and gardenctl |

The dependency direction is games/tools → engine. Shared contracts use Swift
`package` access: this is a source monorepo, not a promised stable binary SDK.

## Authoring contract

A `GameProject` supplies its identity, asset directory, generators, atmosphere
capability and `GameExperience` constructor. `AssetGenerator` supplies semantic
controls with units/ranges, generated parts or a custom compiler, optional
animation, and optional interior inspection cameras. IDs are scoped to a project.
The same source compiles in the game and Soundstage; neither has a parallel
creature model or hand-maintained list of species-specific inspector controls.

`AssetSource` stores a compact generator recipe plus optional stable part
overrides. Explicit `fields` assets still support direct composition. Semantic
Swift recipes expand to `FieldExpression`/`Shape`, then to meshes. Geometry caches
exclude pose settings. Animation changes transforms of compiled parts.

`AnimationDefinition` provides clips, durations, pose/root functions and a
fixed-step behavior adapter. The game owns its brain and versioned state.
Soundstage stores an opaque deterministic snapshot plus display telemetry, and
calls the same brain through seeded scenarios. It owns time, stimuli, playback,
selection and replay, not rescue or scare policy. Scenario bounds and interior
views are supplied by the project instead of assuming an outdoor origin.

Materials are project-owned Metal snippets inserted into the common surface
pipeline. `projectSurface` adjusts albedo, roughness and bump inputs before shared
PBR and scene grading. This is a deliberately small shader contract, with existing
integer material categories; it is not a finished arbitrary material-graph DSL.
Art direction remains global within a project and affects objects, lights and sky.

## Runtime contract

A `GameExperience` owns player pose, lighting choices, render items, HUD facts,
interaction, rules and saves. The host advances it at 60 fixed ticks per simulated
second. `FrameComposer` supplies common camera/light uniforms; `MetalRenderer`
receives prepared frames and knows no expedition, creature or editor state.
Cave uses a perspective shadowed spotlight and fixed exposure. Closed-world
projects skip the large atmospheric caches and their update work. Soundstage can
still inspect Cave objects under the outdoor atmosphere.

Sanctuary retains its height-field terrain and grounded movement. Cave has a true
3D solid field with ceilings, walls, branch passages and floor; capsule movement
and beam visibility query that field. Large cave surfaces and small outcrops
compile in separate bounds to preserve local detail. Collision combines their
source fields. Rasterized surfaces remain approximations of those definitions.

Audio uses procedural mono buffers positioned through an AVAudioEnvironmentNode.
Cave emits drips, distant scrapes and encounter impacts. This is spatial playback,
not simulated acoustic wave propagation or field-derived reverberation.

## Persistence and workflow

Player progress remains at `.sanctuary/saves/`; Cave uses `.cave/saves/`.
Validation uses named slots and restores the player's slot. Meshes are rebuilt
from code/recipes and are never save-game truth.

Soundstage uses `.soundstage` for its command protocol and shared capture reports.
Checkpoints/review state are scoped under `.soundstage/projects/<id>/`.
Switching projects remembers each working study, changes materials and catalog,
and clears cross-project edit history. Existing Sanctuary checkpoints are copied
forward without overwriting newer work. Last-session studies carry project IDs;
legacy Sanctuary studies remain readable.

Build individual products with `swift build --product Cave` (or Sanctuary or
Soundstage). `scripts/build` produces all three locally signed app bundles.
Swift changes need an app restart; recipe edits and shader reloads do not.
See `SOUNDSTAGE.md` and `CAVE.md` for concrete workflows.

## Limits

The cave is a bounded architecture stress test: one creature, two controlled
encounters, one objective and a return route. It is not a second flagship game.
Its rock/creature art and sound are prototypes. No streaming, general navigation,
rigid-body physics, terrain persistence, water or complete ecological simulation
is implied. Atmosphere/weather approximations remain documented in ATMOSPHERE.md.
This separation enables further games without claiming the engine is finished.

See `MONOREPO_VALIDATION.md` for evidence and measured limits of this migration.

## Production simulation and testing

`SimulationCore` contains the renderer-free `GameSimulation` contract and player/input types.
`CaveWorld` and `SanctuaryWorld` in their games' Content targets now own production movement,
collision, visibility and simulation coordination. The native experiences delegate to them.
Sanctuary's shared procedural layout supplies both renderer placements and CPU collision solids,
including the same placement RNG continuation used for grass. GPU meshes are presentation data.

`Engine/TestKit` supplies typed scenarios, deterministic campaigns, recordings, replay and CPU
measurement. Game-owned Testing targets supply fixtures, assertions and workloads. The WrelaTest
executable imports these registration targets without importing FieldEngine, GameHost, Metal or
Soundstage. Tools/Testing owns the launcher and reporting; its native driver runs recorded inputs
against the real GameHost simulation and compares state. `GameProject.inspectSimulation` adapts
opaque game checkpoints into a generic Soundstage creature inspection without game imports in
the editor. See [TESTING.md](TESTING.md) for contracts, commands and explicit limits.
