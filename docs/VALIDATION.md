# Current validation — September 11, 2026

M4 MacBook Air, 8 GPU cores, 16 GB. Native 1920×1080, four-sample MSAA,
release build. The original garden measurements are preserved in
[the historical report](VALIDATION_INITIAL.md); they do not describe today's
higher-detail mesher or atmosphere.

## Correctness and visual checks

- 17 Swift field/compiler tests passed, including shared cut-edge topology,
  constrained dual extraction, simplification, meshlets and wind replay.
- The complete Metal renderer compiled on Apple M4.
- 11 Soundstage checks passed: native capture size, exact paused repetition,
  isolation, atomic invalid edits, wet/dry appearance, sun projection and metadata,
  plus zero GPU errors.
- Nine garden checks passed: Swift/Metal field agreement, grounded movement,
  native and repeatable captures, fixed stepping, live scale/query response,
  parameter rejection and zero GPU errors. The old giant-seed collision assertion
  was removed because the seed is now 35 mm tall; it was no longer a valid
  obstacle test. This suite does not certify every solid collision configuration.
- Inspected a six-light, three-angle sky matrix, a seven-light, four-angle stone
  matrix, wet/dry seed frames and three weather seeds. The reference audit
  documents visual findings and remaining approximations.
- Used native Soundstage lighting/sky controls and native garden movement.
  The garden check confirmed that the shared atmosphere appears in the game;
  a screenshot cannot certify absence of all temporal flicker.

## Performance

Live cloud updates are included in measurements. Developer-triggered shader,
weather rebuild and capture costs are separate. `frameGPUBySkyPhase` groups
whole-frame GPU durations by update phase; these are not isolated pass timings.
Metrics resets discard older in-flight submissions.

Final garden sample: 25.6 seconds, 59.59 submitted frames/s.
GPU median 16.50 ms, p95 21.66 ms, p99 26.96 ms.
Frame callback median 16.67 ms, p95 17.84 ms, p99 19.40 ms.
The GPU tail still exceeds 16.7 ms; steady 60 Hz presentation is not established.
This sample had no concurrent build or capture. Raw report:
`.sanctuary/sky-integration-performance.json`.
No 15-minute benchmark was run. Submission/callback timing is not direct display
presentation telemetry and does not establish sustained thermal performance.

## Evidence

Local reports are under `.sanctuary/` and `.soundstage/`, with exact-frame
source/shader fingerprints in capture sidecars. See [the visual audit](SKY_VISUAL_AUDIT.md)
and [atmosphere notes](ATMOSPHERE.md) for the representation and its limits.

Final isolated live-sky sample: 3035 recent frames, GPU median
5.03 ms, p95 12.98 ms, p99 14.16 ms. This includes
light propagation and angular cache updates. Raw report:
`.soundstage/final-live-performance.json`. The final finite-footprint fade was
added after this timing sample; it changes a few scalar operations per cloud
sample, so these numbers are for the immediately preceding shader fingerprint.
