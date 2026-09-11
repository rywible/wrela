# Field garden validation

Measured on September 10, 2026, on the development MacBook Air: M4, 8 GPU cores, 16 GB memory. The app is a release build rendering to a fixed 1920 × 1080 target.

Compiled source fingerprint:

`45260d371709d28af0d49c8b691df803913d9ebaa2255cd10084d096a145ff78`

## Correctness

- 10 Swift unit tests passed.
- 10 running-app validation checks passed.
- Generated Metal field evaluation matched Swift over 24,576 samples across six graphs. Maximum absolute error was approximately `9.54e-7`, below the `2e-4` acceptance tolerance.
- Two paused captures with identical settings produced identical PNG contents at 1920 × 1080.
- A 24-metre commanded route remained grounded. A forward movement attempt toward the seed pod stopped at its collision surface rather than passing through it.
- Live pod scaling changed both captured geometry and the garden-space distance query.
- Out-of-range and extreme command values were rejected without applying invalid state or crashing the app.
- No Metal command errors were reported during these checks.

## Computer-use evidence

The app was launched, inspected, and operated through the Codex computer-use tool. Native controls exercised included the garden/studio toggle, lighting, scale slider, keyboard movement and looking, and mouse drag looking.

Two short W taps moved the player by 0.24 metres after the input fix. An 80-point horizontal and 15-point downward drag changed yaw by 0.24 radians and pitch by -0.045 radians, verified through the running app's state report.

The nine-view studio study revealed incorrectly oriented triangles at a hard subtraction edge. Orientation now comes from the tetrahedron's linear interpolant rather than the exact field gradient at the polygon center. The regression test was explicitly run against the previous algorithm and failed; it passes with the fix. Small faceted cut-edge artifacts remain an approximation-quality issue, documented in the README.

## Short performance sample

The user requested skipping the proposed 15-minute benchmark. A 60.1-second fixed-arrival-view sample ran on battery with animated wind, no screenshots during sampling, and no concurrent build.

| Metric | Observed |
| --- | --- |
| Submitted frames per second | 59.89 |
| Final-window GPU median | 8.52 ms |
| Highest rolling GPU p95 | 12.41 ms |
| Final-window GPU p99 | 13.79 ms |
| Final-window CPU encoding median | 0.29 ms |
| Final-window CPU encoding p95 | 0.50 ms |
| Final-window frame callback median | 16.67 ms |
| Final-window frame callback p95 / p99 | 20.22 / 28.34 ms |
| Maximum process RSS sampled | 164.2 MiB |
| Visible triangles at arrival | 607,885 |

These results establish a useful initial GPU budget, but do not establish perfectly paced presentation or sustained thermal performance. Callback timing has tail spikes that deserve further investigation. Submitted frame rate is not direct presentation telemetry, and this single view does not represent the future game's full workload.

Raw local reports are generated in `.sanctuary/validation.json`, `.sanctuary/benchmark.json`, and `.sanctuary/study.json`. Each captured image has a sidecar recording the exact view, parameters, time, and source/shader fingerprints. They are intentionally excluded from version control.

## Remaining work

Refine appearance and frame pacing, then add one creature and a complete tracking/capture expedition. Ray tracing, persistent terrain editing, ecological simulation, and world streaming remain later milestones. This validation applies to the initial field garden and agent loop.
