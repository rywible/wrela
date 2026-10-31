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

docs/plan-world-class-3.md §0.1, D27 (**proposed**). The table has a row per gate per scene per
device, each cell "holds", "misses by ..." or "blocked: ...". Part C started it with the M4's
performance rows; Part H finishes it. The M4 Air numbers are Chrome at 1080p, Epic,
toolchain 0.10.3: the frame from C6's 600 s sustained run, memory from the same run's GPU peak,
and the starts from C5's three-run medians (salts new to the machine).

| Gate | Scene | M4 Air | Note |
|---|---|---|---|
| GPU frame p50 < 12 ms, after 600 s | vale | misses by 6.37 ms (18.37) | Its two vegetation passes draw every card twice, 18.8 ms serial (C6). |
| | floor | misses by 1.34 ms (13.34) | Its fog is 6.5 ms serial, beside the cards. |
| Paced p99 < 20 ms | vale | misses by 11.48 ms (31.48) | The same frames' overruns. |
| | floor | misses by 15.45 ms (35.45) | |
| 0 of 1,200 paced frames over 33 ms | vale | misses by 6 | |
| | floor | misses by 27 | |
| 1,200 frames begun within 20.4 s, one in flight | vale | misses (42.9 a second; 58.8 needed) | |
| | floor | holds, by 0.02 a second (58.84) | Within run-to-run noise: the 120 s Epic run after it begun 56.9. |
| GPU memory < 512 MiB | vale | misses by 27.8 MiB (539.8 peak) | It held 478.8 at playable. Its walk now runs 1,260 frames (C4's 1,020, under 512 at 508.2), and the probes baked round the moving eye are what grows. |
| | floor | misses by 223.6 MiB (735.6) | Its built things' realizations (C4). C4's calibration lever, which took ~24.5 MiB off, was reverted in review: its count missed quads past the room it calibrated in. |
| Cold start < 5 s | vale | misses by 1.69 s (6.69) | Compile-bound at the GPU process's pace (C5). |
| | floor | misses by 8.33 s (13.33) | Its creature fields' realizer pipelines. |
| Warm start < 3 s | vale | **holds** (2.21) | |
| | floor | misses by 2.38 s (5.38) | Load-bound: its frames 3-102. |
| Lower tiers measured, each under the one above | vale, floor | **holds** (C7) | Epic > High > Medium > Low on both. |
| Other devices | all | blocked: deferred to a later plan (D37) | |
| Fidelity (G-reference) | vale | pending: Part G's review | |
| Grown plants and creatures (G-grown) | vale | pending: Parts E and F | |
| Four styles, no change to `engine/` | vale | **holds** (D10) | `wrela tool styles -- --check`: `Cel`, `Painterly`, `Ink` and the two fitted looks each added in a commit that changes no engine file; `Physical` the engine's own. Coherence: `Cel` holds its 60 bars, `Painterly` 59, `Ink` Blocked ("D4–D7b"). | |

## The hypotheses (docs/plan-world-class-3.md §7)

Each ships only if its own gate says it pays; each gets its outcome here when it's decided (§H3).

| ID | Hypothesis | Decided at | Outcome |
|---|---|---|---|
| H1 | Specialization alone (C1) recovers at least 2 ms of the clearing's GPU frame and 3 ms of the floor's | C1, C6 | not held: compiling the switches out moved nothing (the clearing 12.39 → 12.68 ms GPU p50; its fog and distance volume were off at run time already: "C1, every switch a constant") |
| H2 | Most of the visual-review gain comes from the grown floor (E3–E5), not from lighting | E8's panel against D11's | pending |
| H3 | Mark coherence (D7) is the hardest problem in Part D and the bars will be held by `Cel` and `Ink` before `Painterly` | D7's table | half held: mark coherence was the hardest problem (Ink is Blocked on it), but `Painterly` holds 59 of 60 bars and `Ink` none of its flicker bars ("D4–D7b, the looks' coherence") |
| H4 | The learned style beats hand-tuned `Painterly` on flicker and pop at equal preference | D9 | withdrawn (D41): the learned style is deferred until Ryan rules on docs/citations.md's rule |
| H5 | Growth is byte-identical across devices on the CPU without fixed-point arithmetic | E7's hashes | pending |
| H6 | Motion, not rendering, decides the creature shots (9, 10) | F10's panel against E8's | pending |
| H7 | Bounce colour (G1) moves the "light" row more than any other single item | G1's panel | pending |
| H8 | The vale needs no more than three of Part G's items to reach G-reference | G's tallies | pending |

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
