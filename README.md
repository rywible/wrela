# Sanctuary

A native Swift + Metal field garden: the first playable foundation for a procedural wildlife sanctuary game. Walk through a generated landscape, inspect a composed field object, and change its scale and lighting while the app runs.

All visual content is generated from code. There are no imported models or texture assets. This is an early engine prototype, not the finished sanctuary game.

## Run

Requires a Mac with Xcode and the Swift 6 toolchain. The mesher vendors the MIT-licensed meshoptimizer C++ sources; see `Sources/CGeometry/PROVENANCE.md`.

```sh
./scripts/run
```

This builds a release executable and a locally signed app at `.build/Sanctuary.app`, then opens it. `./scripts/build` builds without launching. Quit the running app before launching a rebuilt version. The bundle records the workspace path for local agent commands; rebuild after moving the checkout.

The render target is **1920 × 1080**, independently of window size or Retina scaling. Drag the window larger or smaller without changing the benchmark resolution.

For appearance work, use `./scripts/soundstage` and `./scripts/stagectl`. See [the soundstage guide](docs/SOUNDSTAGE.md), [atmosphere research notes](docs/ATMOSPHERE.md), and [repository agent instructions](AGENTS.md).

## Controls

| Action | Control |
| --- | --- |
| Walk | W A S D |
| Run | Hold Shift while walking |
| Look | Drag in the scene, or use arrow keys |
| Garden / object studio | Tab or the top-right button |
| Lighting | 1 morning, 2 sunset, 3 overcast; or buttons |
| Change seed pod scale | Live slider |
| Studio camera | Drag to orbit; scroll to zoom |
| Reset camera | R or Reset view |
| Capture | F or Capture frame |
| Pause animation | P |
| Hide / show field notes | H |

Arrival, Meadow, and Gate buttons provide repeatable garden viewpoints. Captures omit the native interface and save a PNG plus JSON metadata in `.sanctuary/captures/`.

## Agent tools

Keep the app running with its window available. Commands acknowledge completion and fail explicitly on invalid parameters. They operate on the real rendering and movement systems.

```sh
./scripts/gardenctl status
./scripts/gardenctl scene studio
./scripts/gardenctl pause true
./scripts/gardenctl lighting sunset
./scripts/gardenctl camera --orbit 0.8 --elevation 0.2 --distance 6.8
./scripts/gardenctl parameters --scale 1.3
./scripts/gardenctl capture --label seed-sunset
./scripts/gardenctl study
```

`study` captures four angles under seven lighting presets, then restores the prior scene, camera, lighting, and pause setting. It uses the production renderer. Each capture records its frame, simulation time, parameters, source fingerprint, shader fingerprint, camera, and timing context.

```sh
./scripts/gardenctl reloadShaders
./scripts/gardenctl verifyFields
./scripts/gardenctl query --x 5 --y 2 --z 12
./scripts/gardenctl move --z 2
./scripts/gardenctl step --frames 60
./scripts/gardenctl replay docs/arrival-replay.json
```

Shader reload reads `Sources/SanctuaryGame/Resources/Garden.metal` and preserves the running world. Compilation failure retains the last valid pipelines. Swift graph/engine edits require a rebuild. Parameter scale edits reuse the compiled mesh and update its spatial queries immediately.

`move` uses player collision; `camera` is a debug teleport grounded to the terrain. `step` requires a paused simulation. Movement axes are local: positive Z means forward. Camera angles are radians. Garden coordinates are metres, Y-up, looking toward negative Z at yaw zero.

See [the agent workflow](docs/AGENT_WORKFLOW.md) for a practical inspection loop and the file protocol.

## Check

```sh
swift test
./scripts/validate
./scripts/benchmark --seconds 60
```

The first command tests field math, conservative bounds, meshing, winding, terrain agreement, and repeatable generation. The second requires the app and checks generated Metal against Swift, player movement, collision, captures, stepping, and live parameters. Reports and images are local under `.sanctuary/`.

The benchmark measures the animated garden at a fixed arrival camera for the requested duration (one minute by default). Leave the scene untouched during the run. It reports submitted frame rate, rolling CPU/GPU/frame-interval statistics, process RSS, and power source. Submitted frame rate is not direct display-presentation telemetry. The test is a baseline for this scene, not a guarantee for future open-world content or prolonged thermal behavior.

## Implementation

- **FieldCore:** typed shape graph, validated parameters, bounds, CPU field evaluation, analytical terrain definition, deterministic random generation.
- **FieldCompiler:** sparse dual contouring with constrained QEF fitting, a tetrahedral ambiguity fallback, attribute-aware mesh simplification, meshlet generation, terrain extraction, and generated Metal field functions.
- **SanctuaryGame:** composed world, Metal renderer, camera/collision, native app controls, object studio, and agent bridge.

The renderer uses generated meshes, instance culling, three distance-based detail levels for trees and stones, a directional shadow map, procedural sky/materials, and wind-deformed meadow geometry. Shape composition includes spheres, boxes, capsules, tori, translation, uniform and nonuniform scale, union, subtraction, and smooth union.

Terrain is a general implicit field; it is never assumed to be a safe marching distance. Shape bounds account for conservative distance scaling during nonuniform deformation and blending. Spatial queries and visuals use the same source definitions, with explicitly approximate mesh surfaces and player collision.

## Current limits

This build delivers the field garden and inspection loop. Creature rescue, persistent terrain edits, ecology, water simulation, world streaming, and legendary encounters are subsequent milestones in [the build plan](docs/BUILD_PLAN.md).

Rendering currently uses rasterized field-derived meshes. Generated Metal field evaluation is implemented and checked numerically; the field-only ray renderer has been removed. Mesh detail selection uses projected error with hysteresis. Mesh shaders draw meshlets and 4× MSAA stabilizes coverage. Fine cut edges and distant silhouettes still need visual refinement. The player uses grounded movement and approximate static-object collision; it has no jumping or rigid-body physics yet.
