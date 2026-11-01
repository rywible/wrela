# Bench

Performance claims about wrela's programs are recorded here: what was measured, on what, with
which build. The plan these come from is [plan-world-class.md](plan-world-class.md).

Each `wrela tool bench` table below is one run: per scene, the GPU's frame span (saturated,
timestamped), the intervals between paced frames as a player sees them (and how many were over
16.7 and 33 ms), each paced frame's CPU time, the native host's frame call (its GPU waits
included), a cold start (first frame, the program's `playable` print, the pipelines' build,
the bytes loaded) and the GPU memory at its most. 1920×1080 (the capybara also 2940×1912), 60
warmup frames; Chrome is headless Chrome stable (155.0.8059.86) driven by `std::tool::browser`.
From toolchain 0.12.0 a frame shows when it's submitted: the first frame and the playable time
(the frame the program printed `playable` in) are their frames' submissions (`submitted_ms`);
rows on earlier toolchains took the frames' starts (`began_ms`), which come before their
pipelines let them be drawn. The intervals and throughput stay on starts, which a timer paces.
Other engines' rows ("Other engines on this machine") keep their own harness's first frame.

**Reading the intervals.** Headless Chrome's paced frames jitter round the display's 16.67 ms: the
leapling, whose GPU frame is 2.1 ms, has 226 to 302 of its 600 intervals over 16.7 ms and a 99th
percentile of 18.2 to 19.6 ms. So a count over 16.7 ms measures the timer's jitter as much as
the program, and a frame the program missed shows as an interval over 33 ms (a refresh skipped).
The interval columns of one run move by a few ms from run to run (the repeats below).

## Measuring

**Sustained** (`wrela tool bench -- --sustained <s>`, docs/plan-world-class-3.md §C3, D39): a
fanless machine's GPU slows as it warms, so a gate measured cold says too little. A sustained
run opens the play run's page again and again for `s` seconds before its measured run (the walk
from its start each time), keeps each page's GPU spans, and records in `sustained.json` the median
span of the pages begun in the first minute and of those begun in the last (`minute_1_ms`,
`minute_last_ms`) and their ratio; a ratio over 1.15 is marked "throttling" (the host reports no
thermal reading, so the ratio and the time of day are the machine's state). The measured play run
then runs with one frame in flight (test mode's `inflight=1`, where the plain run has two): the
worker paces frame starts by a timer, records each start (`began_ms`) before drawing, and waits
for the oldest frame's GPU work once two are pending, so a frame starts only after the one two
before it is done on the GPU, and a cadence of starts is a lower bound on the GPU's throughput.
`throughput.json` records how many frames started, over how long, how many a second and how many
intervals were over 25 ms: frame starts, not presented frames (presentation isn't visible from a
page). A row's "Sustained" cell says `sustained <s> s` with the two minutes, the ratio, the starts
a second and the intervals over 25 ms; "no" where the run wasn't sustained. The gates read the
measured frames: G-sustained's (§C6) are a GPU p50 under 12 ms, a paced p99 under 20 ms, no
interval over 33 ms, and at least 59 starts a second (`report::gates`: a steady 19 ms stream,
52.6 a second, fails on starts alone).

## 2026-10-25: the baseline, B1 (`071d717b`), on toolchain 0.6.2

Apple M4 MacBook Air (macOS 26), `apple metal-3`. B1's code (the plan's §B1: before any of
Part B's engine changes), its pin moved to 0.6.2 in a worktree so both sides run one toolchain.

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | GPU peak MiB |
|---|---|---|---|---|---|---|---|---|---|---|
| clearing-path | 10.53 / 11.34 / 11.59 | 15.60 / 17.82 / 18.38 | 132 / 1 | 0.78 / 1.62 | 9.42 / 11.60 | 328.53 | 3166.82 | 2523.97 | 4.2 | 475.5 |
| last-green-shot | 21.82 / 22.58 / 27.98 | 22.31 / 28.65 / 32.00 | 357 / 4 | 5.18 / 6.46 | 17.34 / 19.46 | 495.02 | 11839.44 | 5197.02 | 14.0 | 1468.4 |
| last-green-walk | 15.26 / 18.31 / 21.23 | 15.42 / 19.50 / 32.01 | 382 / 7 | 4.66 / 5.58 | 14.90 / 20.47 | 543.53 | 12012.78 | 5246.75 | 13.9 | 1469.4 |
| capybara-run | 1.69 / 1.74 / 1.80 | 16.60 / 18.79 / 19.47 | 287 / 0 | 0.21 / 0.84 | 0.56 / 1.73 | 167.44 | 0.00 | 546.58 | 2.8 | 44.3 |
| capybara-run-large | 4.52 / 4.68 / 4.96 | 16.68 / 18.43 / 18.81 | 293 / 0 | 0.19 / 0.70 | 0.60 / 4.64 | 196.01 | 0.00 | 549.96 | 2.8 | 98.4 |
| herd | 4.50 / 5.23 / 6.63 | 16.57 / 18.52 / 19.43 | 278 / 0 | 0.32 / 0.94 | 1.08 / 1.24 | 162.22 | 0.00 | 290.56 | 1.5 | 112.4 |
| leapling | 2.14 / 2.35 / 2.36 | 16.67 / 17.47 / 18.22 | 276 / 0 | 0.14 / 0.51 | 0.24 / 0.28 | 109.03 | 0.00 | 20.39 | 0.4 | 0.0 |

## 2026-10-25: B13 (`78a31b6c`), toolchain 0.6.3

The same machine, the same day, against the baseline above (`--compare`), after the council's
review fixes: the built things fitted to their meshes (the Last Green's GPU memory), the frame
measuring as 0.6.2 and 0.6.3 time (the clearing's intervals). The two commits after it change
only a switched-off path and a test's call. The cleanup after those (`b51c6ab5`, `3390e42b`,
`024d6f77`) and the last review's fixes change code these scenes run, but not what they do.
With measuring off (the native host, 640 × 360, against `246d7d65`), each scene's sim is the same
every tick, and its frame is one the base's own runs draw: a run of either lands on one of a few
frames (the clearing's within 4/255 of each other, over at most 2,142 pixels), and two runs of
one build, the same commands to the byte, can differ so; the closest pair across the builds is
the same to the pixel. Their times weren't measured again. (The baseline ran on 0.6.2; 0.6.3 changes only when the
browser starts timing a label, which B1's code never asks.)

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | GPU peak MiB |
|---|---|---|---|---|---|---|---|---|---|---|
| clearing-path | 10.18 / 10.91 / 11.15 | 10.09 / 17.91 / 18.64 | 124 / 0 | 0.44 / 1.39 | 9.52 / 10.86 | 400.67 | 3956.89 | 3082.18 | 4.9 | 513.4 |
| last-green-shot | 18.92 / 19.91 / 25.72 | 18.80 / 23.49 / 40.15 | 317 / 8 | 4.73 / 5.66 | 12.39 / 16.60 | 667.19 | 14099.28 | 7492.50 | 15.0 | 780.8 |
| last-green-walk | 12.59 / 14.85 / 18.53 | 13.05 / 19.57 / 28.96 | 70 / 9 | 4.09 / 5.87 | 11.63 / 14.44 | 641.93 | 13928.06 | 7410.71 | 14.9 | 780.8 |
| capybara-run | 1.05 / 1.59 / 1.97 | 16.68 / 17.98 / 18.40 | 291 / 0 | 0.25 / 0.62 | 0.40 / 0.58 | 177.64 | 0.00 | 604.24 | 3.6 | 81.9 |
| capybara-run-large | 2.01 / 3.06 / 3.58 | 16.67 / 18.59 / 19.39 | 271 / 0 | 0.23 / 0.42 | 0.51 / 2.87 | 171.57 | 0.00 | 614.28 | 3.6 | 183.0 |
| herd | 4.65 / 6.02 / 6.65 | 16.67 / 17.20 / 17.64 | 222 / 0 | 0.37 / 0.95 | 1.11 / 1.33 | 153.49 | 0.00 | 319.61 | 1.6 | 112.4 |
| leapling | 2.14 / 2.36 / 2.44 | 16.66 / 18.17 / 18.62 | 254 / 0 | 0.12 / 0.47 | 0.24 / 0.28 | 103.06 | 0.00 | 109.04 | 0.4 | 0.0 |

The change from B1, as `--compare` printed it:

| Scene | GPU span p50 | Interval p95 | CPU p50 | First frame | GPU peak |
|---|---|---|---|---|---|
| clearing-path | 10.53 → 10.18 ms (−3.3%) | 17.82 → 17.91 ms | 0.78 → 0.44 ms (−43%) | 329 → 401 ms (+22%) | 476 → 513 MiB (+8%) |
| last-green-shot | 21.82 → 18.92 ms (−13.3%) | 28.65 → 23.49 ms | 5.18 → 4.73 ms (−8.6%) | 495 → 667 ms (+35%) | 1,468 → 781 MiB (−46.8%) |
| last-green-walk | 15.26 → 12.59 ms (−17.5%) | 19.50 → 19.57 ms | 4.66 → 4.09 ms (−12.3%) | 544 → 642 ms (+18%) | 1,469 → 781 MiB (−46.9%) |
| capybara-run | 1.69 → 1.05 ms (−37.7%) | 18.79 → 17.98 ms | 0.21 → 0.25 ms | 167 → 178 ms | 44 → 82 MiB (+85%) |
| capybara-run-large | 4.52 → 2.01 ms (−55.5%) | 18.43 → 18.59 ms | 0.19 → 0.23 ms | 196 → 172 ms | 98 → 183 MiB (+86%) |
| herd | 4.50 → 4.65 ms (+3.2%) | 18.52 → 17.20 ms | 0.32 → 0.37 ms | 162 → 153 ms | 112 → 112 MiB |
| leapling | 2.14 → 2.14 ms | 17.47 → 18.17 ms | 0.14 → 0.12 ms | 109 → 103 ms | 0.03 → 0.03 MiB |

The capybara's GPU memory is its parallax strips (B10.3: four layers, 1.5 by 1.25 screens each).
The Last Green's fell by its built things' meshes, fitted to what they hold (558 MiB of room
they didn't use), and by its compressed impostors (B6).

A cold start varies from run to run: the Last Green's opening was playable in 11.6, 11.9, 14.1
and (in two later pairs, against the same code without the banks' cluster path) 10.1 and 10.1 s
today, its pipelines' build 4.3 to 7.5 s of it.

**Repeats** (Chrome only, `--no-native`, two more runs a side, alternating B1 and B13 at
`b137893c`, before the review fixes: the scenes' intervals didn't change with them): the paced
intervals, p95 / p99, over 33 ms.

| Scene | B1 | B13 |
|---|---|---|
| clearing-path | 18.02 / 18.85, 0; 18.51 / 19.44, 0 | 17.85 / 19.06, 1; 18.35 / 19.34, 1 |
| last-green-shot | 23.11 / 32.08, 4; 23.77 / 40.31, 7 | 23.56 / 43.54, 7; 23.32 / 37.44, 7 |
| last-green-walk | 28.12 / 32.96, 11; 28.05 / 32.97, 8 | 16.54 / 28.66, 9; 23.03 / 28.81, 9 |
| capybara-run | 18.88 / 20.14, 0; 18.02 / 18.40, 0 | 18.25 / 18.67, 0; 18.93 / 19.72, 0 |

Over the three runs a side, the clearing's and the capybara's intervals didn't change; the Last
Green's walk has 57 to 111 of its 1,200 intervals over 16.7 ms where B1 had 382 to 397, and a
p99 of 29 ms where B1's was 33.

## 2026-10-26: the B1 attribution (world-class 2, toolchain 0.8.0)

The Last Green's walk, measured by `wrela tool bench -- --scene last-green-walk --no-docs` on the
M4 Air at `2b922010` with B1's bench in the tree (its pages keep their runs in
`build/bench/<date>-<time>/<scene>/{gpu,play,cold,warm}/`, the play run times each frame's
labels, the fourth run opens the cold build again in the same profile). The machine was also
running other work for part of the run, so these intervals are the hitches' evidence, not a
baseline.

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| last-green-walk | 12.72 / 15.00 / 18.28 | 14.73 / 23.35 / 28.83 | 218 / 6 | 4.18 / 4.97 | 10.82 / 14.46 | 619.59 | 10406.02 | 3897.22 | 4.6 | 8706.17 | 0.1 | 780.8 |

Hitches (frames over 33 ms, the five heaviest labels each; phases are `engine::frame::PHASES`,
each from its mark to the next, so `sim` holds the game's frame code before `draw` too):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 61 | 59.44 | 4.94 | vegetation 3.16, creature 2.91, terrain 2.83, clouds 2.36, temporal 2.04 | sim 3.51, cook 0.25, finish 0.14, passes 0.06, choose 0.05 | 939 | 541 |
| 203 | 33.52 | 5.46 | shadows far 18.82, vegetation 3.21, creature 2.90, clouds 2.89, terrain 2.79 | sim 4.81, cook 0.17, finish 0.07, passes 0.05, choose 0.01 | 1005 | 114 |
| 210 | 90.96 | 4.76 | vegetation 3.23, creature 3.02, terrain 2.89, clouds 2.69, probes bake 2.61 | sim 3.74, cook 0.38, finish 0.07, passes 0.04, choose 0.01 | 939 | 113 |
| 546 | 55.48 | 4.13 | bloom 15.86, clouds 2.65, creature 2.33, vegetation 2.31, terrain 2.28 | sim 3.57, cook 0.13, present 0.06, passes 0.04, choose 0.01 | 945 | 113 |
| 822 | 46.18 | 4.08 | bloom 15.46, vegetation 2.36, creature 2.01, temporal 1.92, look 1.92 | sim 3.59, cook 0.13, present 0.05, passes 0.03, choose 0.01 | 945 | 114 |
| 1106 | 37.65 | 4.00 | bloom 14.55, clouds 2.54, vegetation 2.39, creature 2.22, terrain 2.18 | sim 3.46, cook 0.13, present 0.06, passes 0.03, choose 0.01 | 945 | 113 |

0 of 6 unattributed (no label over 2 ms and a CPU time under 2 ms). Read as B4's evidence: the
far cascade's redraw lands whole on one frame (203); the bloom's chain takes 15 ms on three
frames of the walk where it takes about 1 ms on the others; frames 61 and 210 hold no label
heavier than the frame's usual, so their time is outside the timed passes (the first frames'
uploads at 61, 541 KiB written; at 210 the probes' bake starting).

The ten slowest pipelines to compile in the cold run (each built again alone; a name twice is
two variants):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| cook_materials | 1197.15 | 165.2 |
| skin_vertices | 1177.58 | 103.0 |
| place_vertices | 828.38 | 92.2 |
| cull_blocks | 704.77 | 114.6 |
| cull_blocks | 641.54 | 114.6 |
| cook_level | 589.23 | 165.2 |
| cook_materials | 498.91 | 165.2 |
| place | 311.97 | 59.3 |
| skin_vertices | 266.66 | 103.0 |
| cull_blocks | 248.89 | 114.6 |

The pass viewer on the same frame the gates read (`wrela tool passes -- games/last-green --frame
300 --input last-green-walk`): 29 passes, 24 render and 5 compute, in order: shadow moving,
clouds, vegetation prepass, vegetation, terrain, creature, occlusion pyramid, vegetation late
(two), ambient occlusion, sky, water (three), reflections copy, motion, lines, temporal (a
dispatch and a pass), look (a dispatch and a pass), bloom (eight). The plan's "22" counted
another frame's passes; this is the recorded count.

## 2026-10-26: B2, the cold start (world-class 2, toolchain 0.8.1 and the 0.9.0 checkout)

What each item found or changed, on the M4 Air:

- **B2.1, compressed at the edge.** No test Worker could be deployed from this machine (no
  Cloudflare credentials here), so the live site, whose Worker serves the same `CONTENT_TYPES`
  as this branch, was asked (`curl -sI -H 'Accept-Encoding: br, gzip'`, release
  `3676b81c1276de005613`, `last-green/`): `game.wasm` answers `content-encoding: br`, 8,319,320
  bytes as 2,817,252 on the wire; `manifest.json` `br`, 148,534 as 6,558; `pipeline_0.wgsl` `br`,
  1,326 as 575. Cloudflare compresses every type the release serves: nothing to change.
- **B2.2, the big constants.** `wrela build games/last-green --report data`: 4,357,984 bytes in
  1,402 constants. The three over 256 KB: `OBSTACLES` (1,000,248 in two) and the fallen trees'
  per-tile lists (`FALLEN`, 934,736 in its fifteen largest), both read by the sim from its first
  ticks (the player's and the fawn's collisions), which the plan's own rule keeps in the module;
  and `LAND` (782,896), the bare and grown views' land, whose readers are functions passed by
  value to the jobs that place those views' trees. Shipping `LAND` needs those views' streaming
  to carry the land as data: not done. The section stays 4.36 MB.
- **B2.3, tiers.** 152 pipelines, 94 in tier 0 and 58 in tier 1; none of the 17 `CLUSTERED`
  ones (B2.4 had brought seven back; `838d1e60` keeps them out); `lights::bin`,
  `probes::trace_again`, the shafts, `particles::admit` and the map's overlay are tier 1 already.
- **B2.4, the load chain.** All 26 kinds cook by frame 2 (12 before); the built things and the
  banks' prototypes no longer wait for the wood, and come as many a frame as the budget allows.
  Native, 960×540, one worker: playable at frame 204 with the kinds alone moved, then 81.
- **B2.5, the realizer through tapes.** Through their tapes (box-filtered noise is one tape
  operation in 0.9.0), the built things' meshes match their compiled ones to 0.01 mm, but the
  load took 38 s to playable in Chrome, cold and warm alike (the GPU's time interpreting
  unpruned tapes at every corner, not the pipelines'), against 9.2 s compiled: the load keeps
  the compiled kernels. Pipeline tiers that follow calls were tried in the toolchain and
  reverted: 129 pipelines went to tier 1, many needed at load, and Chrome made those one at a
  time, 58 s to playable.

The step's gate, three runs each (`wrela tool bench -- --scene last-green-walk --scene
clearing-path --no-native`, the 0.9.0 checkout):

| Scene | Cold playable ms | Pipelines ms (tier 0) | Warm playable ms | Playable at frame |
|---|---|---|---|---|
| last-green-walk | 13,475 / 7,490 / 7,612: median **7,612** (G-cold 5,000: misses) | 6,746 / 3,695 / 3,239 | 6,323 / 5,316 / 5,613: median **5,613** (G-warm 3,000: misses) | 83 to 89 (220 at B1) |
| clearing-path | 4,794 / 4,785 / 5,168: median **4,794** (2,500: misses) | 3,964 / 3,305 / 3,503 | 1,420 / 1,417 / 1,444 | 47 |

The first run's walk shared the machine with a compile. What's left of the walk's cold start is
its pipelines (3.2 to 3.7 s in tier 0, frame 1 waiting for them) and its first frames' work
(the first frame alone holds 587 ms of CPU and 142 ms of its first render pass).

**The B1 attribution, read again.** In a paced run (`paced=1`), the frame's last labelled pass,
the bloom's, reads about 24 ms of GPU time on every frame: its end waits for the screen, so it
says nothing of the bloom. The walk's three late hitches (frames 550, 836 and 1129 of this run,
546, 822 and 1106 of B1's) show *less* bloom than the frames round them; each comes a frame or two
before a probe replacement starts (`probes occluders` and `probes open sky`, then `probes bake`
two frames later). B4 works against those, not against the bloom.

## 2026-10-26: B4, the hitch sources (world-class 2, the 0.9.0 checkout)

What each item found or changed (`wrela tool bench -- --scene last-green-walk --scene
last-green-shot --no-native`, the M4 Air, the playing run's labels):

- **B4.1, the far layer.** The 65 baked tiles' crowns land `FAR_PER_FRAME` (8) a frame, nearest
  first, each under `may(Work::Tile)`; the far cascade waits for the last. Natively, with the
  far layer's file held to frame 200: the layer over 11 frames, playable at frame 263.
- **B4.2, the sky.** The clouds' noise cooks in 9 slices (`sky::NOISE_SLICES`), the weather and
  shadow maps in 4 bands each (`sky::MAP_BANDS`), each slice asked of the planner, and at most
  4 ms of them planned a frame (`SKY_FRAME_US`). Measured with a 6 ms cap, the heaviest frame's
  sky labels (`clouds noise`, `clouds weather`, the dome's `clouds` and the `air`): **5.42 ms**
  in the opening (frame 268), 6.74 in the walk (frame 42); before the cap a loading frame cooked
  12.2 ms of noise. At 6 ms the load's noise and weather took 6 frames; at 4, the frame fixture
  cooks the load's 9 slices of noise and its weather over 10 frames, and a weather change in
  play its 8 bands in one frame (a band is 0.36 ms).
- **B4.3, built things as jobs: not needed.** A built thing's tally is already a `Pending`
  polled on a later frame, and they're realized only while loading (B2.4): the heaviest frame's
  `built things` is 46.7 ms (walk frame 8), under twice the loading budget (60 ms).
- **B4.4, one field write a frame** once playable (all of them while loading or capturing), the
  neighbours' refreshes counted against `IN_FLIGHT`.
- **B4.5, the far strips** (`shadows far`, 20.8 ms at most) are listed, not counted: B6.
- **B4.6, measured costs.** `MEASURED` times 12 kinds by their labels (`air`, `clouds noise`,
  `clouds weather`, `kind`, `built things`, `terrain cook`, `tile` added), each time divided by
  the steps the timed frame ran. `cook::costs()` holds the walk's running averages at frame
  1,200 (µs, the ladder's step / the average): air slice 291/289, noise slice 2,711/2,412,
  weather band 364/363, dome phase 3,388/2,723, ground level 455/394, near strip 2,168/1,810,
  far strip 1,388/1,312, occluders 455/408, probe strip 4,235/4,118, kind 64,841, built thing
  8,000/7,542, tile 364/292 (the creature's mesh slice is timed by no label: its record stays).
  A kind's 65 ms stays an estimate (4 ms) in the table: planned at its cost, the load would cook
  one a frame and repeat each loading frame's own 25 ms 26 times.
- **B4.7, the first optional step, bounded**: once playable, a step planned past twice the
  playing budget panics in a debug build (`Planner::may`). It never fired in the Last Green's
  tests or the frame fixture.

The step's gate (one run each, the 0.9.0 checkout):

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 33 ms | Cold / warm playable ms |
|---|---|---|---|---|
| last-green-shot (480 frames) | 17.56 / 19.09 / 24.10 | 20.44 / 22.41 / 31.98 | **2** (8 at B1: meets) | 7,832 / 6,700 |
| last-green-walk (1,200 frames) | 12.50 / 14.42 / 17.89 | 14.89 / 23.14 / **26.47** (22: misses) | **5**, 4 without `shadows far` (9 at B1; 2: misses) | 11,880 / 6,606 |

The walk's cold start shared the machine with a compile (7,675 ms the run before). Pixels at
frame 600 against B3: the clearing's path 16.6% differ, 99% of them by 1 or 2/255 and 0.2% of
the screen by more (at most 32/255, over the crowns: the sky's and the probes' work lands on
other frames); the Last Green 14.7%, 97% by 1 or 2/255, at most 22 (its run-to-run noise is
about 10%). Its hitches:

| Frame | Interval ms | Heaviest label, GPU ms | What it is |
|---|---|---|---|
| 88 | 33.47 | shadows far 18.94 | a far strip (B6) |
| 95 | 126.54 | probes bake 5.05 | the first frame lit by the probes: its labels sum to about 20 ms, the rest is the frame's new pipelines being made |
| 552 | 56.97 | bloom 16.04 | |
| 833 | 62.04 | bloom 24.19 | |
| 1114 | 44.58 | bloom 13.25 | |

None is the ground's cook, a built thing, a kind, the sky's noise or weather, or a tile's
placement. The last three are the B2 reading's: a frame or two before a probe replacement
starts (`probes occluders`, `probes open sky`, then `probes bake`), the bloom's label holding
the wait for the screen. B5.5 shifts the probes rather than replacing them.

## 2026-10-26: B5, the frame (world-class 2, the 0.9.0 checkout)

Each item's numbers: label p50s from the saturated run's timings (`wrela tool bench -- --scene
last-green-shot --scene last-green-walk --no-native`), the M4 Air, Chrome; pixels native at
640×360 against the step before at frame 600 (`wrela-host --rgba`).

- **B5.1, the grass.** Blades are drawn in the depth prepass (cut to their fronds there, shaded
  at equal depth after), a blade the last frame's pyramid hides isn't placed while the camera
  moves, and each of the five rings keeps its placement until its cells, the render origin, the
  probes' grid or the thinning change, or `KEPT_FRAMES` (8) pass (the pyramid tests the rings
  past the first: near, what hides a blade is mostly what moves). One bench of all three: the
  opening's `vegetation` 3.95 → **3.50 ms** (2.4 asked: misses), `vegetation prepass` 1.40 →
  1.88, the placement (in `cull`) 1.33 → 0.31, `terrain` 2.28 → 1.84 (the blades' depth hides
  ground it shaded); its GPU span p50 17.56 → 17.12. The walk: `vegetation` 2.44 → 2.04,
  `terrain` 2.22 → 1.85, the prepass 1.61 → 1.79, `cull` 1.45 → 1.48. The clearing's still
  camera: 40 placements in 64 frames (the test's limit, 5 × 8). Pixels: the clearing's path
  0.23% differ, all but 4 by 1/255.

- **B5.2, one bounds list.** `engine::cull` has `Bound`, a frustum test (`in_frustum`, the
  planes after Gribb and Hartmann 2001) and `cull_bounds`/`bound_draws`: a bound's kind is the
  draw it's an instance of, the pyramid hides what last frame's depth hid, and each kind's list
  and draw come out (`engine/tests/cull`: 10,000 bounds of four kinds, 2,211 drawn, each kind's
  list exactly what the CPU's test sees). `engine::pool` copies the built things' and the banks'
  stones' meshes into one set of buffers (each thing's own given back): they're drawn with one
  multi-draw (a mesh's draw's instances what was culled; a vertex finds its mesh by its vertex,
  its item by the mesh's room and the instance, needing no first instance) and cast with
  another; the tower's near and far meshes hand over by their items' coverage. The water is laid
  in 64 m tiles (`engine::water::tiled`), the tiles the frustum meets drawn by one multi-draw,
  and its copy is made only over the screen they cover, or not at all (the mist's copy stays
  whole; a test on the floor's water). Not done: the characters (their CPU test stays) and the
  rocks (their own GPU culling) through the one kernel. The walk's frame 600 records **78
  logical draws** (30 asked: misses): the wood's ways and phases, the grass's rings (B5.1's
  kept rings are a draw each, prepass and shading), the characters' parts. The Last Green's
  pixels at frame 600: 11.4% differ, at most 28/255 (its noise).
- **B5.3, fewer passes.** `wrela tool passes` now counts a pass that joins the one before it
  (`std::gpu`'s `join`, which the host runs as one: the recorder says so in 0.9.0) with that
  one, and the full-screen ones. The Last Green's walk at frame 300: 28 GPU passes, 15
  full-screen → **26 and 13** (20 and 11 asked: misses). The sky joins the pass before it (the
  ambient occlusion's dispatch moves after it); the develop brings the bloom's second level up
  itself (`bloom_up`'s tent, where a pass added it to the first); levels narrower than 64 texels
  aren't drawn (none at 1080p). Not done: the lines into the resolve (the resolve reconstructs
  from four samples an output pixel, each of which would need its line test, at the output's
  size: four times the lines pass's 0.4 ms), the mist into the froxels (B10), the water's copy
  scissored (B5.2). The clearing's pixels: one differs, by 1/255.
- **B5.4, cloud rays (D17): off.** The march at half the dome's size each way (2048 × 640, a
  frame's share 40,960 rays where it was 163,840), put in the dome bilinearly, moves the
  clearing's frame 600 by up to 28/255 (15.5% of pixels differ, 1.2% by more than 2/255: the
  clouds' edges); the light loop's early exit at an optical depth of 4 with its last four
  samples at twice the step moves it by up to 45/255 (87% differ: the faint scattering octaves
  see past an optical depth of 4, `exp(-od · 0.3³)`, so they brighten), and an exit that keeps
  every octave under 2% (an optical depth of 145) never comes before the last sample. Neither
  holds 2/255, so the frame marches whole as before and the half march is
  `Quality::clouds_halved`, off, for a device that needs the time (B8). With it off, the
  clearing's frame 600 still moves (4.1% of pixels, at most 21/255, from frame 40 on), where
  the step before rebuilt moves 3 pixels by 1/255: the build has one pipeline more (the
  upsample's), and the load's schedule shifts with it.

- **B5.5, the probes shift, and their occluders off the main thread.** The walk's three late
  hitches (B4) went with probe replacement: with it off, the walk had 2 frames over 33 ms, not
  5. A replacement now shifts the grid by whole columns when only the eye moved
  (`Baked::shifted`: the columns both grids hold and their voxels copied into the spare, the new
  band's occluders cooked into the kept volume, only the new columns baked: 144 of 648 probes
  for a two-column shift in `engine/tests/probes-renew`, the held light and the occluders'
  density where both grids reach unchanged). That alone left the hitches: a print showed the
  Last Green gathering the grid's occluders on the frame that starts it, 33 to 40 ms in Chrome
  (65 natively for the first bake), fitting each limb's capsule to its mesh's planes. The stage
  now gathers them on a helper as the eye moves (`gather_occluders`, a job round a place each 8
  m, used within 12 m), and the frame takes them when it wants them. Walk: **1 frame over 33
  ms of 1,200** (33.9 ms; 5 before; G-hitch's 2: meets), paced p99 25.9 ms; opening: **none of
  480** (2 before), and the first probe-lit frame's 100 ms is gone. GPU span p50: opening 17.12
  → 13.27 ms (B5.2's water copy 1.74 → 0.63 ms, the pool's culling), walk 12.49 → 13.31; the
  walk's labels' p50s sum 23.09 → 19.33 ms (bloom's wait aside): the span rose where the work
  fell (not explained yet).
- **B5.6, early-Z for depth-writing fragments: nothing to do yet.** A11 recorded the gap
  (docs/toolchain-gaps.md, "A fragment can't say its depth only grows": no conservative-depth
  attribute in Chrome's WGSL or naga's writer), so `WithDepthGreater` is still `WithDepth`. One
  correction for when it lands: with reversed depth the depth test passes the greater value,
  so an early rejection needs a fragment's written depth never *greater* than its rasterized
  one (`WithDepthLess`), the draw at its bound's nearest point (an impostor's card at its
  sphere's near side), not at its farthest: a merge triangle at the far plane (z = 0) fails the
  early test everywhere and still can't be rejected.

The step's gate (one run each, the 0.9.0 checkout, after B5.5):

| Gate | Asked | Measured |
|---|---|---|
| G-profile, the opening's GPU span p50 | 12.0 ms (18.9 at B1) | **13.27** (misses by 1.27; its heaviest label `vegetation` 3.04) |
| G-profile, the walk's | 9.0 ms (12.6) | **13.31** (misses; the labels' work fell 3.8 ms, the span rose 0.8) |
| G-draws, the walk's frame 600 | under 30 logical draws | **78** (misses) |
| G-hitch | at most 2 frames over 33 ms | walk **1** of 1,200, opening **0** of 480 (meet) |
| G-pixels | within 2/255 | B5.1 and B5.3 hold it on the clearing (4 and 1 pixels past 1/255); B5.2 within the Last Green's noise; B5.4's switch is off, its build's schedule moves 4.1% of the clearing's pixels by up to 21/255 |
| G-stills | | not run per step: the final gate's (B16) |

## 2026-10-26: B6, shadow pages (world-class 2, the 0.9.0 checkout)

**B6a and B6b: built, measured, and set aside** (branch `b6-virtual-shadow-pages`, commit
`a4c1e9ce`). The static shadows as a virtual shadow map: ten clipmap levels (8 m to 4 km, level
0 left out: its half-metre pages were drawn again every few steps) of 16 × 16 pages of 128
texels in a 4096² pool (1,024 pages), pages asked for by the last frame's depth (a marking pass,
read back) and 384 reserved (the coarsest level whole, each finer level's middle 4 × 4), at most
16 drawn a frame (64 loading), a tile's box invalidating its pages, two sets so a moving sun's
next set is drawn beside the one read. Its tests passed (bookkeeping, shadow-stability on a page,
the frame fixture, the clearing's), and the walk's `shadow pages` p50 was **0.57 ms** (1.5
asked). It didn't ship, measured in Chrome on the Last Green against B5:

| | B5 | Pages (best of four tunings) |
|---|---|---|
| Opening GPU span p50 | 13.27 ms | 16.8 to 18.1 |
| Walk GPU span p50 | 13.31 | 17.9 to 18.3 |
| Walk frames over 33 ms of 1,200 | 1 | 56 to 114 |
| Cold start to playable | 7.3 s | 14 to 25 s |

Two costs, each found by taking the other away: the page table's lookup in every lit pixel (a
dependent read before the depth samples) took about 3 ms of the opening's frame (with the static
lookups skipped: vegetation 5.45 → 3.77 ms, terrain 4.83 → 2.94, span 18.1 → 14.8); and a
page's drawing draws the built things, the banks and the ground whole into every page, and a
coarse page kilometres of trees, so the reserved set's 384 pages cost several times the
cascades' 60 strips (choosing the casters once a level or once a 4 × 4 block only moved the cost
from the choice into the casting). What the plan names for both is what this attempt lacked:
per-page caster lists from the one bounds list (B5.2's kernel testing each caster against the
pages to draw), and a lookup cheap enough for a forward renderer's every pixel (a shadow mask
from the depth, or the table where the fragment's own page is known). The engine keeps its
cascades.

**B6c, PCSS: a switch, off.** `ShadowQuality::soft` (`Soft::Pcf`, **proposed** default, or
`Soft::Pcss`): an 8-texel blocker search in the near cascade, the penumbra the sun's 0.53° disc
(`Light::sun_size`) spreads at the blockers' distance, then 12 taps that wide (Fernando 2005).
`engine/tests/shadow-pcss`: a blocker 16 m over its receiver casts a 13-texel penumbra, one 1 m
over it 2 (PCF: 2 and 2). In the opening it costs **0.71 ms** more over the lit passes' p50s
(vegetation 3.04 → 3.23, terrain 1.99 → 2.26, the creature 1.82 → 2.09; the span 13.27 →
13.54): over the 0.4 ms asked, so it stays a switch with that number. With `Pcf` the shaders
compute what they did.

## 2026-10-26: B7, memory (world-class 2, the 0.9.0 checkout)

The frame prints `memory shares:` once at `playable`, and the bench reads it into a table (the
program's own counts: what's held, then each share). The Last Green's walk at 1080p in Chrome:

| Share | MiB at playable |
|---|---|
| held (buffers and textures) | 584.5 |
| targets | 92.6 |
| impostors | 78.0 |
| sky (the dome 4096 × 1280 half floats: 40.0; the shape noise 16.0; the weather and its shadow 16.0) | 74.8 |
| shadows (two cascades' two 2048² maps: 64.0; the moving map 4.0) | 68.0 |
| wood | 58.1 |
| built (B5.2's pool: 177.3 before these items) | 57.4 |
| wood lists | 41.5 |
| probes (30.1 before item 5) | 23.1 |
| realizer (the fawn's and the wolf's meshes) | 18.3 |
| terrain | 10.8 |
| reflections | 7.5 |

- **B7.1.** The breakdown above; the game's shares add `realizer` (the casts' meshes, against
  `memory::REALIZER`, so `evict_after` can fire).
- **B7.2, impostor kinds streamed: not done.** The impostors are one array texture a kind a
  layer (26 × 1024² compressed: 78 MiB); streaming them needs a kind-to-layer indirection in the
  wood's choosing and shading.
- **B7.3.** The herd's close-up test: the realizer's bytes 225 MiB as the close-up begins, 68
  after 1,000 frames with the other grazers unseen.
- **B7.4.** The dome stays 4096 × 1280 (D17 off): 40 MiB of the sky's 75.
- **B7.5.** The probes' voxels are given back once cooked (a shift reads the density texture):
  the probes' share 30.1 → 23.1 MiB.
- **B7.6, a pool for transient targets: one texture.** The dabs are drawn into the texture the
  water's copy held (the water and the mist read it before the look writes the dabs): 7 MiB at
  1080p. The plan's general pool (the shafts, the see-through targets, the glow's chain) isn't
  built.
- **The pool fitted, its vertices packed.** B5.2's pool grew by doubling: once everything built
  is in it's fitted to what it holds (177 → 100 MiB), and its vertices are a place and an
  oct-encoded normal (16 bytes, where a skinned vertex is 48): 57 MiB.
- **B7.7.** `Targets`' notes say what each costs at 1080p (84 MiB in all).

The gate: the walk's `gpu_peak` **857.7 MiB** (512 asked: misses; about 890 before), the
opening's **819.1** (560: misses). What's held at `playable` is 585 MiB: the peak's other 270
MiB come and go during the load (built things realized into room for their whole grid until
their counts come, the pool's copies as it grew). The walk had no frame over 33 ms (GPU span
p50 13.27 ms), and the opening none (13.19).

G-pixels (native host, 640×360, B6's `c4df4f16` against B7's `8f49f88c`): the clearing's frames
are byte for byte the same; the Last Green's differ in 12.9% of pixels by at most 28/255, most by
1, which is its run-to-run noise (two runs of one build differ by 10 to 12%, at most 28).

## Whether D10 holds

The claim (docs/plan-world-class-2.md §0.1, D10, **proposed**): on a MacBook Air M4 in Chrome at
1080p, every flagship scene's GPU frame is under 12 ms at the median with every `Quality`
switch on, no frame of 1,200 paced frames is over 33 ms and the paced 99th percentile is under
20 ms; each is playable within 5 s of a cold page open and 3 s of a warm one; the Last Green's
floor keeps its GPU memory under 512 MiB. On a MacBook Air M1 with 8 GB the floor holds 60 fps
with the same switches and is playable within 8 s cold. On an Intel Iris Xe laptop in Chrome the
floor's GPU frame is under 25 ms with no frame over 50 ms. The other engines' scenes are recorded
beside these numbers, whatever they show. §B16 fills each cell from its own bench run; a cell
says "holds", "misses by …" or "not measured: why".

| Device | Scene | GPU p50 < 12 ms | Frames over 33 ms of 1,200 = 0 | Paced p99 < 20 ms | Cold playable | Warm playable < 3 s | Memory |
|---|---|---|---|---|---|---|---|
| M4 Air | the Last Green's opening | misses by 4.6 (16.62) | misses: 5 of 480 | misses by 13.7 (33.73) | < 5 s: misses by 10.5 (15.5 s; 7.6 to 8.9 s in B14's quieter runs) | misses by 4.8 (7.8 s) | < 512 MiB: misses by 376 (887.9 MiB at its peak) |
| M4 Air | the Last Green's walk | misses by 6.1 (18.14) | misses: 5 | misses by 5.9 (25.94) | < 5 s: misses by 11.0 (16.0 s) | misses by 5.0 (8.0 s) | < 512 MiB: misses by 350 (861.8 MiB) |
| M4 Air | the clearing's path | misses by 0.43 to 0.65 (12.43 with its volumes off, 12.65 before) | misses: 45 to 46 of 600 | misses by 21.4 (41.42) | < 5 s: holds in the A/B runs (4.5 to 5.0 s), misses in the full bench's (9.0 s) | holds (1.9 s) | |
| M4 Air | the capybara's run | holds (1.12) | holds (0) | misses by 0.33 (20.33) | < 5 s: holds (first frame 0.17 s; it prints no playable) | holds | |
| M1 Air 8 GB | the floor | 60 fps: not measured: no M1 to hand | | | < 8 s: not measured | | |
| Iris Xe laptop | the floor | < 25 ms: not measured: no laptop to hand | over 50 ms = 0: not measured | | | | |

What B16 measured (`wrela tool bench` against B1's report, 2026-10-26, toolchain 0.9.0; the
table is under "2026-10-26: B16" at the end), a sentence each:

- **The Last Green's opening misses everywhere but its first frame.** Its GPU frame is 16.6 ms,
  4.6 over D10's 12 (B13 left it at 14.25; B14's switches are off, and the rest of the rise is
  the machine's: the same build measured 16.5 to 16.7 in B14's runs); 5 of 480 frames over
  33 ms; playable in 7.6 to 15.5 s cold and 7.8 s warm; 888 MiB of GPU memory at its peak. The
  levers, by its labels: the vegetation (5.4 ms), the terrain (4.4), the clouds and the look; for
  the start, the pipelines (7.5 s: 103 in tier 0, 72 in tier 1, and the cascades' three kernels
  in tier 0 though they run only when switched on); for memory, the shadows (71 MB), the sky
  (78 MB), the impostors (82 MB) and the wood's lists (33 MB).
- **The walk misses the same way** (18.1 ms, 5 of 1,200 frames over 33 ms, p99 25.9 ms), its
  hitches what a frame cooks (the hitch table under B16).
- **The clearing's path misses its GPU frame by 0.65 ms and its hitches by 46.** Bisected
  (B5 10.2 ms with no frame over 33, B7 10.8, B9c 11.9, B10 12.4, B15 and B16 12.6). Its own
  switches explain little of it: with the froxel and distance volumes off (it has neither fog nor
  solids; a950ec70 turns them off for it) it measures 12.43 ms and 45 frames over 33, and with
  the probes' sight lookups off too, 12.26 and 50. The rest is in code every frame runs whatever
  the switches say: the shading passes now carry the sight, fog and distance paths and their
  bindings. The next lever is to build those paths only where a scene's quality asks for them
  (a `const` per switch, as B11c showed a false constant builds none of a path).
- **The capybara's run holds** but for a p99 0.33 ms over (the leapling's, a 2 ms frame, is 19.4:
  headless Chrome's pacing).
- **The M1 Air and the Iris Xe laptop are not measured:** neither was to hand (B8, "Devices").
- **The other engines** (Part C, "Other engines on this machine"): three.js, Babylon.js and
  PlayCanvas keep up at 60 Hz with the brief's far sparser content (no frame over 33 ms in
  1,200); three.js's own GPU timing reads 36 ms a sample. (Plan three's B7 found those samples
  each gathered several frames' work, and the scenes' water and Babylon.js's shadows wrong, and
  re-measured: see that section.)

What B13 left (toolchain 0.6.3), against §0.1's first wording, kept for the record:

The claim (docs/plan-world-class.md §0.1, **proposed**): at 1080p in Chrome, each flagship scene
holds 60 fps with a 99th-percentile frame interval under 16.7 ms, no shader or streaming hitch,
playable within 5 s of a cold page open, at a visual tier that native engines run under 30 fps
on the same machine. As measured, it doesn't hold as written for any scene: no program can meet
a 99th percentile under 16.7 ms in headless Chrome (the leapling's is 18.2 ms), so the scenes
below are judged by their GPU frame against 16.7 ms and their frames over 33 ms.

- **The clearing's path:** holds 60 fps. Its GPU frame is 10.1 to 10.2 ms, its intervals are the
  leapling's within a ms (p99 18.6 to 19.6 ms), 0 or 1 frame of 600 over 33 ms, and it's playable
  in 3.5 to 4.0 s. The next lever is that one hitch.
- **The Last Green's opening (the shot):** misses. Its GPU frame is 18.7 to 18.9 ms, 2 ms over
  a refresh, so it runs at about 53 fps (interval p50 18.5 to 18.8 ms), with 7 or 8 of 480 frames
  over 33 ms; and it's playable in 10 to 14 s, 5 to 9 s over. The levers, by its labels' GPU times in Chrome (`wrela
  profile games/last-green --chrome --input build/gates/LG_SHOT.json --compare <B1's build>
  --pairs 3`: span 21.76 → 18.59 ms, −14.2%): the terrain's cook (4.65 ms), the vegetation
  (3.93), the probes' bake (2.69), the near cascade's strips (2.45) and the clouds (2.35); for
  the start, the pipelines' build (5.3 s of it) and the streaming behind it.
- **The Last Green's walk:** misses, by its hitches. Its GPU frame is 12.4 to 12.6 ms, but 7 to 9
  of 1,200 frames are over 33 ms and its p99 is 29 ms. The next lever is what those frames cook
  (`wrela profile --chrome` on the walk's frames over 33 ms).
- **The capybara's run:** holds, at 1080p and at 2940×1912: GPU frame 1.1 and 2.0 ms, the
  leapling's intervals, no frame over 33 ms.
- **The herd and the leapling:** hold (GPU frame 4.7 and 2.2 ms, no frame over 33 ms).
- **"At a visual tier that native engines run under 30 fps":** not checked. The other engines'
  scenes (§0.6) are in a sibling repository that doesn't exist yet, so there is no number to
  compare with.

## 2026-10-26: B9, light with visibility (world-class 2, the 0.9.0 checkout)

Each scene's labels below are the medians over the frames each ran in (`probes again` runs on
some frames only), from the bench's GPU run of the Last Green at 1080p in Chrome on the M4 Air,
against B7's run (`build/bench/2026-10-26-154945`).

**B9a, the probes' sight.** Each probe keeps what it sees (DDGI's depth, Majercik et al. 2019):
6 × 6 octahedral texels of the mean and deviation of its rays' hit distances, made by a kernel of
their own (`probes::see`, a texel an invocation) from the hits the bake and the re-trace write. A
lookup weighs the four columns of the nearer layer by Chebyshev's bound and DDGI's backface term,
and turns those weights into where its bilinear samples fall (four reads, not eight: a wall
between a point and a column stands between it and both of its layers).
`ProbeQuality::visibility` (on) switches the lookup; off, it's the last one's.

- **The tests** (engine/tests/probes-visibility, the frame's probes after 20 rounds of re-tracing):
  a point 1 m inside a closed room beside open ground gets **0.0%** of the open side's sky light
  (34.7% without the sight); a point 1 m inside a closed box **1.05%** of the meadow's (9.9%);
  the meadow's 50 points move by at most **0.61/255** of their light.
- **Off is the last lookup:** the clearing, native, 640 × 360, 240 frames, against B8: **1 pixel by
  1/255** (the new kernels' pipelines move the schedule), with the re-trace's planning as it was
  (the next item changes it, and with it the clearing's light in 33% of pixels by up to 17/255).
- **The re-trace planned at its own cost.** A strip traced again (32 columns, 64 rays) was planned
  at a bake strip's measured cost (4.2 ms on the ladder at B7; 0.57 ms is what it takes); the
  sight made the bake strip 20% dearer, its rung 5.3 ms, and no playable frame had room for it.
  `Work::ProbeAgain` now has its own cost (B7's 600 µs to start, then measured by its label).
  The walk traces again on 720 frames of 1,260 (443 at B7).
- **G-profile** (one run each; the plan's estimate: under 0.3 ms together):

| Label, p50 ms | walk B7 | walk B9a | shot B7 | shot B9a |
|---|---|---|---|---|
| probes again (a strip) | 0.57 | 0.75 | 0.66 | 0.83 |
| probes bake (a strip, at load) | 3.01 | 3.72 | 2.72 | 3.14 |
| vegetation | 2.01 | 2.03 | 2.95 | 3.08 |
| terrain | 1.83 | 1.86 | 1.98 | 2.17 |
| creature | 0.51 | 0.55 | 1.84 | 2.03 |
| GPU span | 13.27 | 13.80 | 13.19 | 13.71 |

  The walk's re-trace and lookups rise 0.28 ms together on a frame that traces (meets); the
  shot's, close to the fawn, 0.68 (misses: its pixels read the probes four times as often). The
  span rises 0.5 ms in both, the re-trace running on more frames among it.
- **G-memory:** the probes' share 24.2 → 31.8 MB: the sights' 7.6 MB (52,488 probes × 36 texels ×
  4 bytes), nothing else.

**B9b, the solids' distance volume** (`engine::distance`). 192 × 64 × 192 voxels of 0.5 m round
the eye (a 3D texture of 32-bit floats: as a storage buffer it took a shading pass past WebGPU's
eight), toroidal, moved by whole 8 m bins; each voxel the distance to the ground (the clipmap's
relief) and to the solids its bin lists, at most 4 m. `Scene::solids` (new) gives them: the Last
Green's are its occluders that are solid (5,928 capsules of trunks, limbs and stones) and its
built things as tapes (134 of 135: the tower's 1,575 operations are past a `ShapeTape`), each
landing a new version. A cook is eight slabs, one a frame (`Work::Distance`). Its users: the
probes' rays step through open air by it (two steps at most: the leaves aren't in it) and end at
a solid as at the ground; the contact shadow marches it toward the sun (nearest voxels, a read a
step, as the screen's march did) and so doesn't fail at the screen's edge; the ambient occlusion
adds four cones read at 1.5 and 4 m past GTAO's metre.

- **The tests** (engine/tests/distance): a sphere's volume within 0.233 m of its distance at 1,000
  points, a cuboid's tape within 0.239 (half a voxel is 0.25); a probe's ray toward a trunk stops
  3.51 m on (its surface is 3.5) with the trunk's light, one beside it reaches the sky; a contact
  shadow beside a post at the screen's edge is the one beside a post in its middle (0 and 0).
  A post thinner than a voxel (0.3 m round) shadows only half: its middle lies between voxels'
  middles, which never see inside it. The Last Green's test counts its solids after the load.
- **G-profile** (one run each, steering on: B9c's commit follows):

| Label, p50 ms | walk B9a | walk B9b | shot B9a | shot B9b |
|---|---|---|---|---|
| probes again (a strip) | 0.75 | 0.76 | 0.83 | 0.87 |
| distance (a slab; 197 of 1,260 frames, 34 of 540) | | 0.32 | | 0.43 |
| ambient occlusion (B7: 0.53, 0.31) | 0.53 | 0.69 | | 0.59 |
| terrain | 1.86 | 1.90 | 2.17 | 2.23 |
| creature | 0.55 | 0.53 | 2.03 | 2.09 |
| GPU span | 13.80 | 13.97 | 13.71 | 13.88 |

  `probes again` doesn't fall (asked: 30%): its rays spend their steps in crowns, lit by the
  probes there, which the volume can't skip, not in open air. The contact march through the
  volume costs what the screen's did once it reads a voxel a step (trilinear, it cost 0.5 ms
  more in the shot's terrain and creature). A slab costs 0.32 ms (asked: 0.25); a frame cooks
  one on a sixth of the walk's frames, the volume moving every 8 m.
- **G-memory:** a new `distance` share, 10.9 MB (the volume's 9.4 and its solids' lists).

**B9c, the re-trace steered** (`probes::next_strip`). A strip of 32 columns is still traced a
frame; which one is chosen on the CPU, which knows what changed: the oldest if two rounds haven't
traced it, else the one the sun turned most (degrees) and the sky's version moved most since it
was, up to five times as much the nearer it lights what the eye sees (its nearest column within
12 m, or ahead within 40 m), its age over a round past that. The plan's GPU bucket select of 32
columns anywhere would need the probes' buffers written out of order, which a kernel's slots
aren't; the strip is the unit the re-trace already has.

- **The tests** (CPU, the Last Green's 81 × 81 grid, 206 strips): with nothing changing, every
  strip within a round, as in turn; after a 5° step of the sun, the strips under the eye within
  **3 frames** and every strip the eye sees (34) within **34**: in turn, within 206 (the `Sun`
  fixture's frames to convergence: a sixth).
- **G-pixels** (the clearing, native, 640 × 360, 600 frames, steered against in turn): 21.8% of
  pixels differ, 96% of them by 1/255, at most 27/255 (misses 2/255). The clearing's sky changes
  version as its air and weather finish cooking; steered, the near strips are traced first after
  each, and from then the two runs trace strips in different orders, each probe's last traces
  with other turns of the spiral (kept half of): they differ by those traces' noise. Steady
  state, with nothing changing, is the order taken in turn again, a round later.

## 2026-10-26: B10, light in the air (world-class 2, the 0.9.0 checkout)

`engine::fog` (Wronski 2014; Hillaire 2015): 160 × 90 froxels over the frustum, 48 slices from
0.5 to 200 m (exponential), jittered along their rays each frame. Each frame a froxel takes its
density (the scene's `FogField`: the engine's height fog, spheres of mist, the crowns' density
from the probes' volume times 0.02) and the light it scatters toward the eye (the sun through
the static cascades and the clouds' shadow, Henyey-Greenstein g 0.5; the sky, from the nearer
probe layer's constant terms), blended with the last frame's reprojected (0.9) unless its density
moved by more than a fifth. A workgroup a column integrates it (Hillis-Steele's scan in workgroup
memory), and `lit::through_air` applies it before the air's aerial table; the sky pass applies
its last slice. `Scene::fog` (new; none by default); the Last Green's: the height fog, a sphere
of mist at each fall (`history::MISTS`), the canopy's haze (B switches it). The shafts
(`engine/shafts.wrela`) and the falls' mist pass (`water::mist_over`) are gone: `wrela pipelines
games/last-green` names neither `march_shafts`, `add_shafts` nor `mist_over`.

- **The tests** (engine/tests/fog): a uniform fog of 0.02 per metre shows 0.9048, 0.6699 and
  0.3015 of points 5, 20 and 60 m ahead (`exp(-σd)`: 0.9048, 0.6703, 0.3012); a step to 0.04
  settles in **1 frame** (asked: 16); with the eye 10 m under the ground, every froxel in its
  shadow, the light in front of those points is the same with the sun's in-scatter and without
  (within 0.0005).
- **G-profile** (one run each; asked: the fog at most 1.0 ms in the opening and the frame's net
  rise at most 0.6 ms, the shafts and the mist gone): the first run, 64 slices, each slice's
  integral a loop over those before it and the sky from `held_at`'s ten samples, took **2.93 ms**
  on the walk and 1.79 in the opening, the walk's span +1.31 ms. With 48 slices (the plan's first
  lever), the scan, and four samples for the sky:

| | walk B9 | walk B10 | opening B9 | opening B10 |
|---|---|---|---|---|
| fog, p50 ms | | 1.91 | | **0.78** |
| water (the mist pass was its) | 0.73 | none | 0.62 | 0.20 (296 frames) |
| GPU span p50 | 13.97 | 14.24 (**+0.27**) | 13.88 | 14.17 (**+0.29**) |

  The opening meets both; the walk's fog is 1.9 ms (the wood: its canopy's haze on most froxels).
- **G-memory:** a `fog` share of 16.6 MB (three volumes of 160 × 90 × 48 half floats); the shafts'
  1.8 MB target is gone.
- **Off is B9's image:** the clearing, which has no fog: 2 pixels by 1/255 against B9c (native,
  640 × 360, 240 frames).

## 2026-10-26: B11, geometry from the field (world-class 2, the 0.9.0 checkout)

**B11c, the cluster path leaves the game (D14).** `engine::stone::ClusterStones` and its shading
are gone, and with them the Last Green's `CLUSTERED` switch and its branches in its choosing, its
prepass, its shading, its load and its readiness (the stage's lines went with B10's commit, the
same file). `engine/virtual.wrela` stays for `examples/clusters`, the soft rasterizer and its
tests, with a note that no game draws through it. `wrela pipelines games/last-green`: **93
pipelines before and after**: since toolchain 0.8.0 (A6) the false constant built none of the
path's 17. The path was off, so nothing the Last Green ran changed.

**B11d, rock as a material** (`engine::rock`). A stone's colour and roughness cooked per vertex
at realization by where the vertex is (`cook_rock`, a `RockLook` through `stone::realize_rock`):
strata (bands about 0.3 to 1.2 m deep along a bedding direction, warped by a noise, each its tone
between 0.72 and 1.18 of the rock's), cracks (Worley's edge distance on 1.6 m cells, darkened by
up to 0.45), lichen (faces under 30°, over half a metre above the ground, in patches: a grey-green,
a fifth at most) and wetness (within 0.6 m of the nearest water's level: 0.6 of the colour,
roughness 0.35). The colour's alpha holds the roughness, which `lit_stone` and the pool's stones
now read (it was a constant 0.8; a rock cooked from its field alone still says 0.8). The plan's
`Material.rough` and `.wet` aren't needed: stones keep their four-byte colours, the wet band in
the colour and the roughness. The Last Green's stones, standing slabs, waystones, crag beds and
bank stones are rock (its dressed masonry, the blocks, the walls and the tower, keeps its look);
each built thing's water is the nearest water tile's level, a bank stone's its own lowest 0.3 m.
Its parameters are proposed.

- **The test** (engine/tests/rock): a 2 m cube of rock, bands 0.5 m deep, the water 0.3 m over its
  foot: **4** distinct tones up its sides; its wet foot 0.14 of luma against 0.26 just above.
- **G-profile:** a frame's `built things` (its realizing and colours, at load) p50 4.80 ms on the
  walk and 7.85 in the opening, against 4.77 and 5.92 at B10 (asked: at most 2×).

**B11a, levels of a built thing from its field.** Every built thing but a crag's beds now has a
far level: its field extracted again with four times the cell and tolerance
(`blockouts::realize_far`, `FAR_COARSER`), which takes over across a band where its largest cell
covers about four pixels (`far_band`: a stone's and a slab's middle 50 m, a waystone's 37, a
block's 75, a wall's 124; the tower keeps its 64 to 96 m), each level drawn over its share of the
pixels (the dithered coverage the tower's two levels had). The Last Green's 115 such things hold
760,361 vertices near and **132,522 far (17.4%)**.

- **Measured:** the GPU span doesn't move (walk 14.24 ms, opening 14.30; 14.15 and 14.27 at B11d):
  the built things are a small share of the frame. A load frame's `built things` p50 7.70 ms (the
  far level realized too; 4.80 before), the `built` share 60.2 → 64.7 MB; playable 7.5 and 7.4 s.
- **Not done:** the plan's continuous blend (each fine vertex's coarse normal from the filtered
  field and its presence from the coarse level's occupancy, the shading normal and a dithered
  fade blended per instance), its prototype's switch sweep on the tower, a crag, a post and a
  bank stone from four views (B11a.0's acceptance number), the thin-rod and coarse-normal tests,
  and levels for the banks' placements, the rocks and the herd. The sweep needs a capture tool
  with each instance's level pinned (none exists yet); the herd's realizer has dithered levels of
  its own; the banks and the rocks are a few prototypes placed many times, each level another
  prototype in the pool. What draws now is the tower's dithered cross-fade, for everything built.

**B11b, the crags as solids: already so.** Since the land's re-architecture (#330) a crag of two
tiers or more (`land::place::bedded`, 20 m and up) stands as beds of rock (`crag_slabs`): built
things realized from their fields, the ground's substrate leaving their volume out
(`Ground::substrate_height`), collision reading the full ground. B11d gives them rock's look.
What stays a heightfield is a lower crag, an outcrop, by design (one rounded mass, soil in its
pockets: the land's note says a 4 m rock cut as one slab reads as a blank sheet). Nothing in the
ground changed, so `height_at` and the sim are as they were. The beds' realized surface against
their field (the realizer's `measured_error`) wasn't measured here.

## 2026-10-26: B12, water (world-class 2, the 0.9.0 checkout)

1. **Flow.** The water's mesh already carries a flow per point (`WaterVert.flow`, a course's
   direction times a speed from its slope: `land::water`'s 0.35 + 2.5 × slope, close to the
   plan's), so no field layer is cooked for it. What was missing is bounded advection: the
   ripples drifted as `p − flow × t`, which draws them out without end where the flow varies.
   They're now carried in two phases half a 2 s period apart, each started again when its
   weight is none (`water::advected`; Vlachos 2010). Test: a marker on a 1.2 m/s course moves
   6 cm in 0.05 s in either phase, the weights a whole at every time.
2. **Falls.** The mist is the air's (B10: `mist_over` is gone). Each fall within 60 m of the eye
   throws spray each frame (soft, the foam's colour at 0.6, 0.3 to 0.8 m, 1.5 s: about 200 a
   second) and a column of foam at its foot (light that adds, 0.8 s): `stage::spray`, the floor's
   `Scene::emit`, the frame's particles' first use. The foam is the additive kind the particles
   have (`SPARK`), not a new one. Test: a fall in view throws spray and foam each frame, one past
   60 m none. Neither benched scene passes within 60 m of a fall.
3. **Wet margins: already so.** The baked wet field darkens and soaks the ground by a stream
   (`ground::covered`'s `soaked`, its banks and beds), and rock's wet band (B11d) reads the
   nearest water's level.
4. **The planar reflection of the mere (D15): not done.** A mirrored pass of the terrain, the
   wood's far ways, the built things' coarse levels and the sky, with its own targets, is the
   largest item here; the reflections stay the screen's (marched in threes) and the specular
   probe's. D15 is off by its absence.
5. **Culled by tiles: done at B5.2** (64 m tiles, one draw, the copy scissored).

- The render package's water tests hold (`water_reflections_marched_in_threes...`,
  `..._follow_raster_jitter...`). The opening's `water` label p50 0.20 ms, as before (asked: at
  most 1.0).

## 2026-10-26: B13, the image (world-class 2, the 0.9.0 checkout)

1. **The resolver: measured and reverted.** Dilated motion (FSR 2's: a moving surface's motion
   from the nearest depth of the 3 × 3 round the pixel's nearest sample) took the opening's
   `temporal` from 1.73 to 2.72 ms and its GPU span from 14.29 to 15.22 ms (allowed: 0.15 ms);
   kept to the screen box where something can move (`Resolve::moving`), the same, the box
   covering nearly the whole frame where the grass bends. Reverted (8e697e46): the resolve 1.72
   ms again. The reactive mask (wider boxes, a higher blend floor for cards, grass, water and
   particles) and the lock for thin features aren't built: each trades flicker for ghosting,
   and their settings want the lookdev gate's moving measures (B16's run), which B13 didn't
   have. The render package's seven temporal tests held with and without the dilation.
2. **Cloth that reads.** `Lit::sheen` and `sheen_tint`, `light::sheen_light`: Charlie's
   distribution (Estevez and Kulla 2017) with Ashikhmin's visibility for the sun, a fit of its
   directional albedo (0.12 head on to 0.9 edge on) for the sky. The climber's coat: sheen 0.6,
   tint (0.08, 0.06, 0.12), dark in the hood's tunnel; its albedo stays its 0.012 (the plan's
   0.02 is near it). Test: near-black velvet's edge **9.6** times its middle with a sheen of 1,
   **0.6** without (asked: over 1.3, under 1.1).
3. **HDR output (D11).** `Look::output` (SDR by default, as D11 says; HDR a setting) and `peak`
   (4.0); with HDR the frame asks for an HDR screen in Display P3 once, and the develop, while
   `screen_is` says HDR, outputs linear light rolled off above SDR white toward the peak, chroma
   kept, no SDR curve or display grade. Test: 0.5 stays 0.5, 3.0 white shows between 1 and the
   peak, nothing past it. SDR's path is the same code as before.
4. **`f16` twins: none kept.** `wrela precision` at tol 1e-3: `grass::bloom_drift` over ±500 m
   (49 operations, 5 safe, 44 unsafe: the lattice hash's range), `noise::noise3` over ±200 m (46,
   7 safe, 38 unsafe), `light::aces` over 0–16 (9, 5 safe, 4 to watch), `temporal::tamed` over
   0–64 (4, all safe). The paths that cost (the cover noises, the sward, the sky's march) hash
   world positions, which no `f16` holds; the one wholly safe function is four operations, too
   few to save a tenth of its label. The reports are kept with the step's run.
5. **Relaxed SIMD (D16): stays off.** `wrela profile games/last-green --chrome --optimize
   --frames 480 --compare <a --relaxed-simd build> --pairs 7` (its default scenario, the herd, at
   1280 × 720): CPU frame 8.64 → 8.46 ms (−2.2%, the pairs' ranges overlapping), GPU span −4.8%,
   inconclusive. D16 asks 3%: `wrela build --release` keeps it off.
6. **Leaves against the sun.** A leaf's light through it gains a narrow lobe toward the sun
   (strength 0.35, proposed), on the crown's outer leaves, under the sun's shadow: cards,
   impostors and volumes alike.

- **The step's gate** (the opening's GPU frame p50 with every switch of B5–B13 on, asked: 12.0
  ms): **14.25 ms** (misses by 2.25). Its heaviest labels: vegetation 3.2, look 2.6, clouds 2.5,
  terrain 2.3, creature 2.2, temporal 1.7, fog 0.8, probes again 0.9 (where it runs).

## 2026-10-26: B14, the frontier (world-class 2, toolchain 0.9.0)

Four experiments, each behind a switch that is off unless its rule says it pays (D18). The
decisions, then each experiment's numbers.

| Experiment | Switch | Its rule | Measured | Default |
|---|---|---|---|---|
| Neural impostors (14.2) | `Quality::neural` | on past 160 m if PSNR ≥ 32 dB and the pass ≤ 1.3× the atlas's | PSNR 13.76 dB (rung D over all 256 views, the game's tree); every kind through the net: the frame 421.8 ms against 16.6 | **off** |
| Learned history rejector (14.3) | `Quality::learned_resolve` | on if held-out PSNR ≥ +1.0 dB over the heuristics, the resolve ≤ +0.2 ms, flicker and pop not up | +0.10 dB held out (rung A); the resolve +27.6 ms | **off** |
| Radiance cascades (14.4) | `Quality::cascades` | on at the finest rung ≤ 2.0 ms with trunk shade +15 % and no leak | 13.1, 4.4 and 3.2 ms at R1–R3; shade +95 to +109 % | **off** |

### 14.1 Small networks and `studio/fit`

`engine/neural.wrela`: two-hidden-layer MLPs (ReLU, linear out), a hand-written backward pass,
Adam, MSE and L1; weights shipped as half floats two a word and evaluated in a shader
(`eval_gpu`). The trainer is on the CPU (a step's batch split over the program's helpers in the
fit fixtures): the nets are small, and a GPU backward pass for thousands of weights is a project
of its own.

- A hand-set 2 → 2 → 1 net is its hand computation at four inputs. The gradient against central
  differences at 100 weights of a 4 → 8 → 8 → 2 net: within **1.6e-3** relative (f32's rounding;
  the plan asked 1e-3, the test asks 2e-3). A 1 → 16 → 16 → 1 net learns `sin` over [-π, π] to a
  held-out MSE of **1.7e-4** within 2,000 steps (asked: under 1e-3).
- The shader's evaluation against the CPU's at 1,000 inputs: within **0.13/255** after scaling
  (asked: 1/255).
- `wrela tool fit -- --net sine --out content/kit/sine_net.wrela`: a 1 → 32 → 32 → 1 net in
  **20 s** (asked: under 60), held out 1.9e-5; read back from its half floats, `sin` within
  0.0033 on average (0.015 at worst) at 100 points (asked: 1e-2). It refuses paths outside
  `content/` and `build/`.
- **1d, the truth renderer** (`wrela tool fit -- --net rejector`, with 14.3's dataset): the
  floor driven natively at 1280 × 720, its shot 2 settled, time frozen (F10) and the eye dollied
  along its look (F11); every 32 frames the resolve's findings at 20,000 pixels, then the dolly
  paused and the lookdev instrument's reference drawn there (F12: every knob at its most, no
  history, the jitter on an 8 × 8 grid), its 64 frames' mean of this frame's luma at the same
  pixels the truth. 50 frames in **701 s** (14 s a frame: 200 would take about 47 minutes;
  asked: 20 for 200). The resolver's own blend against the truth: **32.96 to 36.74 dB** a frame,
  35.04 on average (asked: at least 30). The world is frozen while the eye moves (the frozen
  time is what lets the reference be the play frame's): the dataset has the camera's motion, not
  the wind's or the creatures'. A correction from the council's review: until it, F12 turned
  the history **on** where it meant off (`Frame::set(Off::History, on: true)`), so every
  reference above, and the lookbook's quality references of this part, averaged frames the
  history had already filtered. The resolver's 35.04 dB and 14.3's dataset were measured
  against a truth with the resolve in it; they flatter the resolver and weren't run again (its
  rejector stays off). F12 now turns the history off.

### 14.2 Neural impostors

The dataset (2a): the great tree's impostor baked as the engine bakes a kind's (16 × 16 views of
64²), read back as 1,048,576 rows `(view_uv, texel_uv) → (rgb, coverage, oct normal, toward,
depth)`, every 16th view held out (983,040 rows trained on, 65,536 held out). The ladder (2b),
each rung 4,000 steps of 16,384 rows (Adam at 1e-3), three seeds, PSNR over colour (weighted by
the true coverage) and coverage at the held-out views:

| Rung | Seed | PSNR dB | colour MSE | coverage MSE | s | loss, first 500 steps → last |
|---|---|---|---|---|---|---|
| A | 1 | 12.19 | 0.02030 | 0.08575 | 176 | 0.03834 → 0.02867 |
| A | 2 | 12.57 | 0.01890 | 0.07844 | 172 | 0.03861 → 0.02849 |
| A | 3 | 11.70 | 0.02237 | 0.09634 | 177 | 0.04797 → 0.03091 |
| B | 1 | 12.65 | 0.01846 | 0.07702 | 144 | 0.03280 → 0.02713 |
| B | 2 | 12.29 | 0.02224 | 0.08236 | 144 | 0.03497 → 0.02711 |
| B | 3 | 12.68 | 0.01578 | 0.07826 | 157 | 0.03523 → 0.02718 |
| C | 1 | 14.09 | 0.01110 | 0.05666 | 177 | 0.02645 → 0.02322 |
| C | 2 | 13.98 | 0.01271 | 0.05726 | 204 | 0.02702 → 0.02264 |
| C | 3 | 13.57 | 0.01367 | 0.06312 | 254 | 0.03210 → 0.02433 |
| D | 1 | 14.39 | 0.01111 | 0.05244 | 357 | 0.02599 → 0.02149 |
| D | 2 | 14.41 | 0.01058 | 0.05250 | 303 | 0.02464 → 0.02086 |
| D | 3 | 14.45 | 0.01098 | 0.05170 | 443 | 0.02462 → 0.02089 |

Rung D again after the council's review, the fixture baking the great tree with the game's seed
(1, the kind's place plus one; the table above baked seed 7, a tree the game doesn't draw):

| Rung | Seed | PSNR dB | colour MSE | coverage MSE | s | loss, first 500 steps → last |
|---|---|---|---|---|---|---|
| D | 1 | **14.39** | 0.01075 | 0.05253 | 282 | 0.02605 → 0.02219 |
| D | 2 | 14.15 | 0.01251 | 0.05476 | 267 | 0.02469 → 0.02136 |
| D | 3 | 14.32 | 0.01113 | 0.05325 | 259 | 0.02461 → 0.02147 |

(`wrela tool fit -- --net impostor --rungs D --seeds 3 --steps 4000 --out
content/kit/great_tree_net.wrela`, 2026-10-27.) The same tree as the game's, the same result.

Parameters: A 488, B 1,480, C 616 and a 2,048-value grid, D 3,368 and an 8,192-value grid. No
rung reaches its mark (26, 29, 32, 33 dB: these are results under the fixed budget). Every curve
still falls at 4,000 steps, slowly (D's last 2,000 steps: 0.0216 → 0.0209), so the budget, not
only the capacity, bounds them; but the step from B to C (the grid) is the only large one: a
crown's leaves are detail at the texel, which a net of the view's and the texel's coordinates
doesn't hold and a 32² grid over a 64² cell only begins to.

2c, the shader: `impostor::net_texel` evaluates a kind's net (and its grid, bilinear) where a
view is read, in the far crowns' cut and shading, under `Quality::neural`
(`wood::Wood::read_through`; the Last Green gives its great tree the net when the switch is on).
Test (engine/tests/neural-impostor): a rung-D-shaped net with a grid, the shader's eight values
at 1,000 points of the views against the CPU's evaluation of the same half floats: within
**0.000/255**.

2d, the measures and the rule (the opening at 1080p, `wrela tool bench`, two runs each, the
machine settled: its frame without any switch 16.65 ms at the start and 16.50–16.63 at the end):

- **PSNR against the atlas, all 256 views at 64²:** rung D's best seed (3), **13.86 dB** (14.45
  held out; trained again on the game's tree, seed 1's 13.76 dB and 14.39 held out), written as `content/kit/great_tree_net.wrela` (the great tree: the floor's one tree
  whose kind the fixture bakes).
- **Time:** the great tree alone through its net: the GPU frame 16.64 ms against 16.65 (a single
  tree, too little of the frame to tell). Every kind through the same net (a measuring
  setting, `impostor::EVERY_KIND` in be308267, since removed: its images were the one kind's): the vegetation pass **390.0 ms** against 5.4 and the
  GPU frame **421.8 ms** against 16.6. The evaluator is the plain one (`neural::eval_gpu`: each of
  rung D's 3,264 multiply-adds reads its weight from storage and unpacks its half): the plan's
  `vec4h` dot products would cut that several times, not the 50 that 1.3× asks.
- **Bytes per kind:** the atlas 12 MiB plain (1024², RGBA8 and two RG16F), **3 MiB** compressed
  (BC7 and two BC5); rung D's net and grid **22.6 KiB** (11,560 half floats), 136 times fewer
  than the compressed atlas.
- **The rule** (on past 160 m if PSNR ≥ 32 dB and the pass ≤ 1.3× the atlas's): both missed, by
  18 dB and by far: **off**. The trained net stays (`content/kit/great_tree_net.wrela`), the
  switch reads it, and the bytes are the case for a next attempt: a net per kind 136 times
  smaller than its atlas if its detail could be learned (a finer grid than rung D's, more steps).

### 14.3 A learned history rejector

`engine::temporal`'s resolve is split into what it finds at a pixel (`found`: this frame's
reconstruction, the history reprojected and clipped, the heuristics' share, and what a learned
rejector reads) and the blend (`blended`), so a capture kernel (`found_rows`) reads the same
findings the resolve blends; the render package's seven temporal tests pass unchanged. Under
`Quality::learned_resolve` the share is the sigmoid of `REJECTOR`'s net (rung A: 14 → 16 → 8 →
1) over the 3 × 3 samples' luma round the nearest, the history's luma unclipped, how far the point
moved, how far its old depth is, whether thin geometry covers it, and its class; capped as a
settled view's share is.

- **3a, the dataset:** 50 frames (above), of each frame's 20,000 asked pixels those whose share
  isn't forced, up to 5,000 the heuristics clipped and 5,000 they didn't: **388,140 rows**
  (312,351 from the first 40 frames to train on, 75,789 from the last 10 held out).
- **3b, the baseline:** the heuristics' blend against the truth: 35.04 dB a frame on average
  (above); **34.10 dB** over the held-out rows.
- **3c, the ladder** (4,000 steps of 4,096 rows, Adam from 1e-3, the loss `|mix(history,
  current, a) − truth|`):

| Rung | Inputs | Held out: heuristics → learned | Trained on: heuristics → learned | s |
|---|---|---|---|---|
| A | 14 | 34.10 → **34.21 dB (+0.10)** | 35.17 → 35.50 (+0.33) | 33 |

  The mark is +1.0 dB: A misses it, and so does what it fits on the frames it trained on. **B and
  C (the samples' chroma, the history's 3 × 3) aren't tried: Blocked** on a capture with 27 more
  floats a row; with A at a tenth of the mark held out and a third of it on its own frames,
  what's left for a net to learn over these heuristics on this truth is small.
- **3d:** the shader's share against the CPU's at 998 pixels of a dollied frame (those not
  forced; `wrela tool fit -- --net rejector --check`): within **0.000/255**. The opening's
  `temporal` with the learned share: **29.4 to 29.8 ms** against 1.78 (two runs each): rung A's
  360 multiply-adds a pixel, each weight read from storage and unpacked, at every output pixel
  of 1080p. **The rule** (on if +1.0 dB held out, at most +0.2 ms, flicker and pop not up):
  missed on the first two: **off**, and G-look's moving measures weren't needed. The net stays
  (`content/kit/rejector_net.wrela`) with the switch.

### 14.4 Radiance cascades

`engine/cascades.wrela` (Sannikov 2023): four cascades of screen-space probes on the last
frame's depth, c0's probes 8 scene pixels apart with 4 × 4 directions (octahedral, the whole
sphere), each cascade's probes twice as far apart with twice the directions a side, over 0–0.5,
0.5–2, 2–8 and 8–32 m. Each ray marches the solids' distance volume with the conservative
bounded advance (4a); what it meets gives back an albedo of 0.3 under the sun (the shadow maps)
and the probes' sky, and c3's rays that meet nothing hold the clouded sky. The merge (c2 to c0)
is depth-aware at every query (an upper probe whose distance differs by over a tenth is weighed
out), radiance and transmittance together; the resolve, at half the scene each way, takes the
four nearest c0 probes (5 %) and the cosine-weighted sum of their directions, and writes the
light over π where the ambient occlusion goes: the next frame's shading reads it for the sky's
term (`ambient::cascaded_at`) in place of the probes' times the occlusion. The directions turn
each frame in a cycle of four. Known limitations: the crowns' leaves aren't in the volume (the
rays see solids alone), every surface's albedo is 0.3 (no surface cache), and a ray that leaves
the volume (96 m round the eye) goes on as open air.

- **4a, the march** (engine/cascades.wrela's tests, analytic shapes, 16 and 8 steps): a wall
  2 cm alongside a ray never stops it; slabs 1 m and 10 cm thick in c3's interval, and one at
  30°, are met; over 10,000 rays each grazing a ball, **0** of 5,964 that a 256-step march meets
  are passed (asked: under 1 %). After the council's review the march no longer takes its
  tangent estimate on trust (a ray 1.05 m past a unit ball's centre was "met" where the ball
  was 0.2 m away): the step is sampled at its quarters and an estimate is a hit only within
  10 cm of a surface. Then **2 of 5,966** are passed at 16 steps and **28 of 5,966 (0.47 %)**
  at 8, and rays 15, 30 and 50 cm past a ball meet nothing. (4d's times are from before this:
  the march samples more a step now; the switch is off either way.)
- **4a–b on the GPU** (engine/tests/cascades: flat ground through `engine::frame` with the
  cascades on, a hollow box and a wall in the volume): from inside the closed box **0 of 64**
  rays leave it; a wall a metre before a probe is met by **c1 alone**; up from over open ground
  no cascade meets anything; and the light the cascades leave over the open ground in view is the
  sky's (its cosine-weighted mean over the hemisphere, 1,024 directions) within **5.6 %** on
  average (asked: 10 %). Not built: the merge's tests with a lit wall beside a sphere and a post
  (4b's), which need the surfaces lit as the scene's are; the open-sky and closed-box ones stand
  for them.
- **4d, the rungs** (the opening at 1080p, two runs each; the shade, the luma of the off still's
  darkest quarter of pixels in `02-beech-high-forest`, a stand-in for the trunks' shaded sides,
  whose class the lookbook's stills don't carry; the leak, `16-the-fawn-in-the-glade` under the
  climber's hem, looked at):

| Rung | c0 probes | Steps | `cascades` ms | GPU frame ms (off: 16.65) | Shade, darkest quarter | Leak under the hem |
|---|---|---|---|---|---|---|
| R1 | 8 px | 16 | **13.12** | 29.35 | +95 % (0.026 → 0.051) | none seen |
| R2 | 16 px | 16 | **4.42** | 20.53 | +109 % | none seen |
| R3 | 16 px | 8 | **3.18** | 19.42 | +91 % | none seen |

  Every rung gives the shade far more than the +15 % asked (the probes' sky under the canopy is
  dimmer than open rays through a volume without leaves: the leaves are this spike's missing
  occluder) and none meets 2.0 ms: **off**. The stills (`build/stills-b14/`) show the trunks'
  shaded sides lit and the ground under the canopy a little darker, and a defect: dark bands
  across thin trunks where the bark's probes find no light, which pushing a probe out of a solid
  the volume holds it in (`cascades::outside`) didn't cure. The next levers are the plan's: a
  surface cache in place of the 0.3 albedo, the leaves' density in the rays' transmittance, and
  the cascades traced at a lower rate (a quarter of the directions a frame).

## Public visual references

The vale's look against freely licensed photographs and footage of its real subjects
(docs/plan-world-class-3.md §0.5 G-reference, §B5; the review: docs/lookdev.md, "The reference
review"). The thresholds are internal quality targets, not statistical evidence of engine
equivalence.

### 2026-10-28: the baseline (B5), today's renderer, toolchain 0.10.0

The vale as Part B leaves it (every switch on, its creature shots with the fawn), its twelve
evaluation shots at 1920×1080 (`build/vale-b5c/stills`, kept and never regenerated) against
`references::vale::VALE`'s twelve evaluation references. Eight agent judges in fresh contexts, each
given the rubric, the pairs and the references' credits and conditions, told to judge by viewing
alone; all eight passed both calibration pairs (an original against its 3×3 blur passed eight
times, and against its quarter-resolution copy, each image under a neutral name).

- **Mode:** attributed (each reference's credit, licence, source and conditions shown beside it).
- **The shots' hash** (SHA-256 over `lookbook.wrela`'s `SHOTS` as JSON, every field of each):
  `88c877da27abda2b01a81d2777a35b66b533f7c5b82902306b64c2825ede6ca1`.
- **The references' manifest hash** (every field of each record):
  `b78df49ef41682168868ef3443f88f0d7a423eaf4f8d0595d938924b551bdaf3`.
  Both are frozen from here (`studio/compare`'s `FROZEN_SHOTS`, `FROZEN_MANIFEST`): every later panel
  asserts them.
- **Order:** seed 5 (`build/vale-b5c/key.json`); every judge's file in `build/vale-b5c/judges/`, the
  tally in `build/vale-b5c/tally.json`.
- **Commands:** `wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR --
  build/vale-b5c --a build/vale-b5c/stills --set evaluation --seed 5`, the eight judges, then
  `wrela tool studio/compare --write ws=. -- build/vale-b5c --tally`.

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 1.38, 1.12, 1.25, 1.12, 0.62 | 2.50, 3.00, 2.62, 2.12, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.12, 3.00, 2.25, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 1.88, 1.88, 2.00, 1.00 | 2.12, 3.00, 2.62, 2.38, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.38, 1.00, 1.00, 0.25, 0.75 | 2.00, 3.00, 2.38, 2.00, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.12, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 2.88, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 1.75, 2.00, 1.75, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 1.88, 2.00, 1.62, 2.00, 0.88 | 2.50, 3.00, 3.00, 2.50, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.88, 1.88, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 1.00, 0.00 | 2.75, 2.88, 2.00, 2.25, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.75, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 2.88, 3.00, 3.00 |

Overall: 0.0 % prefer the still or rate the pair equal (fails: 50 %); per pair: a pair lost worse than 6 to 2; majority: 0 pairs won or equal (fails). **Fails.**

Ranked losses (the reference's mean less the still's, summed over the pairs):

- artifacts: 26.75
- colour: 15.88
- value: 14.38
- depth: 13.62
- light: 12.50

Each pair's worst row, and its judges' sentences on it:

- Pair 1, artifacts:
  - c1: None seen in the photo; the render shows polygon faceting on the log ends, stretched bark texture and hard-edged water cutouts around the trunk.
  - c2: None seen in the photograph; the render shows stretched bark texture, faceted cylinder silhouettes, logs interpenetrating and a hard-edged water card.
  - c3: The photograph shows none; the render has faceted cylinder geometry, a water plane slicing through the log and ground, stray translucent shapes, stretched bark textures and spiky branch stubs.
  - c4: The photo shows no artifacts, while the render shows faceted cylinders, stretched and repeating bark texture, dark outlines and a hard-edged water polygon cutting into the log.
  - c5: The photo shows none; the render has faceted cylinder silhouettes, stretched bark texture, dark outline seams and a hard-edged water plane cutting the logs.
  - c6: None seen in the photograph; the render shows smeared, stretched bark textures, faceted log ends and hard-edged water planes cutting through the logs.
  - c7: None seen in the photograph; the render shows faceted polygon silhouettes, badly stretched bark textures, a hard-edged translucent water sheet over the soil and spikes clipping into the ground.
  - c8: None seen in the photo; the render shows faceted log ends, stretched texture and a hard-edged water cut-out around the log.
- Pair 2, artifacts:
  - c1: The render shows dithered leaf decals, a smeared blotch near the centre and logs hovering above the ground; none seen in the photo.
  - c2: The render shows floating leaf cards, logs hovering over the ground and blocky shadow shapes; none seen in the photograph.
  - c3: The render has scattered leaf decals that read as noise, blocky stippled shadows, floating fallen logs and a smudged dark blot mid-frame; the photograph shows none.
  - c4: The render shows floating leaf cards, dithered shadow edges and sticks crossing oddly, while the photo shows nothing beyond a slightly clipped sky.
  - c5: The render has stippled leaf-card scatter, dithered foliage edges and floating or intersecting logs; none are seen in the photo.
  - c6: The render shows stippled, dithered shadow edges, sparse floating leaf cards and logs that hover or clip the ground; none are seen in the photograph.
  - c7: The render shows dithered checker patterns in the shadows, leaf cards floating over the soil, sticks intersecting the terrain and a smeared blotch at centre; the photograph shows only minor compression.
  - c8: The render shows stipple dither in sunlit patches, floating leaf cards and a blurred smear on the ground; none seen in the photo.
- Pair 3, artifacts:
  - c1: The render shows stippled shadow dithering, repeating bark tiles on the near trunk and a floating stump with roots; none seen in the photo.
  - c2: The render shows a tiled block pattern on the near bark, stamped decals on the ground, and spiked log stubs; none seen in the photograph.
  - c3: The render shows a regular stipple pattern on the ground, star-shaped spiky log ends, stamp-like bark and a floating white stump; the photograph shows none.
  - c4: The render shows repeating bark rectangles on the near trunk, dithered leaf shadows, outlines and odd spiky stumps, while the photo is clean.
  - c5: The render shows dithered shadow speckle, blocky tiled bark, spiky log stubs and a floating pale stump; none are seen in the photo.
  - c6: The render shows a tiled, blocky bark texture on the near trunk, a dotted tiling pattern on the floor, a floating white trunk stub and crude star-shaped log ends; none are seen in the photograph.
  - c7: The render shows a floating stump with leg-like roots, spiky polygon branch stubs, stretched log textures and dot-pattern dithering in the shadows; none are seen in the photograph.
  - c8: The render shows dithered shadow stipple, brick-like bark tiling and spiked log ends; none seen in the photo.
- Pair 4, artifacts:
  - c1: None seen in the photo; the render shows repeating arrow-shaped decals along the right, thin squiggle lines and streaky shadow aliasing.
  - c2: None seen in the photograph; the render shows stamped triangular decals, thin wiggly lines and smeared shadow banding.
  - c3: The photograph shows none; the render has arrow-shaped decals along the sunlit edge, squiggly line marks, and stippled, streaky shadows.
  - c4: The render shows triangular arrow-shaped stamps, black squiggle decals and streaked shadow sampling, and the photo has none.
  - c5: None are seen in the photo; the render has arrow-shaped decal marks, squiggly dark line patterns and streaky smeared shadows.
  - c6: None seen in the photograph; the render shows triangular arrow-shaped decals, thin black squiggle lines and smeared, streaked shadow shapes.
  - c7: None seen in the photograph; the render shows arrow-shaped triangle decals, thin squiggle lines on the surface and streaky, stepped shadow edges.
  - c8: None seen in the photo; the render shows blocky stepped shadow patterns, arrow-shaped decals and drawn-looking crack lines.
- Pair 5, value:
  - c1: The photo spans dark shaded trunks to bright leaves; the render's half-frame of dark mud and pale haze sit in a narrow mid range.
  - c2: The photograph runs from deep shade to lit leaves; the render's large dark ground and pale haze leave the forms in a narrow band.
  - c3: The photograph runs from dark shade to sunlit leaves and every trunk reads; the render's foreground is a large field of one muddy mid-tone and the trees sit in a narrow band.
  - c4: The photo spans dark understorey to lit trunks, while the render's values are compressed into dull mid-tones.
  - c5: The photo has a full range from dark trunks to lit foliage; the render's large dull ground plane and mid-green foliage compress the values.
  - c6: The photograph has a full range from shaded trunks to sunlit leaves; the render sits mostly in the mid-tones with a dark, featureless ground slab.
  - c7: The photograph reads from dark trunks to lit leaves; the render's large ground slab is a single dull midtone and the trunks are uniformly pale.
  - c8: The photo spans dark trunks to sunlit leaves; the render is dominated by a large flat midtone ground.
- Pair 6, artifacts:
  - c1: None seen in the photo; the render shows repeating brick-like bark on the near log, a pale translucent patch mid-frame and dark outlines on the snags.
  - c2: None seen in the photograph; the render shows blocky tiled bark on the near log, spiky branch stubs and blob-like cloud shapes.
  - c3: The photograph shows none; the render has blocky, tiled bark on the near log, spiky branch ends, blobby cloud edges and a faint pale patch mid-frame.
  - c4: The render's near log carries a tiled brick-like bark pattern with outlines, and its blobby clouds and forked dead trunks look synthetic.
  - c5: The photo shows none; the render has blocky tiled bark on the logs, low-poly log ends and blobby cloud shapes.
  - c6: None seen in the photograph; the render shows a tiled, brick-like bark texture on the near log, blobby clouds, a smeared plain texture and a pale patch in the mid-distance.
  - c7: None seen in the photograph; the render shows a repeating brick-pattern log texture, black outlines on silhouettes, a smeared flat plane at the horizon with a pale blotch in it, and puffy cloud cut-outs.
  - c8: None seen in the photo; the render shows outlined dead trees, brick-patterned bark, blobby cloud shapes and a pinkish patch in the mid-ground.
- Pair 7, artifacts:
  - c1: The render shows leaf decals sprayed like stickers, a bluish seam down the trail and stretched texture on the near log; none seen in the photo.
  - c2: The render shows floating leaf cards, untextured pale trunks, spiked log stubs and a flat-shaded deer; none seen in the photograph.
  - c3: The render has scattered leaf decals floating over the ground, a low-detail deer, star-shaped log ends and a pale smeared strip along the path; the photograph shows none.
  - c4: The render shows floating ground-cover leaf cards, a faceted deer, a tiled log and a stray water strip, while the photo has none.
  - c5: The render has noisy leaf-card scatter, stretched log textures and a flat-shaded low-poly deer; none are seen in the photo.
  - c6: The render shows black-edged leaf cards, a hovering log, crude star-shaped log ends and a low-detail deer; none are seen in the photograph.
  - c7: The render shows black leaf cards floating above the soil, a seam strip across the trail, stretched log textures and an unshaded creature; none are seen in the photograph.
  - c8: The render shows confetti-like floating leaf cards, ground stipple and a pale seam line across the ground; none seen in the photo.
- Pair 8, artifacts:
  - c1: The render shows a crude low-poly deer, a hovering log, dithered foliage edges and a few isolated grass sprites; none seen in the photo.
  - c2: The render shows a plastic-looking flat-shaded deer, floating grass tufts and a smeared foreground ground texture; none seen in the photograph.
  - c3: The render has a low-poly deer with faceted shading, a fallen log that appears to float, and painterly foliage blotches; the photograph shows none.
  - c4: The render shows a low-poly deer, smeared ground texture and floating grass sprigs, while the photo is clean.
  - c5: The render shows a low-poly deer with oversized ears, a smeared ground texture and stippled foliage edges; none are seen in the photo.
  - c6: The render shows a floating, low-detail deer, blotchy foliage clumps and a repeating bark texture; none are seen in the photograph.
  - c7: The render shows a low-polygon deer with a toon-like shadow patch, blotchy foliage clusters and grass blades floating on bare soil; none are seen in the photograph.
  - c8: The render shows a dark blotch on the deer, floating grass cards and smeared foreground ground texture; none seen in the photo.
- Pair 9, artifacts:
  - c1: None seen in the photo; the render shows dark outline strokes on trunks, a stray blue vertical line at the right and spiky stub branches on the logs.
  - c2: None seen in the photograph; the render shows dark outlines around trunks, spiked log stubs and a thin blue vertical leak at right.
  - c3: The photograph shows none; the render has hard dark outlines around the trunks, spiky star-shaped log ends, and a stray bluish sliver at the right edge.
  - c4: The render shows black outlines on trunks, fog halos at silhouette edges, blotchy foliage and spiky log ends.
  - c5: The photo shows none; the render has dark toon-like outlines on the trunks, spiky log stubs, tiled bark and dithered foliage edges.
  - c6: None seen in the photograph; the render shows black outline strokes around trunks and logs, a tiled log texture, a spiked log end and a blue sliver at the right edge.
  - c7: None seen in the photograph; the render shows dark ink-like outlines on trunks, a halo around the main trunk, spiky polygon log stubs and a stray blue vertical line at the right edge.
  - c8: None seen in the photo; the render shows black outlines on trunks and logs, faceted spiky log ends and blocky leaf clumps.
- Pair 10, artifacts:
  - c1: The render shows heavy pixelated shadow-map aliasing, a flat translucent quad for the waterfall and floating grass blades; none seen in the photo.
  - c2: The render shows heavily blocky shadow maps, stretched log texture, floating grass blades and a flat translucent water card; none seen in the photograph.
  - c3: The render has heavily pixelated shadow maps, stretched wall textures, a flat translucent waterfall slab and a blurred bark texture on the log; the photograph shows none.
  - c4: The render shows heavily pixelated shadow maps, stretched wall texture and a flat translucent card standing in for the waterfall.
  - c5: The render has severe blocky shadow-map patterns, stretched wall textures, smeared surfaces and floating grass blades; the photo shows none beyond mild sharpening.
  - c6: The render shows blocky, pixelated shadow maps, stretched wall textures, a flat translucent waterfall sheet and floating grass blades; none are seen in the photograph.
  - c7: The render shows heavily pixelated, dithered shadow maps, a waterfall reduced to a flat translucent blue card, stretched log textures and floating grass blades; none are seen in the photograph.
  - c8: The render shows heavy blocky shadow dither, a flat translucent waterfall quad, scribbled crack lines and floating grass cards; none seen in the photo.
- Pair 11, artifacts:
  - c1: None seen in the photo; the render shows a magnified, blurred, stretched bark texture and visible polygon facets at the silhouette.
  - c2: None seen in the photograph; the render shows a blurry, stretched bark texture in repeating bars and a faceted silhouette.
  - c3: The photograph shows none; the render has a blurred, magnified bark texture, repeating slot tiling and faceted silhouette edges.
  - c4: The render shows a magnified, blurred and repeating bark texture and a faceted silhouette edge, and the photo has none.
  - c5: None are seen in the photo; the render has heavily stretched, blurred bark texture and faceted polygon silhouettes.
  - c6: None seen in the photograph; the render's magnified, blurred bark texture, smeared stripes and faceted silhouette are all plainly visible.
  - c7: None seen in the photograph; the render shows a heavily magnified, blurred, repeating bark texture and a faceted silhouette.
  - c8: None seen in the photo; the render shows smeared, stretched magnified bark texture.
- Pair 12, artifacts:
  - c1: The render shows tiled brick-like bark, dithered foliage edges and outline strokes on the trunk; none seen in the photo.
  - c2: The render shows tiled blocky bark, dark outlines on the limbs, stub branches and pixel-clumped foliage; none seen in the photograph.
  - c3: The render has a blocky, tiled bark texture with hard outlines and blotchy foliage cards with ragged edges; the photograph shows none.
  - c4: The render's trunk shows a tiled brick-like bark pattern with outlines and its foliage is blotchy, while the photo is clean.
  - c5: The render has blocky tiled bark, dithered foliage card edges and dark outlines; none are seen in the photo.
  - c6: The render shows a tiled, brick-like bark texture, dark outlines on the limbs and blotchy foliage clumps; none are seen in the photograph.
  - c7: The render shows a brick-like tiled bark texture, black outlines on the trunk and blotchy foliage clusters; none are seen in the photograph.
  - c8: The render shows brick-like bark tiling, dithered leaf-clump edges and black outlines; none seen in the photo.

Judges: 8 used; rejected by the calibration: 0.

**Verdict: fails** (expected). Overall: 0 of 96 judgements prefer the vale or rate the pair equal
(rule (a): 50 %); every pair is lost 8 to 0 (rule (b): no worse than 6 to 2); no pair is won or equal
by the judges' median (rule (c): 6 of 12). Hill to climb: pairs won or equal, 0 → 6 of 12 at §H. The
ranked losses above are the baseline loss record Part G chooses its work from (§G0).

**Two earlier attempts, superseded** (kept for the record; neither is the baseline):

- `build/vale-b5c`'s first, `build/vale-b5` (below): its calibration pairs' files were named
  `controls/blurred.png` and `controls/quarter.png`, which gave their answers away; its hashes left
  out fields a judge is shown (a reference's creator, its processing) and a shot's framing (its
  field of view); its creature shots had no creature; and its tally counted an unresolvable
  "equal" and a judge's file twice (plan three's Part B council). Its tally was 0 of 96, the
  same as this one.
- `build/vale-b5b`: the tool fixed, the fawn drawn; a single 3×3 blur pass proved invisible at
  the size an agent judge is shown a 1080p still (5 of 10 judges viewing honestly rated it equal;
  one found the original by comparing files' checksums, the originals then byte copies of a
  still), so the calibration couldn't separate judges and the panel was laid out again with the
  stronger blur and marked originals.

#### The first attempt (superseded)


The vale as B1 left it, every switch on, its twelve evaluation shots at 1920×1080
(`build/vale-b5/stills`, kept and never regenerated) against `references::vale::VALE`'s twelve
evaluation references. Eight agent judges in fresh contexts, each given the rubric, the pairs and
the references' credits and conditions; none rejected by the calibration (a still against its 3×3
blur and its quarter-resolution copy).

- **Mode:** attributed (each reference's credit, licence, source and conditions shown beside it).
- **The shots' hash** (SHA-256 over `lookbook.wrela`'s `SHOTS` as JSON):
  `922566c1df111e6c40d02f93fd398eeddab31bb15cbeaef6ffa01a014e8381f9`.
- **The references' manifest hash:** `825cf5a0ca9400cfd61ee133f0ea739ff81bd6701e341164d4b91873ca93f40a`.
  Both are frozen from here (`studio/compare`'s `FROZEN_SHOTS`, `FROZEN_MANIFEST`): every later panel
  asserts them.
- **Order:** seed 5 (`build/vale-b5/key.json`); every judge's file in `build/vale-b5/judges/`, the tally
  in `build/vale-b5/tally.json`.
- **Commands:** `wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR --
  build/vale-b5 --a build/vale-b5/stills --set evaluation --seed 5`, the eight judges, then
  `wrela tool studio/compare --write ws=. -- build/vale-b5 --tally`.

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.25, 1.00, 0.38 | 2.12, 3.00, 2.75, 2.12, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 0.88 | 2.00, 2.88, 2.25, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 1.88, 2.00, 2.00, 0.88 | 2.12, 2.88, 2.75, 2.38, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.00, 0.12 | 2.00, 3.00, 2.38, 2.00, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 2.75, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 1.50, 2.00, 1.50, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 1.62, 1.25, 1.88, 2.00, 0.62 | 2.00, 3.00, 2.88, 2.25, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 1.88, 2.00, 1.25, 1.75, 1.00 | 2.62, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 1.00, 0.00 | 2.62, 2.50, 2.00, 2.12, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.62, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 2.88, 2.75, 3.00 |

**Verdict: fails** (expected). Overall: 0 of 96 judgements prefer the vale or rate the pair equal
(rule (a): 50 %); every pair is lost 8 to 0 (rule (b): no worse than 6 to 2); no pair is won or equal
by the judges' median (rule (c): 6 of 12). Hill to climb: pairs won or equal, 0 → 6 of 12 at §H.

**The ranked losses** (the reference's mean less the vale's, summed over the twelve pairs): the
baseline loss record, from which Part G chooses its work (§G0).

| Row | Lost |
|---|---|
| artifacts | 28.12 |
| colour | 16.12 |
| value | 15.00 |
| depth | 13.25 |
| light | 12.50 |

Each pair's worst row, and its judges' sentences on it:

- Pair 1, artifacts:
  - j1: The photograph shows none, while the render shows faceted cylinders, stretched and brick-patterned bark, a hard-edged transparent water polygon and grass ghosting through the water.
  - j2: a shows none; b has stretched, smeared bark, faceted cylinders, a transparent water plane slicing through the log with a hard edge, and a hollow ring hanging off the log end.
  - j3: A is clean; b shows stretched, smeared textures on the stump end, faceted cylinders, tiling bark and a hard-edged translucent water polygon under the log.
  - j4: The photograph shows no artifacts; the render shows faceted cylinders, stretched and blocky bark texture, hard polygonal edges where logs meet the water and a translucent green grass ghost.
  - j5: A shows no artifacts, while b has faceted log geometry, stretched bark textures, hard polygonal water cut-outs and translucent sheets under the log.
  - j6: None seen in the photograph; the render shows faceted log geometry, stretched bark textures and hard polygonal water edges cutting through the ground.
  - j7: a shows nothing beyond normal photographic softness, while b shows faceted low-poly log ends, stretched and smeared bark texture, and a hard-edged water polygon cutting through the logs.
  - j8: A shows nothing beyond photographic noise; b shows heavily faceted log geometry, badly stretched and smeared bark texture, and a hard-edged translucent water slab clipping through the log.
- Pair 2, artifacts:
  - j1: The render shows dark leaf cards scattered as visible quads, logs crossing without contact, a blurred smudge mid-frame and a translucent strip at the bottom, while the photograph shows only mild JPEG softness.
  - j2: a shows logs floating and crossing in the air, scattered leaf-card confetti, a translucent smudge mid-frame and stippled foliage; b shows none.
  - j3: A shows floating leaf cards, logs hovering at odd angles, dithered trunks and a smeared patch in the middle; b shows nothing beyond mild compression.
  - j4: The render shows dithered shadow edges, a dark smudge mid-frame and noisy floating leaf cards; the photograph shows no artifacts beyond minor compression.
  - j5: A has scattered leaf sprites, stippled dithering in the shadows, flat untextured trunks and floating logs, while b is clean apart from a slightly clipped sky.
  - j6: The render shows scattered floating leaf cards, logs crossing in mid-air, smeared ground texture and a translucent sheet at bottom right; none seen in the photograph.
  - j7: a shows dark, noisy leaf-card litter, dithered checker patterns in the shadowed trunks and floating poles, while b shows none.
  - j8: A shows stippled leaf-card scatter, dithered canopy edges, sticks clipping into the slope and a faint smeared ghost near the centre; b shows no rendering artifacts.
- Pair 3, artifacts:
  - j1: The render shows a repeated stamped dot pattern on the ground, brick-patterned bark, spiky faceted log ends and a floating white trunk base, while the photograph shows none.
  - j2: a has a dotted grid pattern in the ground shadows, tiled brick-like bark, a white stump floating on the slope, and stepped log edges; b shows none.
  - j3: A shows a tiled bark pattern on the near trunk, stretched log textures, spiky stub branches and a white stump floating mid-slope; b is clean.
  - j4: The render shows stippled dither patches on the ground, a blocky tiled bark texture and spiky log stubs; the photograph shows none.
  - j5: A has stippled shadow patterns, a floating white stump base, spiky low-poly log ends and blocky bark tiling, while b shows none.
  - j6: The render has a white stump floating mid-air, stretched bark, cone-like branch stubs and dotted shadow patterns; none seen in the photograph.
  - j7: a shows a repeating brick-like bark pattern on the near trunk, dithered dots in the ground shadows, spiky log ends and a floating white stump base, while b shows none.
  - j8: A shows a trunk base hanging in the air at the upper middle, repeated stipple decals on the ground, blocky stretched bark on the near trunk and spiky faceted log stubs; b shows none.
- Pair 4, artifacts:
  - j1: The photograph shows none, while the render shows tiled arrow-shaped decals, black squiggle lines and streaky, blocky shadow texels.
  - j2: a shows none; b has repeated triangle decals at the top right, wiggly black crack lines drawn on the surface, and smeared, streaky shadow bands.
  - j3: A is clean; b shows blocky, stretched shadow-map dapple, repeated arrowhead-shaped decals and wiggly dark crack lines that read as tiling.
  - j4: The photograph shows none; the render shows triangular decal shapes, streaky smeared shadow bands and thin crack lines that read as overlays.
  - j5: A is clean, while b shows arrow-shaped decals, drawn squiggle lines and streaky, stretched shadow-map smears.
  - j6: None seen in the photograph; the render shows arrow-shaped decals, thin squiggly line marks and streaked shadow-map patterns.
  - j7: a is clean, while b shows repeated arrow-like triangular decals, drawn-looking wavy contour lines and smeared shadow streaks.
  - j8: A shows nothing; b shows dark triangular decal marks down the right side, smeared and streaked shadow aliasing, and drawn-looking squiggle lines over the surface.
- Pair 5, light:
  - j1: The photograph's side light from the left reads on the trunks and dapples the path with natural falloff, while the render's overcast is soft but flat, with a uniform ground and shadowless trunks.
  - j2: a has dappled sun from the upper left, picking out trunks and striping the path (it is not actually overcast); b is flat overcast with no direction and little occlusion under the canopy.
  - j3: A's side light picks out trunk after trunk with soft falloff into shade; b's diffuse cloud light leaves a large ground plane with almost no modelling.
  - j4: The photograph's dappled sun from upper left models every trunk and the path; the render's overcast light is even but lacks occlusion under the canopy and on the ground.
  - j5: In a, sun shafts through the canopy streak the trunks and path with believable falloff, while b is flat, diffuse light with almost no modelling on the ground or the trunks.
  - j6: The photograph's light slants down the trunks and pools on the path with clear falloff; the render's flat cloud light gives no direction and no contact shading where trees meet ground.
  - j7: In a light falls from upper left onto the trunk sides and dapples the path, while in b the overcast light is flat and there is no occlusion where trees meet the ground.
  - j8: A (despite its overcast file name it is sunlit) shows sun shafts striking trunk sides from the upper left with clear falloff into shade; b's cloud light is shapeless, with almost no modelling on the ground or trunks.
- Pair 6, artifacts:
  - j1: The photograph shows none beyond faint noise, while the render shows brick-patterned log bark, blobby clouds, a flat plain to the horizon and an odd pale patch mid-right.
  - j2: a shows only minor noise; b has a brick-tiled log texture, a translucent patch near the horizon at mid-right, and clone-stamped puffy clouds.
  - j3: A is clean; b shows a brick-like tiling bark pattern on the near log, a stray pinkish rectangular patch mid-right and blobby, repeated cloud shapes.
  - j4: The photograph shows none; the render shows blocky tiled bark on the logs, blobby repeated cloud puffs and a blurred low-detail distant plain.
  - j5: A is clean, while b has brick-like tiling on the log, low-poly branch spikes, blobby hard-edged clouds and a stray pale patch mid-frame.
  - j6: None seen in the photograph; the render shows a pinkish ghost blob mid-frame, brick-patterned bark on the log and a flat, untextured far plain.
  - j7: a is clean, while b shows puffy flat-bottomed blob clouds, brick-tiled bark on the foreground log, outline strokes and a stray pink patch among the trees.
  - j8: A shows nothing; b shows brick-pattern stretched bark on the near log, a pale smeared patch in the mid forest, a tiled-looking flat far plain and lumpy blob-shaped clouds.
- Pair 7, artifacts:
  - j1: The render shows thousands of floating dark leaf cards, a faceted spiky log, a translucent strip and floating trunk bases, while the photograph shows none.
  - j2: a has leaf cards scattered like floating confetti, a translucent ribbon across the ground, blocky log texture, and no animal on the trail; b shows none.
  - j3: A shows thousands of floating leaf cards, a spiky log with stretched texture and odd trench geometry; b is clean.
  - j4: The render shows black-fringed leaf cards scattered like confetti, a faceted log with stretched texture, stippled dither and unlit trunks; the photograph shows none.
  - j5: A has black-edged leaf sprites, a low-poly log with stretched textures, unshaded pale trunks and a seam line across the path, while b is clean.
  - j6: The render shows floating leaf cards, a translucent strip across the trail, logs hanging in the air and stretched textures on a faceted log; none seen in the photograph.
  - j7: a shows confetti-like black leaf cards, a smeared low-poly spiked log and hard-edged strips in the ground, while b shows none.
  - j8: A shows hard dark-edged leaf cards hovering over the ground, a faceted foreground log with stretched texture and a translucent strip with hard edges along the trail; b shows none.
- Pair 8, artifacts:
  - j1: The render shows a smeared floor texture, a log passing a trunk without contact, floating grass sprigs and brick-patterned bark, while the photograph shows none.
  - j2: a has stippled foliage edges, orange branch stubs, smeared ground texture and stray grass blades, and no grazing animal is visible; b shows none.
  - j3: A shows a log hovering over the ground, tiling bark, blobby foliage and floating grass sprites; b is clean.
  - j4: The render shows clumpy foliage silhouettes, an unshaded orange trunk, smeared near-field ground texture and floating grass blades; the photograph shows none.
  - j5: A has dithered, stippled foliage clumps, repetitive bark tiling and floating grass sprouts, while b is clean.
  - j6: The render shows tiled stretched bark, a log floating behind a trunk and blotchy foliage cards; none seen in the photograph.
  - j7: a shows blobby painted leaf sprites and repetitive bark with no obvious tiling, while b shows none.
  - j8: A shows blobby dithered foliage, blocky bark texture and stray floating grass blades; b shows none.
- Pair 9, artifacts:
  - j1: The photograph shows none, while the render shows black ink outlines around trunks, faceted spiky log ends and hard-edged leaf clumps.
  - j2: a shows none; b has dark ink-like outlines around every trunk and branch, a blue sliver leaking at the right edge, and spiky star-shaped log ends.
  - j3: A is clean; b shows hard dark outline strokes around trunks, blocky foliage edges, tiling bark on logs and a spiky stub branch.
  - j4: The photograph shows none; the render shows dark outlines around trunk silhouettes, a blue edge on a far trunk and spiky log stubs.
  - j5: A is clean, while b has dark outlines around trunks, low-poly log spikes, flat foliage cards and a blue sliver at the right edge.
  - j6: None seen in the photograph; the render draws dark outlines on trunks that stay sharp through the fog, shows a stray blue line at right and blocky leaf clusters.
  - j7: a is clean, while b shows black outline strokes on trunks and hills, blocky leaf clumps, tiled log bark and spiky log ends.
  - j8: A shows nothing; b shows dark outline halos around trunks and branches, faceted spiky logs, blocky foliage and a stray blue line near the right edge.
- Pair 10, artifacts:
  - j1: The render shows blocky low-resolution shadow-map texels, stretched wall textures, a waterfall that is a translucent quad and floating grass blades, while the photograph shows none.
  - j2: a has pixel-blocky shadow maps across both cliffs, stretched wall textures, a waterfall that is a translucent cyan quad, and crack lines; b shows none.
  - j3: A shows heavily pixelated shadow maps, stretched wall textures, a flat translucent quad for water and floating grass blades; b is clean apart from mild phone sharpening.
  - j4: The render shows heavily pixelated shadow maps, streaked wall textures, a flat translucent waterfall slab and floating grass; the photograph shows none.
  - j5: A has blocky shadow-map aliasing, stretched wall textures, a flat translucent waterfall quad and floating grass, while b is clean apart from mild phone sharpening.
  - j6: The render has severely pixelated shadow maps, stretched wall textures and a translucent waterfall sheet; none seen in the photograph.
  - j7: a shows heavily pixelated shadow-map dither, stretched texture on the walls, a flat translucent quad for the waterfall and floating grass blades, while b is clean.
  - j8: A shows severely aliased, blocky shadow-map pixels on both walls, stretched wall textures and a waterfall rendered as a flat translucent slab; b shows none.
- Pair 11, artifacts:
  - j1: The photograph shows none, while the render shows heavily stretched, smeared bark texture with banded faceting.
  - j2: a shows none; b is a magnified, blurry, tiling bark texture of vertical bars on a faceted cylinder.
  - j3: A is clean; b shows a heavily magnified, blurred, streaked texture, a faceted silhouette and visible seams.
  - j4: The photograph shows none; the render shows a blurred, magnified, stretched bark texture and a faceted silhouette.
  - j5: A is clean, while b shows a magnified, blurry bark texture, visible polygon facets and seams, and stretched UVs.
  - j6: None seen in the photograph; the render shows blurred, stretched bark texture, visible banding and faceting.
  - j7: a is clean, while b shows heavily magnified, smeared and stretched bark texture and visible horizontal polygon seams.
  - j8: A shows nothing; b shows magnified, blurred, stretched bark texture with smeared vertical slabs and a faceted silhouette.
- Pair 12, artifacts:
  - j1: The render shows brick-patterned bark, dithered, blotchy leaf edges, dark outlines on branches and spiky stubs at the bottom, while the photograph shows none.
  - j2: a has brick-patterned tiled bark, a dark outline along the trunk, dithered pixel-cluster foliage edges and stiff snag branches; b shows none.
  - j3: A shows a repeating brick-like bark texture, dark outline strokes on branches and splotchy foliage cards; b is clean.
  - j4: The render shows a blocky tiled bark texture, dark outlines on the trunk silhouette, ragged pixelated foliage edges and spiky branch stubs; the photograph shows none.
  - j5: A has brick-like bark tiling, dithered foliage edges, black outlines and crude stick branches at the bottom, while b is clean.
  - j6: The render shows brick-like tiled bark, dark outlines and blotchy leaf clusters; none seen in the photograph.
  - j7: a shows a repeating brick-pattern bark texture, hard-edged blotchy leaf sprites and outline strokes, while b is clean.
  - j8: A shows brick-like stretched bark, dark outlines on branches, dithered blotchy foliage and stray stick shapes at the bottom; b shows none.

The references (each credited in docs/citations.md):

- `vale-e01-floor`: https://commons.wikimedia.org/wiki/File:Beech_and_ferns_in_Gullmarsskogen.jpg
- `vale-e02-canopy`: https://commons.wikimedia.org/wiki/File:Blacklick_Woods_-_Sunlight_through_American_beech_1.jpg
- `vale-e03-trunk`: https://commons.wikimedia.org/wiki/File:Senden,_Venner_Moor_--_2013_--_2345.jpg
- `vale-e04-water`: https://commons.wikimedia.org/wiki/File:A_waterfall_on_the_stream_at_Oerddwr_-_geograph.org.uk_-_7747358.jpg
- `vale-e05-rock`: https://commons.wikimedia.org/wiki/File:Cross-bedded_quartzose_sandstone_(Dundee_Sandstone,_Middle_Pennsylvanian;_cliffs_along_Walnut_Creek_Bottom_Road,_west_of_Dundee,_Ohio,_USA)_3_(23232718273).jpg
- `vale-e06-meadow`: https://commons.wikimedia.org/wiki/File:Wildflower_meadow,_Heartwood_Forest_(28861233370).jpg
- `vale-e07-mist`: https://commons.wikimedia.org/wiki/File:D%C3%BClmen,_Umland,_Sonnenaufgang_--_2012_--_8069.jpg
- `vale-e08-vista`: https://commons.wikimedia.org/wiki/File:Sun_Set_over_Phewa_Lake.jpg
- `vale-e09-deer`: https://commons.wikimedia.org/wiki/File:Hind_(228949475).jpeg
- `vale-e10-walking`: https://commons.wikimedia.org/wiki/File:Samiec_sarny.jpg
- `vale-e11-deadwood`: https://commons.wikimedia.org/wiki/File:Bracket_fungi_on_fallen_log_Border_Track_Lamington_National_Park_Queensland_IMGP2516.jpg
- `vale-e12-overcast`: https://commons.wikimedia.org/wiki/File:Kitschentalrangen-20200629-RM-155854.jpg

### 2026-10-31: the third panel (E8), the grown vale, toolchain 0.13.0

docs/plan-world-class-3.md §E8: the vale with Part E's grown world (every plant grown: G-grown 0 of
26,922 instances authored), its twelve evaluation stills at 1920×1080 (`build/stills-e8`, from
`wrela tool lookbook -- build/stills-e8 --package examples/vale --stills --size 1920x1080 0 … 11`
on Linux pop-os, Radeon "Lucienne": B5's and D11's were the M4's) against the same references
(fetched again here from their manifest's URLs, each checked against its SHA-256: 21 of 21),
B5's seed (5), eight agent judges in fresh contexts, each given the rubric, the pairs and the
references' credits and conditions and told to judge by viewing alone; all eight passed both
calibration pairs. The hashes asserted: shots `88c877da…`, manifest `b78df49e…`.

```sh
wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR -- build/e8-panel --a build/stills-e8 --set evaluation --seed 5 --refs-dir $WRELA_REFERENCE_DIR
wrela tool studio/compare --write ws=. -- build/e8-panel --tally
```

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 2.00, 1.00, 1.88, 1.12, 0.00 | 2.25, 3.00, 2.75, 2.00, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.00, 2.00, 0.88 | 2.00, 3.00, 2.12, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.00, 2.75, 2.88, 2.50, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.75, 1.00, 1.50, 0.12, 0.00 | 1.88, 2.88, 2.38, 1.88, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 2.88, 3.00, 3.00, 3.00, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.75, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 2.00, 1.00, 2.00, 1.75, 0.00 | 2.00, 3.00, 3.00, 2.12, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.75, 1.75, 1.00 | 2.75, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.75, 0.00 | 2.00, 2.88, 2.00, 2.62, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.38, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.38, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.12, 2.00, 1.00 | 3.00, 3.00, 2.50, 2.25, 3.00 |

Overall: 0.0 % prefer the still or rate the pair equal (fails: 50 %); per pair: a pair lost worse than 6 to 2; majority: 0 pairs won or equal (fails). **Fails.**

Ranked losses (the reference's mean less the still's, summed over the pairs):

- artifacts: 29.12
- colour: 16.50
- depth: 13.25
- value: 12.25
- light: 8.75

| Panel | Pairs won or equal | Share of 96 | artifacts | colour | value | depth | light |
|---|---|---|---|---|---|---|---|
| B5 (the baseline) | 0 of 12 | 0 % | 26.75 | 15.88 | 14.38 | 13.62 | 12.50 |
| D11 (physical) | 0 of 12 | 0 % | 26.62 | 16.25 | 14.12 | 12.50 | 9.50 |
| **E8 (the grown vale)** | **0 of 12** | **0 %** | **29.12** | **16.50** | **12.25** | **13.25** | **8.75** |

**H2** ("most of the visual-review gain comes from the grown floor, not from lighting"): no gain
to attribute. Every pair is still lost 0 / 0 / 8; the rows moved by at most 2.5 (artifacts up
2.5, value down 1.9, the rest within 0.8). The judges' sentences name what decides the pairs, and
the grown plants are only part of it: the bark close up (stretched, smeared, tiled "brick"
relief: pairs 3, 11, 12), the shadows' dithered, blocky dapple (2, 4, 10), stamped arrow-shaped
marks and squiggles on the rock face (4), the fallen trees' root plates (1, 3), untextured grey
pole trunks far off, and leaf cards scattered like confetti on the soil (2). Every judge's file is
in `build/e8-panel/judges/`.

### 2026-11-01: the fourth panel (F10), the grown creature, toolchain 0.16.0

docs/plan-world-class-3.md §F10: the vale with Part F's grazer (a roe fawn grown from its genome,
grazing in shot 9 and walking toward the eye in shot 10), its twelve evaluation stills at
1920×1080 through `Physical` (`build/stills-f10`, `wrela tool lookbook -- build/stills-f10
--package examples/vale --stills --size 1920x1080 0 … 11` on Linux pop-os, Radeon "Lucienne")
against the same references (each checked against its SHA-256), B5's seed (5), eight agent judges
in fresh contexts given E8's brief; all eight passed both calibration pairs. The hashes asserted:
shots `88c877da…`, manifest `b78df49e…` (the baseline's).

```sh
wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR -- build/f10-panel --a build/stills-f10 --set evaluation --seed 5 --refs-dir $WRELA_REFERENCE_DIR
wrela tool studio/compare --write ws=. -- build/f10-panel --tally
```

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 1.88, 1.38, 1.88, 1.00, 0.38 | 2.00, 3.00, 2.75, 2.00, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.12, 1.88, 2.00, 2.00, 1.00 | 2.00, 3.00, 2.12, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 1.50, 2.00, 2.00, 0.75 | 2.00, 3.00, 2.25, 2.50, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.00, 1.38, 1.00, 0.00, 0.00 | 2.00, 3.00, 2.75, 2.00, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 1.62, 1.00, 1.38, 1.62, 0.12 | 2.00, 3.00, 2.88, 2.25, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 2.00, 1.88, 1.12, 1.00, 1.00 | 2.75, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.00, 2.12, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.88, 0.00 | 2.50, 2.88, 2.12, 2.62, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 0.75, 1.00, 1.00, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.88, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 2.38, 2.75, 3.00 |

Overall: 0.0 % prefer the still or rate the pair equal (fails: 50 %); per pair: a pair lost worse
than 6 to 2; majority: 0 pairs won or equal (fails). **Fails.**

Ranked losses: artifacts 28.75, colour 16.75, depth 15.50, value 13.88, light 10.88.

| Panel | Pairs won or equal | Share of 96 | artifacts | colour | value | depth | light |
|---|---|---|---|---|---|---|---|
| B5 (the baseline) | 0 of 12 | 0 % | 26.75 | 15.88 | 14.38 | 13.62 | 12.50 |
| D11 (physical) | 0 of 12 | 0 % | 26.62 | 16.25 | 14.12 | 12.50 | 9.50 |
| E8 (the grown vale) | 0 of 12 | 0 % | 29.12 | 16.50 | 12.25 | 13.25 | 8.75 |
| **F10 (the grown creature)** | **0 of 12** | **0 %** | **28.75** | **16.75** | **13.88** | **15.50** | **10.88** |

**H6** ("did the creature shots move?"): no. Both creature pairs are still lost 0 / 0 / 8, and
their rows fell a little: "down the trail" (pair 7) 2.00, 1.00, 2.00, 1.75, 0.00 at E8 → 1.62,
1.00, 1.38, 1.62, 0.12; "grazing at the edge" (pair 8) 2.00, 2.00, 1.75, 1.75, 1.00 → 2.00, 1.88,
1.12, 1.00, 1.00. What decides them isn't the creature: the judges fault the leaf cards floating
like confetti over the trail and the water, the cylinder root plates and the smeared ground (pair
7), and the empty brown slope with no atmosphere (pair 8), where the fawn reads "tiny" (a roe fawn
half a metre tall at 6 m and 10 m; E8 drew a larger stand-in). Every judge's file is in
`build/f10-panel/judges/`.

### 2026-11-01: the development panel and Part G's ranking (G0), toolchain 0.18.0

docs/plan-world-class-3.md §G0: the vale at `main` after Part F (808e39ba), its eight development
shots (D1–D8) at 1920×1080 through `Physical` (`build/stills-g0`, `wrela tool lookbook --
build/stills-g0 --package examples/vale --stills --size 1920x1080 12 13 14 15 16 17 18 19`, Linux
pop-os, Radeon "Lucienne") against the development set's references (each checked against its
SHA-256), seed 5, eight agent judges in fresh contexts given E8's brief; all eight passed both
calibration pairs. The development panel ranks Part G's work and stops it; the sealed twelve are
tallied once per item for the record only.

```sh
wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR -- build/g0-panel --a build/stills-g0 --set development --seed 5 --refs-dir $WRELA_REFERENCE_DIR
wrela tool studio/compare --write ws=. -- build/g0-panel --tally
```

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.75, 3.00, 2.25, 3.00, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 1.50, 1.00, 2.00, 1.00, 0.00 | 2.00, 3.00, 3.00, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.88, 0.00 | 3.00, 2.38, 2.38, 2.88, 2.62 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.38, 3.00, 3.00, 2.00, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.50, 2.00, 1.00 | 2.25, 3.00, 2.00, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.88, 3.00, 2.88, 2.25, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 1.75, 2.00, 1.62, 1.00, 1.62 | 3.00, 3.00, 3.00, 2.00, 3.00 |

Overall: 0.0 % prefer the still or rate the pair equal (fails: 50 %); per pair: lost worse than
6 to 2; majority: 0 of 8 (fails: 4). **Fails.**

**(1) The five rows' losses** (the reference's mean less the still's, summed over the eight
pairs, from `tally.json`): **artifacts 16.00, colour 9.38, light 9.00, value 8.38, depth 8.25.**

**(2) Each pair's worst row** and what its judges said (every sentence is in
`build/g0-panel/tally.md` and `judges/`):

- D5 the shore at evening, artifacts: "scattered untextured cylinder logs lying at implausible
  angles, plain pale trunk columns and a smeared ground texture".
- D4 the stream, artifacts: "stair-stepped pixelated shadow maps", "faceted low-poly trunks",
  "logs that pass through each other"; "no stream is visible".
- D6 the vale from the west, light: "the land gets no rim or key from the low sun".
- D3 another old oak, artifacts: "an obviously tiling rectangular bark texture magnified to
  blockiness and a trunk whose flat lower edge hovers over the ground".
- D1 the forest floor, artifacts: "a branch passing straight through the trunk", "a repeated dash
  pattern" of bark, "a translucent glassy streak" over the ground.
- D8 the wood under cloud, artifacts: "identical cylinder logs lying at implausible criss-cross
  angles, pinched radial log ends, tiled bark".
- D7 the creature in the wood, artifacts: "an untextured mannequin-like creature on stick legs",
  "a detached shadow blob", "a repeating leaf-litter tile".
- D2 the canopy against the sun, value: "a two-value silhouette against the sky with few middle
  tones on the forms".

**(3) The order** (`studio/compare`'s `menu`: an item is named when a pair of its classes has a
judge's sentence on its rows holding its words; its loss is its rows' over the pairs naming it;
the map's words and classes are §G0's, tested by `every_menu_row_names_a_development_shot` and
`synthetic_losses_select_their_items`):

1. **G7, colour and value** (4.75 lost, 2 pairs): D1's "its trunks and background are washed out"
   (value) and D3's "the range" (value: the photo's tone-mapping compresses it; the render's
   trunk "a repeated light-dark stripe with no form shading").
2. **G3, leaves against the light** (2.25, 1 pair): D2's "acid-green translucent leaves against
   near-black" against the render's "dull" blue-grey silhouettes (colour, light; five sentences).
3. **G5, virtual geometry from fields** (2.00, 1 pair): D6's "blobby" (artifacts: "blobby clouds"
   with "a grid of identical cylinder logs"; two sentences).
4. **G1, bounce light** (1.62, 2 pairs): D8's "falls off naturally under the canopy" against the
   render's light with "almost no occlusion or contact shadow" and D1's "the foliage shading is
   flat" (light; ten sentences).

**(4) Not named:** G2 (reflections), G4 (shadows at Epic) and G6 (the fall): not needed unless a
later panel names them. (D4's "pixelated shadow maps" is G4's sentence, but the map gives G4 the
classes 6 and 1, and D4 is class 4: by the rule it isn't named.)

What decides the panel is mostly off the menu: artifacts lead by 6.6, and its sentences name
the bark's tiling and stretch, the fallen logs' placement and interpenetration, faceted trunks,
the creature untextured, and the stream not drawn. Once the four named items are built, if the
panel still fails, those are the items the implementer names next (§G0's rule).

### 2026-11-01: G7, colour and value, toolchain 0.18.0

docs/plan-world-class-3.md §G7, the ranking's first item. Linux pop-os (Radeon "Lucienne"), not
the M4 Air: measures and judgements, no timing.

**What changed.** The physical look's curve (`styles::physical`, `light::tone_contrast`): the
exposed light's contrast about 0.1 (a little under middle grey, where most of a metered band
falls, so the median holds) raised to 1.2, and a shoulder that rolls the lights off toward white
from 3.0 (`light::soft_shoulder`: the ACES fit, which clips from 7.24, continued as an exponential
of the same value and slope). The meter's ceiling (`Metering::hi`) 4 → 8: the evening and dusk
views sat at the old ceiling (D5's and D6's medians rose 61 → 78 and 24 → 39 when it lifted;
a ceiling of 16 moved nothing further). Searched on the eight development shots alone (each
variant's stills, `build/g7-*`): the meter's ceiling alone; the contrast 1.2 about 0.18 (medians
fell, D6 7 % near black); about 0.1; with the meter's band from the lower quartile (everything
brighter, D6 unmoved); 1.12; then 1.2 about 0.1 with the ceiling at 8. Not fitted to any
reference's pixels.

**Wrela's luma quartiles** (sRGB, 0–255; p5 / q1 / median / q3 / p95) and **clipped share** (a
channel at 255, or every channel at 0; under 1 % on every shot, the gate), before → after:

| Shot | Before | After | Clipped before → after |
|---|---|---|---|
| D1 | 39 / 61 / 80 / 116 / 184 | 31 / 55 / 76 / 119 / 197 | 0.00 → 0.00 % |
| D2 | 63 / 89 / 114 / 154 / 204 | 58 / 88 / 119 / 165 / 218 | 0.03 → 0.04 % |
| D3 | 30 / 59 / 86 / 112 / 174 | 23 / 54 / 84 / 116 / 187 | 0.00 → 0.00 % |
| D4 | 25 / 42 / 74 / 124 / 197 | 18 / 35 / 71 / 130 / 210 | 0.00 → 0.00 % |
| D5 | 32 / 46 / 61 / 83 / 122 | 38 / 57 / 78 / 109 / 157 | 0.00 → 0.00 % |
| D6 | 3 / 15 / 24 / 43 / 174 | 4 / 24 / 39 / 71 / 217 | 0.39 → 0.49 % |
| D7 | 18 / 61 / 97 / 127 / 174 | 11 / 55 / 98 / 134 / 187 | 0.00 → 0.00 % |
| D8 | 39 / 70 / 88 / 114 / 161 | 32 / 65 / 87 / 118 / 173 | 0.00 → 0.00 % |

The twelve, measured only (never tuned): every one under 1 % clipped after (the most, shot 7,
dusk from the crest, 0.72 → 0.92 %, all of it black under the crest's foreground); their medians
within 4 of before but shot 2 (52 → 72) and 7 (25 → 35), their p95 up 7 to 42. Every quartile
of the twenty is in `build/stills-g7-before`, `build/stills-g7`, `build/stills-g0` and
`build/g7-final` (the measure: Rec. 709 weights on the encoded values).

**The sky's colour.** The air is Hillaire's (2020) precomputed atmosphere with Earth's Rayleigh
and ozone values (docs/citations.md); D2's clear sky away from the sun reads xy (0.253, 0.261),
CCT 17,000 K before and 20,800 K after (McCamy's cubic), inside the 3,758–34,573 K that
Hernández-Andrés et al. measured over 2,600 daylight spectra in Granada (JOSA A 18(6), 2001).

**Tests** (`engine`): `the_contrast_keeps_lumas_order_and_rolls_off_its_lights` (every doubling
from a thousandth to 8 brighter and under 1; `tone`'s at a contrast of 1 below the knee; the
shoulder continuous); `metering_keeps_the_scenes_order` (eight scenes two stops apart and a dusk
past the old ceiling keep their order at their metered exposures).

**G-look on all twelve** (blind, two judges of six shots each, before against after, 960×540,
sides from a seed, each calibrated against an eight-pass blur, which both rejected): after
preferred 2 (shot 2, the old oak: "brighter ridges give a slightly fuller value range"; shot 7:
"lifts the shadows and haze so the forest forms and the recession into the distance actually
read"), before preferred 2 (shot 1: "b's visible sun disk anchors the backlight"; shot 6: "A's
orange cast overwhelms the foreground"), equal 8 ("indistinguishable by eye").

**The development panel** (`build/g7-panel`, eight judges, all calibrated):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.25, 3.00, 2.50, 3.00, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 1.25, 1.00, 1.88, 1.00, 0.00 | 2.00, 3.00, 3.00, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.25, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.88, 0.00 | 3.00, 2.62, 2.62, 2.88, 2.38 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 1.88, 1.00 | 2.12, 3.00, 3.00, 2.00, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.38, 2.00, 1.38, 2.00, 1.00 | 2.12, 3.00, 2.25, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.88, 2.00, 1.00 | 2.75, 3.00, 3.00, 2.62, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 1.88, 2.00, 2.00, 1.00, 1.50 | 3.00, 3.00, 3.00, 2.12, 3.00 |

0 of 8 pairs. Losses **artifacts 15.88, colour 9.62, value 9.00, depth 8.88, light 7.75**
(G0's 16.00, 9.38, 8.38, 8.25, 9.00: their sum 51.13 against 51.01, flat; light down 1.25,
value and depth up 0.6 each). Re-ranked: **G4 (4.00, D5 and D1), G3 (2.12, D2), G1 (0.88, D8 and
D1)**; G7 and G5 no longer named. The next item is G4.

**The sealed panel, for the record** (`build/g7-sealed`, once for G7, not used to choose):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 2.00, 1.88, 2.00, 1.12, 0.12 | 2.00, 3.00, 2.50, 2.00, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 0.88 | 2.00, 3.00, 2.25, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 1.88, 2.00, 1.88, 0.75 | 2.00, 3.00, 2.25, 2.00, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.12, 1.12, 1.00, 0.00, 0.00 | 2.00, 3.00, 2.38, 1.88, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 2.62, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 1.88, 2.00, 1.88, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 1.62, 1.00, 1.38, 1.38, 0.00 | 2.12, 3.00, 2.75, 2.62, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.75, 1.25, 0.75 | 2.38, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.12, 2.12, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 0.88, 1.00, 1.00, 0.88, 0.00 | 2.38, 2.88, 2.12, 2.12, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 0.75, 1.00, 1.00, 0.00, 0.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.25, 2.00, 1.00 | 3.00, 3.00, 2.25, 2.62, 3.00 |

0 of 12. Losses artifacts 29.50, colour 15.88, depth 14.38, value 12.25, light 10.38 (F10's
28.75, 16.75, 15.50, 13.88, 10.88: their sum 82.39 against 85.76).

### 2026-11-01: G4, shadows at Epic, toolchain 0.18.0

docs/plan-world-class-3.md §G4, named by the development panel's re-ranking after G7 (D5's and
D1's "hard" shadows). Linux pop-os (Radeon "Lucienne"), **not the M4 Air**.

**What changed.** PCSS on at `Epic` (`tier::Tiered`); the moving map softened as the static maps
are (its blocker search and 12-tap filter over its own 79 m of depth, `Shadows::moving_depth`).
Not taken: the near cascade at 4096 (the vale's GPU peak is already 579.5 MiB against 512, §E1.3:
memory doesn't allow); the blocker search at half size (below).

**Cost** (`wrela profile examples/vale --serial --size 1920x1080 --warmup 600 --frames 300`, the
vale's first view, GPU medians, before → after): vegetation 11.669 → 12.110, terrain 4.837 →
5.008, water 4.459 → 4.652, fog 1.363 → 1.493 ms; the rest within 0.01. **+0.94 ms** on this
iGPU, whose vale frame runs 61 ms against the M4's 17.8 (§E1.3): about 0.28 ms there if it scales
with the frame. The plan's 0.8 ms is the M4's: **re-measure there**, and build the half-size
blocker search only if it's over.

**Tests** (`engine/tests/shadow-pcss`): the post's foot sharper than its tip
(`a_higher_blocker_casts_a_wider_penumbra`: 13 texels under a blocker 16 m up, 2 under one 1 m up);
`the_cascades_seam_shows_no_line` (along a soft edge from the near map alone out past the far
map's, a quarter texel apart: 0.07/255 between neighbours at most, against 1/255).

**G-look on shots 1 and 6** (blind, one judge, before against after, calibrated against an
eight-pass blur, which it rejected): shot 1, after ("a's shadows are smooth and grounded while
b's carry a dithered, hatch-like noise in the penumbrae"); shot 6, before ("b's over-softened
shadows flatten the scene": the evening sun is low, its blockers far, the penumbra wide).

**The development panel** (`build/g4-panel`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.88, 3.00, 2.50, 2.88, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 2.00, 1.12, 2.00, 1.00, 0.12 | 2.00, 3.00, 3.00, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.50, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.12, 0.88, 0.00 | 3.00, 2.50, 2.50, 2.88, 2.50 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.38, 3.00, 3.00, 2.00, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.25, 2.00, 1.38, 2.00, 1.00 | 2.12, 3.00, 2.38, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.62, 3.00, 2.88, 2.50, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.88, 1.00, 1.62 | 3.00, 3.00, 3.00, 2.12, 3.00 |

0 of 8. Losses **artifacts 15.75, colour 9.38, depth 8.50, value 8.38, light 7.75** (G7's 15.88,
9.62, 8.88, 9.00, 7.75: their sum 49.76 against 51.13). Re-ranked: G7 (2.88, D3), G3 (2.00, D2), G4
(2.00), G1 (1.25, D8 and D1); G7 and G4 are built. The next item is G3.

**The sealed panel, for the record** (`build/g4-sealed`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 1.25, 0.12 | 2.00, 3.00, 2.88, 2.00, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 1.88, 0.38 | 2.00, 3.00, 2.38, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.12, 1.88, 0.25 | 2.00, 3.00, 2.50, 2.12, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.25, 1.25, 1.00, 0.00, 0.00 | 2.00, 3.00, 2.00, 2.00, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.25, 2.00, 1.00, 2.00, 0.88 | 3.00, 3.00, 3.00, 2.75, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 1.75, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 2.00, 1.00, 1.50, 1.38, 0.00 | 2.12, 3.00, 3.00, 2.62, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.62, 1.12, 0.50 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.50, 0.00 | 2.38, 3.00, 2.25, 2.75, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 0.62, 1.00, 1.00, 0.00, 0.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.38, 2.00, 1.00 | 3.00, 3.00, 2.25, 3.00, 3.00 |

0 of 12. Losses artifacts 30.88, depth 16.25, colour 15.75, value 12.62, light 10.62 (their sum
86.12 against G7's 82.39: the panels' judges differ by a few points run to run).

### 2026-11-01: G3, leaves against the light, toolchain 0.18.0

docs/plan-world-class-3.md §G3 (D2: "acid-green translucent leaves against near-black"). Linux
pop-os (Radeon "Lucienne"), not the M4 Air.

**What changed.** A leaf's light through it, `sun × transmission × (1 − thickness) × vein`
(`impostor::leaf_through`): its thickness from how deep in its crown its card is (`THICK` 1.5 m,
**proposed**), the sun's transmission past the canopy its shadow lookup's (the shading passes
don't carry the probes' density volume the plan names; the leaf's own shadow, cast by every
crown's cards, is that transmission: a deviation), its veins the atlas's tissue (eight pinnate
pairs beside a narrower midrib, `plant::secondary_veins`), its lobe toward the sun 0.35 → 1.3
(**proposed**). The core's 0.4 floor is gone.

**Tests** (`engine`): `a_lone_leaf_glows_against_the_sun_and_the_crowns_core_doesnt` (a lone card
at least 0.3 of the sun, the core under 0.05); `a_leafs_veins_cover_a_tenth_of_it` (**14.3 %** of
the blades over eight seeds' sprigs; the first widths gave 27.6 %).

**G-look on shot 2** (blind, one judge, calibrated): **after** ("a keeps the canopy dark where it
faces away from the sun, so the backlit leaves glow against it, while b's uniformly bright,
saturated foliage flattens the forms"); light 2 → 3, colour 1 → 2, value 2 → 3. Shot 1 equal.

**The development panel** (`build/g3-panel`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.62, 3.00, 2.25, 3.00, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 2.00, 1.12, 2.00, 1.00, 0.00 | 2.00, 3.00, 3.00, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.50, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.25, 0.75, 0.00 | 2.88, 2.25, 2.50, 2.88, 2.62 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.50, 3.00, 3.00, 2.00, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.50, 2.00, 1.00 | 2.00, 3.00, 2.12, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.50, 3.00, 3.00, 2.75, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 1.00, 1.62 | 3.00, 3.00, 3.00, 2.25, 3.00 |

0 of 8. Losses artifacts 16.00, colour 9.12, depth 9.12, value 7.62, light 7.50 (their sum 49.36
against G4's 49.76); D2's own rows barely moved (still 2.00, 2.00, 2.00, 1.00, 1.62). Re-ranked:
G7 (5.75, 3 pairs), G3 (2.00), G4 (2.00), **G5 (2.00, D6)**, **G1 (1.50, D8 and D1)**; the named
items not built are G5 and G1, in that order.

**The sealed panel, for the record** (`build/g3-sealed`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 2.00, 1.62, 2.00, 1.25, 0.12 | 2.25, 3.00, 2.62, 2.12, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 0.88 | 2.00, 3.00, 2.12, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.12, 2.00, 0.75 | 2.12, 3.00, 2.25, 2.12, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.25, 1.25, 1.00, 0.00, 0.00 | 2.00, 3.00, 2.25, 2.00, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 2.62, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 1.75, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 2.00, 1.00, 1.75, 1.50, 0.00 | 2.25, 3.00, 2.88, 2.38, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 1.88, 2.00, 1.50, 1.25, 0.88 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.12, 2.12, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.12, 1.00, 1.12, 0.75, 0.00 | 2.12, 2.75, 2.00, 2.38, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 0.88, 1.00, 1.12, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.88, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 2.12, 2.88, 3.00 |

0 of 12. Losses artifacts 29.38, colour 15.75, depth 14.62, value 11.62, light 10.50 (their sum
81.87, the least of the four sealed panels so far).

### 2026-11-01: G5.0, the prototype rung (virtual geometry from fields): does not proceed

docs/plan-world-class-3.md §G5, named after G3 by D6's "blobby" (two sentences: "blobby clouds"
and "blobby foliage"). The rung asks one crag through the pruned extraction at four distances
against today's levels: the stills, the pixel difference, the cost, the bytes. **Deviation:** the
rung is worked from the realizer's own numbers, not rendered (no harness draws a crag's beds both
ways; the cluster path that could, `engine::virtual`, left the game at B11c). A crag's beds today
are extracted once at their cell (the near level's largest, 4 cm, `blockouts::far_band`'s; a
crag's beds have no far level), so on the floor's 1080p and 0.82 rad (`PER_METRE` 1242.4) a cell
covers:

| Eye at | Today's cell | The pruned extraction's | Its vertices, against today's |
|---|---|---|---|
| 10 m | 4.97 px | 0.5 px | 99× |
| 25 m | 1.99 px | 0.5 px | 16× |
| 50 m | 0.99 px | 0.5 px | 4× |
| 100 m | 0.50 px | 0.5 px | 1× |

So the pruned extraction would add detail only within about 50 m (a crag's cell is a pixel or
under past it), at up to a hundred times today's vertices at 10 m; the cluster spike (S1a,
`examples/clusters`) is its cost's record. **The rung decides G5.1 does not proceed:** the pair
that named it named the clouds and the foliage, outside G5's scope (the rocks, the crags and the
built things); no judge's sentence on any pair names a rock's or a crag's facets. If a later
panel names them, G5.1 is the item.

### 2026-11-01: G1, bounce light from a surface cache, toolchain 0.18.0

docs/plan-world-class-3.md §G1 (D8's "falls off naturally under the canopy" against the render's
"no bounce"; D1's "flat"). Linux pop-os (Radeon "Lucienne"), **not the M4 Air**.

**What changed.** The solids' distance volume holds each voxel's nearest solid's colour
(`distance::Distance::albedo`, Rgba8, the volume's 192×64×192: **9.4 MB**), cooked with its
distance; a trunk's colour is its kind's bark (`plant::bark_of`, carried on the trees' solid
occluders into the landscape's capsules). A probe's ray that meets a solid lights its colour
(`distance::albedo_at`) where every solid was one grey. **Deviation:** the second texture, each
voxel's direct sun (`lit`, 18.9 MB), isn't built: the trace already lights a hit through the
cascades as it bakes, so it would cache what's read anyway.

**Tests** (`engine/tests/surface-cache`): a red wall's sunlit side bounces red (hue 1.6° against
the wall's 1.1°: within 10°); its shaded side brings back no sun (0); the open ground's bounce is
what it was (the same ray with an empty cache: equal to the bit), and the wall's was grey before.

**Cost and memory:** the steady frame's labels as G4's within 0.05 ms (the cook runs at load and
as the eye moves, a slab a frame; its walk's labels are the M4's to measure); the volume's share
+9.4 MB.

**G-look on shots 1, 3 and 12** (blind, one judge, calibrated): equal, equal, equal ("the two
sides looked the same"). The stills move most on D5 (46 % of pixels by over 4/255) and D3 (16 %).

**The development panel** (`build/g1-panel`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.50, 3.00, 2.75, 3.00, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 2.00, 1.25, 2.00, 1.00, 0.38 | 2.00, 3.00, 3.00, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.50, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.38, 1.00, 0.00 | 3.00, 2.50, 2.75, 2.88, 2.50 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.38, 3.00, 3.00, 2.00, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.12, 2.00, 1.38, 2.00, 1.00 | 2.25, 3.00, 2.12, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.62, 3.00, 2.88, 2.62, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 1.00, 2.00 | 3.00, 3.00, 3.00, 2.38, 3.00 |

0 of 8. Losses **artifacts 15.12, colour 9.25, depth 8.88, value 8.25, light 7.62** (their sum
49.12, against G3's 49.36 and G0's 51.01). Named: G7, G3 and G1, all built.

**The sealed panel, for the record** (`build/g1-sealed`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 1.88, 1.38, 2.00, 1.12, 0.12 | 2.12, 3.00, 2.62, 2.00, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 0.88 | 2.00, 3.00, 2.12, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.12, 1.88, 2.00, 2.00, 0.50 | 2.12, 3.00, 2.50, 2.50, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.00, 1.12, 1.00, 0.00, 0.00 | 2.00, 3.00, 2.00, 1.88, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 2.75, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 1.75, 2.00, 2.00, 2.12, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 1.88, 1.00, 1.75, 1.50, 0.00 | 2.00, 3.00, 2.88, 2.38, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 1.88, 2.00, 1.38, 1.25, 0.50 | 2.75, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.00, 2.25, 2.00, 2.12, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.75, 0.00 | 2.50, 2.62, 2.00, 2.62, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 0.62, 1.00, 1.00, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.62, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.38, 2.00, 1.00 | 3.00, 3.00, 2.00, 2.75, 3.00 |

0 of 12. Losses artifacts 30.00, colour 16.00, depth 14.62, value 11.62, light 11.38 (sum 83.62).

### Part G closes: Blocked

Every item the development panel's ranking named is built, each with both panels (G7, G4, G3,
G1; G5 decided by its rung), and the development panel still fails G-reference's three rules:

| After | Pairs won or equal | Share of 64 | artifacts | colour | value | depth | light | Sum |
|---|---|---|---|---|---|---|---|---|
| G0 (Part F) | 0 of 8 | 0 % | 16.00 | 9.38 | 8.38 | 8.25 | 9.00 | 51.01 |
| G7 | 0 of 8 | 0 % | 15.88 | 9.62 | 9.00 | 8.88 | 7.75 | 51.13 |
| G4 | 0 of 8 | 0 % | 15.75 | 9.38 | 8.38 | 8.50 | 7.75 | 49.76 |
| G3 | 0 of 8 | 0 % | 16.00 | 9.12 | 7.62 | 9.12 | 7.50 | 49.36 |
| G1 | 0 of 8 | 0 % | 15.12 | 9.25 | 8.25 | 8.88 | 7.62 | 49.12 |

The sealed panel, once an item: F10 85.76; G7 82.39; G4 86.12; G3 81.87; G1 83.62 (0 of 12 each).

**Blocked**, the gate's number (pairs won or equal) at 0 of 8 through four built items, the losses
down 1.9 of 51 (4 %). What the judges fault is off the menu. After G1, the development judges'
artifact sentences name (counted over their eight files): **the bark's tiling and magnified
texels, 44 sentences in 7 of the 8 pairs** ("an obviously tiling rectangular bark texture
magnified to blockiness", "a repeated dash pattern"); **the fallen logs interpenetrating, floating
or lying at implausible angles, 24 in 5** ("logs passing through each other and the ground",
"identical cylinder logs lying at implausible criss-cross angles"); the leaves' cards, 18 in 5;
the logs untextured, 12 in 4; the ground smeared or barren, 11 in 4; faceted trunks, 10 in 2 (a near limb is 14-sided, `plant::SIDES`: where the facets come from
isn't traced). **The next items, in that order:**
bark that doesn't repeat or magnify (its tile's 2 m round and 40 m along at 256 × 4096 texels,
1 cm a texel, two to three tiles round an old oak: a detail layer under magnification and the
tiles offset round the limb), the logs laid on the ground by their own contact (no
interpenetration with trunks, logs or the slope), and the leaves' cards. Not built in this run.


### 2026-11-01: Part G again, after the council (round 1)

The council (codex: gpt-6.1-sol and gpt-6-astra, high effort, read-only) found the Blocked note
above premature and four gaps under it; each is answered below, and the part goes on. The note
above stands as what was recorded then.

1. **G1's colour wasn't reached in the game** (both models). A trunk is a solid occluder of the
   probes' density volume as well as the distance volume's, and a ray met its density (20, over
   2 m voxels) and stopped as foliage before it read the distance volume's surface. Fixed
   (3e57a8ac): the occluders cook the leaves' density and the solids' apart (red both, as before;
   green the leaves'; blue the solids'), and with the distance volume a ray reads the leaves'
   alone and meets a solid by the volume, in the leaves as in the open. The surface-cache test
   gives its wall a density occluder as the game's trunks have: on the old trace its bounce reads
   (0, 0, 0); now (1.76, 0.17, 0.12), hue 1.6° against the wall's 1.1°, by the open sky and by
   the probes alike.
2. **G-look's "before" preferences** (G7's shots 2 and 7, G4's shot 6) and its measures (2) and
   (3) unsupplied. Rounds 2 and 3 below; the measures in "G-look's measures".
3. **G5.0 not rendered.** It is now ("G5.0, rendered").
4. **The menu's next items not built.** G8 (the logs), G9 (the bark), below.

**G-look, rounds 2 and 3** (each item judged alone: the head against the head without it, the
shots as in the rounds before; a fresh blind judge each, calibrated on an eight-pass blur, which
each rejected; keys in `build/g-private`). Levers between rounds:

- G7: the contrast on luminance (each channel scaled as its luminance, so a colour's saturation
  is kept: a power on each channel turned dawn in the hollow's haze orange) and eased back above
  1.0 so 8 comes out as it was (the sky beside the sun went 249 → 253 of 255 and its disc was
  lost in it); then the meter's ceiling 8 → 6 → the engine's 4 (dusk from the crest washed out).
- G4: PCSS's filter radius half the penumbra (it took the sun's angular diameter as its radius:
  twice Fernando's penumbra), then its widest 24 → 6 texels (a low sun's far blockers spread the
  crown's dapples to a smear).

Shots are numbered from 1 here, as the plan numbers them (G7's round-1 record above names them by
the lookbook's index, from 0: its "shot 1" is the canopy against the sun, shot 2 here).

| Item | Round 2 (`build/glook-*r2*`) | Round 3 (`build/glook-*r3`) |
|---|---|---|
| G7 | shots 1–6 equal; 7 (dawn) **after**; 8 (dusk) **before** ("washes out the sky"); 10 **before** (a "stray bright speck": none is in the pixels, 0 over 220 against under 160 in the other); 9, 11, 12 equal | 8 **before** at a ceiling of 6 ("a slightly washed-out lilac grade"); 10 equal: the ceiling went back to 4 |
| G4 | shot 1 **after** ("b's shadows are noisy and cross-hatched"); 6 **before** ("over-softened shadows flatten the ground") | 6 **after** ("soft, plausible dapples beat stair-stepped edges") |
| G1 | shots 1 and 12 equal; 3 **before** ("a cold blue cast" on the trunk's shade: its blue less red rose 0.7 → 6.5 of 255 in the mid-tones) | 3 equal ("practically identical") |

**The final round** (the head with G8 and G9 in, 3a8cf0e9's ceiling back at 4, against the head
without each item; a fresh judge each, calibrated, which rejected the blur each time;
`build/glook-*f*`): **G7**, all twelve shots equal (two judges of six); **G4**, shot 1 after
("crisp, directional branch shadows and clean ground beat … noisy, dithered shadowing") and shot 6
after ("the blocky, stepped patches on the ground give away the shadow technique" without it);
**G1**, shots 1, 3 and 12 equal. No item's judge prefers "before" on a named shot.

### 2026-11-01: G5.0, rendered: the rung decides

docs/plan-world-class-3.md §G5.0, measured (`engine/tests/crag-rung`, 6fc3ecff; Linux pop-os,
Radeon "Lucienne", not the M4 Air). **The worked numbers in "G5.0, the prototype rung" above are
wrong:** a crag's beds aren't extracted at 4 cm but at 45 cm (`stone::realize_bed`, the floor's
crags; the 4 cm was another built thing's near level). The rung: one piece of a crag's bed (a
`stone::Bed`, 1.2 × 0.8 × 1.0 m, its joints, bedding and three chips) realized as
`stone::realize_bed` realizes it today (its tolerance 50 cm, its cell 45 cm, its colours a cell of
25 cm; the xhigh review's correction of the first run, which took 45 cm for all three) and at the
floor's pixel footprint for an eye 10, 25, 50 and 100 m off (1080p at 0.82 rad:
8 mm, 2 cm, 4 cm, 8 cm), each mesh's surface measured against the bed's own field
(`realize::measured_error`), and each distance's two stills (1920 × 1080, `engine::plain`'s
light) differenced:

| Cell | Vertices | Bytes | Its surface's error | That error in pixels at 10 / 25 / 50 / 100 m | Its realize, host to readback |
|---|---|---|---|---|---|
| 45 cm (today) | 24 | 1,680 | 287.2 mm | **35.90** / 14.36 / 7.18 / 3.59 | 13.6 ms (its pipelines' first) |
| 8 mm (10 m's pixel) | 102,230 | 7,359,720 | 0.60 mm | 0.07 at 10 m | 9.8 ms (50.1 under the GPU queue's load) |
| 2 cm (25 m's) | 16,339 | 1,176,384 | 6.52 mm | 0.33 at 25 m | 2.3 ms |
| 4 cm (50 m's) | 4,071 | 293,088 | 2.59 mm | 0.06 at 50 m | 1.8 ms |
| 8 cm (100 m's) | 1,025 | 73,776 | 12.17 mm | 0.15 at 100 m | 0.9 ms |

| Eye at | The piece's pixels | Today's still against the footprint's: over 8/255 | Mean difference |
|---|---|---|---|
| 10 m | 27,394 | 67.7 % | 33.4 / 255 |
| 25 m | 4,393 | 66.3 % | 32.2 / 255 |
| 50 m | 1,088 | 65.5 % | 31.1 / 255 |
| 100 m | 275 | 65.5 % | 29.9 / 255 |

Today's cell rounds a bed's edges by a cell and smooths its colour over it: its surface strays
36 pixels from its field at 10 m and still 3.6 at 100 m, and two thirds of its pixels differ from
the footprint's at every distance. The footprint's extraction holds within a third of a pixel
everywhere, at 4,300 times the vertices at 10 m (7.4 MB for this piece alone) and 43 times at
100 m. **The rung's verdict:** G5.1's extraction would change how the floor's crags read at
every distance, and its cost needs the per-frame block tree the plan describes (a piece's 10 m
extraction is 9.8 ms here: cached blocks, refined where the surface may be). **It isn't built in
Part G:** the vale draws no realized crag (its band's face is the terrain's), and the
development panel's pair that named G5 named its clouds and foliage ("blobby"), outside G5's
scope; no judge's sentence names a crag. It is the floor's item when the floor's panel names its
rock (Part H, D40).


### 2026-11-01: G8 and G9, the logs and the bark (named from the panel's sentences)

Every item the menu named was built and the development panel still failed, so (§G0) the next
items are named from its judges' sentences, counted over G1's eight files (each judge's notes on
a pair, by theme): **the logs** in all five pairs that show them (40 judges' notes; through each
other and the trunks, floating or criss-crossed: 26 in 5 pairs), and **the bark** tiled or
repeated (32 in 6 pairs) and magnified (23 in 4). Built together and judged by one panel round
(a deviation: §G0 runs both panels after each item; the two touch different things). Linux
pop-os (Radeon "Lucienne"), not the M4 Air.

**G8, the logs laid by their own contact** (`content/trees/kinds.wrela` `laid`, `clearance`):
a fallen tree is laid the way it fell unless its trunk meets a standing trunk or a log laid
before it; then turned the least of six 15° steps either way that clears them all, by 5 cm
past their radii (`CLEAR`); else the clearest way, broken short 15 % at a time until it's clear;
none if it isn't clear at a quarter of its length. The test
(`fallen_logs_lie_clear_of_trunks_and_of_each_other`: 36 oaks 5 m apart, every third fallen
toward the stand's middle or along a row, the hand's kinds and the grown): every log's axis
clear of every trunk and log, two thirds of them kept; without `laid` it fails (a log meets a
trunk by 0.33 m).

**G9, the bark** (`engine/plant.wrela`): an oak's fissures long and tapering to points
(`fissures`' `long` 1.4–2.6 rows and `taper`, the tip no narrower than 9 mm a side), 2.8 to 4.4
cm across at their middles, so the faces between are narrow interlaced ridges: over the tile the
faces cover 33 % (48 % before) and a furrow runs 0.83 m on average (0.32 m before:
`an_oaks_bark_is_long_furrows_between_narrow_ridges`); each plant's bark begins at its own place
on the tile (`bark_offset`, from its foot's quarter-metre cell: `no_two_trunks_show_the_same_bark`);
magnified, the pattern's edges steepened toward a pixel and a half (`sharpened`, Green 2007:
`magnified_bark_keeps_its_edges`, 12 pixels → 3).

**The development panel** (`build/g89-panel`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.88, 3.00, 2.38, 3.00, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 2.00, 1.62, 2.00, 1.25, 0.88 | 2.00, 3.00, 3.00, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 1.50, 1.12, 1.25, 0.38 | 3.00, 2.62, 2.50, 3.00, 2.38 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 1.88, 2.00, 2.00, 1.00 | 2.38, 3.00, 3.00, 2.00, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.38, 2.00, 1.50, 2.00, 1.00 | 2.12, 3.00, 2.50, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.00, 2.00, 1.00 | 2.75, 3.00, 2.62, 2.88, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.75, 1.00, 1.88 | 3.00, 3.00, 3.00, 2.50, 3.00 |

0 of 8. Losses **artifacts 14.25, depth 8.88, colour 8.62, value 8.62, light 7.62** (their sum
47.99, against G1's 49.12 and G0's 51.01). The judges' notes by theme against G1's: the bark
tiled or repeated 32 → 20 (D1 8 → 1), the logs through or floating 26 → 17, star-shaped log and
root ends 17 → 7; but "magnified, stair-stepped bark texels" (31, in D5's and D3's near
wood: the sharpening steepens a bilinear read whose texel grid then shows as stairs), D3's
trunk meeting the ground "in a hard line, no root flare", and the leaves as cut-out cards (16 in
7 pairs). The menu re-ranked: G7 (6.62), G3, G4, G5 (2.00 each), G1 (1.12), all built.

**The sealed panel, for the record** (`build/g89-sealed`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 2.00, 1.00, 2.00, 1.12, 0.25 | 2.00, 3.00, 3.00, 2.00, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.25, 2.00, 2.00, 2.00, 1.00 | 2.00, 3.00, 2.25, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 1.50, 2.00, 2.00, 0.38 | 2.00, 3.00, 2.38, 2.12, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.25, 1.00, 1.00, 0.00, 0.00 | 2.00, 3.00, 2.38, 1.88, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.12, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 2.88, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 1.88, 2.00, 1.75, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 2.00, 1.00, 1.25, 1.75, 0.00 | 2.00, 3.00, 3.00, 2.25, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 1.88, 1.75, 1.62, 1.00, 0.88 | 2.75, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.50, 0.88, 1.00, 1.00, 0.38 | 2.12, 2.62, 2.00, 2.25, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 1.00, 0.75, 1.00, 0.00, 0.12 | 3.00, 3.00, 3.00, 2.38, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.12, 2.00, 1.00 | 3.00, 3.00, 2.00, 2.50, 3.00 |

0 of 12. Losses artifacts 29.00, colour 17.75, depth 13.38, value 13.25, light 8.75 (sum 82.13,
against G1's 83.62).


### 2026-11-01: G10, the wood's form (named from the panel's sentences), and the levers on G8 and G9

From G8 and G9's panel (above): the logs' and roots' ends (7 notes), trunks meeting the ground
"in a hard line, no root flare" (10 in 4 pairs), faceted trunks (10 in 3) and the magnified
bark's stairs (31). Built (Linux pop-os, Radeon "Lucienne", not the M4 Air):

- **A log's broken end is its end grain** (`plant::limb_end`, `plant::end_grain`): pale wood
  ringed out from its heart, darker as it rots, its point facing out along the limb, where the
  bark's round pattern was drawn in to a star (`a_logs_end_is_ringed_wood_not_its_bark_drawn_in`).
- **The root plate is a disc** of torn ground behind the roots, 1.9 trunk radii across and 0.7
  thick (`kinds::log`'s `PLATE_WIDE`, `PLATE_THICK`, **proposed**): twelve blunt roots alone read
  as a star.
- **A limb 20 sides round** (`plant::SIDES`, 14 before: a near old trunk's facets were a hand
  wide).
- **A trunk's foot flared into five buttresses** (`plant::root_flare`): **taken out** after the
  panel (below).
- G9's lever: magnified, the bark tile read through a cubic B-spline before its edges are
  sharpened (`plant::smooth_read`, Sigg and Hadwiger 2005: the bilinear read's texel grid showed
  as stairs down the sharpened edges).
- G8's lever (G-look's final round on G8: a log turned across a crest or over another hung in
  the air): a way is taken only if half the log's length lies within half its radius of the
  ground (`supported`; `a_log_across_a_crest_is_laid_where_the_ground_holds_it`: straight over
  a ridge it doesn't; laid, it does).

**The development panel** (`build/g10-panel`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.50, 3.00, 2.25, 3.00, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 2.00, 1.12, 2.00, 1.12, 0.88 | 2.00, 3.00, 2.88, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 0.75, 1.00, 0.00, 0.00 | 3.00, 2.62, 2.75, 2.88, 2.75 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.25, 3.00, 3.00, 2.12, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.12, 2.00, 1.50, 2.00, 1.00 | 2.12, 3.00, 2.00, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.62, 3.00, 2.75, 2.62, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 1.75, 2.00, 2.00, 1.00, 1.50 | 3.00, 3.00, 3.00, 2.25, 3.00 |

0 of 8. Losses artifacts 15.38, colour 9.75, depth 9.75, value 8.12, light 7.62: **sum 50.62,
up from G8 and G9's 47.99.** D3 fell (its depth 1.25 → 0.00, colour 1.50 → 0.75): its camera
stands at its oak's foot, which the flare brought into the frame ("a wall of bark", "the camera
pressed against the trunk"); D8's judges read the buttresses' kink as "a faceted, polygonal
trunk base" (faceted: 10 → 19 notes; root flare and base: 10 → 19). The notes on log and root
ends fell 7 → 4, the bark tiled or repeated 20 → 14. **The flare is taken out** (e83a1a6e), and the
magnified bark sharpened twice at most, not four times (its near furrows read as "extruded
plastic", "a stencilled pattern"): 295ef776.

**The sealed panel, for the record** (`build/g10-sealed`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 1.50, 1.00, 2.00, 1.00, 0.00 | 2.25, 3.00, 2.75, 2.12, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.12, 3.00, 2.12, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 1.62, 2.00, 2.00, 1.00 | 2.00, 3.00, 2.12, 2.12, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.50, 1.12, 1.00, 0.12, 0.12 | 2.00, 3.00, 2.25, 2.00, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.75, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 1.88, 1.00, 1.75, 1.88, 0.00 | 2.12, 3.00, 2.88, 2.25, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 1.75, 2.00, 1.62, 1.25, 1.00 | 2.88, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.25, 1.00, 1.00, 0.50, 0.50 | 2.12, 2.62, 2.00, 2.38, 2.88 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 1.00, 0.75, 1.12, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.50, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.12, 2.00, 1.00 | 3.00, 3.00, 2.25, 2.50, 3.00 |

0 of 12. Losses artifacts 28.25, colour 17.12, depth 14.12, value 12.00, light 10.62 (sum 82.11,
against 82.13).


**The development panel again, the flare out** (`build/g10b-panel`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.75, 3.00, 2.25, 3.00, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 2.00, 1.38, 2.00, 1.00, 0.38 | 2.00, 3.00, 3.00, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.25, 1.00, 0.25 | 3.00, 2.62, 2.75, 3.00, 2.62 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.25, 3.00, 3.00, 2.00, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.50, 2.00, 1.62, 2.00, 1.00 | 2.12, 3.00, 2.25, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.62, 3.00, 2.62, 2.75, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 1.88, 1.62, 1.88, 1.00, 1.12 | 3.00, 3.00, 3.00, 2.25, 3.00 |

0 of 8. Losses artifacts 15.88, colour 9.62, depth 9.00, value 8.12, light 7.38: **sum 50.00**,
still over G8 and G9's 47.99. The judges' notes: log and root ends 7 → 5, but now
"mushroom-" and "bone-ended" logs and "capped" posts (the end grain lit pale on the break's
cone, and the root plate's disc); faceted 19 → 6; the trunks' feet 19 → 12. The sealed panel
(`build/g10b-sealed`): artifacts 29.00, colour 17.12, depth 14.38, value 12.50, light 9.88 (sum
82.88).

**G10's parts didn't move the panel** (the flare, the ends, the root plate, the twenty sides,
two rounds: 50.62, then 50.00 against 47.99): all four are taken out (e83a1a6e, 14bafc3a,
d5a8abe3, 22156280). The levers on G8 (its support) and G9 (its cubic read, its sharpening
halved) stay, judged with G8 and G9 in "Part G closes, again".


### 2026-11-01: G8 and G9, their G-look's last rounds

Each judged alone (the head against the head without it, on all twelve shots, two fresh judges
of six; calibrated on an eight-pass blur, which each rejected; numbered from 1 as the plan does).
Levers between rounds: G8, a way taken only if half the log lies on the ground (`supported`), and
the prefilter by the log's chord (council round 2); G9, the cubic read, the sharpening halved, a
log's bark in plates and a beech's bands unsharpened.

| Round | G8: after / before / equal | G9: after / before / equal |
|---|---|---|
| at G8 and G9's panel (`build/glook-g8f*`, `glook-g9f*`) | 4 / 4 / 4 | 4 / 3 / 5 |
| with G8's support, G9's cubic read (`glook-g8z*`, `glook-g9z*`) | 5 / 4 / 3 | 5 / 3 / 4 |
| the last head (`glook-g8l*`, `glook-g9l*`) | **7 / 2 / 3** | **6 / 1 / 5** |

**Blocked, G-look on G8:** "before" on shot 11 (the fallen beech: "more of the scene with less of
the blocky, plate-textured log bark", a log laid into the view) and shot 8 (dusk from the crest:
"a plate-textured log with a smaller log stuck into it"). Next levers: the logs laid in the
history, so a laid log's place is every reader's (the footing, the interest, the growth), and
its clearance tested against every fallen limb (the root plate's too), not the trunk alone.
**Blocked, G-look on G9:** "before" on shot 1 (the forest floor's near trunk: "stacked tiled
blocks" against "real furrowed bark"). Next lever: that kind's own pattern (a beech's smooth bark
under the oak's furrows' tile).

### 2026-11-01: Part G's cost, the vale's frame

`wrela profile examples/vale --serial --size 1920x1080 --warmup 600 --frames 300`, main (808e39ba)
and the part's last head, on Linux pop-os (Radeon "Lucienne", RADV), **not the M4 Air**: GPU
medians, ms.

| Label | main | head | change |
|---|---|---|---|
| vegetation | 11.686 | 12.639 | +0.953 |
| vegetation prepass | 5.707 | 5.528 | −0.179 |
| terrain | 4.837 | 3.769 | −1.068 |
| water | 4.460 | 3.488 | −0.972 |
| temporal | 3.615 | 3.613 | 0 |
| look | 2.692 | 2.831 | +0.139 |
| bloom | 2.139 | 2.165 | +0.026 |
| fog | 1.366 | 1.497 | +0.131 |
| the rest | 3.001 | 3.028 | +0.027 |
| **every frame's, summed** | **39.503** | **38.558** | **−0.945** |
| probes again (a re-bake, every other frame) | 3.33 | 4.46 | +1.12 |

The vegetation's rise is PCSS on the leaves and limbs and the bark's cubic read; terrain's and
water's fall is PCSS's widest filter, 24 → 6 texels (its taps stay near each other: G4's own cost
was +0.94 ms at 24). The re-bake's rise is the trace reading the distance volume each step in
the leaves too (G1's fix), an average of +0.56 ms over the frames: on the whole, about 0.4 ms
under main. A frame's call 60.38 → 60.58 ms (this iGPU's vale is far over budget: the M4's numbers
are §C6's). **The M4's to re-measure**, with the walk's cook and its memory (below).


### 2026-11-01: Part G's walk and memory, and G-look's measures

Linux pop-os (Radeon "Lucienne", RADV), Chrome headless through `std::tool::browser`, **not the M4
Air**. `wrela tool bench -- --scene vale-walk --no-docs --sustained 120`, main (808e39ba) and the
part's head:

| Build | GPU span p50 / p95 / p99 ms | Interval p50 / p99 ms | Playable ms | GPU peak MiB | Sustained 120 s, minute 1 / last |
|---|---|---|---|---|---|
| main | 55.34 / 184.54 / 211.94 | 57.38 / 216.36 | 10,575 | 579.8 | 55.54 / 55.54 |
| head | 57.12 / 186.60 / 213.36 | 58.95 / 216.49 | 11,601 | 606.4 | 57.01 / 57.01 |

On the walk the frame is 1.8 ms (3 %) dearer, where the first view's steady passes were 0.95 ms
cheaper ("Part G's cost"): the walk moves, so the probes re-bake and the cook run. The GPU peak
rises 26.6 MiB, 9.4 of it G1's colour volume (`distance` 14.3 → 23.7 MB in the frame's shares);
the rest is the second static cascade's map held at the peak's frame in this run and not in
main's (`shadows` 33.6 or 50.3 MB from frame to frame in both). Both builds are over G-memory's
512 MiB here (main's 579.8 too): the M4's numbers decide it (§C4: 508.2 there).

**G-look's measures, (2) and (3)** (`wrela tool lookbook -- … --fast --psnr --runs 3`, the twelve,
each the median of three; PSNR and SSIM against the build's own frozen-time reference; flicker
×10⁻⁶; `build/fast-main` in a worktree of 808e39ba, `build/fast-head`), main → head:

| Shot | PSNR dB | SSIM | flicker | pop | trail px |
|---|---|---|---|---|---|
| 1 the forest floor | 23.58 → 22.91 | 0.774 → 0.784 | 18.65 → 26.95 | 0.0182 → 0.0165 | 3 → 3 |
| 2 the canopy against the sun | 25.41 → 23.93 | 0.863 → 0.850 | 4.23 → 8.74 | 0.0385 → 0.0415 | 3 → 4 |
| 3 an old oak | 36.08 → 31.97 | 0.971 → 0.928 | 1.98 → 2.46 | 0.0559 → 0.0782 | 3 → 3 |
| 4 the fall | 23.59 → 24.86 | 0.723 → 0.849 | 10.61 → 13.87 | 0.0056 → 0.0061 | 5 → 5 |
| 5 the band's face | 25.59 → 27.46 | 0.802 → 0.851 | 0.84 → 0.88 | 0.0078 → 0.0092 | 3 → 2 |
| 6 the meadow at evening | 25.46 → 24.47 | 0.800 → 0.787 | 44.40 → 54.51 | 0.0056 → 0.0061 | 2 → 2 |
| 7 dawn in the hollow | 28.84 → 27.23 | 0.874 → 0.857 | 6.02 → 11.01 | 0.0038 → 0.0046 | 3 → 3 |
| 8 dusk from the crest | 36.21 → 36.00 | 0.932 → 0.927 | 9.24 → 9.15 | 0.0020 → 0.0024 | 2 → 2 |
| 9 grazing at the edge | 25.11 → 23.82 | 0.839 → 0.830 | 26.25 → 38.16 | 0.0046 → 0.0054 | 3 → 3 |
| 10 down the trail | 26.37 → 25.77 | 0.812 → 0.807 | 34.10 → 41.55 | 0.0059 → 0.0066 | 3 → 3 |
| 11 the fallen beech | 23.08 → 23.95 | 0.767 → 0.832 | 29.44 → 26.03 | 0.0200 → 0.0249 | 4 → 4 |
| 12 the wood under cloud | 28.10 → 27.03 | 0.798 → 0.782 | 82.93 → 111.42 | 0.0076 → 0.0089 | 3 → 3 |

Flicker rose on ten of twelve, PSNR fell on eight. Each item's share, by one run of six shots each
with the head less that item (`build/fast-a-*`; flicker ×10⁻⁶, the head's / without the item):

| Shot | the head | less G7 | less G4 | less G3 | less G1 | less G8 | less G9 |
|---|---|---|---|---|---|---|---|
| 1 the forest floor | 26.94 | 21.90 | 26.59 | 25.13 | 26.96 | 24.79 | 27.01 |
| 2 the canopy against the sun | 8.74 | 7.67 | 8.30 | **5.15** | 8.74 | 8.71 | 8.54 |
| 3 an old oak | 2.47 | 2.09 | 2.49 | 2.47 | **1.94** | 2.88 | 2.55 |
| 7 dawn in the hollow | 11.02 | 8.63 | 11.04 | 10.94 | 11.09 | **7.46** | 11.09 |
| 9 grazing at the edge | 38.19 | 29.92 | **31.39** | 37.63 | 38.34 | 40.97 | 38.33 |
| 12 the wood under cloud | 111.87 | **80.72** | 111.42 | 111.87 | 110.92 | 113.61 | 111.50 |

And PSNR: G7 took about a dB off every shot; G1 1.7 dB and G9 1.8 dB off the old oak's (shot 3,
with pop 0.0679 → 0.0782 by G9). The decisions, by §0.5 (a measure that rises needs the calibrated
judge's "after" on every named shot, and the rise named deliberate):

- **G7, out of the vale** (7137ab89): its contrast raised flicker 14–39 % on every shot measured
  (its mid-tones' slope squares the frame-to-frame noise) and its judges rated it equal on all
  twelve. The engine keeps the knob (`Look::contrast`, 1 by default).
- **G4, named deliberate:** PCSS's soft edges flicker more under the jitter on shot 9 (+22 %);
  its judges preferred it on both its shots.
- **G3, named deliberate on shot 2** (the canopy against the sun, +70 %: the leaves' light
  through them glows and shimmers, the point of G3, its judge "after"); **Blocked** on shot 1 (+7
  %, judged equal). Next lever: the light through a leaf in the temporal history's clamp.
- **G1, Blocked on shot 3** (the old oak: flicker +27 %, PSNR −1.7 dB, judged equal): the shade
  now reads the solids' coloured hits, whose normals come from the distance volume's 25 cm voxels.
  Next lever: a solid hit's normal from its capsule, not the volume's gradient.
- **G8 and G9**, already Blocked on their G-look; their measures with them (G8 shot 7's +48 %
  is its logs laid into that view; G9's on shot 3).

**The merged head's measures** (G7 out of the vale; `build/fast-final`, the twelve, three runs each),
main → merged:

| Shot | PSNR dB | flicker ×10⁻⁶ | pop | trail px |
|---|---|---|---|---|
| 1 the forest floor | 23.58 → 24.08 | 18.65 → 21.92 | 0.0182 → 0.0149 | 3 → 3 |
| 2 the canopy against the sun | 25.41 → 24.96 | 4.23 → 7.70 | 0.0385 → 0.0387 | 3 → 4 |
| 3 an old oak | 36.08 → 32.72 | 1.98 → 2.08 | 0.0559 → 0.0693 | 3 → 3 |
| 4 the fall | 23.59 → 26.13 | 10.61 → 10.87 | 0.0056 → 0.0053 | 5 → 5 |
| 5 the band's face | 25.59 → 28.79 | 0.84 → 0.72 | 0.0078 → 0.0081 | 3 → 2 |
| 6 the meadow at evening | 25.46 → 25.65 | 44.40 → 44.16 | 0.0056 → 0.0055 | 2 → 2 |
| 7 dawn in the hollow | 28.84 → 28.19 | 6.02 → 8.63 | 0.0038 → 0.0042 | 3 → 3 |
| 8 dusk from the crest | 36.21 → 35.82 | 9.24 → 9.54 | 0.0020 → 0.0025 | 2 → 2 |
| 9 grazing at the edge | 25.11 → 25.06 | 26.25 → 29.93 | 0.0046 → 0.0048 | 3 → 3 |
| 10 down the trail | 26.37 → 26.89 | 34.10 → 35.15 | 0.0059 → 0.0059 | 3 → 3 |
| 11 the fallen beech | 23.08 → 25.20 | 29.44 → 20.31 | 0.0200 → 0.0222 | 4 → 5 |
| 12 the wood under cloud | 28.10 → 28.22 | 82.93 → 87.29 | 0.0076 → 0.0076 | 3 → 3 |

PSNR up on seven shots, down on five (most on shot 3, G1's and G9's: Blocked); flicker up on
eight, by 2 to 82 %: shot 2's is G3's leaves (named deliberate), shot 9's G4's PCSS (named
deliberate), shot 7's G8's logs (Blocked with G8), shot 1's G3's and G8's (Blocked), shots 4, 8,
10 and 12 within 3 to 5 % (a run's own spread is up to 13 %, docs/lookdev.md).

### Part G closes, again: Blocked

**The development panel at the last head** (`build/glast-panel`; the twelve's stills
`build/v-head9`, the eight's `build/v-head10d`, at 54ff586e's code):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | D5-the-shore-at-evening | vale-d5-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.50, 3.00, 2.25, 3.00, 3.00 |
| 2 | D4-the-stream | vale-d4-water | 0 / 0 / 8 | -1 | 2.00, 1.12, 2.00, 1.00, 0.88 | 2.00, 3.00, 3.00, 3.00, 3.00 |
| 3 | D6-the-vale-from-the-west | vale-d6-vista | 0 / 0 / 8 | -1 | 1.12, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 2.00 |
| 4 | D3-another-old-oak | vale-d3-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.25, 1.25, 0.00 | 3.00, 2.62, 2.88, 2.88, 2.50 |
| 5 | D1-the-forest-floor | vale-d1-floor | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.12, 3.00, 3.00, 2.12, 3.00 |
| 6 | D8-the-wood-under-cloud | vale-d8-overcast | 0 / 0 / 8 | -1 | 1.12, 2.00, 1.38, 2.00, 1.00 | 2.25, 3.00, 2.25, 3.00, 3.00 |
| 7 | D7-the-creature-in-the-wood | vale-d7-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.62, 3.00, 3.00, 2.62, 3.00 |
| 8 | D2-the-canopy-against-the-sun | vale-d2-canopy | 0 / 0 / 8 | -1 | 1.75, 2.00, 1.88, 1.00, 1.50 | 3.00, 3.00, 3.00, 2.50, 3.00 |

0 of 8. Losses artifacts 15.12, colour 9.50, value 8.88, depth 8.88, light 7.50 (sum **49.88**).

**The sealed panel, for the record** (`build/glast-sealed`):

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 2.00, 1.12, 2.00, 1.12, 0.00 | 2.00, 3.00, 2.75, 2.00, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 2.00, 2.88, 2.00, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 1.50, 2.00, 2.00, 0.62 | 2.00, 3.00, 2.38, 2.00, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.50, 1.00, 1.00, 0.12, 0.00 | 2.00, 3.00, 2.50, 2.00, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 2.88, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.62, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 2.00, 1.00, 1.62, 1.50, 0.00 | 2.00, 3.00, 3.00, 2.25, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 2.00, 2.00, 1.75, 1.00, 0.88 | 2.88, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.50, 1.00, 1.00, 0.50, 0.12 | 2.25, 2.88, 2.00, 2.38, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 1.00, 0.62, 1.00, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.62, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.00, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 2.38, 3.00, 3.00 |

0 of 12. Losses artifacts 29.38, colour 17.50, depth 14.88, value 13.00, light 9.12 (sum 83.88).

The development panel through the part (0 of 8 every time; sums of the five rows' losses):

| After | Sum | artifacts | colour | value | depth | light |
|---|---|---|---|---|---|---|
| G0 (Part F's head) | 51.01 | 16.00 | 9.38 | 8.38 | 8.25 | 9.00 |
| G7 | 51.13 | 15.88 | 9.62 | 9.00 | 8.88 | 7.75 |
| G4 | 49.76 | 15.75 | 9.38 | 8.38 | 8.50 | 7.75 |
| G3 | 49.36 | 16.00 | 9.12 | 7.62 | 9.12 | 7.50 |
| G1 | 49.12 | 15.12 | 9.25 | 8.25 | 8.88 | 7.62 |
| G8 and G9 | **47.99** | 14.25 | 8.62 | 8.62 | 8.88 | 7.62 |
| G10 | 50.62 | 15.38 | 9.75 | 8.12 | 9.75 | 7.62 |
| G10, the flare out | 50.00 | 15.88 | 9.62 | 8.12 | 9.00 | 7.38 |
| G10 out, G8 and G9's levers | 48.75 | 15.12 | 9.38 | 8.50 | 8.50 | 7.25 |
| the last head (the council's and the review's fixes) | 49.88 | 15.12 | 9.50 | 8.88 | 8.88 | 7.50 |

The sealed panel, once a round: F10 85.76; G7 82.39; G4 86.12; G3 81.87; G1 83.62; G8 and G9 82.13;
G10 82.11 and 82.88; the last head 83.88 (0 of 12 each).

After these panels, G7's contrast left the vale for its measures (7137ab89: "G-look's measures",
above). Its judges had rated the vale with it and without it equal on all twelve shots, and the
development panel didn't move with it (51.01 → 51.13), so these tallies stand for the merged head
within the panels' own spread; they weren't run again.

**Blocked**, the gate's number (pairs won or equal) at 0 of 8 through eight items and every lever
on them. The losses fell to 47.99 with G8 and G9 and have held within 48.75 to 50.62 since: three
rounds of the same items differ by up to 1.9, the panels' judges' own spread run to run (G0 to G7,
no change of shot: 51.01 to 51.13). After G8 and G9, G10's four parts (the logs' end grain, the
root plate's disc, the trunks' root flare, limbs twenty-sided) didn't move it and were taken out:
the "several items stopped moving it" §G0 closes on. What the last panel's judges still fault
(their notes on the eight, by theme; `build/glast-panel/judges`): the logs (44 notes in 6 pairs; 19
in 5 through each other or floating), magnified or stretched textures (26 in 6: near bark and the
logs'), bark tiling (25 in 5), the stream (18 in 3: D4's shot shows none of it, and the shots are
fixed), the leaves as cards (14 in 5), a translucent plane (13 in 5: the stream's sheet), trunks
and logs faceted (10 in 2). The next levers, in
that order: the logs laid in the history (so every reader sees the laid log) and drawn fewer and
less alike; bark a field near the eye, not a magnified tile (the ridges' own `ridges_at` where a
pixel is under a texel); the leaves' cards with relief and thickness; a trunk's foot that meets
the ground without moving the shots' near trunks into the camera.

### 2026-11-01: the final panel (H2), Part H's head, toolchain 0.18.0

docs/plan-world-class-3.md §H2.2 (D38): the vale as the plan leaves it (Part G merged, G7 out of
the vale, the probes' solids fix; Part H changes nothing the vale draws), its twelve evaluation
stills at 1920×1080 (`build/stills-h2`, `wrela tool lookbook -- build/stills-h2 --package
examples/vale --stills --size 1920x1080 0 … 11`, Linux pop-os, Radeon "Lucienne") against the
same references, B5's seed (5) and mode (attributed), eight agent judges in fresh contexts given
B5's brief; all eight passed both calibration pairs (none rejected). The hashes equal B5's:
shots `88c877da27abda2b01a81d2777a35b66b533f7c5b82902306b64c2825ede6ca1`, manifest
`b78df49ef41682168868ef3443f88f0d7a423eaf4f8d0595d938924b551bdaf3` (`build/h2-sealed/key.json`;
every judge's file in `build/h2-sealed/judges/`).

```sh
wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR -- build/h2-sealed --a build/h2-twelve --set evaluation --seed 5 --refs-dir $WRELA_REFERENCE_DIR
wrela tool studio/compare --write ws=. -- build/h2-sealed --tally
```

| Pair | Shot | Reference | Still / equal / reference | Median | Still's means (light, colour, value, depth, artifacts) | Reference's means |
|---|---|---|---|---|---|---|
| 1 | the-fallen-beech | vale-e11-deadwood | 0 / 0 / 8 | -1 | 2.00, 1.50, 1.88, 1.12, 0.25 | 2.12, 3.00, 2.75, 2.12, 3.00 |
| 2 | the-meadow-at-evening | vale-e06-meadow | 0 / 0 / 8 | -1 | 2.25, 2.00, 2.00, 2.00, 1.00 | 2.00, 2.88, 2.12, 3.00, 3.00 |
| 3 | the-forest-floor | vale-e01-floor | 0 / 0 / 8 | -1 | 2.00, 1.50, 2.00, 2.00, 0.88 | 2.00, 2.88, 2.62, 2.25, 3.00 |
| 4 | the-bands-face | vale-e05-rock | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.00, 0.00 | 2.00, 3.00, 2.62, 2.00, 3.00 |
| 5 | the-wood-under-cloud | vale-e12-overcast | 0 / 0 / 8 | -1 | 1.00, 2.00, 1.00, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 6 | dusk-from-the-crest | vale-e08-vista | 0 / 0 / 8 | -1 | 1.88, 2.00, 1.88, 2.00, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 7 | down-the-trail | vale-e10-walking | 0 / 0 / 8 | -1 | 1.88, 1.00, 1.38, 1.75, 0.00 | 2.12, 3.00, 3.00, 2.38, 3.00 |
| 8 | grazing-at-the-edge | vale-e09-deer | 0 / 0 / 8 | -1 | 1.75, 1.88, 1.50, 1.12, 0.88 | 2.50, 3.00, 3.00, 3.00, 3.00 |
| 9 | dawn-in-the-hollow | vale-e07-mist | 0 / 0 / 8 | -1 | 2.00, 2.12, 2.00, 2.12, 1.00 | 3.00, 3.00, 3.00, 3.00, 3.00 |
| 10 | the-fall | vale-e04-water | 0 / 0 / 8 | -1 | 1.62, 1.00, 1.00, 0.62, 0.25 | 2.62, 2.88, 2.00, 2.62, 3.00 |
| 11 | an-old-oak | vale-e03-trunk | 0 / 0 / 8 | -1 | 1.00, 1.00, 1.00, 0.00, 0.00 | 3.00, 3.00, 3.00, 2.62, 3.00 |
| 12 | the-canopy-against-the-sun | vale-e02-canopy | 0 / 0 / 8 | -1 | 2.12, 2.00, 2.00, 2.00, 1.00 | 3.00, 3.00, 2.00, 2.75, 3.00 |

Overall: 0.0 % prefer the still or rate the pair equal (fails: 50 %); per pair: every pair lost 8
to 0; majority: 0 pairs won or equal (fails). **Fails.**

Ranked losses: artifacts 28.75, colour 16.62, depth 15.00, value 13.50, light 9.88 (their sum 83.75).

| Panel | Pairs won or equal | Share of 96 | artifacts | colour | value | depth | light | Sum |
|---|---|---|---|---|---|---|---|---|
| B5 (the baseline) | 0 of 12 | 0 % | 26.75 | 15.88 | 14.38 | 13.62 | 12.50 | 83.13 |
| D11 (physical) | 0 of 12 | 0 % | 26.62 | 16.25 | 14.12 | 12.50 | 9.50 | 78.99 |
| E8 (the grown vale) | 0 of 12 | 0 % | 29.12 | 16.50 | 12.25 | 13.25 | 8.75 | 79.87 |
| F10 (the grown creature) | 0 of 12 | 0 % | 28.75 | 16.75 | 13.88 | 15.50 | 10.88 | 85.76 |
| G, the last head | 0 of 12 | 0 % | 29.38 | 17.50 | 13.00 | 14.88 | 9.12 | 83.88 |
| **H2 (the plan's end)** | **0 of 12** | **0 %** | **28.75** | **16.62** | **13.50** | **15.00** | **9.88** | **83.75** |

**G-reference: misses**, on all three rules, as it did at B5. Across the plan the light row's
losses fell (12.50 → 9.88) and artifacts rose (26.75 → 28.75: the grown wood's forms are new
faults, the judges' sentences say which: the fallen beech's radial root plate, leaf-card litter,
floating logs, the fall's flat sheet, an old oak's ghosted bark relief, a kinked limb). What the
judges name first is still artifacts and colour on every pair. Every pair's worst row and its
eight sentences are in `build/h2-sealed/tally.md` (the tally's output, kept with the panel).

Each pair's worst row, and its judges' sentences on it:

- Pair 1, artifacts:
  - j1: The photo is clean; the render has an implausible radial bundle of capsule-shaped roots, painted stripe textures, black outlines, a hard-edged flat water polygon and ground decals.
  - j2: The photograph is clean; the render shows an implausible star-shaped cluster of capped cylinders, a hard cut-out water edge, a decal-like claw mark on the ground and repetitive banded bark.
  - j3: The photo is clean; the render shows an implausible star-shaped root bundle, dark crescent marks on the ground, and a hard-edged water plane.
  - j4: Photo is clean; the render shows implausible star-shaped capsule logs, a hard-edged water decal, black claw-like ground marks and stamped bark.
  - j5: The photo is clean, and the render shows a cartoon cylinder root ball, a hard-edged sheet of water and stamped dark claw marks on the ground.
  - j6: The photo is clean; the render shows an implausible radial cylinder cluster, a hard-edged water sheet, floating black stroke decals and outlined geometry.
  - j7: The photo shows none; the render has an implausible radial cluster of identical cylinders for a root plate, a hard-edged flat water sheet, and floating dark arc decals on the ground.
  - j8: The photo is clean; the render shows an implausible radial bundle of cylinders as a root ball, a hard-edged water slab, stray dark arc decals and drawn outlines.
- Pair 2, artifacts:
  - j1: The render has floating leaf-card litter scattered over bare dirt, cardboard-like logs, a hard-edged path and sprite-like foliage clumps; the photo has none.
  - j2: The render's ground is covered in flickery confetti-like leaf cards, with floating straight logs and blobby foliage clumps; the photograph is clean.
  - j3: The render shows leaf cards hovering over bare ground, floating logs, and a hard-edged pale plane in the foreground; the photo is clean.
  - j4: The render scatters chunky floating leaf shards, has floating logs and plastic trunks, and leaves a barren ground plane; the photo is clean.
  - j5: The render's floor is scattered with noisy leaf cards, logs resting implausibly and a hard-edged path at the bottom, while the photo is clean.
  - j6: The render shows confetti-like leaf cards sprinkled over bare ground, hovering logs and blotchy foliage clusters; the photo is clean.
  - j7: The photo is clean; the render has leaf decals scattered on bare dirt like confetti, floating logs, a pale flat path cutting the foreground, and a ring glitch on a trunk.
  - j8: The photo is clean; the render's scattered leaf cards read as speckle noise, logs sit on top of the ground and a translucent band crosses the bottom of the frame.
- Pair 3, artifacts:
  - j1: The render has untextured grey cylinder trunks, a floating pale stump, blocky stylised bark on the left trunk and logs interpenetrating; the photo is clean.
  - j2: The render shows blocky extruded bark on the foreground trunk, star-shaped log ends, featureless grey pole trunks and logs that barely sit on the ground; the photograph is clean.
  - j3: The render shows untextured grey cylinder trunks, interpenetrating logs, blocky bark and an odd pale branch cluster; the photo is clean.
  - j4: The render has brick-like extruded bark, intersecting logs, plain grey cylinder trunks and a floating white trunk base; the photo is clean.
  - j5: The render shows untextured cylinder trunks, interpenetrating logs, lollipop trees and a stamped root disc, while the photo has none.
  - j6: The render shows smooth untextured trunks, oversized bark relief, a floating log end, a stubby flower-like root end and lollipop trees; the photo is clean.
  - j7: The photo is clean; the render has a blocky stamped bark pattern on the near trunk, untextured grey poles, a trunk base hanging in the air and logs that float or interpenetrate.
  - j8: The photo is clean; the render has untextured grey poles, blocky bark on the near trunk, a cartoon root ball and a bare, smooth ground plane.
- Pair 4, artifacts:
  - j1: The photo is clean; the render has a repeated arrowhead decal pattern down the face, squiggle-line decals, smeared shadows and no sign of the banding the scene is named for.
  - j2: The render shows rows of repeated triangular arrow decals, tiled squiggle marks, stretched blurry texture and facet seams; the photograph is clean.
  - j3: The photo is clean; the render shows repeating triangular stamp marks, squiggle decal lines and a smeared texture.
  - j4: The photo is clean; the render shows repeated stamped arrow decals, tiled squiggle lines and a smeared, stretched texture.
  - j5: The render shows repeated triangular stamp decals, squiggle lines and a smeared texture with no bedding geometry, while the photo is clean.
  - j6: The render shows repeated arrow-shaped decal stamps, tiled squiggle lines, smeared texture and no real geometry; the photo is clean.
  - j7: The photo is clean; the render shows repeating triangular arrow stamps in diagonal rows and wavy line decals across a smeared surface.
  - j8: The photo is clean; the render shows repeated triangular stamp decals, wavy line decals and a featureless, implausibly smooth face.
- Pair 5, light:
  - j1: The photo has sun breaking through the canopy and catching trunk sides, with shadowed undergrowth exactly where expected; the render's cloudy light is flat and gives no direction or modelling.
  - j2: The photograph's sun slants through from the left, striping the trunks and the path with a natural falloff into shade; the render's overcast light gives no direction, and the ground has no falloff.
  - j3: The photo's shafts on the trunks show direction, falling off into a dark understory; the render's flat overcast light gives no direction and no falloff on the ground.
  - j4: The photo has shafts of dappled light on the trunks that fall off into the stand; the render's overcast light is flat, with no occlusion under the bush or on the ground.
  - j5: The photo's sun breaks through in shafts that light trunks and path with green bounce in the shade, while the render's overcast is flat with no contact darkening on the bare slope.
  - j6: The photo's dappled sun picks out the trunks and path, with believable falloff; the render's overcast light leaves its big ground plane uniformly lit and formless.
  - j7: The photo has sun shafts striking the trunks with falloff into shade, while the render's overcast light is flat, and its huge foreground ground plane gets no shading cue.
  - j8: The photo's dappled sun picks out the trunks with a readable direction and soft falloff; the render's overcast light leaves the large ground slab flat with no readable direction.
- Pair 6, artifacts:
  - j1: The photo is clean; the render has blocky tiled bark on the foreground log, blobby cloud shapes, a smeared low-detail far plain and dark outlines on trunks.
  - j2: The render's clouds are cut-out puffs, the distant plain is a smeared texture and the bark is tiled; the photograph is clean.
  - j3: The photo is clean; the render's distant plain is a blurry flat texture, its clouds look like cotton balls, the log bark is blocky and the trees are sparse cards.
  - j4: The photo is clean; the render's distant plain is smeared and featureless, its clouds are blobby puffs, its log bark is blocky, and its slope logs float.
  - j5: The render shows blocky log bark, blobby clouds and a flat far plane, while the photo is clean.
  - j6: The render's distant plain is a smeared flat texture, its clouds are blobby and its log has plank-like bark; the photo is clean.
  - j7: The photo is clean; the render has a smeared low-resolution far terrain, blocky tiled bark on the near log and blobby cloud shapes.
  - j8: The photo is clean; the render has blobby clouds, a streaked featureless plain, repetitive tree instances and blocky bark on the log.
- Pair 7, artifacts:
  - j1: The render has a field of floating leaf cards, an outlined stick-legged deer, the implausible capsule root ball and banded water; the photo is clean.
  - j2: The render shows thousands of flicker-prone leaf cards scattered like confetti, a star-shaped log cluster, a crude outlined deer and a flat grey water plane; the photograph is clean.
  - j3: The render shows floating leaf cards, a stick-legged deer, a translucent water slab and an implausible root star; the photo is clean.
  - j4: The render shows a noise-prone scatter of floating leaf shards, the radial capsule log cluster, a blended pale ground patch and a stick-legged deer; the photo is clean.
  - j5: The render shows a crude outlined deer, noisy scattered leaf cards, a hard-edged pale blob and a cartoon root ball, while the photo is clean.
  - j6: The render shows leaf-card confetti everywhere, a radial cylinder root hub, an outlined flat deer and cylinder trunks; the photo is clean.
  - j7: The photo is clean; the render has a stick-legged outlined deer, a cloud of floating leaf cards, an implausible starburst root and hard water edges.
  - j8: The photo is clean; the render is covered in floating leaf cards, has a stick-legged outlined deer and a cartoon root ball.
- Pair 8, artifacts:
  - j1: The render has an untextured deer, blocky bark, a trunk flaring into a cloth-like skirt, cutout foliage and a blurred ground; the photo is clean.
  - j2: The render's deer is low-detail, its foliage is blobby cut-out clusters, the log ends are striped and the ground texture is smeared; the photograph is clean.
  - j3: The render's deer is a smooth low-detail model, its ground texture is smeared and the trunk bases flare like skirts; the photo is clean.
  - j4: The render has a plastic low-poly deer, a trunk base that flares like a cloth skirt, a banded log and cutout foliage; the photo is clean.
  - j5: The render shows a smooth crude deer without a contact shadow, a flared skirt of geometry at a trunk base, cutout leaves and blank log ends, while the photo is clean.
  - j6: The render shows an untextured mannequin deer floating without contact, stretched bark texture, a skirt-like flare at the trunk base and an outlined log; the photo is clean.
  - j7: The photo is clean; the render has an untextured low-detail deer, bark texture stretched at the trunk bases, starburst log ends and blotchy foliage cards.
  - j8: The photo is clean; the render has a low-detail deer floating without contact, blotchy foliage cards and a bare ground plane.
- Pair 9, artifacts:
  - j1: The photo is clean; the render has black outlines on trunks, a rectangular cloth-like panel hanging off a trunk, logs laid in parallel and stub branches.
  - j2: The render shows hard black outlines, a flat rectangular card on a trunk, star-shaped log ends and plain cylinder trunks; the photograph is clean.
  - j3: The photo is clean; the render shows black ink outlines on trunks, a flat card trunk with a visible rectangular edge, and logs in unnaturally parallel rows.
  - j4: The photo is clean; the render has inconsistent ink outlines, a flat rectangular sheet behind the central trunk, stubby-branched logs and cutout foliage.
  - j5: The render shows black outlines on trunks, a flat rectangular bark flap, rows of identical repeated logs and cutout leaves, while the photo is clean.
  - j6: The render shows black outlines, a trunk with a rectangular sheet flap, flat card-like trunks and logs floating above the ground; the photo is clean.
  - j7: The photo is clean; the render has ink outlines on edges, a rectangular bark card floating off a trunk, stacked intersecting logs and a kinked overhead branch.
  - j8: The photo is clean; the render has drawn outlines, a visible rectangular card on a trunk, a floating branch across the top and plain cylinder logs.
- Pair 10, artifacts:
  - j1: The render has untextured plaster-like walls with polygon shards, floating grass blades, a log intersecting the ground and a flat strip for the waterfall; the photo is clean.
  - j2: The render's camera sits almost inside the terrain; the waterfall is a flat cyan strip, the walls are blank and grass blades float; the photograph is clean.
  - j3: The render shows untextured walls, a flat sliver of waterfall, floating grass blades and a smeared ground; the photo is clean apart from mild sharpening.
  - j4: The render's walls are untextured plaster, its waterfall is a thin teal ribbon, and grass blades float; the photo is clean.
  - j5: The render shows stretched smeared wall textures, a flat cyan sheet for the fall and floating grass cards, while the photo is clean.
  - j6: The render shows featureless smeared walls, a camera almost in the ground, floating grass blades, a flat-sliver waterfall and pixelated shadow edges; the photo is clean.
  - j7: The photo is clean; the render's waterfall is a thin flat blue card, the walls are untextured, and the camera nearly clips a ground mound with a few floating grass blades.
  - j8: The photo is clean; the render's walls are featureless, the waterfall is a thin flat card and jagged edges show at the top.
- Pair 11, artifacts:
  - j1: The photo is clean; the render has plastic-looking bark slats, translucent ghosted overlapping layers and horizontal seams.
  - j2: The render's bark is extruded plastic slabs with semi-transparent ghosted layers visible through the surface and seam lines; the photograph is clean.
  - j3: The photo is clean; the render shows ghosted double-image layers, horizontal seams and implausible plastic ridges.
  - j4: The photo is clean; the render's bark is extruded slabs with ghosted, translucent layering and horizontal seams.
  - j5: The render shows the camera clipping into the trunk, horizontal tiling seams and glossy extruded ridges, while the photo is clean.
  - j6: The render shows ghosted transparent layering, extruded plastic-looking bark geometry and camera clipping into the trunk; the photo is clean.
  - j7: The photo is clean; the render shows ghosted double layers of bark relief, stretched smeared texture and a camera clipping into the trunk.
  - j8: The photo is clean; the render shows ghosted, double-exposed layers and plastic-looking bevelled bark geometry.
- Pair 12, artifacts:
  - j1: The render has a branch with an implausible right-angle elbow, striped bark texture and blotchy dithered leaf cards; the photo is clean.
  - j2: The render shows stair-stepped extruded bark, dithered blobby foliage edges and a pipe-like bent branch; the photograph is clean.
  - j3: The render shows a right-angle kinked branch, plastic striped bark and blobby leaf cutouts; the photo is clean.
  - j4: The render has slab-extruded bark, a sharply kinked branch, cutout foliage blobs and slight leaf noise; the photo is clean.
  - j5: The render shows a branch kinked at a right angle, slab-stylised bark and mottled noise in the leaf clumps, while the photo is clean.
  - j6: The render shows plank-like extruded bark with outlines, a sharply kinked branch with a flat seam and blotchy foliage clusters; the photo is clean.
  - j7: The photo is clean; the render has stretched banded bark, a sharply kinked branch and blotchy foliage cards.
  - j8: The photo is clean; the render has blocky plastic bark, a sharply kinked branch and blotchy leaf clusters.

The menu (§G0), the items the pairs name, most lost first:

- G7: 9.25 lost, named by 5 pairs
- G2: 6.88 lost, named by 2 pairs
- G6: 4.62 lost, named by 1 pairs
- G5: 3.00 lost, named by 1 pairs
- G1: 2.00 lost, named by 2 pairs
- G4: 2.00 lost, named by 1 pairs
- G3: 1.88 lost, named by 1 pairs

The menu's ranking from this sealed tally (`studio/compare/menu.wrela`) names G2 (the water's
reflections) and G6 (the fall) second and third. Part G never built them: its selection read the
development panel (§G0), whose eight shots have no fall and no mere, so the faults these pairs
name first (pairs 10 and 6) were never on its menu. Plan four opens with them.

## Public Unreal benchmarks (context only)

None recorded. This subsection is optional (docs/plan-world-class-3.md §B3.3), and plan three
didn't search for one: its gates are local measurements on the M4 Air, and a public benchmark
from another machine can't stand in for one.

## Devices

Each device D20 names (docs/plan-world-class-2.md §0.5), measured by `wrela tool bench -- --lan`
(docs/toolchain.md, "Benching another device"): the Last Green's opening and walk and the
clearing's path at 1080p. A device's tables go here, and so do rows typed by hand from a page's
`show=1` summary (`--row`).

| Device | Rows | Why |
|---|---|---|
| MacBook Air M4 (the reference) | the sections above, from `wrela tool bench` here | |
| MacBook Air M1, 8 GB | none | not to hand: the work was done on one M4 Air, with no other machine on its network |
| Intel Iris Xe laptop, Windows 11 (Chrome and Firefox) | none | not to hand, as above |
| AMD Radeon 780M machine | none | not to hand, as above |
| iPad Pro, Safari 26 | none | not to hand; and Safari takes no flag for a secure origin, so it needs a released build over HTTPS |
| An Android phone | none | not to hand |

The procedure was checked on 2026-10-26 by standing a second, headless Chrome 155 on the M4 in
for a device: a fresh profile, started with `--unsafely-treat-insecure-origin-as-secure` for the
LAN origin the tool printed (`http://10.0.0.109:<port>`), opening each URL as the tool asked for
it (`wrela tool bench -- --lan --scene last-green-walk --frames 120 --no-docs`, the walk cut to
120 frames to check the procedure, not to measure it). Every run put its results; the report
named the machine `apple metal-3 Apple M4 (Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)
AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/155.0.0.0 Safari/537.36)`, and its row:

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Warm playable ms | GPU peak MiB |
|---|---|---|---|---|---|---|---|---|---|
| last-green-walk (120 frames) | 13.34 / 16.92 / 28.28 | 14.88 / 23.56 / 30.95 | 35 / 0 | 4.42 / 5.01 | 671.66 | 12260.68 | 5405.52 | 5115.76 | 878.4 |

Its GPU span is the local bench's (13.27 ms at B7); its cold start is slower (playable 12.3 s
against 8.2, its pipelines 5.4 s against 3.4) and its warm one a little faster (5.1 s against
6.0). The stand-in was a Chrome of its own (none of the toolchain's flags, a fresh profile, the
pages over the LAN address), so this checks the procedure; it isn't a device's row.

## Other engines on this machine

The Last Green's brief in three.js, Babylon.js and PlayCanvas (Part C, rywible/wrela-others: the
floor's own land, water and walk exported by `wrela tool export`, 20,000 trees of three kinds with
three levels and a billboard, 200,000 blades of grass, water, one sun with four 2048² cascades,
TAA, bloom and ACES, at 1080p, 60 warm-up frames and 1,200 measured), each engine at its defaults,
measured by that repository's harness in the Chrome the bench uses and imported with `wrela
tool bench -- --row <report.json>`. Read them with three things in mind: their content is far
simpler than the floor's (the trees are cylinders, cones and ellipsoids; no leaf cards, no bark,
no fields), so this is cost for a sparser picture; their frames are paced by Chrome's
`requestAnimationFrame` at 60 Hz, so an interval of 16.67 ms says a frame kept up, not what it
cost; and PlayCanvas's reflection is a mirror camera's image on the plane's own UVs, not
projected. The floor's own walk, which these scenes follow, on this machine: GPU span p50
18.14 ms, intervals 19.93 / 21.95 / 25.94, 5 of 1,200 frames over 33 ms (B16).

Plan three's B7 fixed what the council found in these scenes (rywible/wrela-others#1's review),
and what its own reviews found after (rywible/wrela-others#2), and these rows replace the
2026-10-26 ones they invalidated.

- **One GPU timer** under all three (`GpuTimer`): every pass of a frame gets timestamp writes,
  each pass belongs to the frame whose submission carries it (Babylon.js submits some a frame
  late), and the frame's span is its first start to its last end, as wrela's own. three.js's own
  timing had resolved whatever frames were pending into one sample (its old 36 ms); Babylon.js's
  and PlayCanvas's weren't read. Babylon.js's four shadow cascades write no timestamps on Metal
  (no fragment stage) and lie inside their frames' spans.
- **Babylon.js drew no trees in its water's targets or its shadow map**: a render target draws
  only the instances its list holds, and it held the trees' sources alone. Now every tree is in
  them, the first two levels casting as in the other scenes; its CPU time is what that costs
  (p50 34 ms: 20,000 instances evaluated for four cascades), and it runs at 30 fps.
- **The water was a 1 km² lake**: a plane over the whole square at the mere's level flooded the
  44 % of the land below it, and no tree stood there. It's now the mere (its reach, from the
  export's `water.json`: −0.482 m, where the scenes had derived −1.09), which the walk never sees:
  every engine's water draws nothing on the walk (PlayCanvas's mirror camera now renders only while
  the water's in view, as the others' reflections do).
- The land is sampled at the export's texel centres; PlayCanvas's grass is set on the land in its
  vertex shader; Babylon.js's TAA accumulates while the camera moves (its default drops it).
- Each run is rejected unless it holds the harness's checks: 1,201 frame boundaries, WebGPU, one
  GPU sample for every measured frame, every pass timed or inside its frame's span, no WebGPU
  error, and each scene's own (the trees upright and turned, in Babylon.js's targets and drawn
  there, PlayCanvas's 4096² atlas of four 2048² cascades and its water out of its own mirror, its
  grass never rewritten, three.js's normal map uploaded, the billboards' bases on the ground, the
  terrain receiving shadows and casting none). No engine compiled a pipeline in the measured
  frames. The first frame is the animation frame after that frame's GPU work is done.

Each row is the second of two sustained runs on a warm machine (2026-10-28, toolchain 0.10.0,
wrela-others `wc3-b7`), nothing else building. The first runs: three.js GPU p50 11.02 ms, 0 over
33; Babylon.js 7.31 ms, 1,187 over 33 (CPU p50 33.86); PlayCanvas 10.58 ms, 0 over 33.

| Engine | Scene | GPU span p50 / p95 / p99 ms | Intervals p50 / p95 / p99 ms | Over 33 ms | Cold first frame ms | Warm first frame ms | Loaded MiB | Measured |
|---|---|---|---|---|---|---|---|---|
| Babylon.js 9.32.0 | babylon-walk | 7.16 / 13.79 / 15.97 | 33.33 / 50.00 / 50.00 | 1194 | 1708 | 1733 | 23.7 | 2026-10-28, apple metal-3 0x0000 Apple M4 (Apple M4, Ryans-MacBook-Air.local): `npm run bench -- --engine babylon` (build/others/babylon-apple-metal-3-0x0000-Apple-M4-Apple-M4-Ryans-MacBook-Air-loc-2026-10-28.json) |
| PlayCanvas 2.23.3 | playcanvas-walk | 8.97 / 13.97 / 14.14 | 16.67 / 16.67 / 16.67 | 0 | 340 | 303 | 17.5 | 2026-10-28, apple metal-3 0x0000 Apple M4 (Apple M4, Ryans-MacBook-Air.local): `npm run bench -- --engine playcanvas` (build/others/playcanvas-apple-metal-3-0x0000-Apple-M4-Apple-M4-Ryans-MacBook-Air-loc-2026-10-28.json) |
| three.js 186 | three-walk | 9.83 / 16.50 / 17.09 | 16.67 / 16.67 / 16.67 | 0 | 532 | 415 | 17.2 | 2026-10-28, apple metal-3 0x0000 Apple M4 (Apple M4, Ryans-MacBook-Air.local): `npm run bench -- --engine three` (build/others/three-apple-metal-3-0x0000-Apple-M4-Apple-M4-Ryans-MacBook-Air-loc-2026-10-28.json) |

## Browsers

Checked 2026-10-25 on an Apple M4 MacBook Air (macOS 26), with toolchain release-0.6.1 (the
branch, before its release) and the workspace at B10 (`dea4ccfc`). Each program was opened from
`wrela run <package>` on 127.0.0.1.

- **Starts:** whether the program reached its first screen and drew it.
- **Features:** the adapter's WebGPU features as the page reads them
  (`navigator.gpu.requestAdapter()`); `std::gpu::features()` reports the ones it names (`f16`,
  `subgroups`, `bc`, `etc2`, `astc`, the 32-bit float filtering).
- **Frame interval:** the time between 600 consecutive `requestAnimationFrame` callbacks, read by a
  script in the page while the program ran. The plan named the games' "F7 debug readout" for
  this, but F7 takes stills and no game shows a 95th percentile; the capybara's F3 readout shows
  a mean.

### The Claude desktop app's browser pane (Chrome 152)

The page: `Claude/3.6556.2 Chrome/152.0.7977.167`, device pixel ratio 2, its canvas 2048 × 1536.
Adapter: Apple, `metal-3`.

| Program | Starts | Frame interval (median / p95 / max, ms) | Errors |
|---|---|---|---|
| `games/capybara` | Yes. A mouse press on `Start` starts the game; a held arrow key moves the capybara | 16.67 / 18.22 / 18.67, running right | None |
| `examples/clearing` | Yes | 16.67 / 18.01 / 33.33 | None |
| `games/last-green` | Yes | Not measured: the pane was hidden by then, and a hidden page's frames are throttled | None |

Features: `bgra8unorm-storage`, `clip-distances`, `core-features-and-limits`,
`depth-clip-control`, `depth32float-stencil8`, `dual-source-blending`, `float32-blendable`,
`float32-filterable`, `indirect-first-instance`, `primitive-index`, `rg11b10ufloat-renderable`,
`shader-f16`, `subgroup-size-control`, `subgroups`, `texture-component-swizzle`,
`texture-compression-astc`, `texture-compression-astc-sliced-3d`, `texture-compression-bc`,
`texture-compression-bc-sliced-3d`, `texture-compression-etc2`, `texture-formats-tier1`,
`texture-formats-tier2`, `timestamp-query`. So `features()` reports all of `f16`,
`subgroups`, `bc`, `etc2` and `astc`.

The fix in toolchain 0.5.1 (ticked programs start in the pane) holds on 0.6.x: no regression, so
no gaps entry.

The pane's key tool presses and releases a key within one frame, which the capybara reads as
no hold, so it doesn't move; a key held through the page (a `keydown`, then a `keyup` 1.5 s later)
moves it. That's the agent's tool, not the toolchain's.

### Safari 26.6.2

Not checked. The agent's screen access to Safari was declined, and Safari's own automation
(`safaridriver`, or JavaScript from Apple Events) is a setting only a person can turn on. To
check by hand: open each address `wrela run` prints, press `Start` on the capybara, and record
the same three columns.

### Firefox

Not checked: Firefox isn't installed on this machine, and installing it was left to its owner.
## 2026-10-26: B16, the final gates (world-class 2, toolchain 0.9.0)

`wrela tool bench -- --compare out/b1-baseline-062.json`, every scene in Chrome 155 and natively
on the M4 Air (`apple metal-3`), against B1's report (the first table in this file). G-pixels
(natively at 1280 × 720, frame 240, each side on 0.9.0's host): the capybara, the herd and the
climber lab **identical** to `build/base-0.7` (0 of 921,600 pixels); the clearing (78 % of
pixels, up to 145/255) and the Last Green (99.8 %, up to 168/255) are the deliberate changes,
their lookbook stills before and after in the PR. G-pipelines: the floor's 152 pipelines on
`main` (94 in tier 0, 58 in tier 1) are 175 (103, 72). G-size: the runtime 0.08 MB gzipped;
the floor's program and shaders 4.90 MB, its cold load 5.1 MiB as served; no `.br` or `.gz`
in the release or in a relaxed build.


| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| clearing-path | 12.65 / 14.12 / 14.38 | 13.39 / 36.80 / 41.42 | 114 / 46 | 0.45 / 0.84 | 13.51 / 17.12 | 451.12 | 9013.69 | 5170.52 | 1.3 | 1924.80 | 0.0 | 528.4 |
| last-green-shot | 16.62 / 19.94 / 25.13 | 17.95 / 21.41 / 33.73 | 432 / 5 | 4.58 / 5.38 | 17.44 / 32.22 | 678.85 | 15513.35 | 7523.10 | 5.1 | 7825.54 | 0.1 | 887.9 |
| last-green-walk | 18.14 / 20.43 / 21.52 | 19.93 / 21.95 / 25.94 | 1100 / 5 | 4.16 / 5.49 | 13.42 / 36.86 | 679.78 | 15980.18 | 7736.77 | 5.1 | 8011.91 | 0.1 | 861.8 |
| capybara-run | 1.12 / 1.78 / 2.13 | 16.59 / 19.12 / 20.33 | 277 / 0 | 0.25 / 0.82 | 0.59 / 1.34 | 174.25 | 0.00 | 539.42 | 0.9 | 0.00 | 0.0 | 81.9 |
| capybara-run-large | 2.00 / 3.03 / 3.55 | 16.68 / 17.73 / 18.20 | 291 / 0 | 0.21 / 0.68 | 0.55 / 2.66 | 186.93 | 0.00 | 567.84 | 0.9 | 0.00 | 0.0 | 183.0 |
| herd | 4.62 / 5.20 / 5.57 | 16.80 / 18.70 / 19.46 | 322 / 0 | 0.36 / 0.99 | 1.21 / 1.79 | 164.61 | 0.00 | 565.78 | 0.3 | 0.00 | 0.0 | 112.4 |
| leapling | 2.12 / 2.35 / 2.36 | 16.66 / 18.53 / 19.38 | 287 / 0 | 0.22 / 0.53 | 0.24 / 0.28 | 92.54 | 0.00 | 4.57 | 0.1 | 0.00 | 0.0 | 0.0 |

Hitches, clearing-path (frames over 33 ms, the five heaviest labels each):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 65 | 33.82 | 0.89 | bloom 12.89, vegetation 2.60, clouds 2.45, vegetation prepass 2.02, creature 2.00 | present 0.14, passes 0.06, cook 0.05, sim 0.01, finish 0.01 | 396 | 90 |
| 89 | 35.19 | 0.39 | bloom 12.96, vegetation 2.53, clouds 2.46, vegetation prepass 2.08, creature 1.96 | present 0.05, passes 0.03, cook 0.02, sim 0.01, choose 0.00 | 396 | 90 |
| 97 | 35.37 | 0.45 | bloom 12.87, vegetation 2.47, clouds 2.31, vegetation prepass 2.03, look 1.95 | present 0.06, passes 0.03, sim 0.01, cook 0.01, choose 0.01 | 396 | 90 |
| 121 | 34.13 | 0.58 | bloom 11.29, clouds 2.59, vegetation 2.55, vegetation prepass 2.05, creature 1.98 | present 0.06, passes 0.04, cook 0.02, sim 0.01, finish 0.01 | 396 | 90 |
| 281 | 35.04 | 0.41 | bloom 13.44, look 2.04, vegetation 2.03, vegetation prepass 1.95, creature 1.57 | present 0.05, passes 0.03, cook 0.02, sim 0.02, choose 0.01 | 396 | 102 |
| 285 | 37.39 | 0.44 | bloom 13.22, clouds 2.61, vegetation 2.04, vegetation prepass 1.93, look 1.81 | passes 0.07, present 0.03, cook 0.02, sim 0.02, choose 0.01 | 396 | 105 |
| 290 | 39.19 | 0.39 | bloom 12.99, clouds 2.38, vegetation 2.04, vegetation prepass 1.93, look 1.80 | present 0.04, passes 0.03, cook 0.02, sim 0.01, choose 0.00 | 396 | 99 |
| 295 | 36.36 | 0.52 | bloom 13.02, clouds 2.55, vegetation 2.00, vegetation prepass 1.95, look 1.78 | present 0.05, passes 0.03, cook 0.03, sim 0.03, choose 0.01 | 396 | 102 |
| 300 | 34.62 | 0.43 | bloom 13.44, clouds 2.65, look 2.09, vegetation 2.06, vegetation prepass 1.92 | passes 0.06, present 0.04, cook 0.03, sim 0.02, choose 0.01 | 396 | 102 |
| 305 | 39.87 | 0.44 | bloom 13.40, clouds 2.20, vegetation 2.10, vegetation prepass 1.92, look 1.88 | present 0.06, passes 0.03, sim 0.01, cook 0.01, choose 0.01 | 396 | 99 |
| 310 | 39.59 | 0.39 | bloom 13.51, clouds 2.31, look 2.04, vegetation 2.01, vegetation prepass 1.90 | present 0.05, passes 0.03, cook 0.02, sim 0.01, choose 0.00 | 396 | 105 |
| 325 | 40.44 | 0.39 | bloom 13.42, vegetation 2.02, vegetation prepass 1.94, look 1.75, terrain 1.53 | present 0.05, passes 0.03, sim 0.01, cook 0.01, choose 0.01 | 396 | 100 |
| 330 | 41.00 | 0.45 | bloom 13.56, look 2.39, clouds 2.24, vegetation 2.01, vegetation prepass 1.94 | present 0.05, passes 0.04, cook 0.03, sim 0.01, choose 0.00 | 396 | 103 |
| 342 | 36.24 | 0.44 | bloom 13.55, clouds 2.34, look 2.10, vegetation 2.05, vegetation prepass 1.99 | passes 0.05, present 0.05, cook 0.03, sim 0.01, choose 0.01 | 396 | 99 |
| 347 | 37.09 | 0.42 | bloom 13.61, clouds 2.63, vegetation 2.03, vegetation prepass 2.01, look 1.88 | present 0.05, passes 0.03, cook 0.02, sim 0.01, finish 0.01 | 396 | 102 |
| 353 | 38.79 | 0.43 | bloom 13.46, clouds 2.29, vegetation 2.03, vegetation prepass 1.96, look 1.86 | present 0.04, passes 0.03, sim 0.02, cook 0.01, choose 0.01 | 396 | 102 |
| 359 | 41.45 | 0.48 | bloom 13.45, look 2.47, clouds 2.11, vegetation 2.08, vegetation prepass 2.02 | present 0.06, passes 0.03, cook 0.02, sim 0.01, choose 0.01 | 396 | 106 |
| 364 | 41.42 | 0.47 | bloom 13.50, vegetation 2.07, vegetation prepass 2.01, look 1.76, terrain 1.58 | present 0.05, passes 0.04, cook 0.03, sim 0.02, choose 0.00 | 396 | 103 |
| 370 | 40.55 | 0.48 | bloom 13.67, look 2.59, clouds 2.28, vegetation 2.14, vegetation prepass 2.08 | present 0.06, passes 0.04, cook 0.02, sim 0.01, choose 0.01 | 396 | 106 |
| 377 | 41.53 | 0.62 | bloom 13.47, clouds 2.22, vegetation prepass 2.08, vegetation 2.08, look 1.95 | present 0.08, passes 0.04, cook 0.02, sim 0.01, choose 0.01 | 396 | 106 |
| 431 | 41.61 | 0.97 | bloom 12.97, look 2.39, vegetation prepass 2.27, clouds 2.19, vegetation 2.12 | present 0.10, passes 0.05, cook 0.03, sim 0.02, choose 0.02 | 396 | 106 |
| 454 | 41.84 | 0.45 | bloom 13.18, look 2.39, clouds 2.38, vegetation prepass 2.31, vegetation 2.13 | present 0.05, passes 0.04, cook 0.02, sim 0.01, choose 0.01 | 396 | 106 |
| 461 | 38.23 | 1.01 | bloom 13.18, clouds 2.61, vegetation prepass 2.35, vegetation 2.13, look 2.04 | present 0.10, passes 0.06, cook 0.02, sim 0.02, choose 0.01 | 396 | 105 |
| 470 | 41.36 | 1.16 | bloom 13.31, clouds 2.98, look 2.43, vegetation prepass 2.37, vegetation 2.09 | present 0.12, passes 0.06, cook 0.03, sim 0.02, choose 0.01 | 396 | 106 |
| 535 | 39.71 | 0.70 | bloom 11.83, vegetation 2.94, vegetation prepass 2.59, terrain 2.23, clouds 2.12 | present 0.08, passes 0.04, cook 0.02, sim 0.02, finish 0.01 | 396 | 96 |
| 541 | 38.35 | 0.48 | bloom 12.52, vegetation 2.94, vegetation prepass 2.53, clouds 2.39, creature 2.33 | present 0.06, passes 0.04, cook 0.02, sim 0.02, finish 0.00 | 396 | 93 |
| 546 | 38.72 | 0.95 | bloom 13.82, vegetation 2.96, vegetation prepass 2.56, clouds 2.54, terrain 2.26 | present 0.13, passes 0.06, cook 0.03, sim 0.02, choose 0.01 | 396 | 96 |
| 553 | 36.10 | 0.52 | bloom 13.87, vegetation prepass 2.39, clouds 2.35, vegetation 2.23, look 2.06 | present 0.05, passes 0.03, choose 0.02, cook 0.01, sim 0.01 | 396 | 105 |
| 558 | 35.79 | 0.50 | bloom 12.99, vegetation 2.98, vegetation prepass 2.54, terrain 2.25, look 1.70 | present 0.05, cook 0.04, passes 0.03, sim 0.02, choose 0.01 | 396 | 97 |
| 562 | 39.77 | 0.50 | bloom 13.59, clouds 2.68, vegetation prepass 2.47, vegetation 2.25, look 1.97 | present 0.06, cook 0.04, passes 0.04, sim 0.02, choose 0.01 | 396 | 102 |
| 568 | 37.18 | 0.40 | bloom 14.14, vegetation 3.01, vegetation prepass 2.59, clouds 2.45, creature 2.37 | passes 0.04, present 0.04, cook 0.02, sim 0.02, choose 0.02 | 396 | 93 |
| 574 | 36.62 | 0.46 | bloom 13.93, vegetation 2.90, vegetation prepass 2.59, clouds 2.46, creature 2.24 | present 0.06, passes 0.05, cook 0.03, sim 0.01, choose 0.01 | 396 | 94 |
| 578 | 38.03 | 0.44 | bloom 13.04, clouds 2.72, vegetation prepass 2.37, vegetation 2.21, look 1.70 | present 0.05, passes 0.03, sim 0.02, cook 0.01, choose 0.01 | 396 | 99 |
| 585 | 36.07 | 0.42 | bloom 12.91, vegetation prepass 2.39, vegetation 2.23, clouds 2.12, look 1.84 | present 0.05, passes 0.03, cook 0.02, sim 0.01, choose 0.01 | 396 | 105 |
| 590 | 35.11 | 0.45 | bloom 13.79, vegetation 2.93, vegetation prepass 2.59, clouds 2.40, creature 2.27 | passes 0.06, present 0.04, cook 0.02, sim 0.01, choose 0.01 | 396 | 93 |
| 595 | 39.57 | 0.46 | bloom 13.33, vegetation prepass 2.40, vegetation 2.25, look 2.18, clouds 2.13 | present 0.06, passes 0.03, cook 0.02, sim 0.01, choose 0.00 | 396 | 105 |
| 601 | 36.03 | 0.52 | bloom 12.73, vegetation prepass 2.40, vegetation 2.22, clouds 2.01, look 1.93 | passes 0.06, present 0.06, cook 0.02, sim 0.02, choose 0.02 | 396 | 102 |
| 606 | 39.20 | 0.45 | bloom 13.68, vegetation 2.95, clouds 2.92, vegetation prepass 2.60, terrain 2.19 | present 0.06, passes 0.03, cook 0.02, sim 0.01, choose 0.01 | 396 | 97 |
| 612 | 41.57 | 0.48 | bloom 12.41, vegetation prepass 2.41, clouds 2.37, vegetation 2.22, look 2.05 | present 0.05, passes 0.03, sim 0.01, cook 0.01, choose 0.01 | 396 | 99 |
| 620 | 40.81 | 0.50 | bloom 13.49, vegetation 2.90, clouds 2.65, vegetation prepass 2.58, terrain 2.17 | present 0.06, passes 0.04, cook 0.01, sim 0.01, choose 0.01 | 396 | 96 |
| 626 | 36.80 | 0.49 | bloom 13.01, clouds 2.57, vegetation prepass 2.41, vegetation 2.22, look 1.69 | passes 0.06, present 0.06, sim 0.02, cook 0.02, choose 0.01 | 396 | 99 |
| 632 | 38.31 | 0.51 | bloom 14.35, vegetation 2.91, vegetation prepass 2.62, creature 2.18, clouds 2.15 | present 0.06, passes 0.04, cook 0.04, sim 0.02, choose 0.01 | 396 | 93 |
| 637 | 42.32 | 0.49 | bloom 13.84, vegetation 2.87, vegetation prepass 2.62, clouds 2.29, creature 2.13 | present 0.07, passes 0.03, cook 0.02, sim 0.01, choose 0.01 | 396 | 93 |
| 645 | 39.58 | 0.47 | bloom 14.17, vegetation 3.00, vegetation prepass 2.65, creature 2.35, clouds 2.31 | present 0.05, passes 0.04, cook 0.02, sim 0.01, choose 0.01 | 396 | 93 |
| 650 | 38.07 | 0.50 | bloom 12.95, clouds 2.61, vegetation prepass 2.47, vegetation 2.33, look 2.14 | present 0.04, cook 0.04, passes 0.03, choose 0.02, sim 0.01 | 396 | 99 |
| 658 | 36.57 | 0.45 | bloom 13.24, vegetation 2.98, vegetation prepass 2.60, clouds 2.33, terrain 2.19 | passes 0.05, present 0.05, cook 0.02, choose 0.02, sim 0.01 | 396 | 96 |

0 of 46 unattributed (no label over 2 ms, CPU under 2 ms).

Memory at playable, clearing-path (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 377.3 |
| targets | 67.2 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 26.5 |
| distance | 9.0 |
| fog | 15.8 |
| terrain | 8.0 |
| reflections | 4.4 |
| lights | 0.4 |
| wood lists | 2.9 |
| impostors | 36.0 |
| wood | 4.8 |

Slowest compiles, clearing-path (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 2705.10 | 162.5 |
| cook_materials | 2501.18 | 226.9 |
| place_vertices | 2309.38 | 73.1 |
| cull_blocks | 1031.27 | 59.8 |
| far_quad+far_shade | 702.94 | 137.2 |
| bake | 651.71 | 115.4 |
| trace_again | 617.14 | 112.8 |
| trace | 612.69 | 95.7 |
| character+character_shade | 557.51 | 89.2 |
| far_quad+volume_shade | 525.09 | 92.8 |

Hitches, last-green-shot (frames over 33 ms, the five heaviest labels each):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 79 | 34.66 | 4.43 | shadows far 18.74, vegetation 6.30, creature 6.22, terrain 6.09, clouds 3.08 | sim 3.70, cook 0.13, finish 0.06, passes 0.05, choose 0.00 | 1265 | 114 |
| 80 | 33.73 | 5.20 | shadows far 17.75, vegetation 6.35, clouds 3.22, vegetation prepass 2.34, shadow moving 2.17 | sim 3.82, cook 0.24, finish 0.06, passes 0.04, choose 0.01 | 1265 | 349 |
| 81 | 34.30 | 4.85 | vegetation 6.26, creature 6.19, terrain 6.07, shadows far 5.74, clouds 2.50 | sim 3.80, cook 0.26, finish 0.07, passes 0.04, choose 0.01 | 1265 | 334 |
| 82 | 34.80 | 4.52 | vegetation 6.70, creature 6.63, terrain 6.52, vegetation prepass 2.41, clouds 2.33 | sim 3.84, cook 0.13, finish 0.05, passes 0.03, choose 0.01 | 1265 | 114 |
| 151 | 40.51 | 4.42 | bloom 23.51, creature 6.75, vegetation 6.70, terrain 6.57, look 3.27 | sim 3.57, cook 0.21, present 0.10, passes 0.07, choose 0.02 | 1207 | 117 |

0 of 5 unattributed (no label over 2 ms, CPU under 2 ms).

Memory at playable, last-green-shot (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 620.5 |
| targets | 90.8 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 30.3 |
| distance | 10.4 |
| fog | 15.8 |
| terrain | 10.8 |
| reflections | 7.5 |
| lights | 0.4 |
| wood lists | 41.5 |
| impostors | 78.0 |
| wood | 58.1 |
| built | 61.7 |
| realizer | 18.3 |
| rocks | 0.0 |
| banks | 0.0 |
| leaves' atlas | 0.0 |

Slowest compiles, last-green-shot (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 2687.20 | 103.1 |
| cook_materials | 2529.73 | 165.3 |
| place_vertices | 2204.37 | 92.2 |
| place_vertices | 1123.07 | 92.2 |
| cook_materials | 981.70 | 165.3 |
| cull_blocks | 952.51 | 114.6 |
| skin_vertices | 920.04 | 103.1 |
| cook_level | 900.51 | 166.0 |
| cull_blocks | 763.38 | 114.6 |
| far_quad+far_shade | 710.30 | 137.2 |

Hitches, last-green-walk (frames over 33 ms, the five heaviest labels each):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 78 | 34.32 | 6.40 | shadows far 19.00, vegetation 6.29, creature 6.21, terrain 6.08, clouds 3.67 | sim 5.10, cook 0.41, finish 0.09, passes 0.06, choose 0.00 | 1265 | 334 |
| 79 | 35.17 | 7.62 | shadows far 17.91, vegetation 6.50, creature 6.38, terrain 6.27, clouds 3.98 | sim 6.33, cook 0.21, finish 0.11, passes 0.06, choose 0.01 | 1265 | 114 |
| 80 | 35.74 | 6.82 | vegetation 6.32, shadows far 5.84, clouds 2.67, vegetation prepass 2.37, temporal 2.23 | sim 5.41, cook 0.34, finish 0.10, passes 0.06, choose 0.01 | 1265 | 349 |
| 81 | 34.11 | 5.60 | vegetation 6.85, creature 6.74, terrain 6.63, clouds 2.80, vegetation prepass 2.43 | sim 4.21, cook 0.29, finish 0.10, passes 0.07, choose 0.01 | 1265 | 334 |
| 359 | 41.24 | 4.04 | bloom 23.87, vegetation 5.84, look 3.35, vegetation prepass 2.39, temporal 2.22 | sim 3.33, cook 0.15, passes 0.07, present 0.07, choose 0.02 | 1207 | 134 |

0 of 5 unattributed (no label over 2 ms, CPU under 2 ms).

Memory at playable, last-green-walk (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 620.5 |
| targets | 90.8 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 30.3 |
| distance | 10.4 |
| fog | 15.8 |
| terrain | 10.8 |
| reflections | 7.5 |
| lights | 0.4 |
| wood lists | 41.5 |
| impostors | 78.0 |
| wood | 58.1 |
| built | 61.7 |
| realizer | 18.3 |
| rocks | 0.0 |
| banks | 0.0 |
| leaves' atlas | 0.0 |

Slowest compiles, last-green-walk (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 2687.72 | 103.1 |
| cook_materials | 2468.71 | 165.3 |
| place_vertices | 2299.32 | 92.2 |
| place_vertices | 1177.83 | 92.2 |
| cull_blocks | 933.48 | 114.6 |
| cook_materials | 920.55 | 165.3 |
| skin_vertices | 912.05 | 103.1 |
| cook_level | 890.93 | 166.0 |
| cull_blocks | 772.72 | 114.6 |
| far_quad+far_shade | 694.20 | 137.2 |

Hitches, capybara-run: none (no frame over 33 ms).

Slowest compiles, capybara-run (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| quad_vertex+sprite | 599.72 | 86.7 |
| full+backdrop | 419.56 | 83.6 |
| full+strip_layer | 358.59 | 67.9 |
| earth_vertex+earth | 162.45 | 27.5 |
| water_vertex+water | 135.61 | 14.5 |
| quad_corner+quad_color | 104.13 | 8.3 |
| full+finish | 86.75 | 2.6 |
| full+copy | 77.19 | 1.1 |
| grass_vertex+grass | 72.70 | 18.1 |
| full+bake_grain | 26.24 | 10.6 |

Hitches, capybara-run-large: none (no frame over 33 ms).

Slowest compiles, capybara-run-large (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| quad_vertex+sprite | 582.70 | 86.7 |
| full+backdrop | 412.42 | 83.6 |
| full+finish | 373.60 | 2.6 |
| full+strip_layer | 359.57 | 67.9 |
| earth_vertex+earth | 162.68 | 27.5 |
| water_vertex+water | 140.55 | 14.5 |
| quad_corner+quad_color | 109.71 | 8.3 |
| full+copy | 78.12 | 1.1 |
| grass_vertex+grass | 70.91 | 18.1 |
| full+bake_grain | 41.36 | 10.6 |

Hitches, herd: none (no frame over 33 ms).

Slowest compiles, herd (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| cull_blocks | 340.28 | 69.0 |
| place_vertices | 318.16 | 86.4 |
| skin_vertices | 158.48 | 37.2 |
| cook_colours | 154.69 | 44.7 |
| skin_coloured+grazer_shade | 119.61 | 7.2 |
| ground+ground_shade | 113.30 | 11.8 |
| pose_instances | 86.95 | 11.4 |
| skin_shadow+depth_only | 43.58 | 2.2 |
| emit_quads | 33.33 | 9.3 |
| finish_mesh | 15.11 | 3.1 |

Hitches, leapling: none (no frame over 33 ms).

Slowest compiles, leapling (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| full+shade | 137.83 | 45.4 |

## 2026-10-28, apple metal-3 Apple M4: B1.5's first run (superseded)

The vale's first bench, its walk stepping only once the vale was playable, so its measured frames
held a stationary load of about 40 frames and missed the walk's last 40, and runs sampled
different stretches of it (plan three's Part B council). Superseded by the run below (W at frame
240, the walk stepping from it, as the floor's walk is scripted), kept for the record.

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| vale-walk | 15.86 / 19.85 / 21.93 | 17.68 / 23.77 / 32.22 | 774 / 11 | 0.81 / 2.51 | 17.68 / 27.94 | 460.80 | 4340.27 | 959.14 | 3.1 | 4203.70 | 0.0 | 781.8 |

Hitches, vale-walk (frames over 33 ms, the five heaviest labels each):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 299 | 34.65 | 0.74 | bloom 13.93, vegetation 3.40, vegetation prepass 2.97, terrain 2.37, creature 2.21 | present 0.10, cook 0.09, passes 0.03, choose 0.00, finish 0.00 | 962 | 670 |
| 303 | 37.40 | 0.73 | bloom 32.61, vegetation late 4.73, look 4.54, water 3.65, temporal 3.62 | present 0.10, cook 0.09, passes 0.02, finish 0.01, choose 0.00 | 962 | 957 |
| 317 | 39.75 | 1.09 | bloom 19.48, vegetation 3.52, vegetation prepass 3.02, terrain 2.70, look 2.66 | cook 0.20, present 0.08, passes 0.02, choose 0.01, finish 0.00 | 962 | 1534 |
| 323 | 36.41 | 0.93 | bloom 23.40, vegetation 3.60, look 3.16, vegetation prepass 2.97, terrain 2.86 | cook 0.18, present 0.05, passes 0.02, choose 0.01, finish 0.01 | 962 | 1245 |
| 389 | 40.12 | 0.45 | bloom 20.24, vegetation 2.82, look 2.82, vegetation prepass 2.32, clouds 2.18 | cook 0.05, present 0.05, passes 0.02, finish 0.01, choose 0.00 | 959 | 382 |
| 404 | 46.48 | 0.79 | bloom 25.26, look 3.43, terrain 2.98, creature 2.84, vegetation 2.78 | cook 0.10, present 0.10, passes 0.02, choose 0.01, finish 0.00 | 958 | 957 |
| 441 | 40.47 | 0.48 | bloom 21.31, look 3.03, terrain 2.97, creature 2.83, vegetation 2.59 | present 0.06, cook 0.05, passes 0.03, choose 0.01, finish 0.00 | 967 | 381 |
| 690 | 44.86 | 0.51 | bloom 22.96, look 5.21, vegetation 3.23, terrain 2.82, temporal 2.79 | cook 0.07, present 0.06, passes 0.02, choose 0.01, finish 0.01 | 959 | 381 |
| 755 | 39.48 | 0.90 | bloom 19.90, vegetation 8.14, terrain 6.99, creature 6.84, vegetation prepass 6.52 | present 0.13, cook 0.11, passes 0.04, choose 0.01, finish 0.00 | 958 | 669 |
| 1070 | 43.25 | 1.06 | bloom 23.22, look 3.98, vegetation 3.79, vegetation prepass 3.31, terrain 2.39 | cook 0.20, present 0.08, passes 0.02, choose 0.01, finish 0.00 | 957 | 1534 |
| 1114 | 46.56 | 1.09 | bloom 23.46, vegetation 4.19, look 3.37, vegetation prepass 3.14, terrain 3.06 | cook 0.21, present 0.08, passes 0.02, finish 0.01, choose 0.00 | 957 | 1534 |

0 of 11 unattributed (no label over 2 ms, CPU under 2 ms).

Memory at playable, vale-walk (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 528.1 |
| targets | 90.8 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 34.0 |
| distance | 10.0 |
| fog | 15.8 |
| terrain | 10.8 |
| reflections | 7.5 |
| lights | 0.4 |
| wood lists | 18.3 |
| impostors | 78.0 |
| wood | 58.1 |
| leaves' atlas | 0.0 |

Slowest compiles, vale-walk (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| ground+ground_shade | 37.90 | 103.7 |
| trace_again | 33.88 | 112.0 |
| bake | 33.54 | 114.6 |
| far_quad+volume_shade | 32.86 | 98.4 |
| water_vertex+water_shade | 29.41 | 86.3 |
| far_quad+far_shade | 28.34 | 95.3 |
| limb+bark_shade | 27.47 | 91.7 |
| card+leaf_shade | 27.02 | 93.6 |
| trace | 26.46 | 95.6 |
| full+dab_map | 26.38 | 83.9 |

## 2026-10-28, apple metal-3 Apple M4: B1.5, the vale's walk (the *now* of Part C)

The vale loads at rest, W at frame 240, the walk stepping from it every run alike: its 1,200
measured frames (after 60 of warm-up) are 180 at rest and the walk's first 1,020. Its fawn is
drawn only in the shots that cast it, not on the walk.

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| vale-walk | 15.96 / 19.70 / 21.40 | 17.81 / 23.15 / 40.14 | 788 / 20 | 0.82 / 2.44 | 20.68 / 29.62 | 489.18 | 4978.83 | 3619.40 | 3.3 | 4173.63 | 0.0 | 781.8 |

Hitches, vale-walk (frames over 33 ms, the five heaviest labels each):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 86 | 89.81 | 0.75 | vegetation 4.37, terrain 4.32, creature 4.20, vegetation prepass 3.58, temporal 3.28 | finish 0.16, cook 0.13, passes 0.04, choose 0.00 | 1032 | 75 |
| 292 | 68.61 | 0.74 | bloom 28.01, terrain 5.45, creature 5.33, vegetation 5.16, look 4.42 | cook 0.14, present 0.07, passes 0.04, finish 0.01, choose 0.00 | 971 | 382 |
| 380 | 36.07 | 2.29 | bloom 17.19, vegetation 6.31, terrain 5.95, creature 5.83, vegetation prepass 4.29 | cook 0.94, present 0.10, passes 0.05, choose 0.01, finish 0.01 | 970 | 2703 |
| 451 | 40.73 | 1.61 | bloom 20.58, vegetation 4.09, vegetation prepass 3.61, terrain 2.89, look 2.82 | cook 0.77, present 0.08, passes 0.03, choose 0.01, finish 0.01 | 962 | 1519 |
| 458 | 39.40 | 1.06 | bloom 19.58, vegetation 3.49, vegetation prepass 3.00, terrain 2.70, look 2.67 | cook 0.26, present 0.07, passes 0.02, finish 0.01, choose 0.00 | 962 | 1534 |
| 464 | 36.34 | 1.25 | bloom 23.65, vegetation 3.57, look 3.21, vegetation prepass 2.89, terrain 2.86 | cook 0.27, present 0.12, passes 0.06, choose 0.01, finish 0.00 | 962 | 1245 |
| 530 | 40.31 | 0.51 | bloom 19.58, look 2.87, vegetation 2.82, clouds 2.54, vegetation prepass 2.37 | cook 0.13, present 0.05, passes 0.02, finish 0.01, choose 0.00 | 959 | 382 |
| 538 | 39.36 | 0.72 | bloom 28.25, look 3.87, terrain 2.96, creature 2.82, temporal 2.77 | cook 0.17, present 0.09, passes 0.03, choose 0.00, finish 0.00 | 959 | 669 |
| 568 | 39.76 | 0.54 | bloom 27.68, look 4.88, terrain 2.81, vegetation 2.64, temporal 2.63 | cook 0.13, present 0.06, passes 0.03, choose 0.01, finish 0.00 | 957 | 381 |
| 599 | 44.52 | 0.44 | bloom 22.38, clouds 5.06, shadows near 4.86, terrain 3.75, creature 3.67 | cook 0.17, present 0.03, passes 0.02, choose 0.00, finish 0.00 | 1037 | 93 |
| 701 | 34.03 | 0.72 | bloom 17.78, vegetation 6.61, terrain 5.93, creature 5.82, vegetation prepass 5.16 | cook 0.15, present 0.07, passes 0.04, choose 0.01, finish 0.00 | 970 | 382 |
| 709 | 34.57 | 0.59 | bloom 17.76, vegetation 6.49, terrain 5.92, creature 5.81, vegetation prepass 5.25 | cook 0.14, present 0.05, passes 0.03, finish 0.00, choose 0.00 | 969 | 382 |
| 785 | 45.13 | 0.62 | bloom 21.83, vegetation 3.60, vegetation prepass 3.20, terrain 3.18, look 3.15 | cook 0.15, present 0.07, passes 0.02, choose 0.00, finish 0.00 | 958 | 669 |
| 791 | 45.44 | 0.49 | bloom 27.10, look 3.75, vegetation 3.73, terrain 3.25, vegetation prepass 3.25 | cook 0.12, present 0.06, passes 0.01, choose 0.01, finish 0.01 | 958 | 381 |
| 822 | 44.71 | 0.50 | bloom 23.14, vegetation 3.32, look 3.21, terrain 2.87, clouds 2.79 | cook 0.12, present 0.06, passes 0.02, choose 0.00, finish 0.00 | 960 | 382 |
| 837 | 42.14 | 0.63 | bloom 22.74, vegetation 3.64, look 3.17, vegetation prepass 3.12, terrain 3.07 | cook 0.14, present 0.07, passes 0.02, choose 0.01, finish 0.00 | 961 | 669 |
| 1172 | 42.80 | 0.68 | bloom 22.39, vegetation 5.14, vegetation prepass 4.05, terrain 3.79, creature 3.69 | cook 0.15, present 0.08, passes 0.02, choose 0.01, finish 0.00 | 956 | 669 |
| 1186 | 43.50 | 0.79 | bloom 22.76, vegetation 4.60, vegetation prepass 3.75, look 3.13, terrain 3.01 | cook 0.17, present 0.10, passes 0.02, choose 0.00, finish 0.00 | 957 | 958 |
| 1209 | 40.14 | 0.47 | bloom 16.17, vegetation 3.08, terrain 2.96, creature 2.77, look 2.39 | cook 0.12, present 0.04, passes 0.01, choose 0.01, finish 0.01 | 957 | 381 |
| 1225 | 45.18 | 0.51 | bloom 26.74, vegetation 3.92, look 3.70, terrain 3.28, creature 3.12 | cook 0.12, present 0.05, passes 0.02, choose 0.01, finish 0.00 | 958 | 381 |

0 of 20 unattributed (no label over 2 ms, CPU under 2 ms).

Memory at playable, vale-walk (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 528.1 |
| targets | 90.8 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 34.0 |
| distance | 10.0 |
| fog | 15.8 |
| terrain | 10.8 |
| reflections | 7.5 |
| lights | 0.4 |
| wood lists | 18.3 |
| impostors | 78.0 |
| wood | 58.1 |
| leaves' atlas | 0.0 |
| realizer | 0.0 |

Slowest compiles, vale-walk (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 1140.81 | 162.5 |
| cook_materials | 931.02 | 227.4 |
| place_vertices | 875.31 | 194.0 |
| cull_blocks | 294.80 | 163.9 |
| far_quad+volume_shade | 52.13 | 98.4 |
| ground+ground_shade | 42.43 | 103.7 |
| bake | 40.69 | 114.6 |
| trace_again | 34.30 | 112.0 |
| character+character_shade | 31.12 | 94.8 |
| trace | 30.89 | 95.6 |

## 2026-10-29, apple metal-3 Apple M4

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB | Sustained |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| clearing-path | 12.39 / 13.88 / 14.16 | 14.32 / 26.00 / 28.32 | 91 / 0 | 0.48 / 1.11 | 13.94 / 20.89 | 472.85 | 4973.58 | 2962.63 | 1.3 | 1908.07 | 0.0 | 503.6 | sustained 600 s: minute 1 12.26, last 12.83 (1.05); 71.6 starts/s, 42 over 25 ms |

Hitches, clearing-path: none (no frame over 33 ms).

Memory at playable, clearing-path (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 352.5 |
| targets | 67.2 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 26.5 |
| distance | 0.0 |
| fog | 0.0 |
| terrain | 8.0 |
| reflections | 4.4 |
| lights | 0.4 |
| wood lists | 2.9 |
| impostors | 36.0 |
| wood | 4.8 |

Slowest compiles, clearing-path (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 1491.88 | 162.5 |
| cook_materials | 1141.04 | 226.9 |
| place_vertices | 1133.18 | 73.1 |
| stone_vertex+stone_shade | 471.63 | 77.9 |
| cull_blocks | 348.23 | 59.8 |
| far_quad+far_shade | 65.28 | 95.3 |
| ground+ground_shade | 55.90 | 103.7 |
| card+leaf_shade | 31.33 | 93.6 |
| far_quad+volume_shade | 31.03 | 98.4 |
| bake | 30.89 | 114.6 |


## C1, every switch a constant (2026-10-29, toolchain 0.10.1)

docs/plan-world-class-3.md §C1: the frame's switches are a type the program names
(`engine::switches`), so a switch that's off builds none of its path. The three runs below are
its, on the M4 Air (`wrela tool bench -- --scene clearing-path --sustained 600`, then `--scene
last-green-walk` and `--scene vale-walk`, back to back).

- **Pipelines** (`wrela pipelines`): the clearing 120 → 113 (no froxels', distance volume's or
  cascades' pipelines; its sights' `see` and `shift_seen` stay, as its sights do), the floor
  175 → 171, the vale 114 → 110, the bare fixture 64 → 58.
- **The switches test** (`engine/tests/switches`; `wrela build engine/tests/frame --set
  FOG=true|false`): fog on, 58 pipelines and the ground's shading 96,873 bytes of WGSL; off, 56
  and 95,649.
- **G-pixels** against build/base-0.9 (240 frames, 1280 × 720): capybara, herd and climber-lab 0
  pixels differ; the clearing 0, or 1 pixel by 1/255 in two runs of four (as Part A saw, 0.9
  against 0.10.0); the floor (LG_WALK) 26.5 % and 28.2 % (0.35–0.38 % over 2/255), its same build
  16.4 % (0.31 %): its run-to-run noise.
- **G-profile, not met.** The clearing's GPU p50 12.68 ms sustained (minute 1 12.51 ms, minute 10
  12.52; C3's run before C1, 12.39): the gate's 10.2 isn't reached. Compiling the switches out
  moved nothing: the clearing's fog and distance volume were already off at run time, and B16's
  reading of its rise (the paths every shading pass carried) doesn't hold. Its passes run alone
  (`wrela profile examples/clearing --chrome --serial --input build/gates/CLEARING_PATH.json`,
  300 frames): the clouds' march 9.34 ms, the vegetation 5.88, its prepass 5.45, the probes
  traced again 3.61, temporal AA 1.80, the look 1.46, the bloom 1.40, the terrain 0.89. The hill
  goes on in C6, which may change pixels.
- **The now for C6:** the floor's walk 14.96 ms GPU p50, the vale's 16.70 (plain runs, after the
  clearing's sustained one: B1.5's vale was 15.96).

## 2026-10-29, apple metal-3 Apple M4

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB | Sustained |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| clearing-path | 12.68 / 14.23 / 14.68 | 13.61 / 16.44 / 25.39 | 26 / 0 | 0.49 / 1.06 | 9.73 / 19.75 | 441.55 | 9359.02 | 5413.62 | 1.2 | 3005.69 | 0.0 | 503.6 | sustained 600 s: minute 1 12.51, last 12.52 (1.00); 73.5 starts/s, 9 over 25 ms |

Hitches, clearing-path: none (no frame over 33 ms).

Memory at playable, clearing-path (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 352.5 |
| targets | 67.2 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 26.5 |
| distance | 0.0 |
| fog | 0.0 |
| terrain | 8.0 |
| reflections | 4.4 |
| lights | 0.4 |
| wood lists | 2.9 |
| impostors | 36.0 |
| wood | 4.8 |

Slowest compiles, clearing-path (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 2694.69 | 162.5 |
| cook_materials | 2425.38 | 226.9 |
| place_vertices | 2354.53 | 73.1 |
| cull_blocks | 966.51 | 59.8 |
| bake | 655.46 | 115.1 |
| trace_again | 646.04 | 112.4 |
| far_quad+far_shade | 556.85 | 72.9 |
| character+character_shade | 496.74 | 75.8 |
| far_quad+volume_shade | 488.50 | 79.4 |
| ground+ground_shade | 436.12 | 84.8 |

## 2026-10-29, apple metal-3 Apple M4

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB | Sustained |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| last-green-walk | 14.96 / 17.64 / 19.38 | 16.72 / 22.36 / 28.39 | 607 / 0 | 4.23 / 5.34 | 13.35 / 27.99 | 632.19 | 13603.69 | 6755.86 | 5.0 | 7622.44 | 0.1 | 876.4 | no |

Hitches, last-green-walk: none (no frame over 33 ms).

Memory at playable, last-green-walk (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 620.5 |
| targets | 90.8 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 30.3 |
| distance | 10.4 |
| fog | 15.8 |
| terrain | 10.8 |
| reflections | 7.5 |
| lights | 0.4 |
| wood lists | 41.5 |
| impostors | 78.0 |
| wood | 58.1 |
| leaves' atlas | 0.0 |
| built | 61.7 |
| realizer | 18.3 |
| rocks | 0.0 |
| banks | 0.0 |

Slowest compiles, last-green-walk (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| cook_materials | 2606.33 | 165.3 |
| skin_vertices | 2091.74 | 103.1 |
| place_vertices | 1629.44 | 92.2 |
| place_vertices | 1121.98 | 92.2 |
| cook_materials | 975.95 | 165.3 |
| cook_level | 901.22 | 166.0 |
| skin_vertices | 900.54 | 103.1 |
| cull_blocks | 771.81 | 114.6 |
| cull_blocks | 715.43 | 114.6 |
| cull_blocks | 637.86 | 114.6 |

## 2026-10-29, apple metal-3 Apple M4

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB | Sustained |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| vale-walk | 16.70 / 20.81 / 22.67 | 18.26 / 23.69 / 32.92 | 865 / 11 | 0.97 / 2.82 | 21.31 / 31.71 | 495.58 | 5966.80 | 3624.10 | 3.2 | 3912.95 | 0.0 | 781.8 | no |

Hitches, vale-walk (frames over 33 ms, the five heaviest labels each):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 90 | 84.69 | 0.82 | vegetation 4.81, terrain 4.66, vegetation prepass 3.87, creature 3.76, temporal 3.18 | cook 0.14, finish 0.12, passes 0.05, choose 0.01 | 1032 | 75 |
| 413 | 33.94 | 0.81 | bloom 22.66, vegetation 6.06, terrain 5.54, creature 5.43, vegetation prepass 4.57 | cook 0.17, present 0.08, passes 0.03, choose 0.01, finish 0.00 | 968 | 669 |
| 444 | 36.26 | 1.06 | bloom 26.85, look 3.73, vegetation 3.20, terrain 2.94, vegetation late 2.80 | cook 0.18, present 0.13, passes 0.03, choose 0.01, finish 0.00 | 962 | 957 |
| 450 | 40.24 | 0.96 | bloom 20.44, vegetation 4.40, vegetation prepass 3.66, terrain 3.32, creature 3.16 | cook 0.20, present 0.13, passes 0.03, choose 0.00, finish 0.00 | 962 | 958 |
| 458 | 35.98 | 1.25 | bloom 18.95, vegetation 3.15, vegetation prepass 2.98, look 2.59, terrain 2.09 | cook 0.31, present 0.09, passes 0.02, choose 0.00, finish 0.00 | 962 | 1534 |
| 468 | 33.36 | 0.73 | bloom 17.26, vegetation 2.79, vegetation prepass 2.72, look 2.45, temporal 1.95 | cook 0.14, present 0.11, passes 0.02, choose 0.00, finish 0.00 | 962 | 669 |
| 472 | 37.28 | 1.05 | bloom 20.91, vegetation 5.47, terrain 4.86, creature 4.69, vegetation prepass 4.21 | cook 0.21, present 0.12, passes 0.03, finish 0.01, choose 0.00 | 962 | 958 |
| 624 | 35.62 | 0.59 | bloom 19.67, vegetation 5.37, terrain 4.91, creature 4.80, vegetation prepass 4.42 | cook 0.13, present 0.07, passes 0.04, choose 0.01, finish 0.00 | 970 | 382 |
| 686 | 33.73 | 1.27 | bloom 19.80, vegetation 6.50, vegetation prepass 5.29, terrain 5.11, creature 4.98 | cook 0.27, present 0.20, passes 0.06, choose 0.02, finish 0.00 | 970 | 958 |
| 1171 | 40.20 | 1.33 | bloom 23.83, vegetation 5.31, vegetation prepass 4.07, terrain 3.93, creature 3.83 | cook 0.31, present 0.09, passes 0.03, choose 0.01, finish 0.01 | 956 | 1533 |
| 1203 | 41.48 | 1.02 | bloom 22.81, vegetation 4.75, vegetation prepass 3.50, terrain 3.38, creature 3.23 | present 0.21, cook 0.18, passes 0.06, choose 0.01, finish 0.00 | 956 | 669 |

0 of 11 unattributed (no label over 2 ms, CPU under 2 ms).

Memory at playable, vale-walk (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 528.1 |
| targets | 90.8 |
| shadows | 68.0 |
| sky | 74.8 |
| probes | 34.0 |
| distance | 10.0 |
| fog | 15.8 |
| terrain | 10.8 |
| reflections | 7.5 |
| lights | 0.4 |
| wood lists | 18.3 |
| impostors | 78.0 |
| wood | 58.1 |
| leaves' atlas | 0.0 |
| realizer | 0.0 |

Slowest compiles, vale-walk (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 2135.91 | 162.5 |
| place_vertices | 1725.41 | 194.0 |
| cook_materials | 1551.34 | 227.4 |
| cull_blocks | 701.00 | 163.9 |
| cook_level | 343.12 | 44.0 |
| bake | 247.30 | 115.1 |
| trace_again | 237.22 | 112.4 |
| cook_distance | 198.22 | 62.6 |
| ground+ground_shade | 185.06 | 94.6 |
| water_vertex+water_shade | 143.62 | 78.8 |

## C4, memory (2026-10-29, toolchain 0.10.1)

docs/plan-world-class-3.md §C4. The bench's GPU peak (the browser host's count of every buffer
and texture, at its most) was mostly a load's transient: at the end of the walk the floor held
658 MiB of its 876, the vale 538 of its 782. Each lever, `wrela tool bench -- --scene <s>` on the
M4 Air (plain runs), peak / end MiB:

| Lever | vale | floor |
|---|---|---|
| before C4 (C1's runs) | 781.8 / 538.0 | 876.4 / 658.4 |
| the wood's kinds pooled as they're cooked (`wood::Pooling`) | 557.9 / 538.0 | 891.0 / 659.0 |
| the realizer calibrates in `CALIBRATION_ROOM` (**reverted** in review: it undercounted the quads of a mesh past that room; the rows below include it) | 557.9 / 538.0 | 866.5 / 659.0 |
| `Pool::reserve`: a frame's built meshes grow the pool once, by a quarter | (none built) | 857.4 / 658.4 |
| the floor's built realizations at most 64 MiB of room a frame (`BUILT_ROOM`) | | (48 MiB: 733.5, playable 18.4 s; 96: 806.1, 9.5 s) |
| a static cascade's second map only while a set is drawn (`shadow::Kept`) | 525.9 / 522.0 | 754.0 / 629.8 |
| a kind's own cards, orders, limbs and bounds let go once pooled | 512.2 / 508.3 | 726.2 / 613.8 |
| the moving map a texel while nothing moves | 508.2 / 504.3 | 728.5 / 613.4 |

- **Sustained** (the two runs below, `wrela tool bench -- --scene vale-walk --sustained 120`, then
  `--scene last-green-walk --sustained 120`; 120 s, not 600, the memory being the load's): the
  vale's GPU peak **508.2 MiB, under 512: met**; the floor's 738.5, not met (its steady state 613).
- **What the floor's steady state holds** (its share table below): held 572.5 MiB, of it the
  impostors 78, the sky 74.8 (the clouds' dome 40 of it), the built things 61.7, the wood 58.1 and
  its lists 39.1. Under 512 needs about 100 more; the transient's lever (built things realized into
  room for what their surface crosses, not their grid) needs a second realization where the room
  is short, and goes on in C6.
- **G-pixels** against build/base-0.9 (240 frames, 1280 × 720): capybara, herd, climber-lab and
  the clearing 0 px differ; the floor 23.1 % and 18.6 % (0.32–0.36 % over 2/255) against its same
  build's 32.4 % (0.39 %): its noise.

## 2026-10-29, apple metal-3 Apple M4

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB | Sustained |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| vale-walk | 15.85 / 19.40 / 21.63 | 21.63 / 28.61 / 35.72 | 1145 / 19 | 1.35 / 4.37 | 20.21 / 30.68 | 482.08 | 5297.56 | 4223.61 | 3.2 | 4076.69 | 0.0 | 508.2 | sustained 120 s: minute 1 17.34, last 19.86 (1.15); 44.8 starts/s, 225 over 25 ms |

Hitches, vale-walk (frames over 33 ms, the five heaviest labels each):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 63 | 36.56 | 3.68 | shadows far 12.54, vegetation 6.51, terrain 6.47, creature 6.34, vegetation prepass 5.72 | finish 0.39, cook 0.36, passes 0.10, choose 0.01 | 1032 | 75 |
| 64 | 37.87 | 2.07 | shadows far 12.36, vegetation 6.73, terrain 6.66, creature 6.52, vegetation prepass 6.02 | cook 0.39, finish 0.27, passes 0.08, choose 0.04 | 1032 | 90 |
| 65 | 33.78 | 1.96 | shadows far 11.91, vegetation 6.65, terrain 6.61, creature 6.48, vegetation prepass 5.87 | cook 0.37, finish 0.22, passes 0.12, choose 0.01 | 1032 | 75 |
| 77 | 35.72 | 1.62 | shadows far 11.10, vegetation 6.99, terrain 6.67, creature 6.50, vegetation prepass 6.25 | cook 0.32, finish 0.14, passes 0.11, choose 0.01 | 1032 | 75 |
| 78 | 33.96 | 2.19 | shadows far 10.33, vegetation 6.84, terrain 6.76, creature 6.59, vegetation prepass 5.94 | cook 0.61, finish 0.16, passes 0.10, choose 0.01 | 1032 | 75 |
| 84 | 33.41 | 2.37 | shadows far 12.56, vegetation 6.66, terrain 6.57, creature 6.39, vegetation prepass 5.83 | finish 0.60, cook 0.41, passes 0.13, choose 0.02 | 1032 | 75 |
| 88 | 34.20 | 2.74 | shadows far 13.95, vegetation 7.20, terrain 7.13, creature 6.97, vegetation prepass 6.37 | cook 0.51, finish 0.41, passes 0.25, choose 0.03 | 1032 | 90 |
| 89 | 34.80 | 2.67 | vegetation 5.77, terrain 5.60, creature 5.44, vegetation prepass 4.88, temporal 4.27 | cook 0.39, finish 0.21, passes 0.08, choose 0.01 | 1032 | 75 |
| 110 | 37.17 | 23.14 | vegetation 5.38, vegetation prepass 4.84, terrain 4.27, creature 4.10, temporal 3.82 | cook 15.09, finish 0.25, passes 0.12, choose 0.02 | 966 | 5340 |
| 111 | 158.33 | 3.99 | probes bake 7.48, vegetation 5.03, vegetation prepass 4.36, terrain 4.10, creature 3.96 | cook 2.25, finish 0.27, passes 0.15, choose 0.02 | 966 | 75 |
| 112 | 48.68 | 1.30 | probes bake 10.83, vegetation 7.36, terrain 6.32, creature 6.18, vegetation prepass 5.70 | cook 0.32, finish 0.16, passes 0.12, choose 0.02 | 966 | 75 |
| 114 | 70.89 | 2.21 | probes bake 8.10, vegetation 4.69, vegetation prepass 4.03, terrain 3.82, creature 3.69 | finish 0.52, cook 0.39, passes 0.10, choose 0.03 | 966 | 75 |
| 117 | 126.84 | 1.55 | terrain 5.64, creature 5.52, vegetation 5.46, vegetation prepass 4.06, temporal 3.62 | cook 0.35, finish 0.18, passes 0.10, choose 0.01 | 966 | 75 |
| 118 | 36.96 | 1.03 | terrain 6.51, vegetation 6.44, creature 6.39, clouds 4.87, vegetation prepass 4.67 | cook 0.29, finish 0.11, passes 0.06, choose 0.00 | 966 | 75 |
| 120 | 120.32 | 1.23 | terrain 7.41, vegetation 7.38, creature 7.25, vegetation prepass 5.45, clouds 4.58 | cook 0.29, finish 0.11, passes 0.07, choose 0.01 | 966 | 75 |
| 409 | 77.02 | 1.66 | bloom 23.72, vegetation 7.20, terrain 6.83, creature 6.71, vegetation prepass 5.17 | cook 0.34, present 0.17, passes 0.05, finish 0.01, choose 0.00 | 969 | 957 |
| 600 | 38.52 | 1.02 | bloom 20.87, terrain 3.98, creature 3.90, vegetation 3.72, vegetation prepass 3.09 | cook 0.26, present 0.10, passes 0.03, choose 0.01, finish 0.00 | 971 | 382 |
| 924 | 36.07 | 1.29 | bloom 24.16, vegetation 12.00, terrain 10.96, creature 10.75, vegetation prepass 8.77 | cook 0.30, present 0.13, passes 0.03, choose 0.01, finish 0.00 | 955 | 669 |
| 1171 | 33.45 | 2.16 | bloom 24.26, vegetation 5.97, vegetation prepass 4.64, terrain 4.35, creature 4.23 | cook 0.50, present 0.18, passes 0.04, choose 0.01, finish 0.01 | 956 | 1533 |

0 of 19 unattributed (no label over 2 ms, CPU under 2 ms).

Memory at playable, vale-walk (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 478.4 |
| targets | 90.8 |
| shadows | 32.0 |
| sky | 74.8 |
| probes | 34.0 |
| distance | 10.0 |
| fog | 15.8 |
| terrain | 10.8 |
| reflections | 7.5 |
| lights | 0.4 |
| wood lists | 18.3 |
| impostors | 78.0 |
| wood | 58.1 |
| leaves' atlas | 0.0 |
| realizer | 0.0 |

Slowest compiles, vale-walk (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 1360.83 | 162.5 |
| place_vertices | 620.37 | 194.0 |
| cook_materials | 615.73 | 227.4 |
| cull_blocks | 282.66 | 163.9 |
| ground+ground_shade | 45.15 | 94.6 |
| reveal | 36.71 | 15.8 |
| bake | 33.86 | 115.1 |
| trace_again | 31.82 | 112.4 |
| character+character_shade | 28.75 | 85.7 |
| full+dab_map | 27.96 | 83.9 |

## 2026-10-29, apple metal-3 Apple M4

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | Warm playable ms | Warm loaded MiB | GPU peak MiB | Sustained |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| last-green-walk | 15.28 / 18.28 / 19.93 | 17.74 / 22.49 / 31.32 | 802 / 10 | 4.93 / 10.76 | 16.61 / 30.77 | 677.53 | 8586.96 | 4340.18 | 5.0 | 6698.05 | 0.1 | 738.5 | sustained 120 s: minute 1 16.00, last 16.21 (1.01); 55.2 starts/s, 45 over 25 ms |

Hitches, last-green-walk (frames over 33 ms, the five heaviest labels each):

| Frame | Interval ms | CPU ms | Labels, GPU ms | Phases, ms | Draws | Written KiB |
|---|---|---|---|---|---|---|
| 90 | 36.50 | 9.40 | shadows far 21.97, creature 4.05, terrain 3.82, vegetation 3.67, clouds 3.46 | sim 8.11, cook 0.35, finish 0.09, passes 0.08, choose 0.02 | 1265 | 115 |
| 91 | 36.53 | 9.75 | shadows far 22.18, creature 4.24, terrain 4.01, vegetation 3.97, clouds 3.41 | sim 8.64, cook 0.25, finish 0.09, passes 0.06, choose 0.01 | 1265 | 115 |
| 92 | 37.03 | 10.59 | shadows far 20.81, creature 4.15, terrain 3.93, vegetation 3.92, clouds 3.36 | sim 8.59, cook 0.50, finish 0.13, passes 0.06, choose 0.01 | 1265 | 334 |
| 93 | 35.59 | 10.60 | shadows far 7.52, creature 3.77, terrain 3.55, vegetation 3.52, clouds 3.12 | sim 8.64, cook 0.49, finish 0.13, passes 0.05, choose 0.01 | 1265 | 334 |
| 472 | 35.21 | 5.41 | bloom 23.63, creature 3.41, terrain 3.34, look 3.28, fog 2.43 | sim 4.70, cook 0.18, present 0.08, passes 0.03, choose 0.02 | 1207 | 139 |
| 506 | 37.96 | 5.01 | bloom 23.11, look 3.58, creature 2.91, terrain 2.86, clouds 2.80 | sim 4.43, cook 0.15, present 0.07, passes 0.02, choose 0.01 | 1207 | 135 |
| 520 | 34.92 | 4.91 | bloom 23.11, clouds 3.45, look 3.15, fog 3.03, creature 2.41 | sim 4.31, cook 0.15, present 0.07, passes 0.04, choose 0.00 | 1207 | 135 |
| 694 | 34.26 | 5.00 | bloom 22.98, look 3.19, clouds 2.94, vegetation 2.41, vegetation prepass 2.22 | sim 4.22, cook 0.17, present 0.05, passes 0.04, choose 0.01 | 1207 | 134 |
| 735 | 34.24 | 4.57 | bloom 22.93, look 4.19, clouds 3.37, fog 2.72, vegetation 2.20 | sim 3.98, cook 0.15, present 0.06, passes 0.03, choose 0.01 | 1207 | 135 |
| 920 | 33.36 | 5.04 | bloom 22.68, look 3.08, fog 2.24, temporal 1.94, vegetation 1.86 | sim 4.50, cook 0.14, present 0.06, passes 0.02, choose 0.01 | 1207 | 136 |

0 of 10 unattributed (no label over 2 ms, CPU under 2 ms).

Memory at playable, last-green-walk (cold, by the program's own counts):

| Share | MiB |
|---|---|
| held | 572.5 |
| targets | 90.8 |
| shadows | 36.0 |
| sky | 74.8 |
| probes | 30.3 |
| distance | 10.4 |
| fog | 15.8 |
| terrain | 10.8 |
| reflections | 7.5 |
| lights | 0.4 |
| wood lists | 39.1 |
| impostors | 78.0 |
| wood | 58.1 |
| leaves' atlas | 0.0 |
| built | 61.7 |
| realizer | 18.3 |
| rocks | 0.0 |
| banks | 0.0 |

Slowest compiles, last-green-walk (cold):

| Pipeline | ms | WGSL KiB |
|---|---|---|
| skin_vertices | 1618.60 | 103.1 |
| cook_materials | 1122.25 | 165.3 |
| place_vertices | 1099.46 | 92.2 |
| cull_blocks | 284.44 | 114.6 |
| place_vertices | 272.80 | 92.2 |
| skin_vertices | 263.03 | 103.1 |
| cook_materials | 208.47 | 165.3 |
| cull_blocks | 152.55 | 114.6 |
| cull_blocks | 56.95 | 114.6 |
| cook_level | 50.06 | 166.0 |


## C5, cold and warm starts (2026-10-29, toolchain 0.10.2)

docs/plan-world-class-3.md §C5. Every number here is from `wrela tool bench -- --scene vale-walk
--scene last-green-walk --starts 3` (M4 Air, nothing else running): `playable_ms`, the median of
three, with the runs in parentheses.

**Every cold start in this file before this section had the system's shader cache warm.**
Chrome's profile is thrown away each run, but macOS keeps a shader cache an app, keyed by a
shader's source. The bench salted its cold runs 1, 2, 3, ... every time, so a salt an earlier
run had used was served from that cache. The vale's cold start was 4.7–5.3 s in the full bench,
and 8.4–9.1 s with salts no run had used. The salt is now new to the machine each run.

| Step | vale cold | vale warm | floor cold | floor warm |
|---|---|---|---|---|
| now (salts new to the machine) | 8722 (9103, 8722, 8663) | 4013 (4013, 3990, 4033) | (salts 1–3, served) | 7321 (7321, 7398, 6725) |
| C5a: the covered frames cook alone; the impostor's maps in one pass | 8397 | 2762 | 13538 | 6585 |
| C5b: the kinds' cards' orders on the helpers | 8029 (7945, 8029, 8116) | 2177 (2127, 2185, 2177) | 13631 (13631, 12966, 13970) | 6509 (6509, 6990, 6055) |
| C5c: toolchain 0.10.2, six pipelines at a time | **6691** (6691, 6935, 6447) | **2209** (2191, 2249, 2209) | **13331** (13331, 13029, 13517) | **5379** (5413, 5308, 5379) |

Tried, and nothing moved (single runs unless said):

- The floor's fawn and wolf realized once its static casters are in (their creature field's
  pipelines are what its frames 0 and 1 waited on): floor 13433 / 5490, three runs.
- Smallest first: floor 12200 / 6516, vale 6066 / 2153.
- Both ends in turn: floor 12729 / 5310, vale 6601 / 2234.
- Two at a time: floor 12196 / 6847, vale 5660 / 2159.
- Twelve at a time: floor 13112 / 4990, vale 7573 / 2201.
- The order a native run first uses the pipelines (a `first-use.json` shipped with the build, a
  0.11.0 kept only as a development build): floor 13474 / 5616, vale 6653 / 2240.
- The plan's levers 2, 4 and 5 hold already. The tiles arrive when the program asks for them
  (the vale's first at 5.1 s) and streaming isn't the last gate (natively the vale is streamed
  by frame 9; its far cascade finishes at frame 79 and its probes later). The floor's frame 0
  CPU is 55–86 ms in all. A release serves `/releases/*` immutable for a year, so a warm start
  fetches nothing.

- **G-warm, the vale: met.** 2209 ms, under 3,000.
- **G-cold, the vale: Blocked at 6691 ms** (gate 5,000). The start compiles at the GPU process's
  pace (docs/toolchain-gaps.md, "A cold start's pipelines compile at the GPU process's own
  pace"): the vale needs 48 of its 108 pipelines before playable, 7.5 s of compiling built one
  at a time, and its frame 0's 17 finish together 1.5 s after they're asked for. Next: smaller
  shaders on the load's path.
- **G-warm, the floor: Blocked at 5379 ms** (gate 3,000). It's load-bound now: its frames 3–102
  take 3.1 s (its built things' realizations, then its cascades and probes), after 1.5 s of
  frames 0–12. Next: its built things realized into the room their surface crosses, not their
  grid (C4's carry, C6), and fewer frames for its cascades and probes.
- **G-cold, the floor: Blocked at 13331 ms.** It's compile-bound as the vale is, with 119
  pipelines used in its first 300 frames. The largest are the fawn's and the wolf's
  creature-field realizer (160–230 KiB of WGSL, with dual-number gradients) and the terrain's
  `cook_level` (140–166 KiB). Next: those shaders made smaller, which moves pixels (Parts D–E).
- **G-pixels:** capybara, herd, climber-lab and the clearing 0 px differ at each step. The floor
  is within its noise, except C5a's covered skip, which puts it 0.05 pp over its noise
  (0.44–0.48 % over 2/255 against 0.10–0.40 %; its commit has why).


## C7, the tiers (2026-10-29, toolchain 0.10.3)

docs/plan-world-class-3.md §C7, D37 (**proposed**). A program names its tier, `pub const TIER:
Tier` (`engine::tier`), and its switches follow it at build time (`Tiered`, folded by 0.10.3) and
its quality at run time (`Tier::quality`). `wrela tool bench -- --scene vale-walk --scene
last-green-walk --tier <tier> --sustained 120 --no-docs --no-native` on the M4 Air, each run
counted from 60 frames after `playable`, the walks begun at frame 900.

| Scene (tier) | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | GPU peak MiB | Sustained |
|---|---|---|---|---|---|
| last-green-walk (Epic) | 15.54 / 19.52 / 20.91 | 17.21 / 29.92 / 36.38 | 855 / 25 | 752.0 | sustained 120 s: minute 1 15.65, last 15.72 (1.00); 56.9 starts/s, 133 over 25 ms |
| vale-walk (Epic) | 18.58 / 23.09 / 25.00 | 23.61 / 29.13 / 31.61 | 1457 / 8 | 541.5 | sustained 120 s: minute 1 21.14, last 20.97 (0.99); 42.6 starts/s, 510 over 25 ms |
| last-green-walk (High) | 15.09 / 18.18 / 19.63 | 16.78 / 29.61 / 34.36 | 769 / 18 | 728.8 | sustained 120 s: minute 1 15.66, last 15.36 (0.98); 58.3 starts/s, 133 over 25 ms |
| vale-walk (High) | 17.31 / 21.53 / 23.54 | 21.93 / 27.36 / 30.08 | 1380 / 7 | 541.5 | sustained 120 s: minute 1 19.17, last 19.31 (1.01); 45.9 starts/s, 264 over 25 ms |
| last-green-walk (Medium) | 12.46 / 14.97 / 16.22 | 14.16 / 26.40 / 29.79 | 234 / 0 | 675.0 | sustained 120 s: minute 1 13.22, last 13.20 (1.00); 68.6 starts/s, 86 over 25 ms |
| vale-walk (Medium) | 12.86 / 16.27 / 17.44 | 18.22 / 23.43 / 27.59 | 1059 / 6 | 485.0 | sustained 120 s: minute 1 14.45, last 14.96 (1.04); 54.5 starts/s, 37 over 25 ms |
| last-green-walk (Low) | 11.30 / 13.79 / 14.86 | 14.08 / 17.73 / 27.03 | 229 / 0 | 621.4 | sustained 120 s: minute 1 12.00, last 11.92 (0.99); 70.1 starts/s, 23 over 25 ms |
| vale-walk (Low) | 11.87 / 15.29 / 16.86 | 16.68 / 20.09 / 26.41 | 717 / 6 | 446.1 | sustained 120 s: minute 1 13.32, last 13.52 (1.01); 59.5 starts/s, 18 over 25 ms |

These are the final code's (d72db31a), run one tier after another straight after C6's 600 s run,
so the machine was warm throughout: the floor's Epic row is 1.5 ms over the 600 s run's p50.

- **G-tier, on the M4: holds.** Each tier's GPU p50 is under the one above's on both walks:
  the floor 15.54 > 15.09 > 12.46 > 11.30, the vale 18.58 > 17.31 > 12.86 > 11.87. Their
  GPU peaks fall the same way, but for the vale's Epic and High (541.5 each).
- **Pipelines** (`wrela pipelines <pkg> --set TIER=<tier>`): the vale 108 / 106 / 104 / 100, the
  floor 169 / 167 / 165 / 161.
- **The settings** (**proposed**, for Ryan): `High` the contact shadows and the reflections off;
  `Medium` the probes' sights off too, the scene at half the output, the near cascade at 1024²,
  the probes baked with 16 rays; `Low` the fog, the distance volume and the ambient occlusion off
  too, the far cascade at 512². The plan's `High` halved the clouds' dome: measured, it was slower
  than `Epic` (the floor 15.86 ms, the vale 18.23), since it upsamples the whole dome after each
  march, and no tier has it. The plan's `Low` had no far cascade; the probes' first bake and an
  arrival wait for it whole, so it's kept, small.
- **G-pixels at `Epic`:** the vale's and the floor's builds have the same 108 and 169 pipelines,
  byte for byte, as before C7; capybara, herd and climber-lab 0 px differ from build/base-0.9, the
  clearing as C6a, the floor within its noise.

## C6, the sustained frame (2026-10-29, toolchain 0.10.3)

docs/plan-world-class-3.md §C6. The runs are `wrela tool bench -- --scene vale-walk --scene
last-green-walk ...` on the M4 Air, Epic. Each run is counted from 60 frames after `playable`
(earlier tables counted the load's covered frames: the vale's GPU span in them was a covered
scene's). The walks begin at frame 900.

| Step | vale GPU p50 / interval p99 / over 33 | floor GPU p50 / interval p99 / over 33 |
|---|---|---|
| the baseline, counted after playable (a1aaf9f4) | 18.56 / 35.91 / 29 | 14.68 / 24.26 / 1 |
| C6a: a whole dome marched every other frame | 15.34 / 36.84 / 21 | 12.88 / 25.79 / 1 |
| **sustained 600 s** (`--sustained 600`), Epic, the final code (d72db31a) | **18.37** / 31.48 / 6 (minute 1 20.51, minute 10 20.65) | **13.34** / 35.45 / 27 (minute 1 13.45, minute 10 15.12) |

The fanless M4 Air runs 15-20 % slower after its first minutes: the plain runs are the cool
machine's, and the sustained row is what the gate reads.

Levers, each measured:

- **Kept: the clouds' march every other frame** (C6a, G-look (3) within each shot's run-to-run
  spread).
- **Dropped: the probes traced again on the dome's resting frames.** The floor's
  `15-the-wildwood` lost 1.48 dB.
- **Not taken: the fog's inject at half its cadence.** Its froxels follow the camera each frame.
- **Dropped: the cards' range 160 → 120 m** (`Wood::cards_limit`), vegetation −2.0 ms serial (to
  90 m, −3.2), and **the leaves' coverage from two area samples, not four** (`leaf_pixel`), the
  prepass −1.3 ms. Each raised a G-look (3) measure that a still can't show a judge
  (development shots at 720p: `12-D1`'s trail 65 → 72 and 77 px, `18-D7`'s flicker 18.6 → 21.1
  and 20.8, reproducible).
- **The hitches.** The vale's are booked to `bloom` at 18-26 ms, but in a paced run a frame's
  bloom pass holds the wait for the screen; alone, bloom is 1.6 ms. They are frames whose GPU
  time overran.

Where a walk's frame goes, each pass alone (`wrela profile <pkg> --chrome --serial --size
1920x1080 --warmup 960 --frames 300`), ms a frame:

| Pass | vale | floor |
|---|---|---|
| vegetation (shading) | 9.63 | 7.68 |
| vegetation prepass | 9.21 | 6.19 |
| fog | 2.77 | 6.51 |
| temporal | 3.31 | 4.28 |
| look | 3.11 | 4.44 |
| terrain | 2.93 | 4.00 |
| bloom | 2.05 | 2.68 |

The vale's cards are 7.5 ms of its prepass, about 4 of it their leaves' coverage test, and 4.8
of its shading pass. The prepass and the shading pass each draw every card.

- **G-sustained: Blocked.**
  - The vale is at 18.37 ms GPU p50 against 12, interval p99 31.48 against 20, 6 frames over
    33 against 0.
  - The floor is at 13.34, 35.45 and 27.
  - Next: the cards shaded from the prepass, not drawn twice (a target of each pixel's card,
    instance and place on the card; at most the shading pass's 4.8 ms of card draws). That
    changes the cards Part E grows, so it's taken with Part E's wood.
  - Then the floor's froxels, and the look, temporal AA and the bloom at the output's size.
- **C1's G-profile, the clearing (carried here): Blocked at 11.52 ms** GPU p50 (gate 10.2; C1's
  12.68). `wrela tool bench -- --scene clearing-path --sustained 600 --no-docs` on 030a0cea:
  span 11.52 / 14.40 / 14.91, interval 14.52 / 25.75 / 30.77, 0 over 33, minute 1 11.56, minute
  10 13.42 (1.16, throttling). C6a's cadence took the clouds' march (9.34 ms serial at C1) to
  every other frame; what's left is its vegetation's two passes (5.88 and 5.45 serial at C1),
  the same lever as the vale's.
- **The hitch table: not empty** (Blocked as above). Every hitch is a GPU frame overrun, and the
  levers are the passes above.

## Whether D27 holds

docs/plan-world-class-3.md §0.1, D27 (**proposed**). A row per gate per scene per device, each
cell "holds", "misses by ..." or "blocked: ...". Part C started it with the M4's performance
rows; Part H finishes it (§H2). The M4 Air's numbers are Chrome at 1080p, `Epic`, toolchain
0.10.3: the frame from C6's 600 s sustained run, memory from the same run's GPU peak, and the
starts from C5's three-run medians (salts new to the machine). Parts D–H changed both scenes'
frames since, and this plan's last machine was Linux (Radeon "Lucienne"), not the M4: each M4
cell is C's number, **to measure again on the M4** with the scenes as Part H leaves them (the
last column says what this machine measured, which is no stand-in for the M4's).

| Gate | Scene | M4 Air | Note |
|---|---|---|---|
| GPU frame p50 < 12 ms, after 600 s | vale | misses by 6.37 ms (18.37) | Its two vegetation passes draw every card twice, 18.8 ms serial (C6). Here: 57.12 ms (Part G's walk). |
| | floor | misses by 1.34 ms (13.34) | Its fog is 6.5 ms serial, beside the cards. Here: 32.20 ms (H1). |
| Paced p99 < 20 ms | vale | misses by 11.48 ms (31.48) | The same frames' overruns. |
| | floor | misses by 15.45 ms (35.45) | |
| 0 of 1,200 paced frames over 33 ms | vale | misses by 6 | |
| | floor | misses by 27 | |
| 1,200 frames begun within 20.4 s, one in flight, at most 12 intervals over 25 ms | vale | misses (42.9 a second; 58.8 needed) | |
| | floor | misses: its starts hold by 0.02 a second (58.84), but at least 27 intervals were over 25 ms (12 allowed: the same run's 27 over 33 ms) | Its starts within run-to-run noise: the 120 s Epic run after it began 56.9. |
| GPU memory < 512 MiB | vale | misses by 27.8 MiB (539.8 peak) | Here 606.4 (Part G's walk: G1's colour volume +9.4 MB among it). |
| | floor | misses by 223.6 MiB (735.6) | Its built things' realizations (C4). Here 743.7. |
| Cold start < 5 s | vale | misses by 1.69 s (6.69) | Compile-bound at the GPU process's pace (C5). Here 10.4 s. |
| | floor | misses by 8.33 s (13.33) | Its creature fields' realizer pipelines. Here 36.9 s. |
| Warm start < 3 s | vale | **holds** (2.21) | Here 7.5 s. |
| | floor | misses by 2.38 s (5.38) | Load-bound: its frames 3-102. Here 14.6 s. |
| Lower tiers measured, each under the one above | vale, floor | held at C7, before Parts D–H; to measure again | Epic > High > Medium > Low on both at C7; Part H changed no tier, Parts D–G changed what each draws. |
| Fidelity (G-reference: ≥ 50 % of 96, no pair worse than 6–2, ≥ 6 pairs won or equal) | vale | misses: 0 % of 96 (50 % needed), every pair lost 8–0, 0 of 12 pairs won or equal | The stills are the same on any device; H2's panel ("Public visual references"). |
| Every plant grown (G-grown) | vale | **holds** on this machine; blocked: the M4's Chrome run | 26,156 instances, 0 authored; the growth's hashes the same in two native runs and Chrome here ("H2"). |
| Every creature grown from its genome | vale | **holds** (Part F) | The grazer grown (`kit::roe`): its proportions, muscles and coat from its genome, its body plan and face authored. |
| Its motion (G-motion, ≥ 40 %) | vale | blocked: 0 % in four rounds (F9) | |
| Steerable (G-steer, IoU ≥ 0.85) | vale | **holds** for the oak (0.874) and the beech (0.880); blocked: the birch (0.689) | E6. |
| Four styles, no change to `engine/` | vale | **holds** (D10, again at H2) | `wrela tool styles -- --check`: `Cel`, `Painterly`, `Ink` and the two fitted looks each added in a commit that changes no engine file; `Physical` the engine's own. Coherence: `Cel` holds its 60 bars, `Painterly` 59, `Ink` Blocked ("D4–D7b"). |
| Size (the runtime ≤ 1 MB, program and shaders ≤ 6 MB gzipped, cold load ≤ 8 MiB) | vale, floor | **holds** | 26 KB; the vale 3.23 MB, the floor 5.59 MB; their cold loads 4.0 and 6.0 MiB ("H2"). |
| Other devices | all | blocked: deferred to a later plan (D37) | |

**The claim** (D27): **it doesn't hold, on the one device it names.** On the M4 Air the vale misses
every frame-time gate (its GPU median 18.37 ms against 12, its p99 31.48 against 20, 6 frames
over 33 ms, 42.9 frames begun a second against 58.8), its memory by 27.8 MiB and its cold start by
1.69 s, and holds its warm start; the floor misses every one. Its fidelity misses
on every judge's ballot: at H2's final panel no judge preferred a still or rated one equal on any of the twelve pairs (0 of 96), the same as at B5. What it claims of content holds: every plant in the vale is
grown (none authored) and its growth hashes the same across hosts here; its creature is grown
from a genome; it renders through four looks with no change to `engine/`; its runtime, program
and shaders fit their sizes. The tiers below `Epic` were measured and ordered at C7. Every M4 number here
is C's, from before Parts D–H, and is to measure again on that machine: until then each M4 cell
is a historical one, not the plan's final measure; no other device has a row (D37). Plan three
isn't complete in the sense of its claim: D27 doesn't hold, and its Part G closed Blocked without
building G2 (the water's reflections), G5.1 (virtual geometry from fields) or G6 (the fall), which
the final panel's menu names. Plan four (`docs/plan-world-class-4.md`) opens with them.

## The hypotheses (docs/plan-world-class-3.md §7)

Each ships only if its own gate says it pays; each gets its outcome here when it's decided (§H3).

| ID | Hypothesis | Decided at | Outcome |
|---|---|---|---|
| H1 | Specialization alone (C1) recovers at least 2 ms of the clearing's GPU frame and 3 ms of the floor's | C1, C6 | not held: compiling the switches out moved nothing (the clearing 12.39 → 12.68 ms GPU p50; its fog and distance volume were off at run time already: "C1, every switch a constant") |
| H2 | Most of the visual-review gain comes from the grown floor (E3–E5), not from lighting | E8's panel against D11's | not held: there was no gain to attribute. E8 lost every pair 0 / 0 / 8 as D11 did, and no row moved by more than 2.5 (artifacts 26.62 → 29.12, value 14.12 → 12.25, light 9.50 → 8.75: "the third panel (E8)") |
| H3 | Mark coherence (D7) is the hardest problem in Part D and the bars will be held by `Cel` and `Ink` before `Painterly` | D7's table | half held: mark coherence was the hardest problem (Ink is Blocked on it), but `Painterly` holds 59 of 60 bars and `Ink` none of its flicker bars ("D4–D7b, the looks' coherence") |
| H4 | The learned style beats hand-tuned `Painterly` on flicker and pop at equal preference | D9 | withdrawn (D41): the learned style is deferred until Ryan rules on docs/citations.md's rule |
| H5 | Growth is byte-identical across devices on the CPU without fixed-point arithmetic | E7's hashes | held on this machine: two native runs and Chrome gave the same archetypes' and first ring's hashes at E7 and again at Part H's head ("Part H, the final gates"); the M4's Chrome, the third device, is to re-measure |
| H6 | Motion, not rendering, decides the creature shots (9, 10) | F10's panel against E8's | not held: neither decided them. Both creature pairs stayed lost 0 / 0 / 8 at F10, and the judges faulted the leaf cards, the root plates, the smeared ground and the empty slope, not the creature's motion or its rendering ("the fourth panel (F10)"); G-motion stayed Blocked at 0 % ("F9") |
| H7 | Bounce colour (G1) moves the "light" row more than any other single item | G1's panel | not held: on the development panel G1 moved the light row's losses 7.50 → 7.62 (G3's to G1's); the most any item moved it was G7's 9.00 → 7.75, within the panel's run-to-run spread, and G7 later left the vale ("G1, bounce light"; "Part G closes, again") |
| H8 | The vale needs no more than three of Part G's items to reach G-reference | G's tallies | not held: eight items (G7, G4, G3, G1, G8, G9 and G10's four parts) left the development panel at 0 of 8 pairs (losses 51.01 → 49.88) and the sealed twelve at 0 of 12 ("Part G closes, again: Blocked") |

## D1, the substance (2026-10-30, toolchain 0.11.0)

docs/plan-world-class-3.md §D1. What the substance costs the vale's walk, in the measuring looks
(`content/styles/fixtures.wrela`: physical light, the substance written; `Every` all of it,
`Formed` the form's, `Caused` the cause's, `Toned` the tone's).

Sustained (`wrela tool bench -- --scene vale-walk [--look styles::fixtures::<look>] --sustained
300 --no-docs --no-native`, back to back, M4 Air), GPU span p50 / interval p99 / over 33 ms:
`Physical` 17.03 / 28.77 / 3 and 16.70 / 30.32 / 12 first; `Every` 18.34 / 32.99 / 14; `Formed`
19.84 / 32.85 / 10; `Caused` 19.07 / 32.29 / 11; `Toned` 19.15 / 32.50 / 9; `Physical` again 18.90
/ 31.69 / 5. The machine warmed 2 ms across them, more than any channel costs, so each look's
passes were timed alone too (`wrela profile examples/vale --chrome --serial --size 1920x1080
--warmup 960 --frames 300`, the vale's `LOOK` pointed at each), ms a frame:

| Pass | Physical | Every | Formed | Caused | Toned |
|---|---|---|---|---|---|
| vegetation | 9.48 | 10.75 | 11.07 | 10.03 | 10.42 |
| vegetation prepass | 9.36 | 8.85 | 9.23 | 9.36 | 9.89 |
| terrain | 4.60 | 5.33 | 5.45 | 4.88 | 4.93 |
| water | 4.41 | 4.68 | 4.79 | 4.64 | 4.88 |
| temporal (no substance) | 3.32 | 3.19 | 3.32 | 3.40 | 3.60 |

With the passes that write none as the control (they drift up to 8 %), writing the five targets
cost 0.2–0.8 ms and the form about 2.8 ms, most of it its shape found in every fragment
(`dpdx` of its point and its form's normal). That's over the plan's 1.0 ms, so its lever was
taken: **the shape at half the scene's size**, found after the passes from the form's target
and the depth (`substance::shaped`), which leaves the shading passes four targets (24 bytes a
sample). Again, interleaved:

| Pass | Physical | Formed | Every | Physical |
|---|---|---|---|---|
| vegetation | 9.70 | 9.57 | 9.95 | 9.71 |
| terrain | 4.58 | 4.93 | 5.13 | 4.73 |
| water | 4.49 | 4.47 | 4.62 | 4.52 |
| shape | | 0.26 | 0.24 | |
| temporal (no substance) | 3.54 | 3.16 | 3.36 | 3.39 |

- **The form: about 0.4 ms** (the shape's 0.26, and the ground's coarser normals' 0.3), under the
  plan's 1.0. D4 measures it again with `Cel`'s mask.
- **The whole substance (`Every`): about 1.1 ms** over the physical look.
- `wrela pipelines engine/tests/frame --json [--set LOOK=styles::fixtures::Every]`: the ground's
  shading's `attachment_bytes` 8 for `Physical` (the most of its 58 pipelines 8), 24 for `Every`.

## D7a, the marks (2026-10-30, toolchain 0.11.0)

docs/plan-world-class-3.md §D7a. The physical look's coherence on the eight development shots,
the baseline the looks' bars are relative to (`wrela tool lookbook -- build/d7a/physical --package
examples/vale --quality --shots dev`, 1920 × 1080, M4 Air, D7a's code):

| Shot | PSNR dB | flicker ×10⁻⁶ | pop | trail px | bar: flicker ×10⁻⁶ / pop / trail |
|---|---|---|---|---|---|
| 12-D1-the-forest-floor | 20.55 | 48.11 | 0.0244 | 77 | 72.17 / 0.0366 / 78 |
| 13-D2-the-canopy-against-the-sun | 45.78 | 0.37 | 0.0012 | 3 | 0.56 / 0.0018 / 4 |
| 14-D3-another-old-oak | 23.56 | 137.85 | 0.1044 | 28 | 206.78 / 0.1566 / 29 |
| 15-D4-the-stream | 20.49 | 31.47 | 0.0093 | 22 | 47.21 / 0.0140 / 23 |
| 16-D5-the-shore-at-evening | 25.24 | 118.44 | 0.0072 | 4 | 177.66 / 0.0108 / 5 |
| 17-D6-the-vale-from-the-west | 36.70 | 14.93 | 0.0035 | 2 | 22.40 / 0.0053 / 3 |
| 18-D7-the-creature-in-the-wood | 23.48 | 16.94 | 0.0309 | 13 | 25.41 / 0.0464 / 14 |
| 19-D8-the-wood-under-cloud | 27.98 | 36.44 | 0.0235 | 5 | 54.66 / 0.0353 / 6 |

The bars (**proposed**): flicker and pop 1.5 times the physical look's, trail its plus a pixel
(`lookbook --bars`, `main::holds`).

The record above is superseded as the bars' baseline by the fast instrument's (below): the
looks are held to the physical look's fast run, which measures over shorter windows.

## D4–D7b, the looks' coherence (2026-10-30, toolchain 0.11.1)

docs/plan-world-class-3.md §D4–D7b. Each look on all twenty shots on the fast instrument
(docs/lookdev.md, "The fast run": 960 × 540, the still's flicker over 32 frames, pop over a
150-frame dolly, trail where it ends), held to the physical look's: flicker and pop at most 1.5
times its, trail at most its plus a pixel. Each measure is the median of three runs, but where a
shot's first run decided it (`--near 0.2`: a measure a fifth over its bar, or every one a fifth
under; docs/lookdev.md, "A record measures only what can decide it"). M4 Air, nothing else
running, the steady planner (`cook::Planner::steady`) while time is frozen; measured after Part
D's council and review (marks before the lines, on the surface's tangent plane, a look's lattice
marks off its characters; the trail's row threshold capped at 0.08).

```sh
wrela tool lookbook -- build/record/physical --package examples/vale --fast --runs 3   # 561 s
wrela tool lookbook -- build/record/cel --package examples/vale --fast --runs 3 --look styles::cel::Cel --bars build/record/physical   # 597 s
wrela tool lookbook -- build/record/painterly --package examples/vale --fast --runs 3 --near 0.2 --look styles::painterly::Painterly --bars build/record/physical   # 554 s, 1 shots once
wrela tool lookbook -- build/record/ink --package examples/vale --fast --runs 3 --look styles::ink::Ink --bars build/record/physical   # 534 s
```

Flicker ×10⁻⁶ / pop / trail px; ✗ a miss of its bar.

| Shot | Physical (the bars' base) | Cel | Painterly | Ink |
|---|---|---|---|---|
| 00-the-forest-floor | 20.97 / 0.0122 / 2 | 21.93 / 0.0091 / 3 | 21.18 / 0.0099 / 2 | 286.52 ✗ / 0.0368 ✗ / 2 |
| 01-the-canopy-against-the-sun | 6.65 / 0.0113 / 3 | 7.58 / 0.0075 / 3 | 6.88 / 0.0089 / 3 | 95.99 ✗ / 0.0417 ✗ / 4 |
| 02-an-old-oak | 1.09 / 0.1112 / 4 | 1.46 / 0.1085 / 4 | 1.15 / 0.1089 / 4 | 111.49 ✗ / 0.1602 / 4 |
| 03-the-fall | 11.82 / 0.0062 / 5 | 12.29 / 0.0062 / 4 | 11.48 / 0.0063 / 4 | 220.68 ✗ / 0.0465 ✗ / 3 |
| 04-the-bands-face | 1.07 / 0.0124 / 3 | 1.34 / 0.0122 / 3 | 1.20 / 0.0109 / 3 | 242.34 ✗ / 0.0303 ✗ / 2 |
| 05-the-meadow-at-evening | 58.29 / 0.0064 / 2 | 39.25 / 0.0054 / 2 | 60.77 / 0.0065 / 2 | 398.24 ✗ / 0.0154 ✗ / 3 |
| 06-dawn-in-the-hollow | 11.29 / 0.0038 / 3 | 11.00 / 0.0033 / 3 | 11.31 / 0.0036 / 3 | 198.03 ✗ / 0.0168 ✗ / 3 |
| 07-dusk-from-the-crest | 9.52 / 0.0021 / 2 | 6.05 / 0.0015 / 2 | 9.24 / 0.0021 / 2 | 640.77 ✗ / 0.0193 ✗ / 3 |
| 08-grazing-at-the-edge | 18.45 / 0.0044 / 2 | 20.60 / 0.0032 / 2 | 16.88 / 0.0035 / 3 | 245.53 ✗ / 0.0150 ✗ / 3 |
| 09-down-the-trail | 54.80 / 0.0069 / 3 | 50.53 / 0.0063 / 3 | 60.69 / 0.0070 / 3 | 404.53 ✗ / 0.0241 ✗ / 2 |
| 10-the-fallen-beech | 44.54 / 0.0101 / 5 | 57.52 / 0.0101 / 5 | 45.35 / 0.0107 / 5 | 453.29 ✗ / 0.1037 ✗ / 4 |
| 11-the-wood-under-cloud | 93.32 / 0.0076 / 2 | 104.56 / 0.0078 / 2 | 90.69 / 0.0073 / 2 | 326.70 ✗ / 0.0190 ✗ / 3 |
| 12-D1-the-forest-floor | 19.05 / 0.0079 / 2 | 15.74 / 0.0071 / 3 | 19.17 / 0.0079 / 2 | 197.93 ✗ / 0.0231 ✗ / 2 |
| 13-D2-the-canopy-against-the-sun | 0.27 / 0.0007 / 2 | 0.37 / 0.0006 / 2 | 0.36 / 0.0007 / 2 | 20.54 ✗ / 0.0031 ✗ / 6 ✗ |
| 14-D3-another-old-oak | 6.39 / 0.0363 / 27 | 4.74 / 0.0151 / 0 | 6.91 / 0.0298 / 30 ✗ | 159.09 ✗ / 0.1105 ✗ / 3 |
| 15-D4-the-stream | 9.59 / 0.0073 / 3 | 10.25 / 0.0060 / 3 | 9.48 / 0.0065 / 3 | 234.63 ✗ / 0.0355 ✗ / 2 |
| 16-D5-the-shore-at-evening | 32.58 / 0.0053 / 2 | 19.13 / 0.0044 / 2 | 35.22 / 0.0053 / 2 | 466.92 ✗ / 0.0214 ✗ / 3 |
| 17-D6-the-vale-from-the-west | 15.69 / 0.0029 / 2 | 11.70 / 0.0024 / 2 | 15.38 / 0.0028 / 2 | 1235.03 ✗ / 0.0204 ✗ / 3 |
| 18-D7-the-creature-in-the-wood | 16.66 / 0.0119 / 3 | 14.87 / 0.0072 / 3 | 14.35 / 0.0090 / 3 | 203.28 ✗ / 0.0185 ✗ / 4 |
| 19-D8-the-wood-under-cloud | 31.11 / 0.0186 / 2 | 36.50 / 0.0130 / 2 | 30.63 / 0.0168 / 2 | 339.01 ✗ / 0.0434 ✗ / 3 |

- **Cel holds all 60 bars**, after two changes from the plan's constants: `FLAT_CLIP` 0 and
  `TERMINATOR` 0.2 (content/styles/README.md; the plan's 0.08, softened to 0.3 on the earlier
  instrument, missed an old oak's flicker at 0.3 on this one, 3.04 against a bar of 1.63).
- **Painterly holds 19 of 20 shots** (59 of 60 bars). It missed all twenty with the
  plan's constants; it holds after `FLAT_CLIP` 0, `FLAT` 0, the program's line darkness for the
  plan's faint 0.25, `STROKE_VARY` 0.04 and `SKY_SIMPLE` 0.7 (the canopy against the sun's
  flicker sat on its bar at 0.5 after the council's fixes: 0.42 against 0.41). Its miss is
  another old oak's trail, 30 against 28 (the physical look's 27). That trail rests on one row: on
  this shot only row 533 has a step of 0.1 in luma (a small dark thing at the frame's corner,
  under soft bark), so the median is that row's run (docs/lookdev.md, "A trail can rest on one
  row"). The form's normal (`NORMAL` 1) is what lengthens it: the fine normal gives 28, the coarse
  30; the stroke variation (0 or 0.02), the strokes themselves, and `FLAT_CLIP` 0.1 or 0.2 leave
  it at 30. The form's normal is the look, so the miss stands.
- **Ink: Blocked.** It misses every flicker bar, by 2.3 to 151 times its bar (median
  10), 19 of 20 pop bars, and holds 19 of 20 trails. Moving every mark before
  temporal AA (`marks::marks_on`) took its flicker down about 70 times, evaluating the lattice at
  the pixel's unjittered centre (`marks::centred`) about 1.5 times, and a wash in place of lines
  on leaves, grass and flowers about twice; a grey ink and a soft line edge about 1.1. Spacing
  (12 and 14 px), the light alone, the layers' thresholds, a coarser lattice and either sign of
  the centring moved nothing. What's left is the hatching's lines themselves: a line a pixel wide
  over a surface that moves under a sub-pixel jitter each frame. Its judge sees the same as moiré.

PSNR of each look's play against its own frozen-time reference (`--fast --psnr`, one run, dB;
the painterly and ink looks' after the fixes, the physical and cel looks' before them, which
changed neither's image but in rounding):

| Shot | Physical | Cel | Painterly | Ink |
|---|---|---|---|---|
| 00-the-forest-floor | 22.26 | 21.01 | 22.34 | 17.81 |
| 01-the-canopy-against-the-sun | 25.53 | 25.84 | 25.73 | 20.03 |
| 02-an-old-oak | 30.86 | 36.27 | 34.94 | 18.28 |
| 03-the-fall | 26.62 | 23.73 | 26.57 | 17.93 |
| 04-the-bands-face | 24.49 | 23.24 | 24.87 | 17.87 |
| 05-the-meadow-at-evening | 25.30 | 24.72 | 25.27 | 18.36 |
| 06-dawn-in-the-hollow | 28.73 | 28.65 | 28.70 | 19.06 |
| 07-dusk-from-the-crest | 36.58 | 37.05 | 36.52 | 18.46 |
| 08-grazing-at-the-edge | 24.34 | 25.23 | 25.10 | 20.18 |
| 09-down-the-trail | 26.16 | 26.35 | 26.10 | 17.22 |
| 10-the-fallen-beech | 27.32 | 25.61 | 27.45 | 16.21 |
| 11-the-wood-under-cloud | 29.41 | 29.19 | 29.53 | 19.20 |
| 12-D1-the-forest-floor | 20.83 | 19.52 | 20.70 | 17.83 |
| 13-D2-the-canopy-against-the-sun | 43.69 | 43.99 | 44.16 | 28.98 |
| 14-D3-another-old-oak | 28.01 | 29.54 | 29.38 | 17.41 |
| 15-D4-the-stream | 20.76 | 18.68 | 20.70 | 17.38 |
| 16-D5-the-shore-at-evening | 27.20 | 27.17 | 27.29 | 16.88 |
| 17-D6-the-vale-from-the-west | 35.54 | 35.95 | 35.51 | 18.93 |
| 18-D7-the-creature-in-the-wood | 23.52 | 24.23 | 23.66 | 20.69 |
| 19-D8-the-wood-under-cloud | 26.07 | 27.16 | 26.93 | 17.58 |

### The looks' judges (D4–D6)

One agent judge per look, in a fresh context, views its twelve evaluation stills at 1920 × 1080
(`wrela tool lookbook -- build/stills/<look> --package examples/vale --stills --shots evaluation
--look styles::<module>::<Type>`) under neutral names, after a calibration pair (a physical still
against its quarter-resolution copy, neutral names): every judge preferred the full still. Each
scores the five rubric rows 0–3 and the *style* row against six of the look's reference set
(§D8's, the Met's CC0 works under `$WRELA_REFERENCE_DIR/styles/<look>`; Ink has none:
"unfitted"). Means over the twelve:

| Look | Light | Colour | Value | Depth | Artifacts | Style |
|---|---|---|---|---|---|---|
| Cel | 1.67 | 1.58 | 1.67 | 1.50 | 1.00 | 0.67 |
| Painterly | 1.75 | 1.67 | 1.67 | 1.50 | 1.08 | 0.67 |
| Ink | 0.75 | 1.00 | 1.08 | 1.08 | 0.00 | unfitted |

- The judges' files are in `build/judges-d/`. The painterly and ink looks were judged again
  after the council's fixes (their images changed); the cel look's judge saw its stills before
  them (its marks only multiply, so the order of its passes changed its image by rounding).
- The first two judges (cel's and painterly's) were handed the wood under cloud's still black
  (docs/lookdev.md, "A still waits for its shot"); their scores were discarded and fresh judges
  given the remade stills.
- Both style judges say the look reads as a game render more than in its set's manner: the cel
  look's flat tones and outlines "gesture toward illustration", the dawn in the hollow its
  closest still to a cel look; the painterly look's foliage dabs nod to painting while its bark
  marks read as stamped texture.
- The ink judge sees moiré over every surface at 1080p: the 6-pixel hatching aliasing on a
  still, the cause of its flicker (Blocked).

## D11, the second panel (2026-10-30, toolchain 0.11.1)

docs/plan-world-class-3.md §D11. The vale's twelve evaluation stills (above) against the
references, B5's panel exactly: its seed (5), its prompt, eight agent judges in fresh contexts
for each look, the shots' and manifest's hashes asserted (`88c877da…`, `b78df49e…`).

```sh
wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR -- build/d11/<look> --a build/stills/<look> --set evaluation --seed 5
wrela tool studio/compare --write ws=. -- build/d11/<look> --tally
```

- **The physical control holds**: all eight judges passed both calibration pairs; every pair 0 /
  0 / 8, B5's tally pair for pair (within two preferences per pair: within none). Its rows'
  losses (artifacts 26.62, colour 16.25, value 14.12, depth 12.50, light 9.50) are B5's (26.75,
  15.88, 14.38, 13.62, 12.50) within 1.2 on four rows; light's is 3 less, the fallen beech and the
  bands' face each 0.5 higher on light than at B5 (1.38 → 1.88).
- **The cel panel** (a record, not a gate): eight judges passed calibration; every pair 0 / 0 /
  8. Its losses: artifacts 23.12, colour 15.62, value 13.62, depth 12.75, light 8.12, each at or
  under the physical look's. Seven judges wrote the *style* sentence on every pair (c6 none): the
  dawn in the hollow is every one's clearest cel read ("clean dark outlines round the trunks and
  logs, flat fills and haze in soft bands"); the tallies are `build/d11/*/tally.md`. Both panels'
  stills predate Part D's council and review: neither look's image changed but in rounding (the
  cel look's marks only multiply, so drawing them before the lines is the same image).

## D8, the fits (2026-10-30, toolchain 0.11.1)

docs/plan-world-class-3.md §D8. Each look fitted on D5 (the shore at evening, a development
shot) to its set of fourteen CC0 works from the Met's Open Access (docs/citations.md, "The fit's
reference sets"): `cel` Hiroshige's and Hokusai's landscape prints, `painterly` Sisley, Pissarro,
Daubigny and Corot. 200 evaluations of CMA-ES from seed 1 (196 run: λ 7; a draw past a range's
end reflected into it), each still 480 px wide after 120 frames settled; the score is the RMS of
the still's statistics' z-scores against the set's (lower is nearer).

```sh
wrela tool studio/fit --write ws=. --write out=build/fit --read refs=$WRELA_REFERENCE_DIR/styles -- --style cel --package examples/vale --shot D5 --reference-set refs/cel --out content/styles/cel_fitted.wrela
wrela tool studio/fit --write ws=. --write out=build/fit --read refs=$WRELA_REFERENCE_DIR/styles -- --style painterly --package examples/vale --shot D5 --reference-set refs/painterly --out content/styles/painterly_fitted.wrela
```

`Cel`:

```text
fit: the look's own constants score 4.4904 (TERMINATOR 0.2000, PALETTE_N 4.0000, HUE_PULL 0.3000)
fit: 20 evaluations, the best 4.4903 at TERMINATOR 0.1917, PALETTE_N 3.7932, HUE_PULL 0.2963
fit: 40 evaluations, the best 4.4243 at TERMINATOR 0.1906, PALETTE_N 3.9824, HUE_PULL 0.1077
fit: 60 evaluations, the best 4.4243 at TERMINATOR 0.1906, PALETTE_N 3.9824, HUE_PULL 0.1077
fit: 80 evaluations, the best 4.4089 at TERMINATOR 0.1998, PALETTE_N 3.9205, HUE_PULL 0.0571
fit: 100 evaluations, the best 4.4089 at TERMINATOR 0.1998, PALETTE_N 3.9205, HUE_PULL 0.0571
fit: 120 evaluations, the best 4.4089 at TERMINATOR 0.1998, PALETTE_N 3.9205, HUE_PULL 0.0571
fit: 140 evaluations, the best 4.4089 at TERMINATOR 0.1998, PALETTE_N 3.9205, HUE_PULL 0.0571
fit: 160 evaluations, the best 4.4089 at TERMINATOR 0.1998, PALETTE_N 3.9205, HUE_PULL 0.0571
fit: 180 evaluations, the best 4.4089 at TERMINATOR 0.1998, PALETTE_N 3.9205, HUE_PULL 0.0571
fit: the best 4.4089 at TERMINATOR 0.1998, PALETTE_N 3.9205, HUE_PULL 0.0571
fit: held out, shot D1: the look's own 3.3950, fitted 3.3510
fit: held out, shot D3: the look's own 4.6330, fitted 4.6279
```

`Painterly`:

```text
fit: the look's own constants score 1.3811 (PALETTE_N 8.0000, HUE_PULL 0.1500, SKY_SIMPLE 0.7000)
fit: 20 evaluations, the best 1.3602 at PALETTE_N 10.2619, HUE_PULL 0.2675, SKY_SIMPLE 0.5440
fit: 40 evaluations, the best 1.3321 at PALETTE_N 11.7733, HUE_PULL 0.3158, SKY_SIMPLE 0.5067
fit: 60 evaluations, the best 1.3321 at PALETTE_N 11.7733, HUE_PULL 0.3158, SKY_SIMPLE 0.5067
fit: 80 evaluations, the best 1.3321 at PALETTE_N 11.7733, HUE_PULL 0.3158, SKY_SIMPLE 0.5067
fit: 100 evaluations, the best 1.3313 at PALETTE_N 10.5453, HUE_PULL 0.2753, SKY_SIMPLE 0.5499
fit: 120 evaluations, the best 1.3313 at PALETTE_N 10.5453, HUE_PULL 0.2753, SKY_SIMPLE 0.5499
fit: 140 evaluations, the best 1.3313 at PALETTE_N 10.5453, HUE_PULL 0.2753, SKY_SIMPLE 0.5499
fit: 160 evaluations, the best 1.3313 at PALETTE_N 10.5453, HUE_PULL 0.2753, SKY_SIMPLE 0.5499
fit: 180 evaluations, the best 1.3313 at PALETTE_N 10.5453, HUE_PULL 0.2753, SKY_SIMPLE 0.5499
fit: the best 1.3313 at PALETTE_N 10.5453, HUE_PULL 0.2753, SKY_SIMPLE 0.5499
fit: held out, shot D1: the look's own 0.9581, fitted 0.9705
fit: held out, shot D3: the look's own 1.5890, fitted 1.6265
```

**Each range stops where the look holds its bars.** Fitted first over the plan's ranges, the cel
look's ramp went to 0.589 (4.49 → 3.94 on D5, D1 3.39 → 3.27, D3 4.50 → 3.83) and an old oak's
flicker missed its bar, 5.77 ×10⁻⁶ against 1.64 (0.3 had missed it too); the painterly look's
sky went to 0.046 simplified and the canopy against the sun's flicker missed, 0.45 against 0.42.
Capping its tones at 8 instead left the sky at 0.015 and missed by more (0.56); with the rest as
fitted, the sky at 0.5 holds it (0.36), while the hue pull at the look's own 0.15 gives 0.44 and
the tones at 8 give 0.54. So `TERMINATOR` is at most 0.2 and `SKY_SIMPLE` at least 0.5.

- **The cel fit** takes the ramp to its cap, four tones and hardly any hue pull: a print's
  broad, unpulled flats. A little nearer its set on D5 and on both held-out shots; most of what
  the uncapped fit gained was its softer ramp.
- **The painterly fit** takes more tones and a harder hue pull, its sky near its floor. A little
  nearer on D5 and farther on the held-out shots: it fits D5, not the set's manner.

**Their G-look**: coherence as the looks' (`--fast --runs 3 --near 0.2 --bars
build/record/physical` for the fitted painterly look, every run for the fitted cel), flicker
×10⁻⁶ / pop / trail px:

| Shot | CelFitted | PainterlyFitted |
|---|---|---|
| 00-the-forest-floor | 21.44 / 0.0090 / 3 | 21.64 / 0.0098 / 2 |
| 01-the-canopy-against-the-sun | 7.07 / 0.0074 / 3 | 7.09 / 0.0089 / 3 |
| 02-an-old-oak | 1.48 / 0.1074 / 4 | 1.50 / 0.1088 / 4 |
| 03-the-fall | 12.23 / 0.0062 / 4 | 11.78 / 0.0063 / 5 |
| 04-the-bands-face | 1.31 / 0.0092 / 3 | 1.20 / 0.0080 / 3 |
| 05-the-meadow-at-evening | 41.60 / 0.0053 / 2 | 62.45 / 0.0064 / 2 |
| 06-dawn-in-the-hollow | 10.94 / 0.0038 / 3 | 11.38 / 0.0036 / 3 |
| 07-dusk-from-the-crest | 5.79 / 0.0014 / 2 | 9.18 / 0.0021 / 2 |
| 08-grazing-at-the-edge | 20.33 / 0.0032 / 2 | 17.03 / 0.0035 / 2 |
| 09-down-the-trail | 50.99 / 0.0063 / 3 | 62.02 / 0.0070 / 3 |
| 10-the-fallen-beech | 57.30 / 0.0098 / 5 | 71.54 ✗ / 0.0079 / 5 |
| 11-the-wood-under-cloud | 102.45 / 0.0079 / 2 | 90.95 / 0.0073 / 2 |
| 12-D1-the-forest-floor | 15.33 / 0.0073 / 3 | 19.42 / 0.0077 / 2 |
| 13-D2-the-canopy-against-the-sun | 0.36 / 0.0006 / 2 | 0.37 / 0.0007 / 2 |
| 14-D3-another-old-oak | 4.56 / 0.0148 / 0 | 6.21 / 0.0294 / 30 ✗ |
| 15-D4-the-stream | 9.88 / 0.0060 / 3 | 9.56 / 0.0063 / 3 |
| 16-D5-the-shore-at-evening | 19.11 / 0.0047 / 2 | 33.31 / 0.0054 / 2 |
| 17-D6-the-vale-from-the-west | 11.55 / 0.0024 / 2 | 15.36 / 0.0028 / 2 |
| 18-D7-the-creature-in-the-wood | 14.66 / 0.0072 / 3 | 14.58 / 0.0092 / 3 |
| 19-D8-the-wood-under-cloud | 38.66 / 0.0124 / 2 | 30.49 / 0.0163 / 2 |

- `CelFitted` holds 20 of 20 shots.
- `PainterlyFitted` holds 18 of 20: the painterly look's own miss (another old oak's
  trail), and the fallen beech's flicker, 71.54 against 66.82, where two of its three runs fell
  in the shot's late state. That shot's still sometimes sees something arrive in its 32 frames
  (its still's pop 0.008 against 0.002; the physical look's own runs read 73.59, 44.48, 44.54):
  run five times, the fitted look's reads 45.31, 46.61, 45.31, 84.15, 46.39, a median of 46.39,
  holding (`build/record/painterly_fitted-shot10-runs5`). The record keeps the median of three.
- Their judges (as D4–D6's: light, colour, value, depth, artifacts, style against the set):
  `CelFitted` 1.67, 1.58, 1.67, 1.58, 0.92, 0.58; `PainterlyFitted` 1.75, 1.75, 1.58, 1.58, 1.00, 0.75.

## D4, the cel look's cost (2026-10-30, toolchain 0.11.1)

docs/plan-world-class-3.md §D4 (5): the flattening's and the creases' cost. Each look's passes
timed alone (`wrela profile examples/vale --serial --size 1920x1080 --warmup 960 --frames 300`,
native, M4 Air, back to back, the vale's `LOOK` pointed at `styles::physical::Physical`,
`styles::cel::Cel` and `styles::fixtures::Unflattened`, the cel look with `FLAT` 0), ms a
frame, median:

| Pass | Physical | Cel | Cel unflattened |
|---|---|---|---|
| vegetation prepass | 8.866 | 8.763 | 8.845 |
| vegetation | 3.659 | 3.683 | 3.680 |
| terrain | 1.753 | 2.041 | 2.036 |
| water | 1.833 | 1.837 | 1.844 |
| temporal | 1.404 | 1.388 | 1.390 |
| marks (creases) | | 0.722 | 0.723 |
| shape | | 0.092 | 0.093 |
| sky | 0.103 | 0.216 | 0.218 |
| look (the dabs' map, the metering) | 1.103 | 0.054 | 0.055 |
| every pass | 22.407 | 22.481 | 22.555 |

- **The flattening: nothing measurable.** The cel look with it and without differ by under
  0.01 ms on every pass that flattens (terrain, vegetation), inside the passes' run-to-run spread.
- **The creases: 0.72 ms**, the marks pass (where every mark is drawn now, before temporal AA),
  and the shape the substance finds them from 0.09. Together under the plan's 1.0 ms.
- **Sustained** (`wrela tool bench -- --scene vale-walk --look styles::<module>::<Type> --sustained
  300 --no-docs --no-native`, back to back: physical, cel, physical; GPU span p50 / interval p99
  / over 33 ms): 19.34 / 30.79 / 2, **18.57 / 30.97 / 1**, 18.88 / 30.78 / 3. The cel look sits
  inside the physical look's own spread (0.46 ms between its two runs), as the per-pass sums say.
- The cel look's frame is within 0.1 ms of the physical look's: its substance (terrain +0.29,
  the sky's +0.11) and its marks (0.81) are paid for by the dabs it doesn't draw (`marks::DABS`,
  the physical look's paper: their map is 1.05 ms of the "look" span).

## D10, G-style (2026-10-30, toolchain 0.11.1)

docs/plan-world-class-3.md §D10. `wrela tool styles -- --check` (13.5 s, the builds cached):

```text
styles: styles::cel::Cel: builds, 109 pipelines
styles: styles::cel_fitted::CelFitted: builds, 109 pipelines
styles: styles::ink::Ink: builds, 109 pipelines
styles: styles::painterly::Painterly: builds, 110 pipelines
styles: styles::painterly_fitted::PainterlyFitted: builds, 110 pipelines
styles: styles::physical::Physical: builds, 109 pipelines
G-style: cel_fitted.wrela added in ee285c32: no engine file changed, holds
G-style: painterly_fitted.wrela added in ee285c32: no engine file changed, holds
G-style: ink.wrela added in 7269c195: no engine file changed, holds
G-style: painterly.wrela added in 525ef4c9: no engine file changed, holds
G-style: cel.wrela added in ca931cda: no engine file changed, holds
G-style: physical.wrela added in caa7c376: the engine's own look
```

The history is recorded in `studio/styles/history.log` (`--record`), which the tool's test reads:
3 passed. The page (`--page build/styles-page --run physical=build/record/physical --run
cel=build/record/cel ...`, every look's fast record) is linked from Part D's PR.

The released toolchain 0.11.1 (the workspace ran a local copy of its build until now) builds the
vale identically: all 335 of its shaders, wasm and manifest files' SHA-256 the same.

## 2026-10-30: the capybara game's nine levels (toolchain 0.10.3)

Linux, AMD Radeon Graphics (RADV RENOIR, integrated), Mesa 26.1.6; headless Chrome stable driven by
`wrela tool bench`. Each side measured alone, nothing else building or on the GPU. The branch is
`capybara/finish-the-game` (nine levels, a backdrop pipeline per habitat, meshes built at load);
`main` is caa7c376. M4 Air: not measured (no M4 to hand here; Ryan's run).

**Starts** (`wrela tool bench -- --scene capybara-run --starts 3 --no-docs --no-native`, median of
3; the program's `playable` print, which the branch adds and `main` lacks, so `main`'s is its
first frame):

| | Cold | Warm | Every pipeline built, cold | Pipelines |
|---|---|---|---|---|
| main | 300 ms (300, 307, 296) | 226 ms (226, 219, 229) | 1,829 to 1,845 ms | 10 |
| branch, first | 502 ms (587, 502, 485) | 401 ms (401, 401, 401) | | 17 |
| branch | 317 ms (336, 315, 317) | 227 ms (229, 219, 227) | 3,035 to 3,051 ms | 17 |

The first branch run made every level's tool-assisted runs before its first frame (the sim playing
37 runs); made for the course shown, as it's shown, the starts are `main`'s again. A cold start's
first frame on the screen waits for its pipelines: the sprite pipeline, built last, at about
3.0 s on the branch against 1.8 s on `main`: the seven habitats' strips no first frame needs are
compiled before it (docs/toolchain-gaps.md, "A cold start's pipelines compile at the GPU process's
own pace", and "A program's `playable` comes before its first frame is on the screen"). Warm,
every pipeline by 75 to 80 ms (`main`: 9 to 57).

**Download** (`wrela build --optimize size`): `game.wasm` 2,577,964 bytes, 804,118 gzipped
(`main`: 2,530,064 and 792,335): nine levels for 1.5% more, as a course's meshes are built when
it's shown and no longer baked (`main` baked 137 KB a course). The shaders: 17 pipelines,
774,168 bytes of WGSL, 187,192 gzipped (`main`: 10, 328,529, 75,989); the largest is the
sprites' at 97.4 KiB, each habitat's strips 52 to 80 KiB.

**Frames** (`wrela profile games/capybara --frames 900 --size 1920x1080 --workers 4`, natively, each
level's searched human any% run as its input). Not paired: `main` has one level. The runs are
scripted at 120 frames a second and `profile` steps 60, so each drifts from its route and may not
cover all its level; the numbers say what each level's view costs, not that every part was seen.

| Level | GPU span p50 / p95 | CPU call p50 / p95 |
|---|---|---|
| The Capybaras' Home | 1.875 / 3.419 ms | 3.229 / 4.781 ms |
| The Capybara Wetlands | 1.789 / 3.268 ms | 3.181 / 4.674 ms |
| The Capybara Wetlands, `main` (`scripts::LEVEL`) | 1.870 / 3.336 ms | 3.264 / 4.673 ms |
| The Savanna | 1.689 / 3.178 ms | 3.014 / 4.502 ms |
| The Rainforest House | 1.725 / 4.397 ms | 3.141 / 5.858 ms |
| The Aviary | 1.730 / 3.182 ms | 2.978 / 4.518 ms |
| The Reptile House | 1.698 / 2.572 ms | 3.054 / 4.130 ms |
| The Polar Coast | 1.738 / 2.534 ms | 2.773 / 3.838 ms |
| The Petting Farm | 1.756 / 2.684 ms | 2.967 / 4.058 ms |
| The Food Court | 1.681 / 2.281 ms | 2.794 / 3.618 ms |

The 95th percentiles are the frames that draw a backdrop strip again (the Rainforest's the
dearest); the scene is about 1.3 ms in each.

## 2026-10-30: the console's server, verifying an upload (toolchain 0.10.3)

How long the console's server takes to play an uploaded capybara run again, which sets
`platform::server::PLAY_TICKS` (a call has 50 ms, `wrela_host::server::Limits`). The console built
with `wrela build console` (release), served by `wrela-serve <build> --dev` on this machine (AMD
Ryzen 5 5500U, 12 threads, Linux), and driven over WebTransport by a script that signs up with a
real P-256 passkey and uploads two of the route search's human-possible runs as a player's
(kept outside the repo: it is Python, aioquic and `cryptography`).

| Run | Ticks | Calls of play (1,000 ticks each) | Upload to verified |
| --- | ---: | ---: | ---: |
| The Home Pond, human any% | 991 | 1 | 0.01 s |
| The Aviary, human 100% | 3,407 | 4 | 0.04 s |

So about 12 µs a tick, network round trips included: 1,000 ticks a call is about 12 ms here.
`PLAY_TICKS` is 400 (about 5 ms here), for a server core several times slower, since a call past
its deadline loses the room's instance and the run starts again.

## 2026-10-31: the console's server on Cloudflare's runtime (toolchain 0.13.0)

The same two runs uploaded to the console's server on toolchain 0.13.0's Cloudflare host, run by
`wrangler@4.151.0 dev` (workerd, the Workers runtime, with a local SQLite store) on the same
machine. The console was built with `wrela build console --set 'RP_ID="localhost"' --set
'ORIGIN="http://localhost:8787"'` and staged as the release stages it (docs/toolchain.md, "The
console's server"). A script outside the repo drives it over HTTP as two browsers would, with
their cookies (Python, `urllib` and `cryptography`).

| Run | Ticks | Calls of play (400 ticks each) | Upload request to its answer |
| --- | ---: | ---: | ---: |
| The Home Pond, human any% | 991 | 3 | 0.05 s |
| The Aviary, human 100% | 3,407 | 9 | 0.07 s |

The upload's request is answered once its run is verified, so its time includes the play, the
store's writes and the HTTP round trip. The same script checked, on that runtime: a sign-up and a
real ES256 sign-in (WebCrypto), a forged signature refused, a tampered run refused, the cookies'
`HttpOnly`, `Secure`, `SameSite=Strict` and `Path=/`, a board answered with `public, max-age=30`
and a replay with `public, max-age=86400`, the export, sign-out clearing the cookie, another
origin's `POST` refused (403), and a client past 60 changes a minute refused (429). Not measured:
CPU time per call on Cloudflare's own machines, which its dashboard gives after the first deploy.

## Zstandard against Brotli (2026-11-01, first deploy)

The gate for the zone's Compression Rule (docs/plan-online-play.md step 12; docs/toolchain.md,
"Delivery"): it stays only if each file's Zstandard bytes are within 5% of its Brotli bytes. The
files are the console's and the capybara's WASM modules and runtime scripts, as the deploy's
Zstandard check names them (studio/deploy `compressed`), on the live site after the first deploy
with the rule set, each fetched once with curl, the bytes on the wire as curl counts them (the
uncompressed column with `accept-encoding: identity`):

```bash
curl -s -o /dev/null -H 'accept-encoding: zstd' -w '%{size_download}\n' https://wrela.dev/b/<sha256>.wasm
curl -s -o /dev/null -H 'accept-encoding: br' -w '%{size_download}\n' https://wrela.dev/b/<sha256>.wasm
```

| File | Uncompressed bytes | Zstandard bytes | Brotli bytes | Zstandard / Brotli |
| --- | ---: | ---: | ---: | ---: |
| the console's WASM | 3,194,857 | 628,372 | 613,832 | 1.024 |
| the console's runtime (`main.js`, `worker.js`, `worklet.js`) | 244,134 | 85,472 | 84,099 | 1.016 |
| the capybara's WASM (session, play build) | 293,659 | 59,286 | 58,010 | 1.022 |
| the capybara's runtime | 244,134 | 85,455 | 84,048 | 1.017 |

Every file is within 5% of Brotli (the worst, the console's WASM, 2.4% larger), so the rule stays.
Each came back with `content-encoding: zstd` when asked with `accept-encoding: zstd`, as the
deploy's own check said of all 9 WASM modules and runtime scripts. Before the merge, main's
console WASM (e25ffdc0's release) measured 570,088 zstd against 554,602 Brotli bytes (1.028).

Release 34035cc91d12f6b0d2ff, main b3081e38 (#351), toolchain 0.18.0; the console at
`https://wrela.dev/b/b72e8191….wasm`, the capybara at `https://capybara.wrela.dev/b/844d61f4….wasm`,
the runtime set `/r/574fd788…/` on both.

## Early Hints (2026-11-01, first deploy)

Whether the zone's Early Hints (docs/plan-online-play.md step 13; docs/toolchain.md, "Delivery")
starts the WASM sooner: each bootstrap page's preloads are `Link` headers in `_headers`, which
Cloudflare sends as a 103 before the page. Checks first: `curl -sv --http2 https://wrela.dev/ -o
/dev/null` shows a `103` before the `200`, and in Chrome the WASM's
`performance.getEntriesByType("resource")` entry has `initiatorType` `early-hints`. Then the
WASM's `requestStart` (ms from the navigation's start, the entry's own clock), on a cold load
(a fresh profile, so nothing is cached), the median of 5, before the zone's Early Hints are on
and after:

| Page | Machine, browser | Before: `requestStart` (ms) | After: `requestStart` (ms) | After: `initiatorType` |
| --- | --- | ---: | ---: | --- |
| the console, `https://wrela.dev/` | not measured | not measured | not measured | not measured |
| the capybara's session, framed by the console | not measured | not measured | not measured | not measured |

The check passes: after a first request that let Cloudflare keep the page's `Link` header,
`curl -sv --http2` gets `HTTP/2 103` then `HTTP/2 200` for `https://wrela.dev/` (5 of 5) and for
the capybara's session page (3 of 3). The timings weren't measured: the zone's Early Hints were
on before the release that carries the `Link` headers deployed, so there is no "before" on the
same release, and the desktop app's browser pane, the one browser at hand, reported the console's
WASM with `initiatorType` `link` (`requestStart` 148 ms), not `early-hints`. Whether the hint
starts the WASM sooner in Chrome is still to measure, with a fresh profile, as above.

Release 34035cc91d12f6b0d2ff, main b3081e38 (#351), toolchain 0.18.0.

## E2, the history's light and events in the bake (2026-10-31, toolchain 0.11.1)

The floor's (`games/last-green`) tiles and its first load (`wrela tool bench -- --scene
last-green-walk`, the cold run's bytes loaded by its first frame and playable) on `main`
(f1d94841) and as E2 changed them. The bake's `VERSION` is 6; a tree's base record stays 22 bytes
(`TREE_BYTES`), its life after the tile's records, as long as it is.

| Record | Tiles (65, raw) | Tiles (gzip -6) | Cold load (bytes) | Against `main` |
|---|---|---|---|---|
| `main`: 22 bytes a tree | 3,308,895 | 1,985,686 | 5,273,467 | |
| the plan's: 74 bytes a tree, 32 samples | 10,353,000 | 4,250,000 | | |
| the life as long as it is, 32 samples a byte each | 5,719,054 | 3,687,267 | 5,938,388 | +12.6 % |
| the lever: 8 late samples, each 4 bits, a 2-byte header | 4,702,022 | | 5,667,726 | **+7.5 %** |

The plan's lever (over 10 %: samples 16–31 halved to 8, the span kept) is taken, with the
samples kept at 16 levels: the nurse tree's early shade still parts its sapling's samples 0–6
(`a_nurse_trees_shade_is_in_a_saplings_early_light`), and every age of a 1,200-year history
reads back through the bake.

## E1.2, trees grown and converted (2026-10-30, toolchain 0.11.1)

`engine/tests/growth-trees` (eight tool tests, on the CPU; the specimens from
`content/trees/genomes.wrela` with seed 1). The conversion `into_plant` keeps a grown graph's
limbs largest section first, each a `Tube<4>` through two nodes with a point held just past the
corner between them (the renderer's tangents are no longer than their chords, so the short piece
holds the curve to the wood: three nodes a tube bowed off thin twigs, the pine at 40 years at
0.92 with every chain kept). The woody IoU draws the graph's nodes and the plant's tubes through
the same rasterizer (each a tube, sixteen capsules along `limb_axis`), 384 px on the longer side,
4 × 4 samples a pixel, by coverage. "Edge" is half the open's light every year.

| Species | Age | Light | Nodes | Chains | IoU at 2,048 limbs | IoU, every chain | Crown volume / leaves' |
|---|---|---|---|---|---|---|---|
| oak | 40 | open | 1,736 | 1,058 | 0.991 | 0.991 | 1.001 |
| oak | 40 | edge | 535 | 354 | 0.990 | 0.990 | 0.998 |
| oak | 100 | open | 5,799 | 3,338 | **0.913** | 0.996 | 0.993 |
| oak | 100 | edge | 1,000 | 643 | 0.992 | 0.992 | 1.005 |
| beech | 40 | open | 1,478 | 857 | 0.994 | 0.994 | 1.005 |
| beech | 40 | edge | 57 | 41 | 0.990 | 0.990 | 0.998 |
| beech | 100 | open | 5,073 | 2,841 | **0.940** | 0.997 | 0.996 |
| beech | 100 | edge | 38 | 29 | 0.993 | 0.993 | 0.999 |
| birch | 40 | open | 740 | 415 | 0.993 | 0.993 | 1.002 |
| birch | 40 | edge | 533 | 285 | 0.995 | 0.995 | 0.996 |
| birch | 100 | open | 2,119 | 1,140 | 0.996 | 0.996 | 0.987 |
| birch | 100 | edge | 1,726 | 921 | 0.998 | 0.998 | 1.000 |
| hawthorn | 40 | open | 3,530 | 2,107 | 0.991 | 0.996 | 1.001 |
| hawthorn | 40 | edge | 107 | 79 | 0.994 | 0.994 | 0.997 |
| hawthorn | 100 | open | 9,562 | 5,563 | **0.868** | 0.998 | 1.000 |
| hawthorn | 100 | edge | 20 | 15 | 0.995 | 0.995 | 0.997 |
| blackthorn | 40 | open | 2,930 | 1,779 | 0.997 | 0.997 | 1.002 |
| blackthorn | 40 | edge | 8 | 6 | 0.998 | 0.998 | 0.997 |
| blackthorn | 100 | open | 6,156 | 3,515 | **0.946** | 0.999 | 1.006 |
| blackthorn | 100 | edge | 4 | 3 | 1.000 | 1.000 | 0.997 |
| pine | 40 | open | 588 | 331 | 0.991 | 0.991 | 0.998 |
| pine | 40 | edge | 217 | 121 | 0.996 | 0.996 | 0.998 |
| pine | 100 | open | 2,615 | 1,436 | 0.997 | 0.997 | 1.014 |
| pine | 100 | edge | 1,570 | 854 | 0.998 | 0.998 | 0.988 |
| alder | 40 | open | 1,474 | 824 | 0.995 | 0.995 | 1.009 |
| alder | 40 | edge | 70 | 42 | 0.996 | 0.996 | 0.999 |
| alder | 100 | open | 4,276 | 2,316 | 0.988 | 0.998 | 0.997 |
| alder | 100 | edge | 1,044 | 578 | 0.999 | 0.999 | 0.999 |

**The conversion's IoU (0.97) is Blocked at `MAX_LIMBS` 2,048:** every chain kept, it is 0.990
or more for every specimen; the cap costs the veterans (the hawthorn at 100 years 0.868, the oak
0.913). Levers, in order: the conversion drawn through one rasterizer for both silhouettes (it
had compared one tapered capsule a node with the tubes' sixteen, so the ceiling read 0.94); runs
fitted with one tube while it stays within a quarter of each node's radius (chains −21 %, the
ceiling down to 0.95: dropped); two nodes a tube with the corner point (ceilings 0.990–0.999);
a bud's least resource 0.5 → 1 (a 100-year oak's chains 4,011 → 3,158; 1.2 starves the pine's
leader, 1.5 the shrubs); `MAX_LIMBS` 512 → 2,048 (4,096 overflows the constants that hold a
`Plant`: docs/toolchain-gaps.md). The test holds 0.86.

**In shade** (the specimen page, `wrela tool grow -- --specimens out/specimens`): a tree whose
resource, split by the Borchert-Honda rule, left no bud its least resource never grew again (an
oak at half the open's light stopped at 80 nodes at 20 years, its 47 buds each given about 0.9 of
the 1 they need); now such a tree grows its strongest buds, as many as a quarter of its resource
buys (`SLOW`), at least one. A branch that doesn't grow is shed after `IDLE_YEARS` ×
`shed_years` only while it's also dim (under twice `shed_light`): before, an oak at half light
grew 651 nodes by 40 years and collapsed to its seed by 100. Tried and dropped: a bud short of
its need shooting with chance its share squared (the hawthorn's chains 5,563 → 8,647), cubed
(7,521), or for terminal buds only (the pine's leader lost); the threshold scaled by `shed_light`
(the beech 3 nodes at a quarter's light).

Under a quarter of the open's light, at 100 years: the birch 338 nodes, the pine 20, the alder
703, the beech 15, the oak, the hawthorn and the blackthorn 3 (a pole). **A species' shade
tolerance isn't in the genome**: the beech, the most tolerant, does worse than the birch, a
pioneer, because the shade's outcome follows apical control (an oak's leader dies a year in ten
past its youth, and a slow tree can't replace it). Recorded for E6's fits (a `tolerance` to
scale the bud's need and the shedding is the lever).

**Cost** (`wrela tool grow -- --species oak --age 100 --out out/oak-100`, the tool's optimized
build on the M4 Air, nothing else running): a 100-year oak (5,799 nodes) grew in **727 ms** at
first, its year's time mostly the buds perceiving the space's markers (511 ms), then the markers'
grid (111 ms) and the shade grid (65 ms). Each marker's bud is now found from the marker, among
the buds on a grid of them (the free markers are a third of the 3,000 in a grown crown; each bud's
search walked 27 cells, most of them empty), the grid of markers holds only the free ones, and
distances are compared squared: **364 ms**, the same tree node for node. Halving the buds' grid's
cell gained nothing; 2,000 markers a year (290 ms) starves the pine's leader (test 1). The plan's
50 ms (**estimated**) isn't reached: E7's first ring sets `NEAR_N` from this. The conversion
takes 114 ms more.

The crown's leaves are now at every live tip (where the shade grid counts them), each tip's
foliage `leaf_sun × ∛leaf_density × 2` across (**proposed**): with the last two years' shoots
only, each a leaf-sized ball, a 100-year oak's crown was 70 small clumps over bare limbs.

## E1.3, the archetypes and the near individuals (2026-10-31, toolchain 0.11.1)

`wrela tool bench -- --scene vale-walk --no-docs` on the M4 Air, nothing else running, `main`
(f1d94841) against this branch: the vale's 16 living kinds grown at build (`trees::archetypes`),
its 24 trees nearest the eye grown as themselves (`land::stage::near_trees`).

| Build | GPU span p50 / p95 / p99 ms | Over 16.7 / 33 ms | Cold playable (frame) | Loaded MiB | GPU peak MiB |
|---|---|---|---|---|---|
| `main`: the kinds authored | 15.80 / 19.10 / 20.77 | 971 / 17 | 459 (6,429 ms) | 3.2 | 540.0 |
| grown, every limb drawn | 34.89 / 45.40 / 48.00 | 1498 / 1085 | | 3.8 | 588.6 |
| grown, limbs by level (`plant::level_limbs`) | **17.81 / 23.02 / 25.02** | 1424 / 4 | 531 (past the cold run's frames) | 3.8 | **579.5** |

What it took to run at all in Chrome: a near individual's growth returns its shape on the heap
(`growth::Shape`) and its plant is made on the main thread (a browser's helper stack holds no
130 KB `Plant`: the first run trapped); the growths start once no tile is coming (they held the
helpers the tiles' cooking needs: the vale never became playable); the probes' occluders take a
tree's 48 thickest limbs (`trees::kinds::OCCLUDER_LIMBS`: each grown tree's thousands of twigs
as capsules held the probes' bake for ever). A grown kind's limbs are drawn by level, its
thickest first: all at the cards' first level, a quarter at the second, a sixteenth at the third,
never fewer than 64 (each a tube draw, every limb doubled the frame). Its crown's foliage is a tip's
leaves over 72 cm (`FOLIAGE` 4: at 36 cm the grown crowns held a quarter to a tenth of the hand's
cards).

**Gates.** G-memory: the vale's GPU peak 579.5 MiB, **not met** (512; `main` misses it too,
540.0): the 39.5 MiB more is the room the 24 individuals hold (each the most cards of any kind,
28,528, and 2,048 limbs). G-profile: the GPU span's p50 2.0 ms over `main`'s, the walk's frames
over 16.7 ms 971 → 1,424. The cold load is 0.6 MiB more (the grown plants' bytes, `GROWN`), and
playable 72 frames later. E7 (the archetypes' bake and the individuals' cache, G-cold) and E8
(the vale regrown) take these up.

**The dolly's pop** (shot 8, `wrela tool lookbook -- <out> --package examples/vale --fast --runs 3
8`, medians of three, against the physical look's fast record on `main`, `build/record/physical`):

| Build | still_pop | pop | flicker ×10⁻⁶ |
|---|---|---|---|
| `main` | 0.00098 | 0.00437 | 18.4 |
| grown archetypes, no individuals | 0.00095 | 0.00483 | 20.0 |
| individuals, swapped in as they're grown | 0.00111 | 0.00542 | 52.3 |
| individuals, the first ring grown before playable | **0.00105** | 0.00515 | 26.6 |

The plan's measure (`still_pop` within 10 % of the kinds' hand-over) **holds: +7.1 %**. A tree's
swap from its archetype to itself pops where it's seen, so the frame now waits for the first ring
(`land::stage::near_ready`: the 6 nearest within 40 m, **proposed**) before it's playable, and
the rest swap in after, nearest first, four growing at a time. That wait is the growths' cost: in a
wood all 24 near trees stand within 40 m, and grown to their full ages (an ancient's 800 years)
they took 18 s; an individual is grown to 120 years at most (`archetypes::GROWN_AGE`,
**proposed**: its form is set by then), and the six are ready about 6 s after the tiles. E7's
budget for the first ring (1,000 ms) is the growth's cost to climb.

## E4 and the grown trees' first lookdev (2026-10-31, toolchain 0.11.1)

E4's small causes on the vale (`the_vales_small_causes_are_placed`, its tiles round its middle):
4,914 fallen trees and 20,848 bracket fungi on them, 74 % of their bark gone (by each one's rot,
`growth::rot_of`, `bare_share`); the litter 0.50 deep on average (`litter_at`); 6 % of the ground
in moss's causes (wet, under the canopy); 4,278 trees old enough for lichen (`lichen_of`).

**The first lit look at the grown trees** (`wrela tool lookbook -- <out> --package examples/vale
--fast 0 1 2 8 10 11`, against the physical look's record on `main`), and what each pass fixed:

1. Limbs as stacks of flat bandages: every tube's last ring closes to a point, and a grown limb
   was a chain of two-node tubes. Runs now go on while they turn under 20° (three nodes a tube on
   twigs, its four points through them; eight on wood 5 cm thick or more, resampled), and a run
   another carries on past its joint has its end carried 0.6 of its radius into the next.
2. Twigs as bands: an instance's thickness (its record's girth against its kind's trunk, to 3×)
   thickened every limb; it now fades from the trunk to the twigs.
3. A shot's eye inside a near tree's trunk, a forest oak a snake: grown boles wandered a metre.
   `growth::straighten` takes back 85 % of a bole's wander toward the line from its foot to its
   crown (its lean into its light kept), none from 60 % of its height up.
4. Green spirals round the trunks: the bark's moss read in the world's x + z against height; now
   in the bark's own place, one tile round the limb.
5. A fallen tree regrown standing on its log, and a veteran grown young (120 years) and
   stretched to its height: neither is a near individual now (`archetypes::grows_as_itself`).
6. A ledge round a trunk: a bole cut at a 20° bend, its carried end standing out of the next on
   the bend's outside. A thick bole is cut only at a turn over 45°.

The fallen trees are a grown trunk laid along +z (the kind's girth, its taper grown) with the
hand's root plate, and bracket fungi that show past their rot (`Leaves::after`); rotting wood is
wetter and shadier, so mossier. Still to look at: shot 11 (its first ring takes too long to grow
for the instrument's wait), the forest birch and pine (poles with tufts), and a low fork's
crease where two tubes meet.

## E6, steering: the fit and the goal (2026-10-31, toolchain 0.13.0)

Linux pop-os (AMD Ryzen, Radeon "Lucienne" integrated, 12 threads); **not the M4 Air**: the fits'
times are this machine's, and a fit is CPU work that runs on `std::par`'s helpers (8 in the test,
`@test(tool, workers: 8)`; a tool test runs 2 unless it says).

**The identity fits** (`wrela test studio/grow three_hidden`; three trees grown with hidden site,
seed and genome deltas, each fitted back from its silhouette alone; 400 evaluations each; best
IoU every 20):

| Hidden tree | IoU | Trajectory (evaluations: IoU) |
|---|---|---|
| oak, 90 years, light 0.7, wind 0.2, seed 7 | **0.874** | 20: 0.775, 40: 0.841, 100: 0.852, 160: 0.863, 320: 0.874 |
| beech, 140 years, open, calm, seed 3 | **0.880** | 20: 0.803, 40: 0.844, 100: 0.855, 160: 0.880 |
| birch, 45 years, light 0.5, wind 0.4, seed 11 | 0.689, **Blocked** | 20: 0.522, 40: 0.617, 220: 0.635, 320: 0.668, 400: 0.689 |

About 85 s a fit here (the whole test 4 m 25 s with its build). What moved it, in order:

| Lever | Oak | Beech | Birch | Note |
|---|---|---|---|---|
| The handoff's first version (one CMA-ES, 2 helpers, on the M4) | 0.775 | | | ~400 s a fit |
| 8 helpers; three phases (broad CMA-ES, the 16 seeds at the best, narrow CMA-ES with the seed held) | 0.780 | | | ~60 s a fit |
| **A tree's draws keyed by its lineage**, not by its genome's values, its site and the draws before (below) | 0.901 | 0.848 | | |
| No slope coordinate (the growth reads none); 16 candidates a generation | 0.898 | 0.836 | | the beech's fall is noise |
| **Low-discrepancy markers** (R3, randomly offset each year) in place of white noise | 0.859 | 0.872 | 0.411 | |
| The masks closed by 2 cells (both alike: a crown's mass, not the sky between its clumps) | 0.874 | 0.880 | 0.689 | |
| The mask at 64 cells a side instead of 96 | | | | no change: the realizations differ at the crown's lobes, not its twigs |

**Why it was stuck: the growth was chaotic, and its stream reseeded.** A grown tree's random
stream was keyed by the hash of its genome's values and its site (`Rng::of`), and its draws were
taken in sequence, so any steered value moved by any amount drew a different tree. Measured on
the hidden oak, each coordinate nudged ±0.03 (in the search's [0, 1]) gave IoU 0.59–0.78 against
the truth, the same as another seed (0.63–0.76): a landscape with no slope for CMA-ES to follow.
Now each shoot's draws come from its lineage's word (`Node::id`, from its parent's and whether it
goes on or branches), each year's space and leader's death from the seed and the year, and the
phyllotaxis' angle runs along each axis (`Node::phyllo`). The age, the leaf size and the crown's
reach became smooth (±0.03 → 0.83–0.94); the rest still jump, because the growth's own decisions
are thresholds on positions (a marker within `KILL` of a node, a bud's nearest marker, `BUD_NEEDS`,
the shed light): even ±0.003 (0.15 % of a value) grows another tree for most genome values. The
low-discrepancy markers lowered the noise those cascades scatter (other seeds at the hidden
beech's parameters: 0.775 → 0.80 mean).

**Blocked: the half-lit birch.** Its tree is 31 clumps on thin limbs (10 % of the mask); its own
other seeds score 0.20–0.33 against it raw, 0.38–0.62 closed by 2 cells, 0.45–0.71 by 4. No
search can reliably beat the target's own realizations, and the fit already does (0.689). The
test holds it at what it reached (`BIRCH_REACHED`, 0.68). Next idea: a growth without hard bud
fates (a bud's shoot by its share, continuously, rather than past `BUD_NEEDS` or not), which
would give a shaded crown's few branches a slope.

Also learned: a scale-free silhouette hardly tells age (the hidden oak, 90 years, fitted best at
219; the beech, 140, at 78).

**The photo fits** (recorded, not gated; the plan's expectation 0.70). Two CC0 photographs of
lone oaks on Wikimedia Commons (docs/citations.md's assets table), cut out by hand with a sky
threshold to black on white (the cuts' script and masks are outside the repo, with the
photographs), fitted as `oak` (`wrela tool studio/grow --write out=build/grow --read
refs=<cuts> -- --fit refs/oak-<n>.png --species oak --out out/photo-<n>`):

| Photo | IoU | Trajectory | Time here |
|---|---|---|---|
| Widgar52, "Die Eiche", Schmarbeck, January 2024 (bare) | **0.761** | 20: 0.740, 100: 0.750, 120: 0.752, 260: 0.756, 280: 0.761 | 1 m 56 s |
| Qasinka, the Nebory memorial oak, May 2020 (in early leaf; crown cut at the background's treeline) | **0.659** | 20: 0.518, 40: 0.562, 100: 0.593, 140: 0.610, 200: 0.613, 240: 0.659 | 2 m 21 s |

The fitted oaks are taller than broad; Nebory's crown is a flat dome twice as wide as it is tall
over its trunk, which no oak within ±25 % of the genome grows: the open-grown oak's crown is too
narrow (a lookdev item for E8).

**The goal** (`wrela test studio/grow a_goal`): an oak goal with a gale at year 40 grows a fork
(2 or more limbs over 70 % of its height, `growth::limbs_through`, E1.2's test 5); a beech goal
of 22 m (light from the south) and of 12 m (from the east) each reach their height within 10 %
with the crown's centroid toward the light. Both pass.

**A crown leans into its gap by where its buds are** (E1.2's test 2, which the keyed draws
failed): an 80-year oak's crown centroid over its reach, seeds 1–8, was 0.10–0.24 (seed 1 had
passed by luck); a bud's light now rises on the gap's side of its trunk (`GAP_SIDE`, 0.4,
**proposed**: the neighbour shades its half of the crown), 0.22–0.32. E1.2's eleven tests pass
(`wrela test engine/tests/growth-trees`: 11 passed).

## E7, the growth's cache and determinism (2026-10-31, toolchain 0.13.0)

Linux pop-os (AMD Ryzen, Radeon "Lucienne" integrated, RADV, 12 threads; Chrome stable headless
through `std::tool::browser`); **not the M4 Air**. The plan's budgets are the M4's: the first
ring's 1,000 ms, G-cold and G-warm need re-measuring there.

**The archetypes are grown at build.** `content/trees/archetypes.wrela`'s `GROWN` is a constant
(`@fuel(2 ** 44)`) the build computes; `wrela build examples/vale` reports it among its
constants ("constants: N computed, M from the cache"), and the vale holds the 16 archetypes' shapes
as 502,640 bytes. Nothing of them grows at load.

**The page's cache.** A near tree's shape is kept in the program's storage under
`grown/<key>` (`growth::grow_key`: the growth's `VERSION`, the genome's values, the site, the
light a year and the events its history's record gives, the seed and the height it's drawn at),
read before it grows and written after (`land::stage::near_trees`). Natively, the first ring
from empty storage then again from its kept shapes: 961 ms (6 grown) → 151 ms (6 from the
storage), the same hash. `engine/tests/growth-trees`' `a_kept_shape_comes_back_and_its_key_names_every_input`:
a shape round-trips through its bytes, the same growth grown twice hashes the same, a gale moved a
year changes the key (a miss) and the tree.

**The first ring.** It was wrongly ready early: `near_ready` counted only the near trees whose
growths had begun, and two began at a time, so the first run's ring was 3 trees and a second's 2.
The stage now keeps the ring as chosen (the `FIRST_N` nearest candidates within `FIRST_RING`), and
while it isn't drawn all of it starts at once. Time from its first choosing to its last tree drawn
(`grown: the first ring, ...`, printed by the stage):

| Step | Native (8 helpers) | Chrome (8 helpers) |
|---|---|---|
| ring of 3 (the early-ready bug) | 835 ms | |
| ring of 6, two growths at a time | 1,831 ms | |
| all six at once | 961, 866 ms | 1,777 ms (1 helper: test mode's default) |
| the near trees' conversion not measured (`shape_of(.., measured: false)`) | 831, 838 ms | 1,215 ms |
| the growth 40 % cheaper (below) | 848, 841 ms | **1,198 ms** |

Its trees here are 16 to 92 years old. Natively they're all grown by 300 ms of the 845, but a load
frame of 450 ms (the main thread waiting on the tiles' work behind them on the helpers) comes
between, and their cards are cooked after it: the ring's time is now the load's frames more than the
growth. The vale's test `the_first_ring_grows_before_the_vale_is_ready` asserts the ring of six is
drawn within its frames and prints its time (1,353 ms there: a frame test's host runs few helpers,
so its growths queue, and a frame test takes no helper count); the budget is the `--hash` runs',
with a played page's helpers. In Chrome here it's 1,198 ms: **to re-measure on the M4** (whose CPU
grew a 100-year oak in 364 ms against this machine's 699).

**The growth's cost** (a 100-year oak, 5,856 nodes, timed phase by phase in a throwaway tool
test's build; that build is slower than the tool's optimized one):

| Lever | Total | Shade | Space | Perceive | Light |
|---|---|---|---|---|---|
| before | 881 ms | 216 | 217 | 340 | 58 |
| the shade's 3 × 3 tent as two [1 2 1] passes | 713 | 60 | 218 | 325 | 58 |
| the envelope's square without `pow` | 688 | 60 | 191 | 327 | 59 |
| the markers' kill inline, no closure | 659 | 60 | 165 | 325 | 58 |
| a marker's own cell first, cells farther than the best skipped, no root | 539 | 61 | 168 | 201 | 58 |
| the markers' angles by a rotation, not a cosine and sine each | **526** | 62 | 153 | 199 | 60 |
| (the buds' grid at half the reach) | 764 | 61 | 167 | 428 | 59 |
| (the kill's cells pruned by their distance) | 570 | 61 | 194 | 203 | 60 |

The last two were slower and are not kept. The `wrela tool grow -- --species oak --age 100`
optimized build: 699 ms before.

**Determinism** (`wrela tool grow -- --hash --package examples/vale`: two native runs from empty
storage and one in Chrome, 8 helpers each; each run's `grown:` lines):

```
native 1 (the first ring in 848 ms):
  grown: the archetypes, 502640 bytes; their hash 0658a73d2350ec65
  grown: the first ring, 6 trees (0 from the storage); their shapes' hash 133b5d9534de9303
native 2 (the first ring in 841 ms):
  grown: the archetypes, 502640 bytes; their hash 0658a73d2350ec65
  grown: the first ring, 6 trees (0 from the storage); their shapes' hash 133b5d9534de9303
Chrome (the first ring in 1198 ms):
  grown: the archetypes, 502640 bytes; their hash 0658a73d2350ec65
  grown: the first ring, 6 trees (0 from the storage); their shapes' hash 133b5d9534de9303
grow: the hashes are the same in every run
```

Identical on this machine's native host and its Chrome (H5 holds here); every earlier growth in
this section hashed the same across hosts too. The M4's Chrome is the plan's third run: **to
re-measure on the M4.**

Run again on Part E's last revision (the council's R1-B-F4: E8 changed the growth, its `VERSION`
and the archetypes' bytes since the run above), the same command; on 776afe31 and again with the
floor's switch and the ring's fallback count (`Landscape::ring_fallen_back`) on top:

```
native 1 (the first ring in 547 ms):
  grown: the archetypes, 508280 bytes; their hash a003a12bbd48d613
  grown: the first ring, 6 trees (0 from the storage, 0 drawn as their archetypes); their shapes' hash 362677e6de00fa1d
native 2 (the first ring in 555 ms):
  grown: the archetypes, 508280 bytes; their hash a003a12bbd48d613
  grown: the first ring, 6 trees (0 from the storage, 0 drawn as their archetypes); their shapes' hash 362677e6de00fa1d
Chrome (the first ring in 1235 ms):
  grown: the archetypes, 508280 bytes; their hash a003a12bbd48d613
  grown: the first ring, 6 trees (0 from the storage, 0 drawn as their archetypes); their shapes' hash 362677e6de00fa1d
grow: the hashes are the same in every run
```

776afe31's run gave the same three hashes (native 548 and 547 ms, Chrome 996 ms). The first ring
in Chrome here is 996 to 1,235 ms between runs, about the plan's 1,000 ms budget: **to re-measure
on the M4.**

**G-cold and G-warm** (`wrela tool bench -- --scene vale-walk --starts 3 --size 960x540 --frames
60 --no-docs`, playable ms, the median of three; this machine, so each step against `main`
006223f8 measured the same way, the same bench tool in both). This GPU loses its device on the
vale at 1920×1080 (`VK_ERROR_DEVICE_LOST` once playable) and draws it at about 1.3 frames a second
in Chrome at 960×540, so the bench gained `--size` and the runs are cut to 120 frames; the numbers
compare runs here and are not the gates' (the M4's, **to re-measure there**: C5 recorded G-cold
Blocked at 6,691 ms and G-warm met at 2,209 before Part E):

| Build | Cold | Warm |
|---|---|---|
| `main` | 9,633 (9,633, 9,606, 9,842) | 6,190 (6,190, 6,016, 6,192) |
| Part E as it came to E7 | 14,143 (14,143, 13,987, 14,271) | 8,222 (8,222, 8,261, 8,157) |
| a kind's impostor baked from its coarsest level's limbs (`plant::level_limbs`, a sixteenth: a grown kind's 2,048 limbs were each a draw in all 256 of its views) | 12,253 (12,108, 12,253, 12,329) | 6,815 (6,428, 6,983, 6,815) |
| the moss's patches read from bound data, not a constant table in the shader (below) | **10,937** (10,937, 10,817, 11,148) | **6,701** (6,839, 6,701, 6,503) |

The moss's patches (`growth::MOSS`, E4's reaction-diffusion, 2,048 words) were a constant the
ground's cook and the limbs' vertex shader indexed; the WGSL copied the whole table into a local
array in each invocation (a texel's, a vertex's), and the clipmap's `cook_level`, which frame 0
waits on, took 1,899 ms to compile against `main`'s 635 (72,649 bytes of WGSL against 45,025). Now
the terrain carries its own copy (`Ground::moss`, read in its inlined methods, as its soil is:
`land::cover::moss_at`) and the wood binds one for the limbs (`Wood::moss`, `moss_tile_in`); the
same words, so the same pixels. `cook_level` is 47,122 bytes.

Still 1.3 s behind `main` cold and 0.5 s warm: frame 1 cooks the 26 kinds with 164 ms more CPU
(the grown kinds' limbs), and the load's other frames are about 10 ms longer each. Warm, the first
ring comes from the page's storage in Chrome (6 trees in 494 ms; cold, 948).

## E8, the vale regrown (2026-10-31, toolchain 0.13.0)

Linux pop-os (Radeon "Lucienne"), not the M4 Air.

**G-grown.** `vale_land_tests::every_plant_the_vale_draws_is_grown`: 26,922 instances (living,
standing dead and fallen), 0 authored; the far layer's kinds, the floor's 33 species and the
ground's 4 fields all grown. The shared kinds list keeps 3 authored kinds the vale never places
(the floor's great tree, two unused far kinds).

**The growth of the vale's region:** the archetypes at build, `GROWN` (21 kinds: 16 living, 2
fallen, 5 standing dead; 508,414 bytes), **19.7 s** of the build here (`wrela build examples/vale
--report consts` from an empty cache: 436 G of fuel; the moss's patches 0.46 s, the lichen's
0.02 s); the first ring at load, **848 / 841 ms** natively and **1,198 ms** in Chrome (E7's runs,
8 helpers; 151 / 494 ms from the page's storage). The M4's to re-measure.

**G-look** (the twelve against `main`'s record made here, `build/record/physical` in a worktree of
006223f8; both through the lookbook's fast run at 960×540; `build/vale-e8b`): the measures, main /
E8:

| Shot | flicker | pop | still_pop | trail |
|---|---|---|---|---|
| 00 the forest floor | 2.36e-5 / 1.93e-5 | 0.0123 / **0.0180** | 0.00134 / 0.00103 | 2 / 3 |
| 01 the canopy against the sun | 7.06e-6 / 4.12e-6 | 0.0112 / **0.0389** | 0.00092 / 0.00058 | 3 / 4 |
| 02 an old oak | 1.27e-6 / **2.02e-6** | 0.1113 / 0.0561 | 0.00019 / 0.00023 | 4 / 3 |
| 03 the fall | 1.17e-5 / 1.04e-5 | 0.0069 / 0.0055 | 0.00061 / 0.00052 | 4 / 5 |
| 04 the band's face | 1.08e-6 / 8.45e-7 | 0.0093 / 0.0065 | 0.00024 / 0.00020 | 3 / 3 |
| 05 the meadow at evening | 5.91e-5 / 4.59e-5 | 0.0063 / 0.0055 | 0.00322 / 0.00253 | 2 / 2 |
| 06 dawn in the hollow | 1.19e-5 / 5.91e-6 | 0.0037 / 0.0037 | 0.00157 / 0.00084 | 3 / 3 |
| 07 dusk from the crest | 9.67e-6 / 9.37e-6 | 0.0020 / 0.0019 | 0.00124 / 0.00119 | 2 / 2 |
| 08 grazing at the edge | 1.91e-5 / **2.97e-5** | 0.0044 / 0.0046 | 0.00106 / 0.00112 | 2 / 3 |
| 09 down the trail | 5.27e-5 / 3.60e-5 | 0.0069 / 0.0060 | 0.00234 / 0.00200 | 3 / 3 |
| 10 the fallen beech | 4.51e-5 / 2.93e-5 | 0.0105 / **0.0199** | 0.00143 / 0.00132 | 5 / 4 |
| 11 the wood under cloud | 9.16e-5 / 8.25e-5 | 0.0075 / 0.0076 | 0.00443 / 0.00423 | 2 / 2 |

The judge (blind, fresh context, calibrated each round on an 8-pass blur and a quarter-resolution
copy, which it rejected): round 1, E8 preferred on 4 shots (1, 7, 10, 11), main on 6, equal 2;
round 2, after the round's fixes (the turn limit and set point, leaves along the last years'
shoots, the root plate), E8 on 4 (1, 5, 10, 11), main on 7, equal 1 (7). The pops on shots 1 and
10 are the camera's motion over high-contrast near surfaces (the lookbook now keeps a dolly's
worst pair of frames: a grown trunk's deeper cracks, the root plate filling the frame), and the
judge preferred E8 there in both rounds: named deliberate. **Blocked:** shot 0's pop and the
flicker on shots 2 and 8, where the judge preferred main; next lever, the near bark's relief
contrast at a dolly's range (the panel's judges fault the bark too).

## E: the floor and the clearing kept (2026-10-31, toolchain 0.13.0)

Linux pop-os (Radeon "Lucienne", RADV), **not the M4 Air**: a pixel comparison, not a gate's
timing, so nothing here needs re-measuring there.

Part E's growth reaches code the floor (`games/last-green`) and the clearing share with the vale:
`trees::kinds`' kinds and every table read from them (the forks, crowns, trunks and logs the sim
and the interest check use), the bark's, the ground's and the rocks' shaders, the grass's species
and the probes' occluders. D40 puts the floor's adoption behind a switch in §H1, judged there on
its 23 shots; so every one of these is opt-in, and off draws as before §E: the floor's
`GROWN_WOOD` (`pub const` in main.wrela, false; `--set GROWN_WOOD=true` builds the grown floor),
`trees::kinds`' `grown` argument (two rows of `SHAPES`; the hand's logs and their rot as before),
`plant::Bark::grown` (a kind's bark caused: cracks, lichen, moss, its limbs' thickening),
`grass::Grass::new(grown)` (`HAND_FLORA`: the 33 rows as the hand set them), `RockLook::grown`,
`land::cover::wood_floor`'s `grown`, `terrain::Cover::rough` unset (0) as the ground's 0.9, and the
hand's kinds' occluders fitted at each tree as before (`plant::Occluding::limbs`). The clearing
passes false. The council's round 2 asked for this before the merge (R1-B-F3).

**The interest check.** The floor's build files (`files/check/report.json`, `shots.json`,
`walk.bin`, `views.bin`, `calibration.json`, `reach.json`: the trees as drawn, their crowns, the
obstacles and the walk) are byte-identical to the merge base's (006223f8).

**The stills.** `wrela tool lookbook -- build/px/floor --stills --capture --size 960x540` (and
`--package examples/clearing`) on this branch and on 006223f8, each built the same way, every shot
twice and the shots that didn't meet five times each side. Capture mode isn't deterministic here:
a shot lands in one of a few states from run to run (the last column: the largest mean difference
between two runs of one build), so each shot's record is its closest pair, branch and base:

| Shot | Runs (branch / merge base) | Closest pair: pixels differing | Its largest difference | Largest difference between two runs of one build (mean, of 255) |
|---|---|---|---|---|
| floor 00 the clearing | 2/2 | 0.000 % | 1/255 | 0.15 |
| floor 01 wood pasture | 2/2 | 0.017 % | 5/255 | 0.00 |
| floor 02 beech high forest | 2/2 | 0.428 % | 14/255 | 3.25 |
| floor 03 the old road | 2/2 | 0 (identical) | 0/255 | 1.88 |
| floor 04 the crag's top | 2/2 | 0 (identical) | 0/255 | 0.91 |
| floor 05 the tower's foot | 2/2 | 0 (identical) | 0/255 | 0.10 |
| floor 06 the ring | 2/2 | 0.000 % | 1/255 | 3.55 |
| floor 07 the mere | 2/2 | 0 (identical) | 0/255 | 0.00 |
| floor 08 the stream in the wood | 5/5 | 0.052 % | 28/255 | 2.42 |
| floor 09 the waterfall | 2/2 | 0 (identical) | 0/255 | 0.00 |
| floor 10 the burn | 5/5 | 0.107 % | 2/255 | 1.71 |
| floor 11 the windthrow | 2/2 | 0.455 % | 12/255 | 1.37 |
| floor 12 the plateau's edge | 2/2 | 0 (identical) | 0/255 | 0.71 |
| floor 13 the town's site | 5/5 | 67.048 % | 120/255 | 3.40 |
| floor 14 the warden's arena | 5/5 | 0 (identical) | 0/255 | 3.28 |
| floor 15 the wildwood | 2/2 | 0.294 % | 5/255 | 1.87 |
| floor 16 the fawn in the glade | 2/2 | 0.080 % | 8/255 | 1.79 |
| floor 17 the fawn under the beeches | 2/2 | 0.364 % | 17/255 | 2.01 |
| floor 18 the fawn by the stream | 2/2 | 0.003 % | 1/255 | 1.12 |
| floor 19 the climber's robe | 2/2 | 0.258 % | 13/255 | 0.00 |
| floor 20 the climber close | 2/2 | 0.416 % | 9/255 | 1.85 |
| floor 21 the climber's hood | 2/2 | 0.211 % | 9/255 | 0.00 |
| floor 22 the climber's train | 2/2 | 0.010 % | 3/255 | 2.52 |
| clearing 00 the opening | 2/2 | 0 (identical) | 0/255 | 0.00 |
| clearing 01 toward the gap | 2/2 | 0 (identical) | 0/255 | 0.00 |
| clearing 02 the great tree | 2/2 | 0.008 % | 3/255 | 0.00 |

Ten of the floor's shots meet a run of the base's to the pixel or within 1/255 on a few pixels, and
two of the clearing's three to the pixel. Twelve of the floor's and the clearing's third differ on
at most 0.46 % of their pixels, by at most 28/255: isolated pixels on near trunks' bark, the same in
every run of a build (shots 1, 19 and 21 have no run-to-run variation at all). For a hand's kind
every term the bark's shaders gained is left out or times 1 (`limb_world` returns the before-§E
vertex, `mossy` its moss); computing the bark's terms without the times-1 factors changed none of
these pixels, so the likeliest cause is the limb shaders' float rounding as their compiled code
changed, which the bark's relief, a normal from the screen's derivatives, makes visible.
Shot 13 never met the base's states in five runs each; its closest pair (1.68/255 mean) is nearer
than two of the base's own runs of it (3.40) and differs on the edges of leaves and grass, as the
runs of any shot do: **within the run-to-run variation, not shown identical.** The capture mode's
variation is the engine's (F7: a still is the frame its resolve settles on); recorded for G's
lookdev instrument.

## F1–F5, the grazer grown (2026-10-31, toolchain 0.16.0)

Linux pop-os (Ryzen, Radeon "Lucienne"), **not the M4 Air**. The searches' and the tests' numbers
are the machine's arithmetic, the same anywhere; the times are this machine's.

**F1–F4** (`engine/tests/growth-creature`): the grown roe's withers 0.71996 m against the handle's
0.72 and its chest 0.325 m against 0.3168 (its measure, the kit's); at age 0.1 its legs' share of
its withers 1.174 times the grown's and its head's of its body 1.339 times; its integrated mass at
age 1 29.67 kg against the curve's 27.91 (6 %); age 0.5 between in every handle; the same body
twice. Its coat's wavelength 14.22 texels (0.1264 m), 2.0 times on a body twice as large; 61 spots
at age 0.1, the largest 0.54 % of the map, none at 1; the same map by seed and another by
another; no seam (one chart). A heavier genome's thigh 1.4997 times the section; the muscles as the
optimized walk sizes them 15.43 kg of 29.67 (52 %: Blocked, F5 below); the same muscles twice.

**F5, the gaits optimized** (`wrela tool gait -- --generations 80 --seconds 60 --write`, 22 min
40 s on 12 threads; CMA-ES from seed 5, 8 a generation; each candidate 60 s after setting off, a
grade's up it and down it). The walker is kinematic; the measures are `kit::gaits`' (effort |τ·ω|
per metre over the feasible frames, the secondary chains frozen; the share of frames the centre of
mass is outside the support of the feet planted through the frame, a hull held to its edge and a
line or a point to a 4 cm band, scored by its worst stride; a planted foot's gap; a swing's
clearance over the ground's top and how deep a swinging sole dips under the ground; speed; each
stride's infeasible share, over 5 % invalid; a stride is any cycle's frames running, rolled a frame
at a time). A smooth body leans its trunk 0.8 of the way toward its planted feet, eased (a
critically damped 20 rad/s, its acceleration at most 4 m/s²), and vaults over its planted legs. The
best total by generation, and the result:

| Gait | total at gen 1 / 20 / 40 / 80 | effort (J/m) | outside (worst stride) | gap (m) | clearance (m) | pierce (m) | infeasible (worst stride) |
|---|---|---|---|---|---|---|---|
| walk, level | 149.5 / 122.9 / 118.0 / 116.8 | 200.7 | 0.000 (0.000) | 0.012 | 0.044 | 0.000 | 0.323 (0.368) invalid |
| walk, 15° | 617.7 / 152.8 / 144.4 / 139.7 | 446.2 | 0.100 (0.122) | 0.000 | 0.024 | 0.000 | 0.626 (0.659) invalid |
| walk, 30° | 1019.9 / 157.3 / 150.9 / 144.9 | 1616.6 | 0.000 (0.000) | 0.019 | 0.056 | 0.000 | 0.843 (0.923) invalid |
| walk, a 0.3 m step | 1078.3 / 156.4 / 152.3 / 151.9 | 104.0 | 0.050 (0.080) | 0.006 | 0.020 | 0.000 | 0.870 (0.960) invalid |
| walk, uneven | 321.3 / 133.3 / 120.4 / 118.2 | 1550.2 | 0.000 (0.000) | 0.016 | 0.053 | 0.000 | 0.258 (0.404) invalid |
| trot, level | 428.1 / 142.3 / 137.8 / 136.6 | 563.8 | 0.000 (0.000) | 0.019 | 0.122 | 0.000 | 0.692 (0.760) invalid |
| trot, 15° | 901.3 / 169.9 / 160.4 / 158.6 | 381.7 | 0.120 (0.174) | 0.015 | 0.005 | 0.000 | 0.782 (0.875) invalid |
| trot, 30° | 1258.9 / 528.3 / 394.7 / 379.7 | 169.8 | 0.156 (0.217) | 0.065 | −0.014 | 0.021 | 0.918 (1.000) invalid |
| trot, a 0.3 m step | 1940.6 / 454.4 / 302.9 / 302.9 | 344.9 | 0.153 (0.200) | 0.036 | 0.031 | 0.017 | 0.842 (0.920) invalid |
| trot, uneven | 746.8 / 155.0 / 142.8 / 139.3 | 3175.7 | 0.068 (0.083) | 0.016 | 0.051 | 0.000 | 0.401 (0.792) invalid |
| bound, level | 1315.0 / 713.7 / 665.2 / 660.0 | 423.7 | 0.796 (0.850) | 0.086 | 0.022 | 0.000 | 0.833 (0.917) invalid |
| bound, 15° | 1404.2 / 825.8 / 771.2 / 749.8 | 283.6 | 0.796 (0.850) | 0.108 | −0.012 | 0.018 | 0.896 (0.958) invalid |
| bound, 30° | 2564.1 / 1421.2 / 1391.3 / 1367.2 | 1417.7 | 0.961 (1.000) | 0.190 | −0.062 | 0.106 | 0.945 (0.958) invalid |
| bound, a 0.3 m step | 1608.7 / 885.3 / 847.7 / 834.7 | 49.1 | 0.919 (0.957) | 0.110 | 0.050 | 0.013 | 0.956 (1.000) invalid |
| bound, uneven | 1116.7 / 737.5 / 707.3 / 686.7 | 359.2 | 0.815 (0.900) | 0.090 | 0.025 | 0.000 | 0.870 (0.958) invalid |

The hand-set walk (the clearing's, on the walker as it was) on level ground over the same 60 s:
**404.8 J/m** → the optimized **200.7 J/m**, 0.50 of it (the plan asks at most 0.9; the hand-set
walk is 76 % outside its hull and its worst stride 68 % infeasible on these measures, its effort
over its feasible frames). The 30° walk holds the plan's gate: none of 60 s outside its support,
up or down, a planted foot at most 1.9 cm off its sole. After a 0.2 m drop no stride is outside
its support from the fourth past it; over the 0.3 m log the swing's top clears it.

The tests (`engine/tests/growth-gait`, 13): the head's spring can't move the effort (to 1e-6);
equal peaks and unequal power differ; a fixture's effort 20 J/m by hand within 5 %; a failed stride
isn't diluted, nor one across a bucket's edge (4 of 41 frames: 9.8 %, not two buckets' 4.9 %); a
hull has no band; a swaying body stands the same anywhere (its feet and frame 10 m from the origin
its own, moved); a hop invalid; the 30° walk holds its support and its feet; a log's top cleared; a
drop recovered by the fourth stride; the search the same from its seed; the baked muscles the
walk's to the bit; the effort under 0.9 of the hand-set walk's.

The levers on the 30° walk, in order, each searched again: the support as the feet planted
through the frame's step; the hind legs' lag bounded to Hildebrand's lateral-sequence walks; a
smooth body's trunk pitched about its hips' middle; its feet set down plumb under its hips; its
height held over the ground averaged along its length; its crouch's range to 0.2 m; a touchdown
placed where the foot came down; each candidate 20 s; then (the council's first round holding a
hull to its edge, the walk outside its hull a sixth of the time) the trunk's sway toward its
planted feet (none: 0.168 outside on level; half the way: 0.020; 0.8: 0) and its vault. The
council's second round (the sway set by each frame's weights, its start pulled toward the world's
origin; fixed buckets for a stride; the sway's acceleration untried) moved the sway into the
walker's state, eased, and the strides to a rolling window; each searched again:

| Run | the sway's acceleration bound | candidate | stability scored | level effort (of the hand-set's) | 30° outside (over the candidate's run; the test's 60 s) | muscles | log pierce |
|---|---|---|---|---|---|---|---|
| 16 (round 1) | none | 20 s | mean | 0.90 | 0; 0 | 52 % | 13.5 cm |
| 17 | 2 m/s² | 20 s | mean | 0.84 | 0.19 | 41 % | — |
| 18 | 2 m/s², 120 generations | 20 s | mean | 1.04 | 0.19 | 62 % | — |
| 19 | 4 m/s² | 20 s | mean | 0.57 | 0; 0.04 | 64 % | — |
| 20 | 3 m/s² | 20 s | mean | 1.05 | 0.02 | 63 % | — |
| 21 | 4 m/s² | 60 s | mean | 0.56 | 0; 0 | 40 % | 15.9 cm (the drop's fifth stride 1.8 % outside) |
| **22** | 4 m/s² | 60 s | worst stride | **0.50** | **0; 0** | **49 %** (52 %: below) | **9.3 cm** |

- **Blocked: every gait's feasibility.** The level walk's worst stride is 37 % infeasible (32 % of
  its frames), the 30° walk's 92 %; the trot and the bound's 76–100 %. The sway's acceleration
  bounded didn't lower it (run 16's 29 %, these 31–37 %). An infeasible frame is one whose
  required wrench the planted feet can't carry as point forces (`engine::loads::allocate`: each
  vertical force at least 0, the horizontal free, so no friction bound to relax): its moment about
  a two-foot line, or past one foot, or none planted. Next lever: the lean's target set where the
  required wrench's centre of pressure stays inside the support (a zero-moment point's, from the
  centre of mass's acceleration, not the planted feet's centroid), then a ballistic flight phase
  for the trot and the bound.
- **Blocked: the muscles' share**, 52 % of the body against 30–45 % (15.43 of 29.67 kg). The
  walks searched sized them 40–64 % (the runs' column, each foot's force then in the growth
  curve's 27.91 kg's weights; the review found the loads are the integrated body's 29.67 kg, and
  read in its weights the baked walk's are 52 %: `wrela tool gait -- --grow --write`), every group
  scaled alike by the genome's gallop foot force over the walk's peak (a walk with a lower peak
  foot force scales its torques higher); its test holds 0.52. Next lever: the muscles' share in
  the gait's objective, or the gallop's loads found by its own search, not a walk's scaled.
- **Blocked: a swinging sole goes 9.3 cm into the log** before its swing's top (the clearance over
  the log's top holds, by 0.1 mm); its test holds 0.094. Next lever: the swing's lift sized once
  as it lifts (it's sized again each tick toward a landing that moves with the body, so its arc can
  change under it mid-swing), sampled against the posed sole along its arc, more finely.
- **Blocked: the 0.3 m step's walk, the trot and the bound** (invalid as above).
- **G-profile:** the grazer's walk is 13.8 µs a frame natively (3,600 frames of a step and its
  palette on uneven ground, the vale's doe walking its baked walk, its lean, vault and nod
  included; 13.7–13.9 over three runs; 10.0 before them), within 0.2 ms of C6's frame. Its grazing
  pose is found once a hold. The M4's to re-measure.

## F7, the grazer touching the world (2026-10-31, toolchain 0.16.0)

Linux pop-os (Radeon "Lucienne", RADV), **not the M4 Air**: the M4's to re-measure.

The grass's contact carries five pushes (`engine::grass::Push`: the grazer's hooves and its body,
`kit::roe::pushes`); a footfall splashes in water (`particles::SPLASH`, 24 drops) and puffs on
bare ground or litter (`particles::PUFF`, 12). The tests (`engine/tests/interaction`): blades
0.05–0.3 m from each planted hoof lean 22° or more (at least 20); a fern 0.5 m to the chest's side
12° away (at least 10); a footfall in water 24 drops, each rising. The cost, `wrela profile
examples/vale --serial --size 1920x1080 --warmup 600 --frames 300` natively at F6 (f63ef9b5) and
with the pushes, GPU median: **vegetation 11.675 → 11.682 ms, its prepass 5.697 → 5.705 ms**
(+0.01 ms each, within 0.2); no `particles` label in either (the first view has no emitter near).

## F9, the motion review (G-motion) (2026-11-01, toolchain 0.16.0)

docs/plan-world-class-3.md §F9; the rubric and rules in docs/lookdev.md, "The motion review".
Linux pop-os (Radeon "Lucienne"): the vale's frames rendered here.

- **The clips.** The vale's: shot 10's three seconds from its still (`wrela tool studio/motion
  --write ws=. -- <out> --package examples/vale --frames-only --size 3840x2160 trail`), frames
  300–474 every sixth (10 a second), the fawn walking at its optimized walk (`kit::walks`' level
  walk at its age). The reference: `vale-motion-walk`, "Roe deer in England" by Peter Trimming,
  CC BY 2.0 (https://creativecommons.org/licenses/by/2.0),
  https://commons.wikimedia.org/wiki/File:Roe_deer_in_England.webm (its copy's SHA-256 checked:
  `da075f7e…`), seconds 94.0–96.9 of its record's 30–100 s (the buck walking steadily), its frames
  extracted at 10 a second by headless Chrome seeking the clip (no frame interpolated; extracted and
  cropped, noted with the credit). Each shown whole and cropped round its animal (`build/f9-private/
  pack-r2.py`), 30 frames a sheet.
- **The conditions.** The vale's: a rendered roe fawn (age 0.2, 0.48 m at the withers, spotted)
  walking three-quarters on toward a still camera at about 10 m in late afternoon sun; the
  reference: an adult roe buck filmed side on, walking across a meadow's edge, the camera panning
  with it. Frame rates: the vale's 60 rendered, the footage's video; both sampled at 10.
- **Calibration:** the vale's strip against itself held on every third frame and against its
  eight-pass blur: no judge of any round preferred a degraded strip (none rejected). Rounds 1 and
  2's judges preferred the vale's each time; rounds 3 and 4 rated some pairs equal (round 3: the
  blur, judges 1, 4 and 6; the held frames, judges 1 and 2; round 4: the blur, judges 2, 5, 6 and
  8; the held frames, judge 6), which the rule doesn't reject.

| Round | The vale's strip | Vale / equal / reference | Share | The vale's means (weight, timing, contact, secondary, readability) | The reference's |
|---|---|---|---|---|---|
| 1 | head on (a 23° field, since withdrawn: the panels' shot is frozen) | 0 / 0 / 8 | 0 % | 1.00, 1.00, 1.00, 0.88, 1.00 | 3.00, 3.00, 2.38, 2.50, 3.00 |
| 2 | three-quarters on, its swing at least 7 cm, 4K cropped | 0 / 0 / 8 | 0 % | 1.00, 1.25, 1.00, 0.88, 1.38 | 3.00, 3.00, 2.75, 2.38, 3.00 |
| 3 | the grown doe; its trunk swaying over its planted feet and vaulting over its legs (F5's walker, the plan's first named lever) | 0 / 0 / 8 | 0 % | 1.00, 1.00, 1.00, 0.12, 2.00 | 3.00, 3.00, 2.38, 2.12, 2.75 |
| 4 | its head nodding twice a stride, ±2.7 cm against its trunk, through its neck's spring (Loscher et al. 2016's ungulate walk), on the walk the council's second round re-searched (its sway eased, its strides rolled) | 0 / 0 / 8 | 0 % | 1.00, 1.38, 1.00, 0.25, 1.75 | 3.00, 3.00, 2.38, 2.12, 2.75 |

Round 2's judges on the vale's clip (its five rows' scores, then their sentences on weight and
secondary motion; every sentence and both rounds' files are in `build/f9-motion*/judges/`):

- judge-1 (the vale's 1 / 2 / 1 / 1 / 2): The body stays almost level and the thin legs swing under it without any visible compression or settling, so the fawn reads light, close to floating. The head stays upright with only a small bob and a late turn toward the camera around 2.1 s, the ears don't move on their own, and the tail is barely visible.
- judge-2 (the vale's 1 / 1 / 1 / 1 / 2): The body rides at a nearly constant height over thin, quick legs and never visibly drops or settles onto a footfall, so the fawn looks light, like a puppet. The head stays stiffly upright and doesn't bob with the stride; it turns from profile to face the camera around 2.1 s, but I see no follow-through in the ears or tail.
- judge-3 (the vale's 1 / 1 / 1 / 1 / 1): The body glides at a constant height while the legs move beneath it, with no visible dip or settle onto each footfall. The head and ears stay mostly upright, with small changes in angle that seem to follow the camera's turn rather than the steps, and no tail motion is visible.
- judge-4 (the vale's 1 / 1 / 1 / 0 / 1): The body stands on its legs, but it rides at a near-constant height with no sink or rise as each foot lands, so it feels light and rigid next to the real deer. The head and ears hold a fixed attitude with no nod or settle as the legs move, and there is no tail motion to see.
- judge-5 (the vale's 1 / 2 / 1 / 1 / 2): The torso rides almost flat with little rise or fall, so the fawn looks light and floaty rather than settling its mass onto each step. The head stays locked in one pose and the ears and tail show no follow-through or settle.
- judge-6 (the vale's 1 / 1 / 1 / 1 / 1): The fawn's body rides almost level over its legs, with little visible dip or settle as each foot lands, so it reads light and puppet-like next to the real deer. The head is held stiffly upright and only turns slowly toward the camera late in the clip, and the ears and tail show no follow-through.
- judge-7 (the vale's 1 / 1 / 1 / 1 / 1): The body glides at constant height over thin, stiff legs, with no bob or settle to show mass landing on each footfall. The head reorients slowly toward camera, but it doesn't nod with the stride, the ears stay fixed and no tail follow-through is visible.
- judge-8 (the vale's 1 / 1 / 1 / 1 / 1): The fawn's body stays level and rigid over its legs with no dip or settle as feet take weight, so it feels weightless next to the real deer. The head holds one upright angle throughout with no stride-linked nod, and the ears and tail stay still.

**G-motion: Blocked**, 0 % against 40 % in four rounds, every row lost (round 4: weight 1.00,
timing 1.38, contact 1.00, secondary 0.25, readability 1.75 of 3, the reference's 3.00, 3.00, 2.38,
2.12, 2.75). Round 4 took the record's next lever, the head's nod driven by the stride: the judges
saw at most "a faint head bob" (secondary 0.12 → 0.25), and "the body floats level over thin
legs" again decided weight. Round 3 took the plan's first
named lever, F5's walker (its trunk's sway and an inverted pendulum's vault, a bob of 7 mm on the
doe): readability rose (1.38 → 2.00), weight didn't move ("the torso glides as a rigid block with
almost no rise and fall"), and secondary motion fell (0.88 → 0.12: "head and ears stay fixed").
The plan's second, F6's parameters, would break F6's own gate (the physical look's motion is none,
its pose the gait's to the bit), so it isn't taken. The judges agree on what decides it: the body
rides level over its legs (no rise and fall, no settle onto a footfall: the kinematic walker
holds its height over its planted feet's mean, its vault a few millimetres), the head's motion is
too small to see at the shot's 10 m, and the legs read thin and stiff. The translucent shapes at
its hind legs that rounds 2 and 3 named are the stream's surface round its hooves (the trail
crosses the shallows; the character's motion target carries its palette against its last, so it
isn't temporal AA's). Next levers: the trunk's settle onto each footfall (a stance leg's
compliance, its fetlock's give, in the walker; its effort searched with it), the nod's range
against the shot's distance (the cited ±2.7 cm is a fifth of the doe's head at 10 m), and the
puffs fainter.

## F2–F8, G-look on the creature's shots (2026-11-01, toolchain 0.16.0)

Linux pop-os (Radeon "Lucienne"), not the M4 Air: measures and judgements, no timing.

**(1) Pairs:** E8's stills (`build/stills-e8`: the floor's fawn stand-in, standing at rest) against
Part F's (`build/stills-f-doe`: the grown roe doe, grazing in 9, walking in 10), shots 9 and 10;
D7 rendered (`build/stills-f-doe/18-…`), no E8 still of it to pair. **(2–3) The measures**
(`wrela tool lookbook -- build/glook-f --package examples/vale --fast --runs 3 8 9 18`, held to
E8's `build/vale-e8b`): every shot holds its bars; E8 → F:

| Shot | flicker | pop | still_pop | trail | clipped |
|---|---|---|---|---|---|
| 08 grazing at the edge | 2.97e-5 → 2.54e-5 | 0.00464 → 0.00460 | 0.00112 → 0.00107 | 3 → 3 | 0 → 0 |
| 09 down the trail | 3.60e-5 → 3.41e-5 | 0.00600 → 0.00597 | 0.00200 → 0.00197 | 3 → 3 | 0 → 0 |
| 18 D7 the creature in the wood | (none at E8) → 1.05e-5 | → 0.00948 | → 0.00074 | → 4 | → 0 |

**(4) The judge** (blind, fresh context each round, the order of pairs and sides from a seed,
calibrated on each round's still against its eight-pass blur, which it always rejected):

| Round | The creature | Shot 9 | Shot 10 | What it said |
|---|---|---|---|---|
| 1 | the fawn (age 0.2) | E8 | E8 | "a tiny flat-orange fawn with stick legs"; "a speck-sized deer … reads wrong in size" |
| 2 | the grown doe, fox-red | E8 | E8 | "a flat, over-saturated orange shape that looks pasted on" |
| 3 | the doe, a muted russet | E8 | **F** | 10: "reads more clearly as an animal and as a scale cue"; 9: "melts into the ground" |
| 4 | the doe, a deep russet | E8 | **equal** | 9: "a salmon deer [that] blends into the ground and looks plastic" |

Each round's lever came from the round before: the doe for the fawn's size (the vale's creature
is now grown, `kit::roe::VALE_AGE` 1), its hide desaturated and then darkened. **Shot 10: the doe
preferred (round 3) and equal (round 4). Shot 9: Blocked**, the judge preferring E8's stand-in
four rounds running; its hide's albedo no longer moves it (a 30 % darker back came out barely
darker in the sun: the doe's lit back is sRGB 191, 154, 118 against the soil's 141, 121, 95), so
the next lever is the creature's lit value, not its colour: the character shading's band light
(`character::character_shade`'s `cel`) lights it about 40 % brighter and flatter than the ground
it stands on. F10's panel and F9's first two rounds judged the fawn; F9's rounds 3 and 4 the doe.

## H1, the floor adopts (2026-11-01, toolchain 0.18.0)

docs/plan-world-class-3.md §H1 (D40). Linux pop-os (Radeon "Lucienne", RADV), **not the M4 Air**:
the stills, the judges and the pixel comparisons hold anywhere; every timing here is this
machine's, and the M4's are to measure (listed in "Whether D27 holds").

**The switches.** Each capability the vale proved is a constant of the floor's build
(`games/last-green/main.wrela`), off unless `--set`:

- `GROWN_WOOD` (Part E's, unchanged here): the floor's kinds grown, its flora and its causes.
- `GROWN_FAWN` (new): the roe grown from its genome as the vale's grazer (`kit::roe` at
  `VALE_AGE`, its `VALE_MUSCLES` and `VALE_COAT`) in the hand fawn's place
  (`games/last-green/fawns.wrela`). A shot stands it where it casts the fawn, its face attending
  to the eye; in play its own walker walks after the sim's fawn at its pace (its way straight
  ahead, as the sim's: `behaviour::Ahead`), its face to the player while the sim's senses notice
  them (F8). The two ages tried before it (0.3, 0.45) were grown by `wrela tool gait -- --grow
  --write` at those ages; they are gone with the switch's own tables (/simplify).
- `LOOK` (new, a type, as the vale's): `--set LOOK=styles::cel::Cel` builds the cel floor. The
  plan's `pub const STYLE` is `pub type LOOK`, since a look is a type (docs/toolchain.md).
- `TIER` (C7's).

The great tree stays authored under every switch (`trees::great_tree`: a character of the lore;
games/last-green/design.md).

**The builds** (`wrela pipelines games/last-green --set GROWN_WOOD=… --set GROWN_FAWN=… --set
LOOK=… --set TIER=…`, all 32): each builds; the pipelines are the same with either grown switch
on or off.

| Tier | Physical | Cel |
|---|---|---|
| Epic | 170 (147 entry points) | 171 (148) |
| High | 168 (145) | 169 (146) |
| Medium | 166 (143) | 167 (144) |
| Low | 162 (139) | 163 (140) |

**A stack the floor's init didn't have.** The stage first held room for both fawns' casts by
value (the other none), and the floor's init then overflowed its stack building the kinds'
plants (a wasm trap in `trees::kinds::log`; each `Plant` is 128 KiB, passed by value). Bisected
over Part G's 46 commits and this one: every Part G commit gets through, this one didn't. Each
fawn is now held in a list (`Fawns`), the one built or none (docs/toolchain-gaps.md).

**The grown floor** (`GROWN_WOOD` and `GROWN_FAWN`): the 23 shots' stills (`wrela tool lookbook --
build/f-<name> --stills --capture --size 1920x1080`, the floor built each way), blind pairs
against the floor as it is, three agent judges in fresh contexts (the shots split 9, 9 and 8,
each pack with a calibration pair, all three passed). **"Before" on 18 of 23**, "after" on 4,
equal on 1:

| Shot | Preferred | Before / after (the rows' sum) | The judge's why (its sides a and b) |
|---|---|---|---|
| floor 00 the clearing | before | 13 / 11 | A's backlit, darker tree gives stronger light and value structure at a similar artifact cost. |
| floor 01 wood pasture | before | 14 / 9 | B's spreading trees and canopy shade give real light, value and depth where A reads as repeated lollipops. |
| floor 02 beech high forest | before | 14 / 9 | A is a convincing, well-layered wood, while B's floating vines and visible leaf cards break plausibility. |
| floor 03 the old road | before | 12 / 10 | A's fuller, rim-lit trees model the light and layer the depth better, while both share similar log and hillside flaws. |
| floor 04 the crags top | before | 8 / 8 | Both share the black occluder, and A's varied broadleaf canopy reads more natural than B's repeated cones. |
| floor 05 the towers foot | before | 10 / 9 | B's full grass and naturally spreading trees remove A's visible tuft pattern and lollipop forms. |
| floor 06 the ring | after | 10 / 12 | A's sun-dappled clearing reads the light and values clearly, while B's dark foreground and hard stalks cost more than its better haze gains. |
| floor 07 the mere | after | 9 / 10 | B's floating black leaf sprites read as stray artifacts against the sky, while the rest of the scene is close. |
| floor 08 the stream in the wood | before | 12 / 9 | b's flared trunk and canopy shade make the plateau feel like it sits under trees, where a looks like a bare lit set. |
| floor 09 the waterfall | before | 9 / 8 | The scenes match, but a's crowned rim trees add scale and life that b's sparse stub lacks. |
| floor 10 the burn | before | 11 / 9 | a's snags read as varied broken trunks, while b's identical forked poles break the illusion despite its better logs. |
| floor 11 the windthrow | after | 9 / 12 | b's logs have real cylindrical form and girth and its centre tree a believable crown, where a's logs read as flat sticks. |
| floor 12 the plateaus edge | equal | 10 / 10 | The two are nearly identical apart from the distant forest texture, and neither difference is large enough to choose on. |
| floor 13 the towns site | before | 13 / 9 | b's trees have believable layered crowns over a deep understorey, while a's spherical bushes and tufted poles look artificial. |
| floor 14 the wardens arena | before | 10 / 10 | b's crowns and the forked pale trunk at left read as grown trees in a deep wood, which outweighs its odd orange branch stubs. |
| floor 15 the wildwood | before | 13 / 8 | b's flared, tapering, bark-modelled trunks and raking sun read as an old wood, whereas a's trunks look like posts in a tangle. |
| floor 16 the fawn in the glade | before | 12 / 10 | B's dense, canopied wood with a shaded floor and layered trunks beats a's sparse lollipop clearing. |
| floor 17 the fawn under the beeches | before | 12 / 9 | A's trees, root flares and dappled floor are far more convincing despite the clipping, while b's poles and stacked logs read as primitives. |
| floor 18 the fawn by the stream | after | 8 / 9 | B's view down the ravine gives clearer depth and its shrub is a more plausible form than a's fan of sticks, though a keeps the fawn in view. |
| floor 19 the climbers robe | before | 12 / 9 | B's thick, canopied trees and visible fawn make a deep, readable wood where a looks like a sparse plantation. |
| floor 20 the climber close | before | 12 / 10 | B's thick, layered trunks and canopy give a convincing wood with stronger value and depth. |
| floor 21 the climbers hood | before | 11 / 9 | A's trees have believable spreading crowns and the floor is dense, while b's logs and snags look like primitives. |
| floor 22 the climbers train | before | 10 / 7 | A reads as a lit wood with canopy and floor shadow, while b is a bare grove of stacked cylinders with floating stray geometry. |

What the judges fault: crowns read as "lollipops" and "repeated cones" on thin bare boles ("posts",
"a sparse plantation"), floating vines and leaf cards (the wildwood's and the beech forest's
looping thin limbs), identical forked snags, logs as "stacked cylinders", trunks without the
hand's flared feet. **Not adopted: `GROWN_WOOD` stays off (Blocked).**

G-look's measures (2) and (3) for the switch alone (`GROWN_WOOD`, the hand's fawn), run after the
council asked for them: the floor's 23 on the fast instrument with PSNR, each build's
(`wrela tool lookbook -- build/fast-floor-<name> --fast --psnr 0 … 22`, `base2` against `wood`):

| Shot | PSNR dB | flicker ×10⁻⁶ | pop | trail px |
|---|---|---|---|---|
| floor 00-the-clearing | 30.26 → 31.30 | 11.86 → 14.20 | 0.0040 → 0.0038 | 2 → 2 |
| floor 01-wood-pasture | 26.33 → 26.45 | 31.26 → 18.64 | 0.0054 → 0.0051 | 2 → 2 |
| floor 02-beech-high-forest | 21.68 → 20.08 | 55.07 → 49.55 | 0.0109 → 0.0118 | 2 → 3 |
| floor 03-the-old-road | 29.63 → 28.91 | 11.17 → 10.47 | 0.0040 → 0.0035 | 3 → 3 |
| floor 04-the-crags-top | 38.09 → 38.22 | 5.36 → 5.49 | 0.0013 → 0.0013 | 2 → 2 |
| floor 05-the-towers-foot | 34.41 → 34.76 | 5.29 → 5.33 | 0.0023 → 0.0018 | 2 → 2 |
| floor 06-the-ring | 25.49 → 24.46 | 89.17 → 45.97 | 0.0082 → 0.0075 | 2 → 2 |
| floor 07-the-mere | 34.10 → 34.54 | 35.86 → 30.16 | 0.0032 → 0.0030 | 2 → 2 |
| floor 08-the-stream-in-the-wood | 25.47 → 26.75 | 13.57 → 14.02 | 0.0062 → 0.0053 | 3 → 3 |
| floor 09-the-waterfall | 38.17 → 36.87 | 7.15 → 7.07 | 0.0018 → 0.0016 | 3 → 2 |
| floor 10-the-burn | 27.39 → 27.22 | 24.75 → 16.20 | 0.0047 → 0.0053 | 2 → 2 |
| floor 11-the-windthrow | 27.92 → 28.30 | 7.30 → 7.57 | 0.0110 → 0.0091 | 3 → 3 |
| floor 12-the-plateaus-edge | 38.06 → 38.39 | 4.67 → 4.92 | 0.0028 → 0.0028 | 3 → 2 |
| floor 13-the-towns-site | 24.33 → 25.02 | 36.04 → 23.82 | 0.0063 → 0.0056 | 2 → 2 |
| floor 14-the-wardens-arena | 22.97 → 23.27 | 47.66 → 30.70 | 0.0070 → 0.0065 | 2 → 2 |
| floor 15-the-wildwood | 24.78 → 22.99 | 43.23 → 38.96 | 0.1053 → 0.0594 | 6 → 2 |
| floor 16-the-fawn-in-the-glade | 23.88 → 28.27 | 86.70 → 59.24 | 0.0085 → 0.0061 | 3 → 3 |
| floor 17-the-fawn-under-the-beeches | 24.14 → 22.89 | 92.24 → 77.15 | 0.0116 → 0.0103 | 3 → 2 |
| floor 18-the-fawn-by-the-stream | 23.28 → 27.53 | 48.78 → 36.14 | 0.0070 → 0.0058 | 3 → 3 |
| floor 19-the-climbers-robe | 23.86 → 26.67 | 85.68 → 73.59 | 0.0101 → 0.0076 | 3 → 2 |
| floor 20-the-climber-close | 24.44 → 26.27 | 121.45 → 112.99 | 0.0143 → 0.0105 | 3 → 3 |
| floor 21-the-climbers-hood | 25.55 → 28.51 | 75.90 → 76.69 | 0.0114 → 0.0106 | 4 → 4 |
| floor 22-the-climbers-train | 25.73 → 27.86 | 109.71 → 93.34 | 0.0103 → 0.0086 | 3 → 3 |

PSNR down on 7 of 23; flicker up past a run's spread (13 %) on 1; pop on 0

The grown wood's flicker is down on 16 shots and up on 7, past a run's spread only on the
clearing's (+20 %); its pop up past the spread on none; its PSNR up on 16. The measures favour it
where the judges didn't.

**A lever tried: the forest kinds grown in the open** (the council's round 1 asked for the levers to
be tried, not named). Every living kind's archetype grown at `stand` 0 (the open) instead of its
forest stand (1 for the oak and the beech, 0.6 for the birch, the pine and the alder), for one
build of the floor with `GROWN_WOOD` (`scratchpad/p-open.py`, not committed), its 23 stills judged
blind against the floor as it is by three judges in fresh contexts (all three calibrations
passed): **after on 13, before on 9, equal on 1** (from 4 / 18 / 1 without it). The judges credit
"natural, varied crowns and branching" over the hand's "stylised lollipop clump" and "cloned
canopies", and still fault "branches loop around the trunk in implausible coils", snags as
"Y-forked poles", logs as "parallel cylinders", and the denser canopy's murk ("walls off the view
and flattens light"). The archetypes are `trees::archetypes::GROWN`, which the vale draws too:
with the lever, `wrela same examples/vale` (60 frames, against its build before) differs in its
commands from frame 0, so it moves the vale's trees, and **it isn't merged**: Ryan's rule is that it
merges only if the vale's panels say it helps, and they weren't run. The floor's own archetypes
(the lever for the floor alone) or the vale's panels with it are plan four's (its §11).
Next levers, after it: the looping whip limbs culled, the snags' forms, the trunks' flared feet.

**The grown fawn alone** (`GROWN_FAWN`, the hand's wood; its four shots: the fawn in the glade,
under the beeches, by the stream, and the climber's robe), one judge in a fresh context a round,
calibrated:

| Its age | The glade | Under the beeches | By the stream | The robe | The judge |
|---|---|---|---|---|---|
| 0.45 (21 kg, 0.66 m at the withers) | before | before | after | before | "reads as a miniature" beside the climber |
| 1.0 (as the vale's grazer, 0.72 m) | **after** | before | **after** | before | "shaded, muted and scaled like part of the scene" where the hand's is "a flat saturated orange cutout"; under the beeches and in the robe's shot it is "an implausibly small speck" |

(At 0.3, 12 kg and 0.57 m, the grown floor's judge read the fawn as tiny, and it wasn't judged
alone.) **Not adopted: `GROWN_FAWN` stays off (Blocked), at 1.0.** The two shots it loses are
composed for the hand's fawn, about 0.9 m at the withers (`fawn::character::SIZE` 0.6 of the
grazer), which the log and the distance hide a roe behind. Next levers: those two shots'
placements for a roe's size, or a larger deer's genome (a fallow deer's 0.9 m): both are Ryan's
calls on the floor's content.

**The cel floor** (`--set LOOK=styles::cel::Cel`, the floor as it is), G-look for a new style:

- (1) Its 23 stills: `build/f-cel` (1920 × 1080).
- (2) and (3) The fast instrument against the physical floor's bars (`wrela tool lookbook --
  build/fast-floor-cel --fast --look styles::cel::Cel --bars build/fast-floor-phys 0 … 22`):
  flicker and pop at most 1.5 times the physical's, trail at most its plus a pixel. **22 of 23
  hold**; the wildwood's trail misses, 12 px against 5 (the median of three runs of each look).
- (4) Three agent judges in fresh contexts scored the stills without a pair on the rubric and
  a *style* row (the treatment consistent across the picture, as one hand drew it), each pack
  with a control (a still's eight-pass blur among them; each scored it under its original). Means
  over the 23: **light 1.96, colour 2.09, value 1.91, depth 2.17, artifacts 1.13, style 1.04**
  (the vale's cel stills at D4–D6: 1.67, 1.58, 1.67, 1.50, 1.00, and 0.67 on the style row
  against its painting set).

| Shot | Light, colour, value, depth, artifacts, style | Flicker ×10⁻⁶ (bar) | Pop (bar) | Trail px (bar) | |
|---|---|---|---|---|---|
| floor 00 the clearing | 2, 3, 2, 3, 1, 1 | 9.28 (17.78) | 0.0030 (0.0060) | 2 (3) | holds |
| floor 01 wood pasture | 2, 2, 2, 2, 1, 1 | 26.02 (46.88) | 0.0049 (0.0080) | 2 (3) | holds |
| floor 02 beech high forest | 3, 3, 3, 3, 2, 2 | 34.17 (86.08) | 0.0070 (0.0161) | 2 (3) | holds |
| floor 03 the old road | 2, 1, 1, 2, 1, 1 | 7.15 (16.76) | 0.0031 (0.0060) | 3 (4) | holds |
| floor 04 the crags top | 0, 1, 0, 1, 0, 0 | 3.08 (8.03) | 0.0010 (0.0020) | 2 (3) | holds |
| floor 05 the towers foot | 2, 2, 2, 1, 1, 2 | 6.71 (7.92) | 0.0025 (0.0034) | 2 (3) | holds |
| floor 06 the ring | 2, 2, 2, 2, 1, 1 | 75.93 (133.76) | 0.0074 (0.0123) | 2 (3) | holds |
| floor 07 the mere | 2, 1, 2, 1, 1, 0 | 32.01 (53.80) | 0.0023 (0.0047) | 1 (3) | holds |
| floor 08 the stream in the wood | 2, 3, 2, 3, 1, 2 | 13.09 (20.35) | 0.0035 (0.0092) | 2 (4) | holds |
| floor 09 the waterfall | 1, 1, 1, 1, 1, 1 | 6.96 (10.71) | 0.0017 (0.0028) | 2 (3) | holds |
| floor 10 the burn | 2, 2, 2, 2, 1, 2 | 19.00 (37.07) | 0.0043 (0.0071) | 2 (3) | holds |
| floor 11 the windthrow | 2, 2, 2, 3, 1, 2 | 6.08 (11.03) | 0.0044 (0.0166) | 3 (4) | holds |
| floor 12 the plateaus edge | 1, 2, 1, 2, 1, 1 | 3.47 (6.99) | 0.0020 (0.0042) | 4 (4) | holds |
| floor 13 the towns site | 2, 3, 2, 2, 2, 2 | 26.90 (54.07) | 0.0059 (0.0094) | 2 (3) | holds |
| floor 14 the wardens arena | 2, 2, 2, 3, 2, 2 | 38.44 (71.09) | 0.0061 (0.0104) | 2 (3) | holds |
| floor 15 the wildwood | 3, 3, 3, 3, 2, 2 | 26.57 (55.78) | 0.1251 (0.1597) | 12 (5) | misses (median of three, each look) |
| floor 16 the fawn in the glade | 2, 2, 2, 2, 1, 0 | 80.17 (130.05) | 0.0072 (0.0127) | 3 (4) | holds |
| floor 17 the fawn under the beeches | 3, 3, 3, 3, 1, 1 | 89.01 (138.37) | 0.0061 (0.0174) | 2 (3) | holds |
| floor 18 the fawn by the stream | 2, 2, 2, 1, 1, 1 | 36.79 (73.18) | 0.0052 (0.0110) | 3 (4) | holds |
| floor 19 the climbers robe | 2, 2, 2, 3, 1, 0 | 77.98 (128.52) | 0.0078 (0.0152) | 2 (3) | holds |
| floor 20 the climber close | 2, 2, 2, 2, 1, 0 | 106.92 (182.12) | 0.0099 (0.0216) | 2 (4) | holds |
| floor 21 the climbers hood | 2, 2, 2, 2, 1, 0 | 69.06 (114.28) | 0.0101 (0.0170) | 4 (5) | holds |
| floor 22 the climbers train | 2, 2, 2, 3, 1, 0 | 83.41 (164.62) | 0.0083 (0.0155) | 3 (4) | holds |

The judges see the cel treatment in the foliage, the bark and the clouds, and not on the
climber's cloak or the fawn ("soft and photographic"), nor in the dense ground cover's noise. The
characters are lit by their own shading (`content/climber/draw.wrela`'s garment, `engine::character`),
which reaches the look's substance (`Style::written`) but not its `shade`; the grass goes through
it (`lit::shown`). The cel floor builds and is judged; the floor's look stays `Physical` (the
plan's default). **Blocked:** the wildwood's trail. Next lever: the cel outlines' history under
temporal AA at the near trunk's silhouette. The characters shaded through the look is the next
item for its style row.

**G-pixels** (`wrela same <pkg> --against <build> --frames 240 --size 1280x720`):

- The herd: **the same** as `build/base-0.9/herd` (240 frames, the same commands and pipelines,
  its last frame within 0.00).
- The climber lab: its last frame **within 0.00** of `build/base-0.9/climber-lab`, its commands
  differing from frame 0 (the engine's command stream, Parts C–G).
- The capybara: differs from `build/base-0.9/capybara` (its last frame 4.42 from it, 10 % of its
  channels): the game itself changed since 0.9.0 (rywible/wrela#343, #345, #351, other sessions'
  work). Against its own last change's build (b3081e38, #351, built here on 0.18.0) it is **the
  same** (240 frames, the same commands). So Parts F, G and H moved none of its pixels.
- `build/base-0.9` was gone from the disk: rebuilt as §B0 made it, from 807f9c08^ on 0.9.0 (`wrela
  build <pkg> -o build/base-0.9/<name>`; 0.9.0's build has no `--release`).

**The floor's walk at `Epic`** (this machine's Chrome at 1080p; `wrela tool bench -- --scene
last-green-walk --sustained 120 --no-docs`, and `--look styles::cel::Cel`, and `--starts 3`):

| Build | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | GPU peak MiB | Sustained 120 s (minute 1, last) |
|---|---|---|---|---|
| the floor | 32.20 / 126.76 / 182.52 | 33.45 / 75.49 / 152.69 | 743.7 | 31.88, 31.94 (1.00) |
| the cel floor | 32.62 / 127.36 / 183.76 | 34.02 / 75.99 / 154.24 | 763.2 | 32.48, 32.51 (1.00) |

The starts, medians of three: **cold 36.9 s** (36.95, 36.83, 37.07), **warm 14.6 s** (14.38, 14.78,
14.62); the first frame 11.4 s cold, 3.0 s warm. Here the floor's gates all miss as they did on
the M4 at C5–C6 (docs/bench.md, "Whether D27 holds"): the M4's numbers are the gates', to measure
again there with Part H's switches.

The grown floor's optimized build runs past the 600 s a tool's build may take
(docs/toolchain-gaps.md), so the bench can't build it: its frame against the floor's is
`wrela profile` (natively, 1920 × 1080, three pairs interleaved):

| Pass (ms, the median of the pairs' runs) | The floor | Grown (`GROWN_WOOD`, `GROWN_FAWN`) |
|---|---|---|
| **GPU frame span** | **34.11** | **32.02** (−6.1 %) |
| vegetation | 6.31 | 4.97 |
| vegetation prepass | 6.02 | 3.16 |
| terrain | 2.26 | 3.98 |
| shadows far | 11.37 | 18.83 |
| probes again | 2.64 | 2.26 |
| motion | 1.01 | 0.51 |
| creature | 1.05 | 1.08 |
| look | 2.88 | 2.97 |

(`wrela profile games/last-green --size 1920x1080 --workers 4 --compare build/floor-default --pairs
3`, 60 warmup and 120 measured frames at the floor's start; the other passes within 0.1 ms.) The
grown kinds draw fewer cards (the vegetation passes), the caused ground costs the terrain 1.7 ms,
and the far cascade draws more (its range 7.96–11.67 against 17.83–19.16).

## H2, the final gates (2026-11-01, toolchain 0.18.0)

docs/plan-world-class-3.md §H2. Linux pop-os (Radeon "Lucienne", RADV), **not the M4 Air**: the
gates defined on the M4 (G-sustained, G-memory, G-cold, G-warm, G-tier, G-grown's Chrome run and
its time) are recorded here as this machine's numbers and listed to measure there. The thirteen:

1. **`wrela suite`** on 0.18.0 (the plan's 0.10.0 is long past: main's pin, rywible/wrela#351):
   **148 of 150 checks pass** (23 m 58 s, before the review's fixes). The two that fail,
   `engine/tests/frame` (2 of 16) and `examples/vale` (3 of 4), are the debug build's budget under
   the suite's load ("a step of work … past twice the playing budget"); each passes alone, as do
   `fmt`, `content/kit` and `games/last-green`, and, after the review's and the council's fixes,
   `fmt`, `games/last-green` (103 tests) and `studio/bench` (22). It isn't an all-pass suite: the
   budget panics under load are plan three's throughout.
2. **G-reference** (D38): **misses**, the final panel under "Public visual references" (above): 0 of
   96 judgements prefer the still or rate it equal, every pair lost 8 to 0, the shots' and the
   manifest's hashes B5's.
3. **G-sustained** (the M4's): not measured here. The last M4 numbers are C6's (the vale's GPU p50
   18.37 ms, the floor's 13.34: "Whether D27 holds"). Here, 120 s sustained, the vale's walk 57.12
   ms (Part G's, "Part G's walk and memory") and the floor's 32.20 ms: no comparison with the M4's.
4. **G-memory:** the GPU peak on the walk here, the vale 606.4 MiB (Part G's) and the floor 743.7
   MiB: both over 512, as on the M4 at C4–C6 (the vale 539.8, the floor 735.6).
5. **G-cold, G-warm** (medians of three, here): the floor cold 36.9 s, warm 14.6 s; the vale's
   cold 10.4 s (11.03, 10.29, 10.42), warm 7.5 s (7.50, 7.17, 8.67). The M4's at C5: the vale 6.69 / 2.21 s, the floor 13.33 / 5.38 s.
6. **G-tier:** held at C7 (each lower tier cheaper than the one above, on both scenes), before Parts
   D–H; to measure again on the M4 (Part H changes no tier; Parts D–G changed what each draws). The floor's pipelines per tier: Epic 170, High 168, Medium 166, Low 162.
7. **G-grown:** **holds here.** `examples/vale/tests/land`'s `every_plant_the_vale_draws_is_grown`:
   26,156 instances, 0 authored (E8 counted 26,922: Part G's logs laid where the ground holds them,
   or not at all). `wrela tool grow -- --hash --package examples/vale`: two native runs and one in
   Chrome, the archetypes' hash `a003a12bbd48d613` (508,280 bytes) and the first ring's shapes'
   `362677e6de00fa1d` in each, the same as at E7; the first ring in 545 and 503 ms natively and
   880 ms in Chrome (the plan's 1,000 ms). The archetypes' growth at build, 19.7 s here (E7). The
   M4's Chrome run, the third device, is to measure there.
8. **G-steer** (E6, unchanged since): the identity fits 0.874 (the oak) and 0.880 (the beech) hold
   0.85; the birch's 0.689 is Blocked ("E6, steering"); the photo fits 0.761 and 0.659, recorded.
9. **G-style:** `wrela tool styles -- --check` holds: each of the six looks builds the vale (109–111
   pipelines), and each look's file was added in a commit that changed no engine file (`cel.wrela`
   in ca931cda, `painterly.wrela` in 525ef4c9, `ink.wrela` in 7269c195, the two fitted looks in
   ee285c32; `physical.wrela` the engine's own). The coherence table and bars: D4–D7b's (`Cel` holds
   its 60, `Painterly` 59, `Ink` Blocked), and the cel floor's 22 of 23 ("H1").
10. **G-motion:** **Blocked** at F9 (0 % against 40 %, four rounds: "F9, the motion review").
11. **G-size:** **holds.** `wrela build <pkg> --optimize` (Binaryen's `wasm-opt`, as the release
    ships a game): the runtime (`main.js`) 77,796 bytes, **26.0 KB gzipped** (≤ 1 MB). The vale's
    program and shaders 9.25 MB raw, **3.23 MB gzipped** (2.49 + 0.74); the floor's 18.03 MB raw,
    **5.59 MB gzipped** (4.02 + 1.58), against ≤ 6 MB. The floor's cold load as served, 6.0 MiB
    (its walk's "Loaded MiB", ≤ 8); the vale's 4.0 MiB. No `.br` or `.gz` in either build (D26).
    The release itself (`wrela tool release`) now ships the console, the docs and the capybara alone
    (rywible/wrela#343 onward), so the two scenes are measured from their own builds.
12. **G-pixels:** the herd the same as `build/base-0.9`, the climber lab's last frame within 0.00 of
    it, the capybara the same as its own last change's build ("H1").
13. **G-bench:** "Whether D27 holds" (above), finished here: a row per gate per scene per device.

## 2026-11-01: the console's attract build, opened hidden under the opening (toolchain 0.18.0)

When the game playing itself shows behind the console's home, from the page's start: the iframe
of `capybara/#attract` added (`host::open`) and made inert (`host::underlay`), seen by a
`MutationObserver` on the page. The console, the capybara's play build and its session built
with `wrela build` (release, no `wasm-opt`) into one directory with a catalog written by hand,
served by `wrela tool serve` on this machine (Apple M4, macOS 26.6.2), in the Claude app's
browser pane, the browser's cache and shader cache warm; one run each, a visit (the 7.7 s
opening plays), not a reload.

| Build | Opened | Shown |
|---|---|---|
| Before: opened once the home had rested 3 s on the game (console/attract.wrela `REST` 3.0) | 17.7 s | not logged (later) |
| After: opened once the selection has stayed 0.5 s and its files are fetched, hidden | 1.0 s | 8.2 s |

"Before" logged only when each iframe first appeared, polling every 50 ms. The opening's frames while the attract build loads behind it (after, the
pane in view): 541 frames in 9 s, none over 34 ms (50th percentile 17 ms, 95th 18 ms). A
cold load over the network (the WASM and the shaders compiled for the first time) wasn't
measured: it moves "shown" only when it takes longer than the opening.
