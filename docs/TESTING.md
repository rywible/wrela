# Testing Wrela

Use `./scripts/test`. It builds the needed products incrementally, owns any app it launches,
and writes a report under `.build/test-runs/`. You do not need to start a game, choose a
save slot, move command files, or restore your workshop after a test.

## Pick the smallest useful loop

| Work | Command | What it exercises |
| --- | --- | --- |
| Everyday engine change | `./scripts/test quick` | Swift unit/integration tests, harness policy tests, boundaries and game smoke scenarios |
| Game rules | `./scripts/test quick --game cave` | That game's unit tests and smoke scenarios; no window or GPU |
| One game scenario | `./scripts/test run --game sanctuary --test rescue-trust` | Production simulation, semantic actions, assertions and recording |
| All game routes | `./scripts/test run --game all` | Includes real movement through both complete routes |
| Changed code | `./scripts/test changed --since HEAD` | Conservative owner selection, including untracked files; shared changes select both games |
| Render a scenario | `./scripts/test render --game cave --test beam-visibility` | Fresh native game; replay actions, compare native/CPU state, capture marked frames, exercise disk saves |
| Native host integration | `./scripts/test host --game sanctuary` | Ordinary command/input adapters, persistence, malformed commands and paused image repeatability |
| Field/Metal numerical agreement | `./scripts/test gpu-check --game sanctuary` | CPU/GPU field comparison and sky traversal verification |
| Workshop transactions | `./scripts/test authoring --suite workshop` | Isolated Soundstage; shape, rig, animation and exact study replay |
| Other editor contracts | `./scripts/test authoring --suite authoring` | Undo/redo, file watching, checkpoints and style; also `creatures`, `projects` and native `review` suites |
| Creature craft | `./scripts/test authoring --suite craft` | All five craft source families, strict rejection, bounded pose fitting, guide/cloth compilation, undo and exact future pixels |
| Creature source and replay | `./scripts/test authoring --suite dynamics` | Guide sculpting, local finish, atomic rejection, geometry reuse and exact future pixels; also `creature-tools` and `performance` suites |
| Agent sculpting | `./scripts/test authoring --suite sculpt` | Actual-render pixel/source selection, bounded intent fitting, influence preview, conflict rejection, layers, matched alternatives and exact study/pixel replay |
| Seed campaign | `./scripts/test dst --game sanctuary --seeds 24 --ticks 900 --seed 17` | Deterministic production simulation, invariants, restore perturbations and coverage |
| Engine CPU performance | `./scripts/test perf --game engine --kind cpu` | Field evaluation, meshing and wind workloads |
| Game simulation performance | `./scripts/test perf --game sanctuary --kind cpu` | Game-owned one/16-actor workloads, 60 ticks each |
| Game graphics performance | `./scripts/test perf --game cave --kind gpu` | Live fixed-view and camera-sweep workloads at native 1080p |
| Engine graphics performance | `./scripts/test perf --game engine --kind gpu` | Isolated live sky and material-calibration scenes through the shared renderer |

`./scripts/test list` lists registered game scenarios. `--test` and `--tag` filter runs;
`--output` chooses a **new** report directory. No external Python packages are required.
A warm full quick suite is roughly 10–15 seconds on the current machine; focused release
simulation scenarios themselves take milliseconds. First builds are longer.

`swift test --filter WorkshopDocumentTests` checks the editor's study codec without
creating a renderer or window: rich source/pose replay, old formatted studies,
neutral-review metadata and preservation of the previous save when writing fails.
These tests also run in the shared `quick` loop. Actual clay pixels, hidden-part
shadows, native controls and study replay still require the authoring suite.

The old `validate`, `validate-expedition`, `validate-cave`, `validate-workshop`,
`validate-authoring`, `validate-creatures` and `validate-projects` entry points now dispatch
to isolated harness sessions. The harness can still invoke their low-level checks through
`WRELA_CONTROL_ROOT`. Ordinary `gardenctl` / `stagectl` remain interactive tools.

## Ownership and authoring

- **Engine/SimulationCore** defines the production simulation interface, fixed-tick input
  events, camera and canonical encoding. It imports no renderer or UI.
- **Engine/TestKit** owns scenarios, assertions, traces, first-difference reporting,
  deterministic campaigns, reduction and workload measurement. It imports no game.
- **Games/<Game>/Content** implements the real simulation. The native experience delegates
  movement, visibility, collision and encounter rules to that same object.
- **Games/<Game>/Testing** owns fixtures, scenarios, simulation workloads, GPU workloads and
  native check manifests. These describe that game's intent and budgets.
- **Engine/Testing** owns subsystem workloads; **Engine/Tests** owns mathematical/compiler
  tests. **Engine/TestKitTests** tests the harness with deliberately failing simulations.
