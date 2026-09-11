# Architecture: build the game, extract what it teaches us

The current milestone is one complete sanctuary expedition. This is a bounded
separation of existing responsibilities, not a general-purpose engine API freeze.

## Ownership and dependency direction

- **FieldCore** defines shapes, gradients, conservative bounds, the `HeightField`
  sampling contract, wind/weather math and deterministic random generation. It
  does not contain this game's terrain recipe.
- **FieldCompiler** consumes those definitions and produces mesh/LOD/meshlet data
  and numerical Metal evaluation. A height-field implementation is supplied by
  the caller; the compiler does not import Sanctuary content.
- **SanctuaryContent** defines the garden's terrain and its patch certificates,
  expedition state machine, fixed-step trust/habitat rules and versioned saves.
  It has no AppKit, Metal or renderer dependency. Gameplay is unit-testable.
- **SanctuaryGame/Runtime** owns GPU resources, frame encoding, culling, atmosphere,
  post-processing and capture readback. `MetalRenderer` accepts a `RenderFrame`
  containing uniforms, wind state and ordered `RenderItem`s. The scene supplies
  transforms, material overrides, sidedness and shadow participation. GPU drawing
  does not instantiate a garden, inspect a workshop or execute rescue rules.
- **SanctuaryGame/Game** assembles content and prepares those frames.
  `SanctuarySession` is the application composition root and fixed-step coordinator.
  `ExpeditionController` owns game progress and persistence;
  `ExpeditionPresentation` translates that state into cached geometry and transforms.
- **SanctuaryGame/Authoring** owns workshop editing, undo, source reloads, reviews,
  studies and project-look publication. It uses the same GPU renderer as the game.
  AppKit input/HUD and the local agent bridge call the same session actions.

These folders remain in one application target deliberately. The first boundary
is ownership and a concrete frame interface, not a network of tiny frameworks.
`SessionDiagnostics` forwards existing tool properties to their GPU owner to
preserve the established protocol. It is an explicit compatibility layer.

Run `scripts/check-boundaries` to catch accidental game/editor dependencies in
math, compilation or GPU code. Module-level tests enforce the content/library
separation; image replay checks the renderer split.

## Behavior and rendering

The frame loop advances game rules at 60 fixed ticks per simulated second, prepares
render items, then submits GPU work. Pausing freezes the rules; exact stepping uses
the same function. The first creature's body parts and habitat primitives compile
once. Breathing, foot motion, bounded wandering and flower growth change transforms
or visibility. The frost patch is a static terrain-conforming overlay, not live
snow transport or soil simulation.

The first expedition state retains a stable creature identity, signs discovered,
trust, rescue/release phase, habitat growth, narrative discovery and player pose.
Atomic saves contain those facts, never generated meshes. See `EXPEDITION.md`.

## What is intentionally not generalized yet

The material shader still contains Sanctuary-specific surface recipes, including
the path appearance. Its material categories and the field asset JSON format are
existing implementation contracts, not a finished extensible material language.
The workshop has specialized tree controls. `SanctuarySession` still coordinates
the game and workshop, and its established diagnostic facade remains in place.
Future work should extract typed material/generator interfaces from actual plant,
geology and gameplay implementations rather than inventing a universal plugin API.

Next: playtest discovery pacing with the intended player; improve the first
creature's animation and appeal; implement meaningful habitat choices and a second
species interaction. General engine reuse gets its separate example only after
those gameplay requirements are understood. Terrain editing, water, streaming and
full ecological simulation are separate milestones, not implied by this cleanup.

## Verification of this pass

The initial renderer split produced a byte-identical 1920×1080 paused tree capture
against the pre-refactor running build. The terrain optimization and existing
performance work were preserved. Art/content added afterward is intentionally new.
Reports live under `.soundstage/architecture-comparison.json` and
`.sanctuary/expedition-validation.json`. Use the native computer-use loop as well as
tests: no state-machine assertion establishes that the experience is fun.

Final validation: 37 unit tests and 75 running-app checks passed (15 expedition,
9 game, 15 stage, 17 workshop, 19 authoring). All Metal pipelines compiled on M4.
The native loop covered three clues, rescue, return, release and the resident's
hint; closing/reopening retained the named playtest's resident and habitat.
The final authoring loop exercised precise wetness editing, undo and native HTML
review. A 32-frame creature study covers four views across six skies and two
indoor rigs; the shared style board compares working references.

A 10.19-second live home/habitat check submitted 60.07 fps, GPU median 9.45 ms,
p95 10.75 ms, maximum 15.61 ms. Frame interval median was 16.67 ms, maximum
17.61 ms. This is a short fixed-view workload check, not a full-route or thermal
guarantee. Evidence: `.sanctuary/architecture-validation.json` and
`.sanctuary/profiles/first-expedition-20260911-094308.json`.
