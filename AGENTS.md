# Working on Sanctuary

Build one beautiful, playable wildlife sanctuary game. Author visual content as
Swift fields and procedural code. Compiled meshes, noise textures and lighting
caches are representations of that source. World units are metres; the seed pod
is 35 mm tall. Target native 1920×1080 at 60 Hz on the M4 MacBook Air.

## Start with the soundstage

Read `docs/SOUNDSTAGE.md` and `docs/ATMOSPHERE.md` before changing appearance.
Use `.build/Soundstage.app` for sky, lighting, material and individual field work.
Start authoring with `stagectl catalog`, `subject`, `rig`, and `saveStudy`.
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
6. Run `swift test` for field/compiler changes, `--check-renderer` for shader/ABI
   changes, and the appropriate local validation script for protocol changes.
   `validate-workshop` checks editing, atomic rejection and exact study/motion replay.
   Test rejection paths and deterministic paused captures.
7. Check the garden after shared renderer changes. Measure live clouds and wind,
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
- SanctuaryGame owns scene assembly, shared Metal rendering and local tooling.
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
