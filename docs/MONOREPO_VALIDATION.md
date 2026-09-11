# Monorepo migration validation · September 11, 2026

## Structure and correctness

All three release apps build. The field/compiler, Sanctuary and Cave test suites
pass **51 tests**. A fresh `swift build --scratch-path /tmp/wrela-cave-independent-build
--product Cave` completed without compiling Sanctuary or Soundstage.
`scripts/check-boundaries` verifies imports and product dependency closures.
Both games compile the shared Metal pipelines; the diffuse irradiance integration
check on Apple M4 reports maximum absolute error **0.006604582**.

Running-app checks passed:

| Suite | Checks | Local evidence |
| --- | ---: | --- |
| Creature anatomy, poses, rejection and deterministic brain replay | 33 | `.soundstage/creature-validation.json` |
| Workshop edits, rigs and exact study replay | 17 | `.soundstage/workshop-validation.json` |
| Undo, checkpoints, source watching and global style | 19 | `.soundstage/authoring-validation.json` |
| Sky/protocol and GPU traversal verification | 15 | `.soundstage/validation.json` |
| Project catalogs, ranges, animation, interior cameras and isolation | 12 | `.soundstage/project-validation.json` |
| Standalone game host, movement, captures and malformed commands | 9 | `.sanctuary/validation.json` |
| Original expedition, persistence and habitat loop | 15 | `.sanctuary/expedition-validation.json` |
| Cave collision, visibility, encounters, objective and persistence | 15 | `.cave/validation.json` |
| Published cave edits reach game pixels and collision | 3 | `.cave/publishing-validation.json` |

The paused pre-migration study reproduces **byte-identical native 1920×1080 PNGs**
after extraction, including the move of surface recipes into the game project.
Evidence: `.soundstage/monorepo-image-comparison.json`. This is a matched-view
regression check, not proof that every possible scene is visually identical.

## Native computer-use loop

Native controls were exercised for project switching, semantic Watcher anatomy,
motion stepping/duration, scenario selection, flashlight switching, beacon
recovery and walking out. Cave interior previews were inspected in the actual
Soundstage window. Sanctuary rescue and release were exercised through native
controls after exact debug positioning; ordinary movement/collision also has
running-app and numerical checks. This is not a claim of a complete unassisted
walking playthrough or a user study of fun/scare effectiveness.

The loop caught useful defects: global-grid loss of small cave outcrop detail,
creatures positioned behind solid walls, stale inspector labels/ranges when
projects reused parameter keys, misleading event timestamps and an outdoor-only
inspection camera. These were corrected at their owning boundaries.

Validation used named game saves and restored the original slots. Temporary
published geometry/creature recipes were backed up and restored. The game reload
replaced both render geometry and the field queried for collision. Shared runtime
sources do not contain a separate Cave or Frostling renderer.

## Short performance observations

One renderer was running during each ten-second live measurement; no captures or
concurrent builds were included. These are fixed-view observations, not a thermal,
full-route or presentation-rate guarantee. No 15-minute benchmark was run.

| Workload | Submitted fps | GPU median | GPU p95 | GPU max | Frame interval max |
| --- | ---: | ---: | ---: | ---: | ---: |
| Cave entrance | 60.08 | 12.15 ms | 12.50 ms | 12.78 ms | 31.88 ms |
| Sanctuary, saved resident scene | 57.68 | 12.94 ms | 17.97 ms | 24.87 ms | 49.49 ms |

Reports: `.cave/profiles/monorepo-cave-20260911-121723.json` and
`.sanctuary/profiles/monorepo-sanctuary-20260911-122759.json`.
Sanctuary's largest GPU frame occurred during the sky-update phase. These results
do **not** establish the requested comfortable 1080p/60 margin. The optional
atmosphere path prevents Cave from allocating/updating the large outdoor caches;
future performance work should still address shared rendering cost and outdoor
update spikes.

## Practical limits

The Cave game is an abstraction test with prototype rock and creature art,
procedural spatial sounds, a beacon objective and two bounded encounters.
It does not provide general navigation or dynamic acoustics. Soundstage exposes
registered anatomy, clips and deterministic scenarios; the registration API and
integer material contract are still evolving source interfaces.

## Review artifacts

The shared motion authoring report contains twelve actual frames across the
Watcher's complete lunge, from front and quarter views:
`.soundstage/studies/cave-shared-authoring-final-20260911-124414-d41935/index.html`.
The interior report contains passage, junction and chamber views under the
flashlight: `.soundstage/studies/cave-interior-authoring-20260911-123958-61a3ee/index.html`.
Each report includes exact study state and source/shader fingerprints.