- **Tools/TestingContracts** checks both games against the shared production contract.
- **Tools/TestRunner** registers game test modules into a renderer-free Swift executable.
  **Tools/Testing** handles process lifecycle, baseline policy and HTML reports.
- **Soundstage** remains the interactive object/motion/lighting authoring surface. The
  harness adds repeatable automation and failure inspection around it.

A game author writes ordinary Swift:

```swift
GameTest("rescue-trust", fixture: "frostling", tags: ["smoke", "render"]) {
    TestStep.action(.interact)
    TestStep.expect("phase", "searching")
    TestStep.advance(300)
    TestStep.range("trust", 0.99, 1)
    TestStep.capture("ready-for-rescue")
    TestStep.action(.interact)
    TestStep.expect("phase", "carrying")
    TestStep.save("rescued")
    TestStep.walk(x: -2, z: -68)
    TestStep.load("rescued")
}
```

`advance` counts exact 1/60-second ticks. `walk` steers the actual player with bounded
movement and collision; it fails if the destination cannot be reached. Fixtures may place
actors or establish progress directly, but that setup is declared by name. A `home` fixture
checks release behavior; `return-traversal` separately checks the walk home. Captures are
markers, so the same test can run without graphics or in the native game.

Observations are a stable, game-owned semantic dictionary. Test goals such as `phase`,
`trust` and `scares`; avoid reaching through renderer implementation details. Numeric ranges
use `range`, exact categorical assertions use `expect`. The entire serialized simulation is
also compared during native replay, including fields not present in the observation API.

`TestProject` registers fixtures, tests and `SimulationWorkload` definitions. New games add
one registration to the test executable and a product mapping to the launcher. Shared
scenario machinery needs no species or quest switches. Add render setup commands and
explicit p95 budgets in the game's `RenderWorkloads.json`; add native checks in
`NativeChecks.json`. Engine-only scenes use the editor's shared renderer and neutral
calibration subjects, without building either game population.

For agent-created reproducers, export a report's `test` object to JSON and use
`./scripts/test run --game cave --scenario /absolute/path/scenario.json`. Swift remains the
source of the checked-in game scenarios. The JSON is a transport for quick reproductions.

## Recording, failures and inspection

Every run directory includes an HTML report, JSON results, build/app logs where relevant,
individual scenario artifacts, and copies of both games' authored inputs under `content/`.
Artifacts include the seed, fixture, initial state, expanded input actions, named saves,
per-step state, observations, failure position and source digest. Screenshot metadata retains
renderer, hardware and source provenance. Keep the **whole run directory** when sharing a
reproducer; the source/binary itself is not archived.

```sh
./scripts/test replay /absolute/run/cave-beam-visibility.json
./scripts/test minimize /absolute/run/cave-failing-case.json
./scripts/test inspect /absolute/run/cave-beam-visibility.json --step 7
./scripts/test inspect /absolute/run/sanctuary-rescue-trust.json --stage --step 4
```

Replay compares recorded state and re-runs the scenario assertions. A failed assertion that
fails identically is a successful reproduction. Reduction preserves the same failure message
and makes at most 40 attempts; it does not promise a globally minimal example. An intentionally
wrong Cave assertion was reduced from 17 steps to 2 during validation.

`inspect` starts a frozen native replay. `--play` resumes it for a computer-use playtest.
Close its window or interrupt the command to end the owned session. `--stage` imports the
recorded creature brain into an isolated Soundstage with the pinned source; change angles,
lights and anatomy freely. The Behavior panel identifies an imported recording. Playing in
Soundstage uses studio stimuli, and its presentation timeline wraps at 60 seconds; it is an
inspection/authoring continuation, **not** a replay of the entire game's surroundings. Use
the game inspector for exact world behavior. Restarting the studio scenario clears the
recording label.

If implementation source has changed, replay refuses by default. `--allow-source-change`
explicitly compares the current implementation against the older recording and pinned content.
State schema/content incompatibilities should produce a useful failure, not silently reset a
world. Visual inspection of a failure is still necessary; passing state assertions says nothing
about whether an animation looks appealing.

Soundstage review subprocesses inherit the isolated command root and the actual running app
bundle path, so Capture & review also works outside the primary checkout. Runs invalidate
their result if implementation/test source changes during execution.

Native render replay deliberately separates the simulation from wall-clock rendering. It does
not prove audio output or OS event delivery. Use `host` plus the game-owned computer-use
checklist for those adapters. This implementation exercised Cave's real F/arrow bindings and
Soundstage's native Behavior/view controls through Codex computer use. It does not claim
an automated perceptual test of sound or artistic quality.

## Deterministic simulation testing

