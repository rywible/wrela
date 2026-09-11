# Wrela

A Swift + Metal engine for games authored from mathematical fields. The monorepo
contains **Sanctuary**, a procedural wildlife discovery game; **The Last Survey**,
a small cave horror game that tests engine reuse; and **Soundstage**, their shared
authoring workshop. Visual assets come from code and compact recipes, with meshes
and lighting caches compiled from that source.

## Run

Requires macOS, Xcode and Swift 6. World units are metres; rendering targets native
1920×1080. These are playable prototypes, not finished games.

```sh
./scripts/build          # Builds all three app bundles
./scripts/run            # Sanctuary
./scripts/run-cave       # The Last Survey
./scripts/soundstage     # Shared workshop
```

Quit an app before reopening a rebuilt version. Game controls: WASD, Shift to run,
drag/arrows to look, E to interact, P to pause. Cave adds F for the flashlight.
Sanctuary progress stays under `.sanctuary/saves`; Cave uses `.cave/saves`.

In Soundstage, choose a project in **Object → Project**, select a subject, then
edit its shape, parts, motion, behavior, lighting or global look. Controls and
ranges come from each game's registered recipes. Save a study to keep an
experiment; Save asset publishes it for the game.

```sh
./scripts/stagectl project cave
./scripts/stagectl subject watcher
./scripts/stagectl rig flashlight
./scripts/stagectl creature --mode lunge --seconds 0.3
./scripts/stagectl shape --set mask 0.95
./scripts/stagectl capture --label watcher-study
./scripts/gamectl --project cave status
```

## Structure

- `Engine/`: field math, surface compiler, rendering, audio, project contracts and native game host.
- `Games/Sanctuary/`: wildlife game, Frostling and landscape recipes, authoring sources and style.
- `Games/Cave/`: volumetric cave, Watcher, flashlight encounters and its own style.
- `Tools/`: shared Soundstage and agent command client.

Each game builds independently with `swift build --product Sanctuary` or
`swift build --product Cave`. Neither links the other game or the editor.
Third-party geometry code is documented in `Engine/CGeometry/PROVENANCE.md`.

## Working guides

[Architecture](docs/ARCHITECTURE.md) · [Soundstage](docs/SOUNDSTAGE.md) ·
[Cave](docs/CAVE.md) · [Sanctuary expedition](docs/EXPEDITION.md) ·
[Atmosphere](docs/ATMOSPHERE.md) · [Agent instructions](AGENTS.md)

Run `swift test` and `scripts/check-boundaries`. With Sanctuary running, use
`validate` and `validate-expedition`; with Cave, `validate-cave`. With Soundstage
on the Sanctuary project, use `validate-workshop`, `validate-authoring` and
`validate-creatures`. `Soundstage --check-renderer --project sanctuary` checks
Metal pipelines from its built executable. Tests complement native play and
actual image inspection. Short performance checks use `scripts/profile`.

This is an evolving source engine, not a stable external SDK. The Cave game is
intentionally small; procedural art, navigation, ecology, streaming, terrain
editing and general physics all have substantial work ahead.

### Testing

`./scripts/test quick` runs the fast checks without opening a game. The same harness provides
game-authored scenarios, native replay/captures, seeded simulation campaigns, isolated authoring
checks and per-game/engine performance workloads. Start with `./scripts/test --help` and
[the testing guide](docs/TESTING.md). Failures can open directly in the game or Soundstage.
