# Authoring experience and shared art direction · September 11, 2026

The workshop now has a dedicated 16:9 viewport, a fixed inspection/playback dock,
and Object, Lighting, Look and Files inspector sections. Every slider has a precise
numeric editor and reset. Native and agent edits share a 50-entry undo/redo history;
full named checkpoints persist across app launches. JSON field sources reload after
a debounce, with last-valid-state retention and undoable reloads.

Capture & review archives pixels, metadata and replay state and opens a native
review window. Reports require no HTTP server. Named frozen baselines compare
actual older pixels against current edits or replayed scenes. Style boards group
candidate and working reference objects, sky views and calibration under matching
lighting and scene looks. See `SOUNDSTAGE.md` for commands and publication semantics.

The project-wide `Games/Sanctuary/Authoring/ArtDirection.json` contains the working brief, palette,
reference subjects and render look. Global controls affect shared material shading,
sun/ambient light, sky and atmosphere contributions, and final image grading.
Preview edits are undoable; Publish project look updates the shared source watched
by both applications. Study previews may retain their own captured look.

## Validation performed

- Renderer compiled successfully on Apple M4, including the enlarged Swift/Metal
  uniform layout and the display-grade input.
- `validate-authoring`: 19 checks passed. Covers exact undo/redo pixels, redo
  invalidation, checkpoint restoration, bad-source retention, automatic reload,
  camera-distance preservation, reload undo/redo, global material/sky changes and
  rejected style edits.
- `validate-workshop`: 17 checks passed; `validate-stage`: 15 checks passed.
- Garden validation: 9 checks passed, including generated Metal/Swift agreement,
  grounded movement, deterministic captures and rejection paths.
- Published a warmer look from the workshop. Verified the game loaded it, then
  restored the project file and verified the running game updated its rendered
  world automatically. All four integration assertions passed; the project file
  was restored afterward. Report: `.sanctuary/shared-look-validation.json`.
- Verified that a frozen baseline was copied byte-for-byte while the new rendering
  changed. Replaying the baseline's archived scene with the same look reproduced
  the exact PNG. Report: `.soundstage/replay-validation.json`.
- Exercised native numeric entry, keyboard undo, inspector navigation, checkpoint
  creation, baseline pinning, automatic review opening and the visual wipe.
  Also walked the garden and inspected the real style-board images.

Actual review artifacts:

- `.soundstage/studies/authoring-20260911-082346-77fb8d/index.html`: stored color
  baseline versus an intentionally monochrome current look. Demonstrates that
  the global grade includes the object, floor, lighting and sky together.
- `.soundstage/studies/archived-replay-20260911-083026-d7f6bb/index.html`: exact
  replay against stored pixels, retaining the old side rather than re-rendering it.
- `.soundstage/studies/style-board-20260911-083059-6907c0/index.html`: 15 captures
  of candidate/reference objects, sky and calibration in noon, sunset and softbox.
  Its reference brief is explicitly provisional. Captures retain their own build
  fingerprints; later numeric-editor polish does not rewrite the captured pixels.

The native WKWebView slider did not respond reliably to accessibility value-setting
in the computer-use loop. Dragging worked. Explicit Baseline/Split/Current buttons
were added so agents do not need to rely on precise pointer dragging.

## Short live measurements

Native 1920×1080, 4× MSAA, live wind/cloud updates included, one renderer at a time.
Game, 10.17 seconds: 60.15 submitted fps; GPU median 13.88 ms, p95 14.81 ms,
max 19.61 ms; frame interval median 16.66 ms, p95 17.63 ms, max 22.76 ms.
Report: `.sanctuary/profiles/shared-art-direction-game-20260911-083547.json`.
Soundstage, 10.23 seconds: 59.83 submitted fps; GPU median 7.06 ms, p95 8.73 ms,
max 10.31 ms. Report: `.soundstage/profiles/authoring-experience-final-20260911-084028.json`.
These short samples are not a sustained thermal guarantee; occasional long frames
remain, and they do not establish a controlled before/after performance difference.

## Boundaries

Global grading does not make incompatible geometry, animation or cloud structure
stylistically compatible. Shape language, detail hierarchy and deliberate grouping
remain authoring decisions guided by the brief and reference board. The initial
assets remain working references, not approved final game art. Existing lighting
approximations are unchanged: indoor fill is not full scene GI and finite-source
softness is approximate. This delivery makes those decisions easier to inspect,
compare, revise and carry consistently into the game.