DST starts from game fixtures and an explicit seed. Test input randomness is independent
of the game's saved RNG. Two production instances receive identical actions, validating
invariants and comparing canonical state after **every action**, not just the final frame.
One instance is restored periodically without changing simulation time. Invalid restores
must be rejected without altering the current state. Reports count observed states and
transitions; a passing campaign with poor coverage is not comprehensive AI coverage.

Game saves include hidden state needed for continuation: RNG, timers, goals, animation state
and controller bookkeeping. Set-valued expedition discoveries encode in sorted order so
separate processes don't produce spurious byte differences. Engine wind/weather save-and-
future tests live in the fast Swift suite. Real filesystem failure tests verify that a failed
rescue save does not commit the gameplay transition. Native route/save checks also caught a
2 cm camera-height snap on Sanctuary reload: saves now retain collision-resolved eye elevation,
with optional decoding for legacy saves and a disk/restart regression test.

Current simulation is synchronous. This is seeded deterministic **simulation** testing, not
a simulated network/storage cluster, thread scheduler, or GPU determinism guarantee across
Apple GPU/OS generations. When asynchronous gameplay arrives, its clocks, job completions and
I/O outcomes need explicit injected scheduling; don't introduce wall-clock sleeps into tests.
Normal scenario replay compares per-step snapshots; DST detects per-action divergence. A
failure caused only by divergent twin execution or restore scheduling may retain the full
recording instead of reducing successfully; preserve the campaign's seed/fixture/tick settings
and rerun that campaign to reproduce that class of failure.

## Performance and visual baselines

GPU checks own an exclusive lease and reject concurrent Wrela render windows. CPU and GPU
performance runs also share a measurement lease. The harness never kills a player/editor
process discovered by name. It closes only the process it launched. Unrelated machine load
can still affect results; short measurements are signals, not a thermal soak certification.

GPU measurements warm up for 3 seconds, then sample 6 seconds per workload by default.
Live clouds, weather and wind remain enabled. `--seconds 3...60` changes this explicitly.
Reports include median/p95/p99/max timings, sample counts, individual GPU stages, frame and
presentation intervals, memory, thermal state, device and render size. Overlapping GPU stage
intervals must not be added. GPU time is not FPS; frame scheduling spikes remain visible.
Game manifests currently budget p95 GPU at 16.67 ms and p95 simulation at 2 ms.

CPU benchmarks exclude fixture construction and report milliseconds for the named workload.
Game workloads include restoring the initial checkpoint and validating the final state;
16 actors × 60 ticks is a batch, not a single frame. These are independent one-creature
game instances; they do not measure a shared ecosystem of sixteen interacting creatures.
As that simulation exists, add a game-owned population fixture for it. Sanctuary uses the
active wild-creature fixture here, so carrying an inactive creature cannot hide AI costs. Meshing and field evaluation have
separate workloads. Warmup is excluded, results are consumed to prevent dead-code elimination,
and workload definition digests prevent accidental comparisons between different work.

Baselines are accepted explicitly after inspecting a successful report:

```sh
./scripts/test accept /absolute/render-run --name cave-reviewed-m4
./scripts/test render --game cave --baseline Testing/Baselines/cave-reviewed-m4
./scripts/test accept /absolute/perf-run --name cave-perf-m4
./scripts/test perf --game cave --kind gpu --baseline Testing/Baselines/cave-perf-m4
```

Acceptance never runs automatically and never replaces an existing baseline name. Baseline
folders are local/ignored; publish a reviewed baseline deliberately if the team needs it.
Visual comparisons use the native ImageCompare executable (no Pillow dependency), show a
5× amplified difference image, and require exact pixels by default. An accepted baseline may
specify a mean RGB tolerance via `--visual-tolerance` (0...0.02). Use that deliberately; never
raise a threshold just to make a regression pass. Same-device/OS, test, seed and capture-count
checks precede comparison. Changed capture dimensions fail.

Performance comparisons reject incompatible device/OS, workload definitions and resolution.
An increase must exceed **both 20% and 0.2 ms** to count as a relative regression. Explicit
workload budgets apply independently. Serious thermal pressure or low power mode invalidates
a GPU measurement. New comparisons fail for missing baselines; no silent baseline generation.

## Local checks and CI tiers

Run focused checks while editing. Before handing off shared changes, run `quick`, both games'
relevant scenarios, and the native layers affected by the change. Run seed campaigns separately
from the editing loop. Reserve rendered/visual checks and GPU performance for an exclusive
Mac runner; do not run them concurrently in a CI matrix.

`Testing/ci-check` provides `quick`, `simulation`, `native` and `performance` tiers as explicit
commands. No recurring task or remote CI runner is provisioned. Native tiers require an active
macOS graphical session with Metal; a headless hosted shell is not a substitute for that surface.
