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
| AC2 | mostly | cached (test), 0.65 MB/km² (test), preview vs bake (test, with drawn crowns), head room (tests.wrela); same bytes in both hosts (F9 hashes the bake: test); preview 2.38 s (measure); bake ≤ 2 min after an edit to measure |
| AC3 | tests written | calibrated: 24 places all round within 0.1 (0.032 mean); landmarks ≥ 1° from the renderer's tags and depth (9 relations); dead 14.3%; every relation holds; openings open |
| AC4 | tests pass | collision into each thing, trunks as drawn (tests.wrela); camera near plane over the walk (a card's half-width counts: 1456 cards met, all faded), no trap, replays both hosts; camera-relative rendering (origin test) |
| AC5 | rounds | harness done (taps, travel, walk, reports both hosts, map M); rounds by tools/round.py (Chrome, paced 1080p, replay saved, lookbook each round) |
| AC7 | measured | cold: playable 1.09 s, 3.51 MB before (gzip, as a host serves); 5 Mbit/s walk: 0 late; glide pace (over the ground's things) and memory (allocator fixed) to measure again |
| AC8 | to measure | systems' slices test written (serial, native); the walk's frames in Chrome; crowns now drawn as wide as the history grew them |
| AC9 | mostly | mere, streams, fall, springs; one surface, where it is, WGSL bounds (tests); three looks (F6); slice in the systems test |
| AC10 | tests pass | lookbook: stills and 10 s clips in Chrome; trails' contrast test; flicker on the walk (look 1.36/255, 2.2% vs plain 7.42/255, 39%); the fawn and the stand-in in three shots (16–18); owner's verdicts pending |
| AC11 | mostly | no grass under closed canopy, cover's reach (tests); one metering rule; exposure settles in 2 s, no flash (test); sun shafts a candidate (B), cost to measure; owner's blind pick pending |
| AC12 | mostly | cooked bark and stone (WGSL test), leafless kinds; trunks differ (test); tower in courses with a stepped break (honest bound 6); far land's fields and footprint-limited cover; owner's verdict pending |
| AC13 | done | map lens: session in both hosts, 3.5 s drag to map (test); provenance: click in Chrome 5 ms (test) |
| AC15 | done | 5817067, b08cb06 |
| AC16 | gate passes | gate 70 s (budget scaled for the cores other processes took); slow checks long; WGSL budgets; runtime 105 KB (38 KB gz) of 1 MB; check/build speed to measure |
| AC17 | deviation | no subagents (the owner's instruction for this session): not run |
| AC18 | to do | vision.md, #26, retrospective |

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

- **A debug build probes only a grid that fits its probe:** a `Slots` index counts the
  dispatch's every group, so a block's slot under `PROBE_BLOCKS` could still index past it.
- **A leaf card fades where its edge nears the eye** (`vegetation::faded` takes its half-width):
  a wide card's centre could sit off the line to the player while its edge met the near plane.
- **The fawn in the floor stands at rest** (a rigid placement of every bone), realized over
  frames only when a shot casts it: M6 judges the cel treatment in the wood, not its gait.
- **Flicker is measured against plain rendering on the floor's walk** (M5's Q8), with the wind
  stilled and water, sky and the stand-in left out (#55's Method).
- **The gate stays as it was; check.sh's budget allows for other processes' cores.** The gate
  took 175 s because the floor was still running in a hidden browser page from earlier play,
  holding the GPU: closed, the gate takes 52 s. (Moving its slow checks to the long tier was
  reverted.) check.sh keeps its fixes: the suite starts first, side jobs say their time, a unit
  test's long name is libtest's, and the budget scales by the cores other processes took.
- **One moving cache's work a frame** (AC8): a terrain cook, a shadow strip or a probe strip;
  at a coarse level's step (every level moves) the clouds' tile waits too. The near cascade in
  12 strips; the first full frame waits for the built things.
- **Rounds are played through the harness in headless Chrome** (tools/round.py): the walk, the
  shots held, the wildwood's road; the replay, the frames' intervals, late tiles, regions and
  sites, and the lookbook each round. Spike 17's stills are in target/spike17-stills.
- **Posting to GitHub is the owner's:** the retrospective and #26's update are drafted here.

- **Play rounds are quick (the owner's direction, 2026-10-10):** "you should be able to identify
  areas for improvement and bugs in a couple minutes of play, not an hour". `round.py --quick`
  plays about 2.5 minutes: a travel to every fourth site (each region's), a 2 s run and a look
  round at each, six lookbook shots held, the screen saved every 130 frames. The 30 and 60
  minute rounds stay in the tool (`--minutes`), but the rounds from round 4 on are quick: a
  deviation from #55 AC5's 30 min and 1 h rounds, by the owner's word.
- **Wind is gusts, not a period (the owner's question 2):** a gust field, noise carried
  downwind at the wind's speed and turning slowly, so gusts sweep across the wood and the
  meadow; a plant feels it with a lag (its response a mix of the gust now and a second ago),
  rocks by the change, and flutters with it. Still a pure function of the wind and the place.
- **Trees grown as trees (questions 3 and the roots):** a broad-leaved tree's trunk crooked and
  leaning, buttress roots flaring into the ground (4 to 6, uneven), 3 to 5 scaffold limbs from
  their own heights at the golden angle, low limbs under the crown so its foot is ragged, the
  leader up into the crown; clumps in masses round the scaffolds' ends, sizes varied. Saplings
  have limbs (a ball on a stick read as a lollipop). Snags snap bluntly, with splinters and
  short crooked limbs (a tapered top read as a trident).
- **The floor's plants from the NVC (question 4):** 15 woodland species from W7, W8, W10, W14
  and W16, and 12 of the open's from MG5 (knapweed meadow), U4 (acid grassland) and M23 (rush
  pasture): each a shape, size, colour and flower, and a habitat (acidity, wetness, light,
  wear). A plant takes its colony's species (4.5 m, warped), some its own; a tenth of the
  meadow's places are forbs. Sources: the NVC's descriptions (JNCC, Rodwell's volumes), W8 and
  MG5 summaries.
- **Geology under the ground (question 5):** bedrock in beds (sandstone and mudstone, dipping and
  folded) with granite at the crags and the Watch; soil deep in hollows and thin on slopes and
  crests, rock where it's thin; acidity from the rock (granite and sandstone acid), which the
  species follow. Cooked detail tiles (leaf litter, soil, rock, moss), sized so the look's
  painting keeps them, rock across the planes it faces. The grass varies in tussocks and
  colour patches.

## Session 2 (2026-10-10): order of work

1. Done: gate fixed (layering), map lens tested both hosts, Chrome heap-growth trap fixed, polled
   tasks with no helpers, provenance, the harness map.
2. Camera-relative rendering (AC4) and its test.
3. Ground cover plants: ferns, bracken, herbs from the history's fields (§9, AC11).
4. Sun shafts as a candidate, switchable (AC11, Q6).
5. The far land: the marbled pattern, the valley's fields (AC12, polish list).
6. Flicker on the walk (AC10); the fawn and the stand-in in three shots (AC10).
7. Measures: AC7 memory, AC8 three runs and slices, AC9 water slice, AC16 speed, AC2 bake time.
8. Play rounds (AC5), the lookbook each round, the polish list.
9. The record (AC18).

## Plan for what's left (session 1's)

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

Each round: `tools/round.py <n>` (Chrome, 1080p, paced at 60 Hz, eight threads): the floor's walk
from the gate (P, 25 min at a run, through every region and the water and past each lookbook
place), then each lookbook shot held 15 s; the replay, the summary and the lookbook in
target/rounds/round-<n>/ (spike 17's stills in target/spike17-stills, round 0's lookbook in
target/lookbook/round0).

### Round 1 (2026-10-10, 30 min of play, 58 min of wall time)

- **Route:** the walk from the gate through all ten regions (the wood pasture, the beech wood,
  the alder carr, the pines, the spine, the burn, the windthrow, the cut, the edge, the watch),
  18 sites within 25 m (the gate, the windthrow, the spine's waystone, the crag, the shore's
  rest, the ring, the fallen giant, the cut's pool, the lip, the edge, the ring's outlier, the
  town's site, the tower, the silver snags, the burnt hall, the old quarry, the last rest, the
  warden's arena); water to 0.48 m (waded). Then the 19 shots.
- **What drew:** the check's report ahead mostly features (0.2-0.3); the tower over the canopy
  from the crag's top; the Fang from the old road.
- **What failed (blocker):** the frames. Paced, the floor ran at 30 fps: frames every 33.3 ms
  (median), 51,740 over 33 ms, worst 129. Back to back it was 10 ms a frame, but the M4's GPU
  lowers its clock under a paced load until a frame fills about three quarters of its interval,
  and the floor's frame missed 16.7 ms at that clock and settled at 30 Hz. Nearly half of it
  was every built thing (the tower at 2 cm, about a million vertices, every stone on the floor)
  drawn every frame from anywhere: Apple's GPUs run a pass's vertex work before its first
  timestamp, so it hid as a 3-4 ms gap between timed passes.
  **Fixed** (aa9babb): built things only in view and near enough to show, the tower's far mesh
  past 80 m: back to back 5.74 ms median, paced 60 Hz (16.9 ms median, GPU spans 12.3).
- **Late tiles:** 18, every one in the 3 s after a lookbook shot moved the camera to a new place
  (a jump, not a pace streaming holds). **Fixed:** the floor counts none in the 3 s after a jump.
- **What looked wrong** (the lookbook): dabs on far ground and rock read as dark dots (the old
  road's crag, the fall's cliff, the plateau's lip); the crag a smooth dome; the stream below the
  fall a wide white ribbon across the valley; herbs under the beeches upright and narrow, like
  grass; a green cast on the beech trunks; the tower stands in a mown lawn.
  **Fixed for round 2:** ground dabs at 60% of a crown's far strength; rock faces in ledges 3 m
  tall and stronger knobs; a fall wide and white only where it falls steeply, the stream below
  at its own width and less foam; herbs broad, low leaves in rosettes. **Left:** the trunks'
  green cast and the tower's lawn (next round).
- **Stills:** target/rounds/round-1/lookbook beside target/lookbook/round0.

### Round 2 (2026-10-10, 30 min of play, 40 min of wall time)

- **Route:** as round 1's: all ten regions, the same 18 sites, water to 0.51 m; the 19 shots.
- **Late tiles:** none.
- **Frames:** every 21.9 ms (median), 410 over 33 ms, the same in every region and from the
  first minute. Not the floor's: test mode's paced runs hashed every submitted byte (for the
  replay's state hashes) and waited for each frame's GPU work before the next could begin, so
  a frame's interval was its CPU time (4.9 ms with the hashing) and its GPU time (13.4) in a
  row. A page in play does neither. **Fixed in the harness:** `ticklog=1` keeps the tick log
  (the replay) without hashing the frames' commands, and `inflight=2` lets two frames' GPU work
  be in flight while each still begins at its time. The same 30 s paced: 16.6 ms median, p99
  18.5, none over 33.
- **What looked wrong:** the burn's shot stood behind two trunks that filled the frame; the
  mere's shot had a shrub over half of it. **Fixed:** both shots' eyes moved (the mere's now
  across open water, a faded sapling's stem at its left third). The far hills' dots are much
  softer; the tower stands in rough grass and heather; the stream below the fall narrows.
- **Spike 17's stills beside them** (my comparison, not blind): the beech wood (the spike's near
  black), the ring (white sponge-like stones on green plates), the tower from a height (a few
  pixels in the spike) and the old road (the path and its waystone read) are better; the
  opening's far hills were dotted in round 1 and are soft now.

## The retrospective (draft, for a comment on #55; the owner posts it)

**What M6 built.** The Last Green's floor: about 3 km² composed as a map in wrela source with the
engine's types, grown by a history baked when the game is built (a cached constant, a file a
tile), checked by the interest check against what the renderer draws, streamed round a stand-in
player in tiles, with the wildwood beyond its edge and the egg's place 10 km out; water (the
mere, the streams, the fall, springs: one surface, three looks); the forest's interior (ground
cover from the history's light, one metering rule, sun shafts as a candidate); cooked bark, rock
and masonry; leafless kinds; trunks that differ; the tower in courses; camera-relative rendering
on tile positions; the play harness, the map lens with provenance in the running floor, and the
lookbook. Language and tools: constants cached by their inputs, a constant's fuel, the clock,
the diagnostics spike 17 found, the allocator's lock, the LOD trap, `wrela run`'s reload.

**Spike 17 beside M6** (filled from the measures): SPIKE17-VS-M6

**The play rounds:** ROUNDS

**What moved, and why:** MOVED

**What the owner judges next** (AC6, AC9's pick, AC10's verdicts, AC11's shafts, AC12's far
land, AC14's rounds): the first walk.
