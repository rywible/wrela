# M6: the Last Green's floor — the task file

The milestone's tasks while it's open (#54 scope, #55 criteria), and the play rounds' logs (AC5).
Deleted when the milestone closes.

## Order of work

1. Language, std, hosts and tools (AC15): constants cached by their inputs; a constant's fuel set
   where it's declared, and constants on several threads; files a build ships beside the program
   (the bake's tiles, fetched); `Text` in a GPU constant; a command over 1 MiB; the allocator's
   lock; the clock; the LOD trap; `wrela run`'s page reload.
2. The engine's vocabulary and tools: `engine::place` (the map's types, tile positions),
   `engine::grow` (the history's general parts), `engine::interest` (the check).
3. The floor (`examples/last-green`): the map, the history, the bake (a cached constant, per
   tile), the check's tests (AC1–AC3).
4. Playing it (AC4): the stand-in, collision on one walkable mask, the third-person camera,
   walks as replays, tile positions.
5. Streaming and caches that follow the player (AC7, AC8): tiles fetched, cooked and dropped
   round the player; the far layer; the wildwood; the forest within its slice.
6. Water (AC9): the mere, the stream, the waterfall, springs; three looks; one surface.
7. Lookdev (AC10–AC12): ground cover, one metering rule, sun shafts (candidate), cooked
   materials, leafless kinds, trunks that differ, the tower and the ring, the far land, trails.
8. The studio (AC5, AC13): the play harness, the map lens, provenance, the lookbook.
9. Play rounds in Chrome (AC5), until the last round finds no blocker.
10. The record (AC16–AC18).

## Status

| AC | State | Notes |
|---|---|---|
| AC1 | tests pass | map in source with engine types; anatomy, area, keepers (2), reach: suite/last_green.rs |
| AC2 | mostly | cached (test), 0.65 MB/km² (test), preview vs bake (test, with drawn crowns), head room (tests.wrela); same bytes in both hosts (F9 hashes the bake: test); preview ≤ 3 s and bake ≤ 2 min to measure (bake ~1:30 with the check) |
| AC3 | tests written | calibrated: 24 places all round within 0.1 (0.032 mean); landmarks ≥ 1° from the renderer's tags and depth (9 relations); dead 14.3%; every relation holds; openings open |
| AC4 | started | collision into each thing, trunks as drawn (tests.wrela); camera near plane, no trap, replays both hosts; camera-relative rendering to do |
| AC5 | started | the floor's walk (P) and the report every 2 s; both-hosts test; rounds to do |
| AC7 | measured | cold: playable 1.09 s, 3.51 MB before (gzip, as a host serves); 5 Mbit/s walk: 0 late; glide pace (over the ground's things) and memory (allocator fixed) to measure again |
| AC8 | to measure | systems' slices test written (serial, native); the walk's frames in Chrome; crowns now drawn as wide as the history grew them |
| AC9 | mostly | mere, streams, fall, springs; one surface, where it is, WGSL bounds (tests); three looks (F6); slice in the systems test |
| AC10 | started | lookbook: stills and 10 s clips in Chrome (test mode's `clip`); trails' contrast test written; flicker to do |
| AC11 | started | no grass under closed canopy (test); one metering rule; exposure settles in 2 s, no flash (cap at 1.25× its target: test); sun shafts to do |
| AC12 | started | cooked bark and stone (WGSL test), leafless kinds; trunks differ (a fork per tree, test); tower relations made out (AC3); far land to do |
| AC13 | to do | map lens, provenance |
| AC15 | done | 5817067, b08cb06 |
| AC16–18 | to do | speed, record |

## Decisions made while working (for the record and the retrospective)

- **The test server gzips** what a host compresses (wasm, js, wgsl, json, bin): load.json counts
  transferred bytes, so the cold start's 6 MB is what a player downloads (3.51 MB).
- **A glide passes over what's on the ground** (M9's gliding is airborne): the pace measure
  needs the eye to move at the pace asked; the measure now reports the pace kept.
- **std's allocator splits and joins large blocks** (and gives back to the bump pointer): a
  freed large block reused for a smaller request lost its tail; the walk into the wildwood ran
  out of memory 6.7 km out. A run case reproduces it (64 MiB then 2 MiB, 40 rounds).
- **A fork per tree** (`Instance::fork`, a bend of the plant's height): chosen in the bake, among
  each tree's neighbours (crowns touching) of its kind, from trees as the bake keeps them;
  wild trees take theirs from a 4 × 4 pattern of their cells.
- **The check sees what the renderer draws**: each tree's crown as drawn (base, top, reach,
  density, trunk), built things as solids, trunks met exactly within 8 m of the eye, crowns as
  ellipsoids (shrubs as cylinders), a crag's site points at its top. Calibrated on 24 fixed
  places, all round.
- **Crowns drawn as wide as the history grew them, spreading half again** (`Instance::wide`,
  `trees::crown_wide`): kinds' crowns were drawn 2–3 times the history's; the wood was darker
  than the history meant (dead share 20%). Trunks and limbs keep their girth.
- **Landmarks rise above the canopy**: the crag 45 m, the Fang 72 m (were 14 and 16, under the
  20 m canopy); a crag's ramp is scree, no trees on it. Views from across their clearings; the
  Fang seen from the Watch (not the town's site, whose wood hides it); rides from the town and
  the arena; the mere's reveal moved west along the Spine (the crag's flank saw the mere).
- **Metering caps a flash**: a scene that brightens at once shows at most 1.25 times its settled
  brightness.

## Plan for what's left

1. AC7: memory measure; commit (gate first).
2. AC12: a fork per tree (`Instance::fork`, a bend of the plant's height; chosen in the bake so
   neighbours of a kind differ; the wildwood's by its cells); the test over the placed trees.
3. Captures for the floor's tests (`test_capture`, `test_camera`, as the clearing's), then:
   AC3's calibration (24 places: `interest::seen_beyond` against the depth) and landmarks
   (≥ 1° from the tags and depth); AC10's trails (8/255 at 20 m) and flicker on the walk;
   AC11's exposure settling in 2 s.
4. AC8: water's slice with water over 30% of the screen; cooking a frame; the full walk 3 runs.
5. AC2: same bake bytes in both hosts; the preview's time; the bake's time.
6. AC16: `wrela check` and `wrela build` times; runtime size.
7. AC4: camera-relative rendering (a render origin in whole tiles) and its test.
8. AC13: the map lens (`wrela studio` on a floor package) and provenance in the running floor.
9. AC11: sun shafts as a candidate (≤ 0.5 ms, switchable).
10. AC5: play rounds (logged here), the polish list; AC18: the record.

## Play rounds (AC5)

None yet.
