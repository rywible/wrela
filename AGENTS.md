# Working on Wrela

Build Sanctuary as the flagship; use Cave as a bounded abstraction stress test. Author visual content as
Swift fields and procedural code. Compiled meshes, noise textures and lighting
caches are representations of that source. World units are metres; the seed pod
is 35 mm tall. Target native 1920×1080 at 60 Hz on the M4 MacBook Air.

## Start with the soundstage

Read `docs/SOUNDSTAGE.md` and `docs/ATMOSPHERE.md` before changing appearance.
Use `.build/Soundstage.app` for sky, lighting, material and individual field work.
Start authoring with `stagectl project sanctuary`, `stagectl catalog`, `subject`, `rig`, and `saveStudy`.
Read the selected project’s `Games/<Project>/Authoring/ArtDirection.json` before authoring objects, lights or skies.
Use the shared scene look rather than compensating individual objects with exposure.
Run `stagectl styleBoard` to compare the candidate with working references and
lighting/sky views; the initial references are provisional, not an approval stamp.
Use `captureReview`, frozen review baselines and named checkpoints for fast iteration.
Subjects and lighting are independent. The tree source is shared with the garden;
save it with `assetSave`, then use `gardenctl reloadAssets` to verify integration.
Use `workshop-study --baseline <study.json>` for matched visual comparisons and
motion scrubbing. Read its HTML viewer, not just the command success result.
It shares the game renderer but builds no garden population and has a separate
`.soundstage` command/capture directory. Use Sanctuary for movement and scene
integration after the isolated object looks right.

1. Build with `./scripts/build`. Swift edits require quitting and reopening the
   relevant app. Shader edits support `./scripts/stagectl reloadShaders`.
2. Use the Codex computer-use app tool to open the app, read its accessibility
   tree and inspect its actual screenshot. Exercise relevant native controls.
   Refresh state before input. Respect the user's ongoing interaction.
3. Use `stagectl` for exact camera, lighting, sky and time settings. Pause before
   before/after captures. A capture acknowledgement means both the GPU readback
   and its metadata have been written. Inspect the real PNGs.
4. Compare front, quarter, back and above under noon, golden hour, sunset,
   afterglow, overcast, rain and indoors. Check wet and dry when material changes.
   For skies, inspect sunward, away and zenith, then run live and move the view.
5. Preserve a study report with source/shader fingerprints. Do not use generated
   illustrations as evidence of the renderer. A successful build is not visual
   validation. Do not call a result majestic or production quality without
   inspecting it; describe remaining defects candidly.
6. For creatures, read the creature section of `docs/SOUNDSTAGE.md`. Start with
   semantic anatomy controls, then inspect idle/blink, hop contacts and behavior
   scenarios. Run `scripts/validate-creatures`, inspect a motion strip and exercise
   the native Motion/Behavior controls. Keep gameplay and authoring on the same
   brain and pose functions. World Y is up and creature forward is -Z; joint
   rotations are right-handed degrees. Inspect a side view to check reach/lean
   directions, not just the front silhouette. Preserve the cute, soft face when refining Frostling.
7. Run `swift test` for field/compiler changes, `--check-renderer` for shader/ABI
   changes, and the appropriate local validation script for protocol changes.
   `validate-workshop` checks editing, atomic rejection and exact study/motion replay.
   `validate-authoring` checks undo/redo, checkpoints, file watching and global style.
   Test rejection paths and deterministic paused captures.
8. Check the garden after shared renderer changes. Measure live clouds and wind,
   not only the paused sky. Report update hitches separately from steady frames.
   Use `scripts/profile` for short live measurements, `scripts/check-sky-motion`
   for aged live/reference pairs, and `stagectl verifySky`
   when changing cloud traversal bounds. Counter stage intervals overlap and
   must not be added into a frame time.
   Short checks are sufficient by default; the user declined a 15-minute test.

## Architecture and truthfulness

- FieldCore owns mathematical definitions, units, conservative bounds and wind.
- FieldCompiler owns sparse dual extraction, tetrahedral ambiguity fallback,
  normal/material boundaries, mesh simplification and meshlet generation.
- Read `docs/ARCHITECTURE.md` before changing ownership or adding gameplay.
  Read `docs/CAVE.md` for the second game and interior authoring.
- SanctuaryContent owns this game's terrain, expedition rules and versioned saves.
- Each Games/<Project> owns content, semantic recipes, material snippets, art direction,
  behavior adapters and GameExperience. Engine/GameHost owns the native play loop;
  Engine/FieldEngine draws supplied frames. Tools/SoundstageKit imports no game.
- Register controls, clips, scenarios and interior views through GameProject and
  AssetGenerator. Do not add species/project switches to shared editor code.
- Select the project explicitly (`stagectl project sanctuary` or `project cave`)
  before authoring. Check catalog; parameter IDs are scoped to the project.
- Game hosts do not depend on Soundstage or the other game. Run check-boundaries.
  Test Cave with validate-cave, Sanctuary with validate-expedition and validate.
  Never put game/editor state back into MetalRenderer.
- Run `scripts/check-boundaries` after architecture changes. For gameplay, run
  `swift test` and `scripts/validate-expedition`, then play the native loop.
  Use a named expedition test slot; preserve the player's default save.
- Meshes remain the primary surface renderer. Keep generated Metal field
  evaluation for numerical verification. Do not revive the deleted all-field
  ray marcher as a second rendering stack without a concrete use case.
- Hillaire atmosphere, volumetric cloud integration and weather simulation are
  distinct systems. State approximations explicitly. Procedural advection is
  not a thermodynamic atmosphere; sky irradiance is not full scene GI.
- Keep Swift/Metal uniform layouts synchronized. Hot reload must replace the
  whole candidate pipeline set only after every shader compiles.
- Never hide performance regressions by excluding normal live cache updates.
  Developer-triggered rebuilds/captures may be measured separately.
- `manifold` in the current extractor report checks oriented edge incidence;
  it does not certify vertex links, self-intersections or collision equivalence.

Continue routine reversible work within the user's authorization. These
instructions add no permission gate, benchmark requirement or delegation rule.

## Testing harness workflow

Read `docs/TESTING.md` when changing testing, simulation ownership, persistence or performance.
Prefer `scripts/test quick --game <owner>` during game edits and `scripts/test quick` for shared
changes. Use `scripts/test run` for CPU-only production scenarios; `render` for native state/image
checks; `host` for ordinary runtime adapters; `gpu-check` for numerical Metal/field verification.
The legacy validate entry points now launch isolated harness sessions automatically.

Author tests/workloads in `Games/<Game>/Testing`; keep shared TestKit free of game imports.
Distinguish fixture setup from movement under test. Do not substitute test-only movement,
visibility, AI or collision implementations. Keep canonical snapshots deterministic across
processes. Test rejection paths and preserve state if persistence fails.

Use failure artifacts directly: `scripts/test replay <artifact>`; `inspect <artifact> --step N`
for the game, or `inspect <artifact> --stage` for recorded creature authoring. Inspect actual
images and exercise relevant native controls through computer use. Imported Soundstage brains
use studio stimuli when resumed; whole-game correctness belongs to game replay.

Run GPU work serially, with other Wrela render windows closed. Never kill unrelated sessions
or silently accept/update baselines. Use short live workloads by default. Preserve p95/max
spikes and workload/device provenance. Keep reports and replay commands in the handoff; avoid
including every capture or build log in the final response. Test roots and authored inputs must
be isolated from player saves and published content.
