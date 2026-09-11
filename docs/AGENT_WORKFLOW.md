# Working on the running garden

Start appearance work in the separate [Soundstage](SOUNDSTAGE.md). Use `stagectl` in place of `gardenctl` there. Read [AGENTS.md](../AGENTS.md) and [atmosphere research notes](ATMOSPHERE.md).

Use both the native interface and the command tool. Computer use checks whether the app is understandable and operable. Exact commands make numerical and visual comparisons reproducible.

## The loop used to build this milestone

1. Build with `./scripts/build`.
2. Open `.build/Sanctuary.app` with the computer-use app tool. Read the current accessibility tree, then inspect a screenshot.
3. Walk and look through native input. Exercise the scene toggle, lighting controls, scale slider, waypoints, and capture button. Refresh the accessibility tree after actions.
4. Read `./scripts/gardenctl status` when an image or accessibility tree cannot establish a numerical result, such as movement distance or active rendering resolution.
5. For art iteration, pause, choose an exact view, and capture. Edit the shader, call `reloadShaders`, and capture again at the same pose/time. Keep only changes that improve the intended visual result.
6. Run `study` to find flaws hidden by a single angle or lighting setup. Inspect the returned real PNGs; do not substitute generated concept images for renderer evidence.
7. Run `verifyFields` after field/compiler changes, and `./scripts/validate` after input, collision, capture, or command changes.
8. Rebuild and restart for Swift edits. Scene parameters and camera actions do not require rebuilding. Check live update performance after the workload settles; do not run a 15-minute benchmark by default.

The native loop exposed two input issues during development: very short key taps could occur entirely between fixed simulation ticks, and synthetic drags did not supply reliable event delta values. Initial movement taps now produce a small step, while drag looking uses successive window coordinates. These fixes are exercised through computer use, not just through the command interface.

The first performance report also exposed unnecessary off-camera rendering and a CPU metric that included drawable waiting. The renderer now culls instances, selects distance-based detail, and reports drawable waiting separately from encoding.

## Useful recipes

### Reproduce a visual change

```sh
./scripts/gardenctl scene studio
./scripts/gardenctl pause true
./scripts/gardenctl camera --orbit 0.5 --elevation 0.2 --distance 6.8
./scripts/gardenctl lighting morning
./scripts/gardenctl capture --label before
# Edit the shader source.
./scripts/gardenctl reloadShaders
./scripts/gardenctl capture --label after
```

Captures acknowledge only after GPU readback and PNG/metadata writing. They are 1920 × 1080 and exclude HUD controls. Metadata is taken from the frame being captured, not from an earlier command response. Pause prevents wind/time differences from confounding comparisons.

### Test shape changes

```sh
./scripts/gardenctl parameters --scale 1.4
./scripts/gardenctl query --x 6.4 --y 2.5 --z 12
./scripts/gardenctl study
```

The query returns the garden-space seed pod distance bound and terrain value. Studio placement is separate from garden placement, but both use the same shape and scale. The terrain value is a height residual, not Euclidean distance.

### Run a repeatable sequence

`replay` consumes an ordered JSON array of actions. The checked-in `arrival-replay.json` pauses, captures, moves with collision, steps time, inspects the object, and restores a live arrival view. It is deterministic at the simulation-command level; it does not synthesize desktop input or promise floating-point lockstep across devices.

## Protocol

The bridge polls `<workspace>/.sanctuary/inbox` on the app's main thread. A request is an atomically renamed JSON file with a UUID `id` and an `action`. Responses are durable JSON files with the same ID in `outbox`. The CLI waits for that response rather than assuming the action completed. Pending commands from an earlier app session are explicitly rejected at startup.

`status.json` is a heartbeat and state snapshot. The CLI checks it before submitting. If the app is minimized, closed, or paused by the operating system, a capture can time out; restore the window and inspect the acknowledgement before retrying. The protocol is local to the checkout and has no network listener.

Supported actions: `status`, `studioObject`, `view`, `sky`, `scene`, `lighting`, `reset`, `camera`, `move`, `pause`, `step`, `parameters`, `query`, `capture`, `clearMetrics`, `reloadShaders`, `verifyFields`.

Supported parameter ranges: exposure 0.5–1.5; wind 0–2; seed pod scale 0.6–1.7. Invalid values fail before applying any part of that parameter command. Shader pipelines are replaced only after all candidate pipelines compile.

## Interpreting evidence

- CPU encoding time excludes waiting for a drawable; it does not include every simulation, UI, or capture cost.
- GPU time comes from completed Metal command buffers. `frameGPUBySkyPhase`
  groups whole-frame GPU durations by sky update phase (not individual pass
  timings). Metric resets discard older in-flight submissions.
- Frame intervals measure callbacks/submission pacing, not direct presentation times.
- Timing windows contain up to 3,600 recent samples and reset on scene switches.
- Allocated Metal bytes are not total application memory. The benchmark separately records process RSS; do not add the two as if they were disjoint.
- A source fingerprint identifies the compiled Swift/shader inputs. A separate shader fingerprint records runtime shader reloads.
- Captures, shader compilation, GPU verification, and other developer tools can cause hitches. Keep them outside clean performance samples and report concurrent activity.

The numerical GPU check uses 4,096 points for each of six field definitions. This catches backend divergence in primitives, transforms, stretching, subtraction, and blending. It is evidence for the implemented vocabulary, not a proof for arbitrary future operators.
