# Bench

Performance claims about wrela's programs are recorded here: what was measured, on what, with
which build. The plan these come from is [plan-world-class.md](plan-world-class.md).

Each `wrela tool bench` table below is one run: per scene, the GPU's frame span (saturated,
timestamped), the intervals between paced frames as a player sees them (and how many were over
16.7 and 33 ms), each paced frame's CPU time, the native host's frame call (its GPU waits
included), a cold start (first frame, the program's `playable` print, the pipelines' build,
the bytes loaded) and the GPU memory at its most. 1920×1080 (the capybara also 2940×1912), 60
warmup frames; Chrome is headless Chrome stable (155.0.8059.86) driven by `std::tool::browser`.

**Reading the intervals.** Headless Chrome's paced frames jitter round the display's 16.67 ms: the
leapling, whose GPU frame is 2.1 ms, has 226 to 302 of its 600 intervals over 16.7 ms and a 99th
percentile of 18.2 to 19.6 ms. So a count over 16.7 ms measures the timer's jitter as much as
the program, and a frame the program missed shows as an interval over 33 ms (a refresh skipped).
The interval columns of one run move by a few ms from run to run (the repeats below).

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
| M4 Air | the Last Green's opening | | | | < 5 s: | | < 512 MiB: |
| M4 Air | the Last Green's walk | | | | < 5 s: | | < 512 MiB: |
| M4 Air | the clearing's path | | | | < 5 s: | | |
| M4 Air | the capybara's run | | | | < 5 s: | | |
| M1 Air 8 GB | the floor | 60 fps: | | | < 8 s: | | |
| Iris Xe laptop | the floor | < 25 ms: | over 50 ms = 0: | | | | |

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
| Neural impostors (14.2) | `Quality::neural` | on past 160 m if PSNR ≥ 32 dB and the pass ≤ 1.3× the atlas's | PSNR 13.86 dB (rung D over all 256 views); every kind through the net: the frame 421.8 ms against 16.6 | **off** |
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
  the wind's or the creatures'.

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
  held out), written as `content/kit/great_tree_net.wrela` (the great tree: the floor's one tree
  whose kind the fixture bakes).
- **Time:** the great tree alone through its net: the GPU frame 16.64 ms against 16.65 (a single
  tree, too little of the frame to tell). Every kind through the same net (`EVERY_KIND`, a
  measuring setting whose images are wrong): the vegetation pass **390.0 ms** against 5.4 and the
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
  are passed (asked: under 1 %).
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

None yet. The plan's §0.6: three.js, Babylon and Unity scenes for the comparison live in a
sibling repository Ryan makes; their numbers go here by hand, with the command and the date.

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

## 2026-10-26: AMD neural-rendering budget baseline (a3ff61a4)

Ryzen 5 5500U, AMD Radeon Graphics **RADV RENOIR** (1002:164c), Pop!_OS,
Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea. Wrela 0.9.0
(compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23), Google Chrome 154.0.8037.57.
All work ran on Pop, sequentially, after the reference and model experiments had stopped.

These are fixed-pose clearing renderer fixtures, with simulation time held at zero,
production heuristic reconstruction, and neural paths off. Scale and planned quality are
fixed (pace=false, measured=false). They include the renderer and presentation passes,
not gameplay simulation. Each run has 240 warmup and 240 measured frames, workers=1,
release defaults. Chrome uses a saturated GPU run and a separate paced run with two frames
in flight; its simulation rate is 60. **This is not the G6 120Hz, 20-minute protocol.**
The 1× fixture uses its initial camera; the 4× adapter selects clearing shot 0. Do not infer
a pure scale speedup from these different views. The 1080p and 4K 4× runs use the same build.

| Run | GPU span median / p95 ms | Frame call median / p95 ms | Paced interval median / p95 ms |
| --- | ---: | ---: | ---: |
| Native, 1080p, native input | 48.474 / 51.424 | 51.592 / 54.403 | — |
| Chrome, 1080p, native input | 46.155 / 48.670 | 1.730 / 2.035 | 47.980 / 95.190 |
| Chrome, 1080p, 4× | 21.696 / 24.226 | 1.740 / 2.090 | 23.550 / 45.540 |
| Chrome, 4K, 4× | 47.487 / 49.936 | 1.715 / 2.015 | 52.095 / 102.610 |

Native frame calls include GPU waits. Chrome CPU calls, GPU spans and paced intervals
are separate quantities; their medians are not added. The profiler’s p95 is nearest rank.
These short, single diagnostic runs establish no confidence interval or sustained tail bound.

| Production pass | Chrome 1080p 4× median / p95 ms | Chrome 4K 4× median / p95 ms |
| --- | ---: | ---: |
| temporal | 4.427 / 4.460 | 17.773 / 17.866 |
| clouds | 6.319 / 7.109 | 6.415 / 7.320 |
| vegetation prepass | 2.153 / 2.212 | 4.376 / 4.411 |
| vegetation | 2.454 / 2.473 | 4.006 / 4.029 |
| bloom | 2.039 / 2.064 | 8.206 / 8.258 |
| probes again | 1.934 / 4.099 | 1.944 / 4.088 |
| look | 0.470 / 0.475 | 1.480 / 1.487 |

Labels may overlap and their medians are not additive. At 4K the temporal pass alone is
17.773 ms median, versus G6’s reconstruction p95/p99 targets of 2/3 ms. Both 4× complete
GPU spans are also well above the 8.33 ms complete-frame target. No acceptance gate passes.
The measurements make temporal reconstruction, clouds and bloom concrete targets for
whole-pipeline optimization; quality must be checked alongside any cost reduction.

The native and Chrome 1× builds both hash to `45b6fdf16ebc84ced56f5d8b7a94f8f76dd29a4e9779ea1d8f620242d735fae2`.
The 4× timing adapter builds hash to `34d728b94e7ad4f118ff3cb9a4afd8bb7fe17983ba68cb16e29a2aa687cf6e65`.
The adapter only calls the existing clearing fixture’s configure(4,0,0), then its frame.
Its complete source and manifest, commands’ workload fields, adapter reports and artifact hashes
are retained at `build/neural-evidence/amd-budget-baseline.json` on Pop. Reports and retained
builds/results are under `amd-native-1080`, `amd-chrome-1080`, `profile-4x` and
`profile-4x-4k` in that evidence directory. Native 1× shares Chrome 1×’s retained build.
The retained browser results contain the paced run’s timestamps; the top-level profile
contains the saturated run’s summary. Do not silently mix those populations.

Reproduction: `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json wrela profile <fixture> --frames 240 --warmup 240 --workers 1 --size 1920x1080 --json`,
adding `--chrome` for the browser, and `--size 3840x2160` for 4K.
Use `studio/reconstruct/clearing` for the native-input fixture and the retained
`build/neural-evidence/profile-4x/fixture` adapter for 4×.


## 2026-10-26 AMD neural patch v1

Development reconstruction-only fixture, moving analytic case 1 at 4× linear scale.
This is not the clearing renderer, a complete game frame, or G6's 120Hz/20-minute protocol.
Pop!_OS AMD Ryzen 5 5500U / Radeon Graphics (RADV RENOIR, PCI 1002:164c);
Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea; Chrome 154.0.8037.57.
Wrela 0.9.0 compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
The browser's adapter string says `rdna-2`; that string is not a verified hardware architecture.

Each run: release default, no wasm-opt, Chrome, one worker, 240 warmup + 240 measured frames,
fps 60 simulation, `serial=false`, no other owned GPU work. A generated wrapper loads the
same 2,520-byte trained half weights before warmup. No truth or feature export runs in this
profile. Input sizes: 480×270→1920×1080 and 960×540→3840×2160. The source model is
`patch-1793063898-289696714`, packed weights SHA-256
`4b9b969a9fd191f25f69cdc6f348f4464f0b2bb7b9d6bd9be687b64bee62fe18`.

| Kernel | Output | Encode median / p95 ms | Decode median / p95 ms | Fixture GPU span median / p95 ms |
| --- | --- | ---: | ---: | ---: |
| Scalar | 1080p | 10.60496 / 10.61512 | 1.11408 / 1.49572 | 12.57280 / 12.98068 |
| Vector encoder | 1080p | 0.82620 / 0.94400 | 1.12740 / 1.50280 | 2.89168 / 3.21676 |
| Vector encoder, explicit decoder | 1080p | 0.81444 / 0.92116 | 1.09000 / 1.49432 | 2.81808 / 3.15860 |
| Scalar | 4K | 41.86348 / 41.90168 | 4.42908 / 5.96628 | 49.45508 / 51.04964 |
| Vector encoder | 4K | 3.17796 / 3.58136 | 4.42456 / 5.96656 | 11.02012 / 12.33172 |
| Vector encoder, explicit decoder | 4K | 3.18584 / 3.60024 | 4.30096 / 5.96528 | 10.88888 / 12.32988 |

GPU span also contains analytic sample rendering and the diagnostic display transform.
Pass medians/p95s are not summed to invent a reconstruction or frame percentile. The
encoder improvement is about 13× in these short runs. The extra decoder rewrite provides
no material p95 improvement; it is not reported as a new speedup. G6's reconstruction budget
is not met: the output pass alone exceeds its p95 limit, before encoding.

Build SHA-256, equal across the two output sizes for each kernel:

- Scalar: `3e2c82996aa84ca67a0b08e476cef5ad30c97fa0700da7dee7e8aa3fa10b42c0`.
- Vector encoder: `6a6b9ca0bf6eda00940ed48efa82a9db9097c28196013d33477becd986532ec8`.
- Vector encoder, explicit decoder: `48f381471b81f4445bf74619366ae275fdf8cc7f99c88c49aa4cb5663fd6244c`.

Reports, generated wrappers, compiled shaders/WASM and paced browser artifacts are retained
in `build/neural-evidence/patch-v1-timing`, `patch-v1-vector-timing` and
`patch-v1-unrolled-timing` on Pop!_OS. Reproduction for each retained wrapper:
`wrela profile <directory>/fixture --chrome --frames 240 --warmup 240 --workers 1 --size 1920x1080 --json`;
repeat at 3840x2160. Rebuilding a wrapper after source changes is not the original build:
the saved build directories and hashes identify the measured artifacts.

The profiler's top-level GPU data comes from its saturated run. CPU frame-call and interval
data come from a separate paced run with two frames in flight; do not mix those sample sets.
The retained `build-1080`/`build-4k` results are the paced run. No p99, stable thermal,
memory-budget, complete-frame or best-in-world claim follows. Independent replay and
continuous scalar/optimized comparisons pass, but **quality rejects this model**:
[development results and controls](neural-rendering-progress.md#input-resolution-neural-patch-model-faster-kernel-rejected-quality).


## 2026-10-26 AMD direct history and source geometry

Same AMD RENOIR / Ryzen 5 5500U, Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea,
Chrome 154.0.8037.57 and Wrela 0.9.0 compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23` as the patch-v1 measurements above.
Analytic moving case 1, 4× linear scale, fixed patch-v1 half weights
`4b9b969a9fd191f25f69cdc6f348f4464f0b2bb7b9d6bd9be687b64bee62fe18`.
Maximum history share 90%, age-bounded cold start, vector encoder.
Release default, no wasm-opt, Chrome, one worker, 240 warmup + 240 measured,
fps 60 simulation, `serial=false`, sequential runs with no other owned GPU work.
No feature/geometry/target export is active in the measured wrapper.

The sampled variant loads low-resolution motion/depth to propose correspondence. The source
variant instead performs two geometry-only authored-field queries per output pixel.
That extra work is included in its output-stage time; it changes the information budget and
is a **full-stack** ablation. No extra base-colour samples or reference-derived routes are used.

| Geometry path | Output | Encode median / p95 ms | Output stage median / p95 ms | Fixture GPU span median / p95 ms |
| --- | --- | ---: | ---: | ---: |
| Sampled buffers | 1080p | 0.81924 / 0.93700 | 1.93696 / 2.18216 | 3.65532 / 3.87524 |
| Authored source queries | 1080p | 0.82600 / 0.96784 | 1.49396 / 1.67916 | 3.25284 / 3.51024 |
| Sampled buffers | 4K | 3.20168 / 3.59812 | 7.53120 / 8.69756 | 14.01756 / 15.06484 |
| Authored source queries | 4K | 3.19400 / 3.60632 | 5.63992 / 6.51420 | 12.27856 / 12.87656 |

The source variant's output-stage median is 22.9% / 25.1% lower here. This is not a claim about
the cost of arbitrary 3D source queries, and these short measurements are not a stable-thermal
or 120Hz/20-minute test. G6 is not met: even the output-stage p95 alone exceeds its budget.
The fixture span also includes analytic base sampling and the diagnostic display transform;
do not sum pass medians/p95s to invent reconstruction or complete-frame percentiles.

Build SHA-256 (same for both output sizes within a variant):

- Sampled: `32fcb4dbf1b3c0e1041902daf049ceb04b60c80573210a2e94039d0b69a7892f`.
- Source: `ec4918718423470a0fb2c99c00860a5ae08152f97d7eae96b0fd60f54e066c49`.

Artifacts are under `build/neural-evidence/patch-history-v1/timing-sampled` and
`timing-source` on Pop!_OS: reports, exact wrappers, compiled WASM/shaders and paced results.
Run `wrela profile <directory>/fixture --chrome --frames 240 --warmup 240 --workers 1 --size 1920x1080 --json`,
then 3840x2160. Saved builds identify the original measurement; rebuilding after code changes
does not. An initial wrapper dependency-path compile failure is retained separately and
contributes no timing. Top-level GPU reports are from saturated execution; retained per-frame
browser artifacts are from the separate paced run. No p99 or complete-memory claim follows.
[Quality and controls](neural-rendering-progress.md#direct-history-and-source-geometry-ablations)
reject all current variants for promotion.


## 2026-10-26 AMD learned history confidence

Same Pop!_OS AMD Ryzen 5 5500U / Radeon Graphics (RADV RENOIR, PCI 1002:164c),
Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea, Chrome 154.0.8037.57 and
Wrela 0.9.0 compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
The browser's `rdna-2` adapter text is not used to identify the hardware architecture.
Moving analytic case 1, 4× linear scale, vector patch encoder and two source-geometry queries
per output pixel. Same frozen 2,520-byte patch weights as above, maximum history share 90%.
The candidate adds a 13-parameter logistic confidence gate (64 stored bytes), model
`gate-1793068787-531418279`, SHA-256
`1742bb47661055d0f53c48fc3b7445d28cc442b76951947f877c5bdd9f413495`.

Release default, no wasm-opt, Chrome, one worker, 240 warmup + 240 measured frames,
fps 60 simulation, `serial=false`. Sequential order: 1080p control/candidate, 4K
control/candidate, with no other owned GPU, build or scorer work. No target or diagnostic
feature export runs in the measured wrappers.

| Variant | Output | Encode median / p95 ms | Output stage median / p95 ms | Fixture GPU span median / p95 ms |
| --- | --- | ---: | ---: | ---: |
| Constant history | 1080p | 0.81924 / 0.92980 | 1.45064 / 1.68872 | 3.18172 / 3.45120 |
| Learned confidence | 1080p | 0.81936 / 0.93996 | 1.46980 / 1.66556 | 3.19852 / 3.43820 |
| Constant history | 4K | 3.19540 / 3.57844 | 5.63216 / 6.50976 | 12.25172 / 12.92108 |
| Learned confidence | 4K | 3.18816 / 3.57220 | 5.73032 / 6.50288 | 12.35944 / 12.87544 |

Median output-stage deltas are +0.01916 / +0.09816 ms; the slightly lower candidate p95s
illustrate why these short unpaired runs are not a precise overhead or significance bound.
This is a reconstruction fixture, not a complete game frame. Source queries are included
in the output stage, and analytic sampling/display are included in the fixture span.
Pass percentiles are not additive. Both variants miss G6 even on output-stage p95 alone;
no 120Hz/20-minute, p99, thermal or complete-memory result is claimed.

Build SHA-256 (identical across the two output sizes within each variant):

- Constant: `2f44ed7c0cec00479c70a1fbe055a4c0a07664e35182cfae1c343ebce320ca7e`.
- Learned: `655823ae868ce82def55c3338a48d768efe6d9b09f6792c1cadd393934445972`.

Artifacts: `build/neural-evidence/gate-v1/timing-control` and `timing-gated` on Pop!_OS,
including exact wrappers, reports, compiled builds and paced browser artifacts.
Profile source archive SHA-256:
`4b4a798e869a65707547b63081508a87f50fd551eb45f880776e09ad0e39f503`.
Reproduce with `wrela profile <directory>/fixture --chrome --frames 240 --warmup 240 --workers 1 --size 1920x1080 --json`,
then 3840x2160; retained builds identify the original measurements. Top-level GPU data comes
from saturated execution, CPU/frame intervals from a separate paced population.
[Full recurrent quality and controls](neural-rendering-progress.md#learned-history-confidence-one-step-gains-recurrent-rejection)
reject all three trained gates; the profiled model was not selected for deployment.


## 2026-10-26 Pop!_OS recurrent confidence training

CPU training on the same AMD Ryzen 5 5500U machine; all renderer validation captures use
its Radeon Graphics / RADV RENOIR GPU. External PyTorch 2.5.1+cpu, NumPy 2.2.6, two Torch
threads and one interop thread. Renderer Wrela 0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`, source based on `4c03ae10`.
No RTX, Mac training/rendering, paid compute or other owned concurrent training/profile work.

Thirteen trainable gate parameters, frozen patch-v1 spatial weights, same initial
`gate-1793068787-531418279` weights for both runs. Sixteen complete training case/scale
combinations (cases 0/3/5/7 × divisors 1/2/3/4), 32 frames each at 192×108. Every output pixel
participates; history is the candidate's own output with gradients through all 32 frames.
256 optimizer updates, seed 3, Adam rate 0.03→0.003. Validation captures are separate.

| Temporal MAE coefficient | Training wall seconds after load | Process peak RSS KiB | Stored output weights |
| --- | ---: | ---: | ---: |
| 0.25 | 139.84 | 4,402,096 | 64 bytes |
| 1 | 137.68 | 4,402,096 | 64 bytes |

Both models ran sequentially in one process. Initial dataset loading/hash/preparation took
9.45 seconds, excluded from the table; peak RSS is the process high-water mark, not a separate
incremental measurement for each model. The process's caches were not controlled. A separate
full 32-frame 4× forward/backward probe took 0.580 seconds; it is a training microbenchmark,
not GPU inference latency. No new GPU runtime, thermal, p99 or G6 measurement is implied.

The benchmark reuses captured data and a pretrained spatial model. It excludes source
generation, dataset capture, independent scoring, broader assets and deployment validation;
it therefore does not establish G7's source-to-validated-representation deadline.
[Both trained models fail quality promotion](neural-rendering-progress.md#full-recurrent-training-improved-tradeoff-still-rejected).
Their model SHA-256 values are `c9c4ccccf9a348cd9fdb58a3e8b44b41e62823804ef1e9a8bc8f88ea0abbe4df`
and `3cab57a84072d3289cc5ada7e5336a99d47631eab78ae56e908bc2fa00cbf60f`.
The complete source, dataset hashes, curves, timings and independent AMD GPU comparisons
are retained in `build/neural-evidence/recurrent-gate-v1/manifest.json` on Pop!_OS.


## 2026-10-26 Pop!_OS joint spatial and history training

Same AMD Ryzen 5 5500U / Radeon Graphics (RADV RENOIR) Pop!_OS machine.
Training uses external PyTorch 2.5.1+cpu and NumPy 2.2.6, two Torch threads/one interop thread.
Actual quality captures use the AMD GPU with Wrela 0.9.0 compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`. Base source `8e43ccb6`, renderer archive
`4b4a798e869a65707547b63081508a87f50fd551eb45f880776e09ad0e39f503`.
No other owned training, capture, build or profile work overlaps the timed training runs.

All 1,260 spatial and 13 gate parameters train through 32-frame feedback. Same initial patch-v1
and recurrent coefficient-1 gate for both candidates; cases 0/3/5/7, all four scales, all
192×108 output pixels. Seed 3, 256 Adam steps, spatial rate 0.001→0.0001 and gate rate
0.01→0.001. The declared norm cap of 1 clips zero steps.

| Temporal MAE coefficient | Training seconds after loading | Process peak RSS KiB | Packed model bytes |
| --- | ---: | ---: | ---: |
| 0.25 | 144.65 | 5,035,388 | 2,584 |
| 1 | 144.29 | 5,035,388 | 2,584 |

The two runs are sequential in one process. Shared loading/hash/preparation costs 8.96 seconds,
excluded from the table. RSS is the process high-water mark; caches were not controlled.
A separate full 32-frame 4× forward/backward probe took 0.564 seconds. These are CPU fitting
measurements, not inference timings or complete GPU memory counts. Runtime architecture is
unchanged, but no new inference speedup is inferred from that fact.

Data generation, independent scoring, broader content and deployment validation are excluded,
and **both models fail quality promotion**, so this does not establish G7. Complete model
hashes, training source/curves, packed-weight probes, independent AMD GPU comparisons and
limitations are in [the development record](neural-rendering-progress.md#joint-spatialhistory-training-and-a-broader-diagnostic-catalog)
and `build/neural-evidence/joint-v1/manifest.json` on Pop!_OS. No new G6 performance result is
claimed. The additional stress catalog's capture elapsed times include build/readback/disk
work and are not renderer latency measurements.


## 2026-10-26 AMD retained-sample kernels


Same Pop!_OS AMD Ryzen 5 5500U / Radeon Graphics (RADV RENOIR, PCI 1002:164c), Mesa 26.1.6, Chrome 154.0.8037.57 and Wrela 0.9.0 compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`. Browser diagnostics use one worker, 4× linear reduction and moving analytic case 1. No owned training, scoring, capture or other profile runs overlap these sequential profiles. Exact shader/fixture archive: `14deba433e93ca9cd0b745b17af7d62348aea09e6792acb587a15758a0ecb45c`.

The learned adapter uploads the exact coefficient-0.25 model once before warmup. The measured decoder includes its geometry queries and retained-sample gathering; retain and display are separate labels. There is no reference or diagnostic feature readback in timing. Reported GPU bytes are instrumented allocations, not a complete measurement of browser/driver memory.


| Decoder / output | Warmup / measured frames | Decoder median / p95 ms | Retain median / p95 ms | Fixture GPU span median / p95 ms | Reported GPU MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| fixed-1920x1080 | 240 / 240 | 60.356 / 60.471 | 0.355 / 0.378 | 61.584 / 61.701 | 100.86 |
| fixed-3840x2160 | 240 / 240 | 241.246 / 241.680 | 1.419 / 1.458 | 245.866 / 246.317 | 403.42 |
| learned-1920x1080 | 240 / 240 | 67.676 / 67.809 | 0.356 / 0.381 | 68.891 / 69.037 | 100.86 |
| learned-3840x2160-short-warmup | 32 / 240 | 270.598 / 271.082 | 1.421 / 1.506 | 275.192 / 275.713 | 403.42 |


The learned 4K attempt with 240 warmup + 240 measured frames timed out at the host 120-second limit. Its failure is retained. A declared retry keeps 240 measured frames and reduces warmup to 32; it completes, but is not a matched-warmup comparison. No p99, 20-minute stability or production whole-frame acceptance claim is made.

**G6 fails.** Decoder latency alone is far beyond the 1 ms/2 ms p95 targets, and the 4K fixture already exceeds the 384 MiB allocation limit. The 336-byte model excludes the sixteen-frame history and other buffers. Source geometry is extra full-stack work, not free fixed-input information. Capturing/training/scoring elapsed times are not renderer latency or G7 evidence.

Quality, exact command lines, environment/build hashes, all successful reports, memory records, timeout and retry plans are retained in `build/neural-evidence/raw-source-v1/manifest.json`. [The development record](neural-rendering-progress.md#raw-sample-kernels-clearer-detail-rejected-quality-and-cost) records why both trained candidates remain experimental.


## 2026-10-26 AMD sample pruning and packed history

Same Pop!_OS AMD Ryzen 5 5500U / Radeon Graphics (RADV RENOIR, PCI 1002:164c),
Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea, Chrome 154.0.8037.57,
Wrela 0.9.0/compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Source base `0cc8b17c`. Sequential browser profiles, one worker, analytic case 1,
4× linear reduction, 240 measured frames. Warmup is 240 frames except learned 4K,
which uses 32 in every listed stage to match the previous successful diagnostic.
No owned capture, training, scoring or other profiling overlaps these measurements.

Stage A rejects impossible support before loading a raw sample or querying visibility.
Stage B adds conservative loop bounds and stores already-half-float colour in a 20-byte
record while keeping f32 motion/depth. Models, observations and the sixteen-frame
horizon are unchanged. The learned timing fixture uploads the frozen λ0.25 model
(336 bytes). Retain and display are separate from the decoder label.

| Stage / decoder / output | Decoder median / p95 ms | Retain median / p95 ms | Fixture GPU span median / p95 ms | Reported GPU MiB |
| --- | ---: | ---: | ---: | ---: |
| original / fixed / 1920x1080 | 60.356 / 60.471 | 0.355 / 0.378 | 61.584 / 61.701 | 100.86 |
| original / fixed / 3840x2160 | 241.246 / 241.680 | 1.419 / 1.458 | 245.866 / 246.317 | 403.42 |
| original / learned / 1920x1080 | 67.676 / 67.809 | 0.356 / 0.381 | 68.891 / 69.037 | 100.86 |
| original / learned / 3840x2160 | 270.598 / 271.082 | 1.421 / 1.506 | 275.192 / 275.713 | 403.42 |
| prune / fixed / 1920x1080 | 25.452 / 25.598 | 0.355 / 0.387 | 26.665 / 26.816 | 100.86 |
| prune / fixed / 3840x2160 | 101.716 / 102.272 | 1.419 / 1.475 | 106.308 / 106.867 | 403.42 |
| prune / learned / 1920x1080 | 33.646 / 33.833 | 0.351 / 0.382 | 34.860 / 35.032 | 100.86 |
| prune / learned / 3840x2160 | 134.491 / 135.238 | 1.421 / 1.497 | 139.080 / 139.802 | 403.42 |
| packed / fixed / 1920x1080 | 20.778 / 21.118 | 0.270 / 0.291 | 21.899 / 22.226 | 77.12 |
| packed / fixed / 3840x2160 | 83.069 / 84.406 | 1.126 / 1.157 | 87.393 / 88.720 | 308.50 |
| packed / learned / 1920x1080 | 27.207 / 27.541 | 0.269 / 0.294 | 28.338 / 28.659 | 77.13 |
| packed / learned / 3840x2160 | 108.796 / 110.106 | 1.128 / 1.208 | 113.113 / 114.418 | 308.50 |

These are unpaired short diagnostic runs, not production frames, p99 measurements
or a 20-minute thermal/stability result. GPU bytes are instrumented allocation totals,
not complete browser/driver memory. The raw ring itself shrinks by 37.5% at these
aligned dimensions. **G6 still fails on latency**; a fixture allocation below 384 MiB
does not establish the complete 4K memory gate. The models also retain their quality
failures; no G1–G7 gate changes.

Stage A preserves all 9,672 checked image/reference/mask files and all four
full-size command/display comparisons. Stage B preserves raw exported observations,
references and masks exactly, but some learned HDR output channels change by one
half-float code step. Fresh scores meet the declared numerical bounds. All four final
displayed frames at 1080p/4K are byte-identical; command hashes/pipeline inventories
intentionally change with the new cache/retain/export implementation. This final-screen
check does not cover every full-size HDR frame. See [the development record](neural-rendering-progress.md#sample-search-cost-and-lossless-observation-packing).

Evidence on Pop!_OS: `build/neural-evidence/raw-kernel-prune-v1/manifest.json`
and `build/neural-evidence/raw-kernel-packed-v1/manifest.json`. Timing source archives:
Stage A `6b998afb04ef602490a2cef4b7368c02fbb97a8425c6d3b1dcf1d54235eafe28`;
Stage B `5673e31d9f1fc73787d260582c712a2bd265b03144cbc2390f14596d96894a89`.
Every profile includes its exact build hash, command, allocation record and workload.


## 2026-10-27 UTC AMD spatial sample index

Pop!_OS AMD Ryzen 5 5500U / Radeon Graphics (RADV RENOIR, PCI 1002:164c),
Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea, Chrome 154.0.8037.57,
Wrela 0.9.0/compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Base `93949167`. Same analytic case 1 at 4× linear reduction, fixed filter and
frozen λ0.25 kernel, one worker. Four sequential Chrome profiles, no owned scoring,
capture, training or other profile work overlapping them. Each has 240 measured
frames and 240 warmup frames, except learned 4K uses 32 warmup as before.

The explicit `raw-index` variant rebuilds two-pixel spatial bins over the sixteen
raw frames. Linked lists have no per-bin cap. Its table below includes the complete
fixture GPU span: retain, clear, build, reconstruct and display are all present.
Label percentiles are separate; they are not added to manufacture a pipeline p95.

| Decoder / output | Previous fixture median / p95 ms | Indexed fixture median / p95 ms | Index clear median / p95 ms | Index build median / p95 ms | Indexed gather median / p95 ms | GPU MiB, previous → indexed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| fixed-1920x1080 | 21.899 / 22.226 | 19.975 / 21.946 | 0.070 / 0.086 | 2.714 / 2.806 | 16.005 / 18.037 | 77.12 → 87.06 |
| fixed-3840x2160 | 87.393 / 88.720 | 82.399 / 89.236 | 0.282 / 0.326 | 10.881 / 11.096 | 66.798 / 73.682 | 308.50 → 348.14 |
| learned-1920x1080 | 28.338 / 28.659 | 26.581 / 28.862 | 0.072 / 0.087 | 2.729 / 2.935 | 22.635 / 24.889 | 77.13 → 87.06 |
| learned-3840x2160 | 113.113 / 114.418 | 106.628 / 115.284 | 0.282 / 0.317 | 10.824 / 11.088 | 91.271 / 99.833 | 308.50 → 348.14 |

The indexed median fixture span is lower in all four unpaired short runs, but
**three of four measured p95 values are worse** and allocations increase. The variant
is not promoted as a generally faster replacement. Rebuilding the index alone takes
about 2.7 ms at 1080p / 10.8–10.9 ms at 4K. It already exceeds G6's reconstruction
budget before gathering. Instrumented GPU allocations exclude unmeasured browser/
driver overhead; there is no p99, production or 20-minute stability claim.

All four final native comparison screens are byte-identical to the previous builds;
command hashes and pipelines differ as expected. Continuous HDR and repeated-run
audits find one-half-float-step changes, including atomic-order variation. All freshly
scored diagnostic quality differences meet the predeclared bounds, but the previously
rejected models remain rejected. [The development record](neural-rendering-progress.md#spatial-index-completeness-and-a-modest-cost-tradeoff)
records complete node checks, cut/reset, repeat variation and limitations.

Evidence on Pop!_OS: `build/neural-evidence/raw-kernel-index-v1/manifest.json`.
Capture source `10b9de7e24f8551617955108249f63c047fe748d91de66237d35cd368f69b049`; timing source
`272732e5702330b51a4ec0118ca37e4b743b0a2ce7788840512216367e91d3e7`; external scripts
`f4ab59a7c12a781611d910d7ea5dc765e861111c6fbdac21cb44cb0307146205`. Reports retain commands, build/environment
hashes, individual retain/display times, all allocations and the initial failed
diagnostic source export. G1–G7 remain unchanged and unmet.


## 2026-10-27 UTC AMD neural kernel context preparation

Pop!_OS AMD Ryzen 5 5500U / Radeon Graphics (RADV RENOIR, PCI 1002:164c),
Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea, Chrome 154.0.8037.57,
Wrela 0.9.0/compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Base `6d769911`. Same analytic case 1, 4× linear reduction and frozen λ0.25
kernel. Eight sequential Chrome profiles, one worker, 240 measured frames,
240 warmup at 1080p and 32 at 4K. No owned training, scoring, capture, build or
other profile job overlaps a profile. Each decoder/resolution runs disabled
then enabled; these are short unpaired observations, not a confidence interval.

`--raw-specialize 1` precomputes the six distance-independent MLP inputs for
each age/layer into 288 vec4 rows (4,608 bytes). Per-contribution evaluation
reads the same model's transposed distance coefficients, prepared biases and
output head. No new fitting or source-colour/target inputs are used.
The table is rebuilt every frame, including cold starts and cuts. Both
disabled and enabled profiles use the current source; the old shader is not
substituted for the new disabled-path control.

| Decoder / output | Disabled fixture median / p95 ms | Prepared fixture median / p95 ms | Prepare median / p95 ms | Prepared gather median / p95 ms | GPU MiB, disabled → prepared |
| --- | ---: | ---: | ---: | ---: | ---: |
| source-1920x1080 | 28.431 / 28.729 | 25.336 / 25.643 | 0.006 / 0.006 | 24.190 / 24.519 | 77.125 → 77.130 |
| source-3840x2160 | 113.441 / 114.739 | 101.052 / 102.308 | 0.006 / 0.006 | 96.700 / 98.001 | 308.497 → 308.502 |
| index-1920x1080 | 27.158 / 29.431 | 25.426 / 27.318 | 0.006 / 0.006 | 21.483 / 23.373 | 87.059 → 87.063 |
| index-3840x2160 | 109.438 / 118.073 | 103.471 / 110.945 | 0.006 / 0.006 | 88.020 / 95.485 | 348.140 → 348.144 |

The prepared source decoder reduces median span by 10.89% at 1080p and 10.92%
at 4K; the indexed decoder improves by 6.38% and 5.45%. All four p95 values
also decrease. Preparation adds 4,592 instrumented resident bytes over the
existing 16-byte placeholder. With preparation enabled, the direct decoder has
the lower median and p95 at both resolutions in these runs, with less memory
than the index. The index is not promoted on these results.

The fixture span includes sampling, retention, preparation, reconstruction and
display, plus index clear/build for the indexed decoder. Label p95 values are
not summed. Resident byte deltas and complete label reports are retained in
`summary.json`; instrumented allocations are a floor and exclude unmeasured
browser/driver overhead. Linux `7.1.5-76070105-generic`
and a performance CPU governor were observed mid-run; the snapshot is not a
continuous thermal/power trace or proof of thermal settling. No p99,
twenty-minute stability, production-scene or presentation-rate claim is made.

Eight final native displayed screens match the immutable previous full-size
builds exactly; the whole command/pipeline comparison may differ. Continuous
192×108 HDR and perceptual comparisons pass the predeclared implementation
bounds, including cuts and zero-head controls. These are tests of the
evaluation rewrite, not improved model quality. Both learned models retain
their previous stress-case rejections; G1–G7 remain unchanged and unmet.

Evidence: `build/neural-evidence/raw-kernel-context-v1/manifest.json` on Pop!_OS.
Capture source `525dcd47275091ab034c62b1ee0dafec819f43c117c262c5a2276c4646a1fdc7`; timing source
`729e9b0cdaecf8bb55e1270cf6d0752eed691079c3c2e3b8b3149833b24cbb8f`; external scripts
`ebb6849cc253588fce6bae0203515af41fbb100f20e453ae25dd953a1ab87f04`.
[The development record](neural-rendering-progress.md#preparing-the-kernels-shared-geometry-terms)
details independent table checks, controls, numerical bounds and limitations.


## 2026-10-27 UTC AMD authored foliage representation audit

Base `e6170e4c`, Pop!_OS AMD Ryzen 5 5500U / RADV RENOIR PCI 1002:164c,
Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea, Linux 7.1.5-76070105-generic,
Wrela 0.9.0/compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
The actual great-tree bake has 256 views of 64² texels. Seed 7 is the shipped
model's training individual. The target is the existing uncompressed bake,
**not a converged source-scene reference**. GPU readbacks export all eight
attributes, current BC7/BC5 blocks and the exact shipped half weights.

| Representation | Payload KiB | Legacy weighted attribute PSNR dB | Coverage precision / recall at 0.5 | Covered normal mean / p95 degrees |
| --- | ---: | ---: | ---: | ---: |
| Existing BC7 + two BC5 | 3072.00 | 37.623 | 100.00% / 100.00% | 5.77 / 13.33 |
| Shipped coordinate/grid MLP | 22.58 | 13.854 | 80.91% / 83.08% | 59.96 / 133.00 |
| Per-view quantized grid trial | 83.64 | 17.752 | 91.42% / 95.91% | 55.08 / 123.54 |

Payload is serialized attribute/model data only, not complete resident memory.
The plain GPU bake uses 12 MiB; the f32 eight-channel diagnostic readback uses 32 MiB.
The instrument intentionally holds multiple representations, output buffers and
scratch; its allocations are not a deployed-memory comparison. BC is the
engine's current mode-6 encoder, not a proven strongest conventional encoder.

The PSNR reproduces the existing trainer's combined coverage-weighted sqrt-albedo
and coverage formula. It is **not** G4's display-image PSNR. Coverage is a static
texel threshold diagnostic, not G3's motion/strand gate. Per-view and separate
albedo, coverage, normal, toward and depth results are retained.

The trial fits all 256 views, whereas the shipped network held out every 16th
view. The table describes atlas reconstruction, not fair unseen-view performance.
It used 4,096 fixed Adam steps of 4,096 positions, four CPU threads and PyTorch
2.5.1+cpu. Fit time was 30.705 s after data loading;
CPU decode of the full atlas took 0.546 s.
These exclude build/data generation, validation and installation, and do not
establish G7. No browser/GPU inference/full-foliage-pass timing is claimed.

The candidate is rejected despite the better training reconstruction. Its
85,648-byte payload alone cannot prove 16× resident compression; normal errors
and damaged coverage already prevent quality acceptance. No model is installed.

Evidence: `build/neural-evidence/appearance-baseline-v1/manifest.json` and
`appearance-grid-v1/manifest.json` on Pop!_OS. Final source archive
`fd13bcdba8dc017b9b899a8ef1efb974cb706bc989be61d9052c23830c30c84f`; external scripts
`ad3de7bf4ac0e9188028179157d8c48124cb6173d2153f1f9afb6a09923d2954`.
[Audit, calibration and limitations](neural-rendering-progress.md#authored-foliage-audits-and-a-rejected-quantized-grid).
G1–G7 remain unchanged and unmet.


## Neural foliage source diagnostics — Pop!_OS AMD, 2026-10-27

`studio/appearance` now rerasterizes authored card/limb material at off-grid
view angles. The fixed public matrix is two placement seeds × eight views
× two card levels, 64×64 output. The 16×16→32×32 spatial-grid comparison fails
all 32 cases. The repaired 32×32→64×64 comparison passes all 32 unchanged
limits: coverage MAE ≤0.001 and p99 ≤0.01, linear-albedo moment MAE ≤0.0005,
normal-vector moment MAE ≤0.002. Observed maxima are 0.000895917,
0.009033203, 0.000085693 and 0.000856251.

Naive f32 summation fails a normal-length invariant; the tested compensation
does not change GPU outputs. Exact sums of Q20 samples replace it. Independent
CPU int64 reductions match exactly at 4,096 and 16,384 samples per pixel;
f64 mean differences are at most 1.604e-7 in those block checks. At the three
worst old/new pixels, directly exported samples identify old sum errors of
4.192e-5–4.729e-5 versus new errors ≤3.193e-7. The failed 2e-5 closeness checks
remain recorded. Full details, including localized atlas-control attribute
differences, are in `neural-rendering-progress.md`.

Full versus coarse source cards differ in coverage MAE by 0.010893–0.025371
and in mutually covered mean-normal direction by 8.481–12.860 degrees.
At stored directions, the earlier grid model improves area-coverage error
relative to the current point BC atlas but still has much worse normal error.
No model is promoted. These unlit material moments, two public seeds and one
footprint do not establish G4/G7, production truth, or a renderer speedup.

Native Vulkan on the Pop!_OS Ryzen 5 5500U / Radeon Graphics (RADV RENOIR,
PCI 1002:164c), Mesa 26.1.6-1pop0~1787580452~24.04~a5619ea,
Linux 7.1.5-76070105-generic; Wrela 0.9.0, compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`. All capture/audit work ran on
Pop!_OS. No browser timings were measured. Capture elapsed includes builds,
cooking and readback and must not be reported as rendering latency.

Evidence `build/neural-evidence/appearance-source-v1/manifest.json`,
SHA-256 `18ecd3ef083820e855b80822914d5895eb78503f96b1e27ad7ceb8a8c61570b7`.
Final source `3fc23e8e9a709bb1cf6bc68fb5cd7217c54c657293b6cece0ed1c3b604927134`; external scripts
`4d82762514accd10faeae3135e1590f509b161d1f17eeb6decb0c4458c9a2167`. Focused checks pass; no full-suite/PR-ready claim.

## Source-integrated appearance-field experiment (Pop!_OS AMD)

Public authored great-tree material diagnostics at 64×64 pixels/view; native
RADV RENOIR (Ryzen 5 5500U integrated Radeon, PCI 1002:164c), Mesa
26.1.6-1pop0~1787580452~24.04~a5619ea, Wrela 0.9.0 compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`. All 256 stored directions
pass the fixed 32²→64² spatial convergence check. CPU PyTorch 2.5.1+cpu,
four threads, fits the fixed 8,192-step two-model checkpoint in 160.781 s;
full prediction plus reference-assisted ablation export takes 2.936 s.

The 161,522-byte neural payload is rejected by the material-quality screen.
At four public off-grid directions, with one identical depth-reprojection
step, mean coverage/normal/toward/crown-depth moment errors are
0.036517/0.037720/0.023352/0.001071 for the candidate and
0.025883/0.034725/0.015426/0.000572 for the 3 MiB conventional control.
The revised control stores toward and crown depth in independent BC5 channels;
normal error falls 6.09% and toward error 68.91% at the fitted directions
versus its earlier same-size layout, with identical colour/coverage and
crown-depth outputs. No best-encoder claim.

Reference batches take 11.605 s (grid 32) and 21.148 s (grid 64);
the final full conventional capture takes 36.657 s. These include
compilation, source cooking, capture/encoding and readback; they are not
renderer latency. Packed payloads exclude resident scratch/cache/fallback.
No browser timing, G4/G7 success, sealed score or human result is claimed.
Model replay, actual AMD BC readbacks, all-view projection identities and
the final byte-identical repeated capture are checked.

See [the complete experiment and limitations](neural-rendering-progress.md#source-integrated-visibility-and-shared-material-field-trial).
Evidence manifest SHA-256 `9d41ea7cec470cada043f4bfdf7a5b2b6686d79c12dc410a32d9dea57c4d9339`; final source
`e8a82d12be929d625eb4aeda4cb11b0335fe9ce8ccca2fbb636a8f4aa11b16bf`; scripts
`fad0c5185d5ddcdb8ba70ae11c6b27a2a029e286bd199e15f01ab29f57f65fa3`. Earlier failed layouts/evaluators are retained.

## Source-initialized neural volume (Pop!_OS AMD)

Fixed public great-tree material diagnostic, 64×64 pixels/view; native Radeon
Graphics/RADV RENOIR (Ryzen 5 5500U, PCI 1002:164c), Mesa
26.1.6-1pop0~1787580452~24.04~a5619ea, Wrela 0.9.0 compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`. Four CPU PyTorch 2.5.1 threads
fit a 48³×2 / 24³×4 quantized-grid volume plus 12→32→32→8 network in 96.066 s,
after 0.620 s source initialization. Full packed CPU decode takes 64.846 s.
These are observations, not renderer/GPU/browser latency or G7 validation.

At four public unseen directions, mean coverage/normal/toward/crown-depth
moment errors are 0.016379/0.023660/0.010109/0.001018 for the candidate,
versus 0.025883/0.034725/0.015426/0.000572 for the fixed stronger BC control.
Crown depth and four fitted-view diagnostics fail the acceptance screen.
Both integration sensitivity checks fail all 16 public cases. The model is
rejected despite better off-grid coverage and normals.

A matched 4,096-step random-grid ablation retains weights, material constants
and sample stream. Its four-view errors are 0.018247/0.025959/0.011253/0.001778
in the same order. Source initialization reduces those errors by
10.24%/8.85%/10.17%/42.77%. This is one controlled training pair on one asset,
not a general or sealed result. The no-prior fit takes 89.239 s; its final
decode overlaps focused type checks and is not a comparative timing.

Actual source card exports and volume arithmetic are checked independently.
The original nearest-half assumption fails; explicit f32 equation exports
and independent toward-zero conversion reproduce every f16 volume component.
The failed check is retained. The final rebuilt capture repeats all six
exports byte exactly. The model occupies 141,748 packed bytes before
scratch/cache/fallback/metadata, and requires 128 point evaluations/pixel
at the tested setting. No GPU model, resident-memory ratio or frontier success.

See [the complete record](neural-rendering-progress.md#source-initialized-neural-volume-and-matched-training-ablation).
Manifest SHA-256 `e0306b6c44d957a6fbca9a80191474b1b1c83e7bcec103b4ffa0e54ca3e73486`; source
`7474c288dc082b72d9cd94423efc5f61cc446c283ccc8eb01a53aa9375c132c3`; scripts
`d9252c27d0d3a66592667340c4c84afcb1064c89be73eaea4ea550e468d9083a`.

## Crown-depth repair and dense integration (Pop!_OS AMD)

Same Ryzen 5 5500U / Radeon Graphics RADV RENOIR machine, Wrela 0.9.0
compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`, Mesa
26.1.6-1pop0~1787580452~24.04~a5619ea, PyTorch 2.5.1+cpu/four threads.
A fixed 2,048-step fit of the existing 33 depth parameters takes 41.366 s;
complete 256-view packed CPU decode 65.315 s. Payload stays 141,748 bytes
before overhead; non-depth moments and unrelated packed parameters are
byte identical. These are observations, not GPU/browser timings.

Off-grid crown-depth moment MAE improves 0.001018→0.000974 at 32 ray
steps / 2×2 spatial samples, still 70.26% above the strong BC control.
All 16 public cases fail 64→128 rays at 4×4, then pass 128→256 rays
and 4×4→8×8 at 256. Dense repaired depth MAE is 0.000920, still 60.88%
above BC. Dense query count is 16,384/pixel versus nominal 128/pixel.
The model remains rejected; no production quality, temporal or runtime claim.

Source-field reconstruction checks all 53,120 exported leaf-card centres
across two seeds within 6.392e-7 m. A 32³ u8 source scalar grid occupies
32,772 bytes and has about 3.6 cm mean card-centre depth error. This is
a source-teacher diagnostic, not integrated image quality or compression.

See [complete controls and per-setting results](neural-rendering-progress.md#isolated-crown-depth-repair-and-dense-integration-audit).
Manifest `76ad089bd8a195fdc8d64933ddb84b91f9c55202e39ba8ed4e74b36ffdf6e6e3`; source `7474c288dc082b72d9cd94423efc5f61cc446c283ccc8eb01a53aa9375c132c3`;
scripts `4c4cf117c7bbe1bb90cb3a8f9ae015131abe7bef26674615a3abff38c8083db7`.

## Independent source scalar depth model (Pop!_OS AMD)

Same Ryzen 5 5500U / Radeon Graphics RADV RENOIR, Mesa
26.1.6-1pop0~1787580452~24.04~a5619ea, Wrela 0.9.0 compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`, PyTorch 2.5.1+cpu/four threads.
A source-initialized 32³ scalar u8 field adds 32,772 packed bytes to the
unchanged 141,748-byte learned volume: 174,520 total before runtime overhead.
It replaces only crown depth. All other stored and dense outputs stay
byte identical. Fitting 4,096 fixed steps takes 49.586 s; full packed CPU
decodes take 73.732 s fitted and 73.536 s unfitted. No renderer timing claim.

At four public unseen directions, crown-depth moment MAE is 0.000419
fitted / 0.000473 unfitted / 0.000974 repaired neural head / 0.000572 BC.
The fitted model beats BC in all five material diagnostics in each of those
four views. It still fails several stored-view gates (coverage 5.02× BC,
depth 2.77× BC), and remains rejected. Single public asset, no sealed result.

All 16 cases pass 128→256 ray and 4×4→8×8 spatial finite-difference checks
after failing the 64→128 step. Dense fitted off-grid depth MAE is
0.000389473, 31.89% below BC; 5/5 dense off-grid screens pass.
Nominal sampling needs 128 volume-network queries plus 128 scalar lookups
per output pixel; dense sampling 16,384 of each. No GPU integration,
resident compression, whole-foliage performance or G1–G7 pass follows.

See [full controls and limitations](neural-rendering-progress.md#independent-source-material-field-beside-learned-visibility).
Manifest `f365b7ae83243e450cf225fdad8a33913a07c083e9a14fee77d56297cd7806b6`; source `7474c288dc082b72d9cd94423efc5f61cc446c283ccc8eb01a53aa9375c132c3`;
scripts `99620c89af3d8cc0250d19de944873ddc3a7e82c3fccaac14376fa0a750dd36f`.


## Source primitive visibility controls (Pop!_OS AMD, 2026-10-27)

[Full methods, raw evidence and limits](neural-source-primitives.md).
Ryzen 5 5500U / Radeon Graphics RADV RENOIR; Wrela 0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`, NumPy 2.2.6.
This is a material-quality/count diagnostic, not a runtime benchmark.
Full source at 64×64 output pixels is truth; all five moment-error screens
retain their mean≤1.05×BC / worst-view≤1.10×BC limits.

Full-count ellipses: stored-four coverage MAE
0.001255631, off-grid-four
0.001252439; fixed BC is
0.003223348 /
0.025882901.
Both full-count kernels pass 5/5 screens in each group, retaining all
26,560 source cards plus 7,392 wood triangles. Integration passes
6/16 at 8→16, 12/16 at 16→32 and 16/16 at 32→64. The smaller count ladder
and native hard-cut comparisons are in the linked record. All six reduced-count
primitive variants fail joint stored-view quality; none establishes
packed/resident compression or GPU cost. Capture and CPU elapsed values
include compilation/cooking/readback or evaluator/file work.

Evidence manifests: `2a232b363a4f279ae0cb3a70d9c000e327933436989d262d8b223220883e2d39` (source controls),
`7d55fc50d287fc0457b4d2ff2026f9cf91063bb2c4abda015d260238e34bc55c` (count controls).
No learned primitive model, production promotion or G1–G7 success.


## Fixed-capacity learned primitive aggregation (Pop!_OS AMD, 2026-10-27)

[Model, controls and complete failures](neural-source-primitives.md#fitted-aggregation-at-fixed-packed-capacity).
Ryzen 5 5500U; source references from its Radeon/RADV GPU; fit uses
PyTorch 2.5.1+cpu with four threads, evaluation NumPy 2.2.6 plus an
independently checked C++17/OpenMP CPU raster. No GPU model timing.

Both unfitted/fitted models occupy 107,680 packed bytes before runtime
sorting, tiling, caches, scratch and fallback. Fixed 4,096-step fit:
255.939 s including wood-query acceleration,
excluding pre-existing source reference generation. All-256 paired
16×16 CPU decode/scoring/writes: 200.232 s.
These are CPU observations, not full rendering or G7 pipeline times.

Coverage MAE over all 256 stored views falls
0.021473852→0.010956385
(48.98%), still 3.58× BC. Four off-grid views improve
0.024310225→0.012674492;
all five off-grid material screens pass, but only the stored normal screen
passes. All sixteen public 16→32 integration checks pass. No promotion.

Evidence: `56fe9d315bdc182559753f30a1a6091f3426b1b6c2dfb63c17adaef30e863f83`. This is a learned explicit
primitive model, not an MLP decoder; no G1–G7 success.


## Source spatial grouping (Pop!_OS AMD, 2026-10-27)

[All controls, methods and failures](neural-source-primitives.md#spatial-aggregation-of-every-source-card).
Ryzen 5 5500U; reused source references captured on its Radeon/RADV GPU.
Clustering and evaluation use NumPy 2.2.6 and independently checked C++17,
four OpenMP threads, no fast-math. No GPU primitive timing is claimed.

Packed data including common header and authored wood: 107,672 bytes for
1,897 groups, 187,360 bytes for 3,320 groups. Runtime allocations remain
additional. Fixed eight-step construction of both models:
0.343 s wall time after source/pattern loading;
all-256, four-model nominal decode/wood/scoring/writes:
241.632 s. Neither is end-to-end G7 timing.

Unfitted 1,897 optical coverage MAE:
0.008003161 stored, 0.008965747 off-grid.
Stored error is 26.95% below the prior fitted 107,680-byte model,
but 2.6155× BC. Only stored normals pass; all five off-grid screens pass.
All four spatial models fail the joint stored gate. Every public 16→32
integration check passes. No runtime promotion or G1–G7 pass.

Evidence manifest `d5910f77992f6870134c09bf3faaa2be6c0a033a33189b615b8ec5481194140a`.


## Global optical calibration (Pop!_OS AMD, 2026-10-27)

[Method, full results and limitations](neural-source-primitives.md#global-optical-calibration-before-a-local-fit).
Ryzen 5 5500U, NumPy 2.2.6 and C++17/OpenMP four-thread CPU evaluation;
source targets were captured on the same machine's Radeon/RADV GPU.
No new GPU primitive runtime or timing.

49 fixed area/shape candidates, 32 stored selection views, same actual
107,672-byte format. Selection chooses 1.15× density and unchanged shape.
Selection elapsed wall time including wood/raster/scoring/writes:
142.385 s; all-256 final
nominal evaluation: 178.036 s.
Both exclude pre-existing source/reference generation; neither is G7
pipeline timing or resident-memory measurement.

All-256 coverage MAE 0.006854065:
14.36% below source optical, 37.44% below the prior prefix fit,
but 2.2400× BC. Stored/off-grid screens: 2/5 and
5/5. Public integration: 8/8 16to32.
The calibrated model remains rejected. No G1–G7 success.

Evidence manifest `fd84699ae12d2f5a4cc49e3ad89f13bc0a57e17f46ef7627197f1697845a891b`.


## Local optical fit regression (Pop!_OS AMD, 2026-10-27)

[Method, controls, full results and loss diagnostic](neural-source-primitives.md#local-optical-fit-regression-against-calibration).
Ryzen 5 5500U; PyTorch 2.5.1+cpu, four threads; NumPy 2.2.6 and checked
C++17/OpenMP CPU evaluator. Source references from Radeon/RADV GPU.
No GPU primitive runtime.

Actual payload stays 107,672 bytes. Fixed 4,096-step seed 5483 fit takes
142.921 s CPU-work wall time including lazy wood acceleration,
excluding prior calibration/source/reference generation and validation.
All-256 final nominal decode/wood/scoring/writes:
183.343 s. These are not G7 pipeline or GPU timing results.

Stored coverage MAE 0.007795158, 13.73% worse than calibration
and 2.5475× BC. All five stored/off-grid means regress versus calibration;
only 1/5 stored screens pass. 8/8 16to32 public sampling checks pass.
Dense weighted MSE also worsens. Reject the local fit and retain calibration.

Manifest `7d534b610dfb2e276b0abe814ed0ca0916e732c6f1be160cc9ed25ebd8ee1ea8`. No runtime, resident-memory or G1–G7 success.


## Controlled smaller optimizer steps (Pop!_OS AMD, 2026-10-27)

[Controlled change, full results and limitations](neural-source-primitives.md#controlled-smaller-optimizer-steps).
Ryzen 5 5500U, PyTorch 2.5.1+cpu four threads; checked C++17/OpenMP and
NumPy 2.2.6 evaluator. Reused Radeon/RADV source references. No GPU model
runtime or resident-memory timing.

Actual packed size stays 107,672 bytes. Identical seed 5483, 4,096-step
sampler/loss/decoder/initial data; only every optimizer rate scales by 0.1.
Fit: 138.226 s elapsed wall time including lazy wood queries,
excluding calibration/source/reference generation and validation.
All-256 nominal CPU wood/raster/scoring/writes:
177.666 s.

Stored coverage MAE 0.006437867, 6.07% below calibration
and 17.41% below the original-step fit. All five stored/off-grid means
improve. Dense weighted MSE 0.011191350.
Still 2.1039× BC stored coverage; 2/5 stored and 5/5
off-grid screens pass. Public integration: 8/8 16to32. Best development
model at this budget; no runtime promotion or G1–G7 achievement.

Manifest `fea09f2254859efa9e1f378ce476c610ab8360847c538f847eb684bf68a9c172`.


## Fixed optical continuation (Pop!_OS AMD, 2026-10-27)

[Exact continuation and complete result](neural-source-primitives.md#fixed-longer-optimization-with-exact-continuation).
Ryzen 5 5500U, PyTorch 2.5.1+cpu four threads, checked C++17/OpenMP evaluator;
reused Radeon/RADV source references. No GPU model runtime measurement.

16,384 steps, seed 5483, 107,672 packed inference bytes. First 4,096 packed
steps exactly reproduce the previous shorter fit. Full state resumes
exactly; negative momentum-reset control changes the result.
Fit elapsed CPU wall time 485.724 s, including checkpoint writes and
lazy wood queries, excluding calibration/source generation/validation.
The fit alone exceeds G7's five-minute complete generation limit.
All-256 nominal wood/raster/scoring/writes
180.499 s CPU wall.

Coverage MAE 0.006235107,
3.15% below the shorter fit; still 2.0377× BC.
2/5 stored and 5/5 off-grid diagnostic screens pass.
Integration: 8/8 16to32. Material screens are diagnostics, not G4 acceptance.
All original targets remain intact and unmet.

Manifest `c2e2a06ab9d44c08db09dedbc983ab9371b4d154ef7cd211f37dbf9c3589aa28`.


## G4 shaded/runtime/residency viability (Pop!_OS AMD, 2026-10-27)

[Complete comparison, failed tails, physical residency and decision](neural-shaded-appearance.md).
Ryzen 5 5500U, hardware Radeon/RADV RENOIR, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0. Solo; frozen 107,672-byte fitted representation; zero new training or references.
Two public views, four controlled lights, three backgrounds; 64×64 pixels.
The first viability assessment finished in 67 minutes 26 seconds of the two-hour box.

Two candidate optimizations only: shade leaf/wood before blending, then 2×2 sampling.
Final mean PSNR 40.128 dB, FLIP 0.01235, LPIPS 0.001889 versus source-informed BC's
30.685 dB / 0.06237 / 0.037719. Two of 24 cases still fail (worst PSNR loss 5.980 dB).
Conventional calibrated primitives using the same renderer reach 39.740 dB /
0.01392 / 0.002116 at essentially identical storage and algorithm.

60 warmup, 240 measured frames. Final/BC paced GPU p95: 0.3141/0.1036 ms;
p99: 0.3161/0.1046 ms. Saturated p95: 0.09808/0.02688 ms.
Separate CPU-start-to-GPU-complete observer (includes updates and compositor commands,
plus browser/driver notification) p95: 4.340/1.906 ms; p99: 4.528/3.812 ms.
Sort/upload, decoding, wood, visibility, integration and final composite included.
This is not a production whole-scene benchmark; initialization is outside steady-state timing.

All requested live GPU buffers/textures: 721,852 versus 4,341,876 bytes, only 6.015×.
Includes output, intermediate images, sort data, wood, retained upload/uniform rings
and fallback/dummy resources. Calibrated control: 721,968 bytes.
Actual DRM residency and process PSS are separately recorded in the report; these
UMA values are not added together or claimed as per-asset ownership.
The final checked-in default adds the same 116-byte inactive source-control helper;
all eight outputs remain byte-identical to the measured candidate.

Decision: **change representation**, retain the frozen model and every failed result.
Next bet: source-compiled visibility layers and pixel-footprint coverage; train a residual
only after a rendered comparison shows a reason. Original G1–G7 unchanged and unmet.
Evidence manifest `44cc42b8607d0781958bbd4409c360cd7ab8a7288d5a8797dfa091d4f8ef46b6`.


## 2026-10-27: two-slab appearance replacement rejected on AMD

Parent ce779eb352e600d77021e8c988be10e6906f50c5.
[Full comparison and explicit decision](neural-shaded-appearance.md#replacement-experiment-reject-two-visibility-slabs).
Ryzen 5 5500U / hardware RADV RENOIR, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 (de9b81e52a7747870dfbd5223cb8aeb41e584d23).
Sequential isolated 240-frame runs after 60 warmups, 64×64, one worker.
Frozen fit and prior conventional calibration; zero training/new references.
Quality uses the same eight AA64 shaded references and 24 background cases.

| Variant | Mean PSNR dB / FLIP | Paced GPU p95 / p99 ms | CPU p99 ms | Saturated GPU p95 ms | Requested live / peak GPU bytes | DRM VRAM / GTT MiB | Browser group PSS MiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| trained-layers2 | 32.84992 / 0.047489 | 0.375642 / 0.376428 | 0.553050 | 0.09548 | 599,688 / 616,072 | 16.582 / 9.289 | 454.899 |
| calibrated-layers2 | 32.71395 / 0.048277 | 0.375042 / 0.375649 | 0.538050 | 0.11424 | 599,688 / 616,072 | 16.582 / 9.289 | 454.088 |
| trained-local2 | 33.35180 / 0.043010 | 0.558454 / 0.559950 | 0.702200 | 0.16740 | 599,688 / 616,072 | 16.590 / 9.383 | 462.865 |
| trained-quad2 | 32.95052 / 0.047441 | 0.390446 / 0.395628 | 0.538550 | 0.10452 | 632,448 / 648,832 | 16.586 / 9.289 | 455.003 |

GPU spans include decoding, wood, visibility, layer resolve and screen composite;
CPU includes updates/uploads/recording, with no global sort. Initialization peak and
retained upload/uniform rings, scratch, caches and dummy/fallback data are counted.
DRM residency and PSS are separate UMA snapshots, not additive per-asset storage.

Global slabs reduce CPU p99 from ordered AA2's 0.973 to 0.553 ms and requested
GPU bytes from 721,852 to 599,688, but lose 7.278 dB and raise GPU p95 from
0.314 to 0.376 ms. Two targeted follow-ups (local depth anchor, independent
four-sample coverage) fail to recover quality. All fail G4; **retire the mixture**.
Only 7.240× full requested allocation reduction versus BC, not 16× residency.
Recorded CPU-start-to-GPU-completion p95/p99: global 4.220/4.452 ms,
calibrated 4.365/4.726 ms, local 4.490/4.605 ms, four-sample 4.205/4.305 ms.
These include host notification latency; small single-run changes are not a robust win.

Focused format/type checks and restored-default eight-image byte parity pass.
No full-suite/PR-readiness claim. Every failure retained in
build/neural-evidence/g4-visibility-layers-v1 on Pop!_OS.

Manifest SHA-256 814030755e130a9fd7a600643fbcef0f303cb6db94d4108f4916cbe7af44ef9e;
source archive 0d143eed9030ee093a7192a7dcb596a5460f1c8bff9a0f09edb758ddc0bf1c68;
external scripts d06f932c804ada03189e50427ab250cf36f4dc5f2daf5e546342f64a2b69b343.


## 2026-10-27: ordered GPU appearance execution rejected on AMD

Parent 8b50a6b7e40072ac12a99c8f51cc7cae702ecc23.
[Full execution comparison](neural-shaded-appearance.md#ordered-gpu-execution-quality-recovered-cost-rejected).
Ryzen 5 5500U / AMD RADV RENOIR, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 (de9b81e52a7747870dfbd5223cb8aeb41e584d23).
One isolated run each: 240 frames after 60 warmups, one worker, 64×64.
Frozen fitted/conventional calibrated parameters; zero training/new references.

| Variant | Mean PSNR dB / FLIP | Paced GPU p95 / p99 ms | CPU p99 ms | Saturated GPU p95 ms | All requested live / peak GPU bytes | DRM VRAM / GTT MiB | Browser group PSS MiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| trained-tile512 | 40.120118 / 0.012470 | 1.586317 / 1.655851 | 0.573050 | 0.71412 | 714,656 / 731,040 | 16.555 / 9.289 | 449.797 |
| calibrated-tile512 | 39.735748 / 0.013968 | 1.587522 / 1.643885 | 0.510000 | 0.72608 | 714,656 / 731,040 | 16.555 / 9.195 | 449.311 |
| trained-cache512 | 40.120118 / 0.012470 | 1.296984 / 1.430936 | 0.500250 | 0.53696 | 836,064 / 852,448 | 16.566 / 9.398 | 455.268 |
| trained-gpu-sort | 40.127682 / 0.012348 | 0.224052 / 0.260471 | 0.444150 | 0.22880 | 722,040 / 738,424 | 16.570 / 9.383 | 457.114 |
| calibrated-gpu-sort | 39.739577 / 0.013921 | 0.915528 / 0.916725 | 0.533050 | 0.22452 | 722,040 / 738,424 | 16.824 / 9.383 | 456.738 |

All 24 shaded cases per variant match their corresponding prior ordered control within
the existing tolerances; the same two BC-relative cases still fail. GPU-global-sort
images match all 16 fitted/calibrated originals byte for byte. Full time includes
updates, projection/decoding, sorting, visibility, wood, compositing and resolve.
GPU allocation includes cache/scratch, image targets, output, dummy/fallback data and
executor rings. Declared workgroup-local storage is 10,244 B per tile group (64 groups)
or 16,384 B for the one global-sort group, separate from global allocations.
DRM/PSS are overlapping UMA snapshots, not additive per-asset memory or physical peaks.

Conflicting initial paced/saturated results required three alternating pairs B,A,A,B,B,A,
original CPU-sort versus GPU-sort/raster. Saturated GPU median 0.07824→0.21404 ms:
+173.57% paired cost, while paired CPU time falls 43.38%. Saturated GPU p95 ranges
0.08100–0.09324→0.23092–0.23552 ms. The final pair's paced GPU p95/p99 is
0.313842/0.315178→0.914434/0.916959 ms. Preserve the favorable initial paced run;
it is not a robust improvement. Read-only 100 ms samples record unchanged auto power,
400–1,800 MHz shader clocks and 35–43°C, without per-frame causal attribution.

Complete CPU-start-to-GPU-completion p95/p99, including driver/browser notification:
tile 4.340356/4.500298 ms; calibrated tile 4.340332/4.531252;
cached 6.070813/6.339993; GPU sort 4.401636/4.547219;
calibrated GPU sort 4.385754/4.539114. These are not input-to-photon times.

Decision: **change representation; retire tile compute, do not promote GPU-global sort**.
Exactly two targeted follow-ups used. Original CPU-sorted AA2 default restored,
format/type checks and eight-image byte parity pass. New diagnostic defaults add
72 dummy GPU bytes relative to parent; actual figures above include their allocations.
No full-suite/PR-readiness claim. All failures, paired outputs, physical snapshots and
source builds retained under build/neural-evidence/g4-tile-ordered-v1 on Pop!_OS.

Manifest SHA-256 19998e152994f78f42b6e63c1b2e23668b66485cad005e5077c303677d6e604e;
source archive 9b52d08a196ec92339a127a05deaef8da89acb2585f1c9621a53a2b26a5fb27d;
external scripts 63616c9a91eebe7c3871e824db68e6289c7cd89a9bb3080213c1be3f933a170a.


### 2026-10-27 — G4 joint shaded light field rejected

Measured on Pop!_OS, Ryzen5 5500U / AMD RADV RENOIR, Mesa26.1.6, Chrome154.0.8037.57;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Source parent
ddbc61318725e7d580455f61ac971756a7da14c7 plus archived experimental source. Solo,
sequential fitting/build/capture/profile/scoring. Six fixed 2,048-step shaded-image fits,
one targeted angular-grid follow-up; no material-moment fitting or new truth references.

Each profile:64×64,240 measured frames after60 warmup,one worker,AMD Chrome WebGPU;
view/light changes every frame. All GPU field decoding, coverage and compositing timed.
Neural coarse-angle result:mean PSNR23.619dB,FLIP0.15422,LPIPS0.22198;GPU p95/p99
2.060/2.160ms;16,520 asset bytes;426,120 live/442,504 peak requested GPU bytes.
Same-input conventional C8 tensor:23.354dB,0.15845,0.21425;GPU1.263/1.374ms;
26,632 asset bytes;436,232 live/452,616 peak. Neural relative quality passes only9/24cases.
Both are substantially worse than the preserved ordered model's40.128dB.

Allocation observer includes host rings/screen/output; actual DRM residency and process
PSS snapshots also retained, without adding overlapping UMA counters or claiming physical
transient peaks. All48new GPU/independent decoder comparisons pass(max2.98e-7); restored
default8outputs byte-identical;format/type pass. No full-suite claim.
Decision:retire tested global image/camera factorization;next source-anchored local
visibility,explicit support/order and exact wood. G1–G7 unchanged.
[Full comparison, image and evidence](neural-shaded-appearance.md#joint-shaded-light-field-retire-imagecamera-factorization).
Evidence:build/neural-evidence/g4-joint-field-v1 on Pop!_OS.


### 2026-10-27 — G4 compact source anchors, two follow-ups, no promotion

Measured on Pop!_OS, Ryzen5 5500U / AMD RADV RENOIR, Mesa26.1.6, Chrome154.0.8037.57;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Source parent:
91002cb9daa06098b59b48947d58dac12818ca6a, plus archived experimental source.
Solo; sequential CPU fitting, builds, native captures, Chrome profiles and scoring.

Two 88-element source-only groupings, two sharper-profile variants and two fixed
2,048-step shaded-image fits. No material-moment fitting. Existing 128 training /
16 validation AA8 teacher poses and AA64 public truth are reused. Profiles: 64×64,
240 measured frames after 60 warmup frames, one worker. Complete rendering includes
exact wood, sort/uploads, ordered visibility, shading/integration and compositing.

Best compact spatial fit: 32.424 dB, 0.04013 FLIP, 0.04164 LPIPS; GPU p95/p99
0.210/0.211 ms; 6,368 packed bytes; 613,500 live / 629,884 peak requested GPU bytes.
It passes 13/24 numeric quality cases versus BC and is 7.70 dB below the frozen best.
Full residency snapshots and CPU-to-final-GPU completion are retained; overlapping
UMA counters must not be summed.

Eight new shaded-gradient parameter groups pass. The strict CPU/GPU maximum RGBA
check failed at a wood edge and is retained; displayed discrepancy is below 0.014%
of actual GPU reference MSE after fitting. GPU outputs judge quality. Format/type
checks pass; the default's eight outputs are byte-identical. No full-suite claim.

Decision: change local opacity/radiance representation, retaining order and exact
wood. Default unchanged; G1–G7 unmet. All six results are preserved.
[Full comparison and limits](neural-shaded-appearance.md#source-anchors-preserve-order-change-local-appearance).
Evidence: build/neural-evidence/g4-source-anchors-v1 on Pop!_OS.


### 2026-10-27 — G4 local response heads retired

Pop!_OS Ryzen 5 5500U / AMD RADV RENOIR,Mesa 26.1.6,Chrome 154.0.8037.57;
Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Parent
90b629584505b5a8e41dc2a870aaedfce8c92178 plus archived experimental sources.
Solo; sequential fitting/build/native capture/Chrome profiling/metric evaluation.

Frozen 88-element compact geometry; neural12, affine and quadratic local response heads
receive identical source/view/light features and per-element latent dimensions.
Three fixed 2,048-step shaded-image/coverage fits on existing 128 AA8 teacher poses;
16 validation poses; public 2 views excluded. No moment loss or new reference captures.
Six measured variants include one follow-up caching the unchanged decoder per primitive.

24 AA64 reference shaded cases. Neural 32.708 dB, FLIP 0.03924, LPIPS 0.04209, coverage MAE 0.023997,
worst boundary p99 2.864px;13/24 numeric cases pass versusBC. Full pacedGPU p95/p99
0.715/0.716 ms,CPU p99 0.516 ms,CPU-to-finalGPUcompletion p95/p99 4.470/4.584 ms.
8,536 packed bytes become615,668 live / 632,052 peak requested GPU bytes. Actual DRM resident
VRAM 17,367,040 / GTT 9,740,288 bytes and browser PSS 480,005,120 bytes recorded separately;
overlapping UMA counters are not summed. No physical 16x saving established.
Affine 32.796 dB; quadratic 33.009 dB. Cached decode has no saturated runtime win.

Decision: retire the tested local-polynomial architecture; next change visibility/coverage
representation and avoid AA2 image-target cost. Frozen default 8 GPU images and native
resource counts unchanged. New gradient/opacity/zero-head checks and format/type pass.
Strict conventional cache byte parity failure retained; actual cached quality scored.
No full-suite claim. G1-G7 unchanged and unmet.
[Full evidence and limits](neural-shaded-appearance.md#local-response-decoder-retired).
Evidence build/neural-evidence/g4-local-response-v1 on Pop!_OS.


### 2026-10-27 — G4 footprint visibility: integration tradeoff, two fixture corrections

Pop!_OS Ryzen 5 5500U / AMD RADV RENOIR, Mesa 26.1.6, Chrome 154.0.8037.57;
Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Parent
8ba904a9936d92b8c9476e46d8fb6142c5a48182 plus archived source. Solo/sequential.
Same AA64 truth: 2 public views x 4 analytic lights x 3 backgrounds. No new reference
generation or training. Frozen 1,897 patches and conventional source-calibrated patches.

Direct four-sample alpha averaging: 35.042 dB versus frozen AA2 40.128 dB.
Follow-up1 streams four complete subpixel passes through reused 64px targets:
40.163 dB, but more complete rendering cost. Follow-up2 reuses exact wood vertices
through indexed drawing, already used in production engine::wood. Eleven variants retained.
Indexed stream GPU95/99 0.948/0.950ms; AA2 0.270/0.272ms; BC 0.105/0.105ms.
Indexed stream169,012 engine appearance/scratch bytes;578,612 live/594,996 peak all
requested GPU bytes. Engine-only18.61x vs BC becomes6.15x with all allocations.
Actual DRM/PSS snapshots recorded separately; overlapping UMA counters not summed.

Retain fixture indexing: paired B,A,A,B,B,A saturated GPU median0.07840 ->0.06836ms,
-12.76%; all3pairs improve12.64-13.01%; CPU inconclusive; +4,028 live index bytes.
Retain bounded BC startup uploads: upload ring1MiB ->256KiB, identical images.
No new neural or engine-wide win claimed. 93.91% of direct filtering's added displayed
error is near wood in12% of pixels; next bet is filtered joint wood/leaf visibility.

56 implementation-control images and default8images byte-identical. Format/type pass;
initial helper-borrow compiler error retained. No full-suite claim. G1-G7 unchanged.
[Full evidence and limits](neural-shaded-appearance.md#fine-visibility-patches-change-wood-sampling).
Evidence build/neural-evidence/g4-footprint-visibility-v1 on Pop!_OS.


### 2026-10-27 — G4 source-curve visibility: retire conditional two-group rendering

Pop!_OS Ryzen 5 5500U / AMD RADV RENOIR, Mesa 26.1.6, Chrome 154.0.8037.57;
Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Parent
7aa4ee338d714e9acf736970fe557afe3e61889b plus archived experimental sources. Solo/sequential.
Frozen 1,897-patch model and equally informed source-calibrated control; same completed
AA64 references, 2 views x 4 lights x 3 backgrounds. No training or new references.

264 half-packed source-derived wood frusta (4,224 bytes), four rays per pixel,
bounded sparse depth records, wood/open conditional foliage blending. Initial frozen
39.428dB. Follow-up1 reuses the idle target in wood-free pixels:39.693dB.
Follow-up2 shares cone projection per workgroup; six measured variants retained.
Final frozen39.693dB vsAA2 40.128dB, FLIP+9.97%,LPIPS+13.30%,coverageMAE.009403vs.008832.
Only10/24 quality cases pass versus conventional AA2;1/24 versus frozenAA2.

Full paced GPU95/99 1.315/1.356ms vsAA2 .270/.272ms; saturated95 .442vs.071ms.
CPU99 1.080ms; CPU-to-finalGPU95/99 4.801/4.985ms. All requested GPUlive/peak
605,808/622,192bytes vsAA2 726,068/742,452; engine-only196,208vs316,468.
Cached workgroup shared storage14,884bytes. Actual DRM residentVRAM17,371,136,
GTT9,838,592, browserPSS480,525,312bytes recorded separately; UMAoverlapnot summed.
No16xphysical or combinedG4advantage. Prior sealed controls reused; no paired speedup claim.

First assessment22m38s afterstart; finaldecision26m36s afterstart, within2h.
Retire this path; next representation must retain independent subpixel visibility.
Default8images byte-identical and nativecounts unchanged; forced overflow8frames invalid;
format/type pass. Initial parse failure and nonidentical cached RGB results retained.
No full-suite claim. G1-G7 unchanged and unmet.
[Full evidence and limits](neural-shaded-appearance.md#source-curve-wood-visibility-retire-the-two-group-path).
Evidence build/neural-evidence/g4-wood-visibility-v1 on Pop!_OS.


### 2026-10-27 — Exact source-program reconstruction with selective field caching

Pop!_OS Ryzen 5 5500U / AMD RADV RENOIR, Mesa 26.1.6, Chrome 154.0.8037.57;
Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Parent
d66971c02ea6268d4b9d444332a4541ac0e63b60 plus archived source. Solo/sequential.
Same completed AA64 truth, 2 public views x 4 lights x 3 backgrounds. No training.
Frozen learned model unchanged; all new methods are conventional and share source inputs.

Reconstruct 26,560 cards from authored source; cache either nothing, 180 bytes of sampling
distribution, or distribution plus exact normal/depth (212,660 bytes). Indexed source
geometry in every compared variant. Four variants, two targeted follow-ups.
All 24 image pairs byte-identical: 40.294314dB, FLIP .01300185, LPIPS .00249142,
coverage MAE .01005954. Archived source-AA8 control also matches all 8 images.

Selective response cache saves 637,228 requested GPU bytes (20.55%): live/peak
2,463,340/2,479,724 versus stored cards 3,100,568/3,116,952. Actual DRM/PSS snapshots
recorded separately; no per-asset physical reduction proved. Atlas plus sample targets
are 89.35% of remaining engine appearance/scratch. No 16x claim.

Standalone full GPU95/99 1.191/1.252ms versus 1.087/1.144ms. Three paired saturated
median changes +2.47%,+4.72%,+2.64%; p95 ratios 1.092,1.167,1.148 fail 1.1 in two pairs.
CPU median +7.45%, variable; no CPU win. Keep as conventional memory tradeoff, not G4 promotion.
Next bet: footprint-aware source coverage and sample-local visibility storage.

First assessment within 12 minutes; decision within 19 minutes. Default frozen8images
unchanged; +12 inactive buffer bytes verified. Format/type pass; no full-suite claim.
All variants and failed timing tails preserved. G1-G7 unchanged and unmet.
[Full evidence](neural-shaded-appearance.md#source-program-reconstruction-a-conventional-memory-tradeoff).
Evidence build/neural-evidence/g4-procedural-source-v1 on Pop!_OS.


### 2026-10-27 — Source leaf storage and conditional material reconstruction

Pop!_OS Ryzen 5 5500U / AMD RADV RENOIR, Mesa 26.1.6, Chrome 154.0.8037.57;
Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Parent
1326773e9fd21b14cc12f7e54b65cd65b7a8bc35 plus archived source. Solo/sequential.
Five conventional AMD paths, two targeted follow-ups. Existing AA64 truth, two views,
four lights, three backgrounds; unchanged frozen model, zero training/new truth.

Independent 128-texel coverage masks with conditional source tissue reconstruction
pass 24/24 numeric cases versus the raw atlas, preserve all eight coverage outputs
and save 786,432 requested GPU bytes (25.36%): live/peak 2,314,156/2,330,540 versus
3,100,588/3,116,972. Mean PSNR40.295825, FLIP.01299942, LPIPS.00249220,
coverageMAE.01005954, worst boundaryp99 2px. RGBA is not fully bit-identical.

Standalone complete GPU95/99 1.204/1.219ms versus1.207/1.208ms. Three paired
saturated p95 ratios1.027,.999,1.032; median change-16.23%. Last paced pair
1.164/1.202ms versus1.150/1.180ms. No paired saturatedp99 available.
Actual DRM/PSS snapshots show no established physical residency reduction.

BC7 fails8/24 cases; 64-texel masks fail2/24 despite improved average metrics.
Direct field evaluation passes24/24 but GPU95=4.180ms. Forcing full material detail
in both raw and mask128 controls raises hybridGPU95 to3.849ms versus1.144ms.
Keep mask128 as a distant control, not all-scale deployment. Frozen learned
representation passes8/24 cases against it: change architecture, no G4 promotion.

First complete assessment8min13s; decision21min18s. Defaults/model/frozen8images
unchanged; +16 inactive texture bytes accounted. Format/type pass; no full suite.
Preserve every failed result. Next: independent surface-ID visibility and deferred
source shading, compared fairly before more fitting. G1-G7 unchanged/unmet.
[Full evidence](neural-shaded-appearance.md#source-leaf-storage-a-useful-distant-control-not-an-all-scale-solution).
Evidence build/neural-evidence/g4-leaf-storage-v1 on Pop!_OS.


### 2026-10-27 — Surface identity and deferred source shading

Pop!_OS Ryzen5 5500U / AMD RADV RENOIR, Mesa26.1.6, Chrome154.0.8037.57;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Parent
124bdc6976e6873d6eafddd6b43d6bfbbaf6d5f6 plus archived sources. Solo/sequential.
Four conventional AMD variants, exactly two optimization follow-ups, no training/newtruth.
Same completed AA64 references, two views/four lights/three backgrounds.

Store exact leaf-card/wood-triangle IDs per sample, reconstruct source shading, round
to the original half target, resolve16samples. All3alternatives pass24/24 numeric
quality cases; all24coverage images identical to control. Both followups'16RGBA
images identical to direct identity. Direct mean40.295971dB, FLIP.01300193,
LPIPS.00249104, coverageMAE.01005954, worst boundaryp99=2px.

Direct identity live/peak2,052,020/2,068,404 requested GPU bytes versus
2,314,160/2,330,544 control:11.33% saving. Source reconstruction plus response cache
uses1,414,788/1,431,172:38.86% saving. Physical DRM/PSS snapshots do not establish
a residency saving. Invariant source upload17,088bytes/frame is fully counted.

Direct paired saturated median-9.64%, but2/3saturated and1/3paced pairs fail GPU95/99
1.1x limits. Source-cache variant fails all3paced pairs. All24raw paired runs retained,
including unfavorable fast-control runs; no tailgate pass inferred from averages.
Per-pixel reuse increased saturated cost with identical images and was rejected.

Full-material branch stress: directGPU95/99=1.426/1.491ms versus packed-field
control3.862/3.927ms, with identical coverage and maxRGB RMSE.00005326.
Archived larger RGBA8 atlas remains faster at1.144/1.179ms. Not real close-up validation.

Decision: change G4 representation again; keep identity as a conventional control.
Next: specialized source constants/decoders, with equally informed analytic and neural
methods. Initial assessment5m31s; decision21m58s. Defaults/model/frozen8images unchanged,
+4inactive texture bytes counted. Format/type pass; no full suite. Failures preserved.
G1–G7 unchanged/unmet.
[Full evidence](neural-shaded-appearance.md#surface-identities-preserve-visibility-but-reject-the-tail-regressions).
Evidence build/neural-evidence/g4-surface-identity-v1 on Pop!_OS.


### 2026-10-27 — Compiled source leaf programs: reject per-fragment decoding

AMD RADV RENOIR/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57, Pop!_OS;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Parent75f446fe
plus archived sources. Solo/sequential,64px,60warmup+240measured frames.
Six conventional variants, two optimization follow-ups, no training or new truth.

A3,328-byte specialized source table passes24/24 static shaded cases against each
original forward atlas, deferred atlas and packed-mask control. Mean40.293874dB,
FLIP0.01303834, LPIPS0.00247681, coverageMAE0.01007283; worst boundaryp99=2px.
Both conservative predicate caches preserve all eight program images. All262,144
source texels checked: at most one8-bit step difference, zero half crossings at
texel centers. Frozen neural model still passes only8/24 vs each current control.

Complete program GPU95/99=2.411/2.469ms vs packed masks1.039/1.043ms;
requested live/peak1,793,204/1,809,588 vs2,052,020/2,068,404 bytes.
Final65,536-byte predicate cache: GPU95/99=2.345/2.405ms,
live/peak1,858,740/1,875,124. Both follow-ups fail runtime. All decoded geometry,
wood, visibility, updates, scratch, caches, fallback and composite are counted.
Physical DRM/PSS snapshots do not establish residency savings; no16xG4 claim.

Decision: retire per-fragment field decoding; next unproven bet is a bounded cache
of source-compiled hardware-filtered tiles, charging misses and rebuilds.
Initial assessment6m27s; decision36m30s. Frozen model/eight images/native memory
unchanged; format/type pass; no full suite. G1–G7 unchanged/unmet.
[Full evidence](neural-shaded-appearance.md#compiled-leaf-programs-quality-survives-repeated-decoding-does-not).
Evidence build/neural-evidence/g4-leaf-program-v1 on Pop!_OS.


### 2026-10-27 — Bounded source coverage tiles

AMD RADV RENOIR/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57, Pop!_OS;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Parent2d9016a9
plus exact archived sources. Solo/sequential,64px,60warmup+240measuredframes.
Nine variants:3conventional controls and3cache configurations, each warm/rebuilt.
Exactly2targeted optimizations, no training or new shaded truth.

All9pass24/24 numeric cases against each control. All32full-capacity tile images
exactly match packed masks;24warm/rebuild pairs identical. Compact coverage storage
85,764bytes vs262,156 for packed subsystem (3.06x), but complete live allocations
1,875,636 vs2,052,028 (8.60% reduction). Physical residency savings remain unproved.

Compact warm GPU95/99=1.034/1.091ms; full-rebuild1.167/1.212ms;
packed control1.040/1.043ms. Three paired warm comparisons:0/3paced failures,
1/3saturated95/99 failures; saturated median+3.15%. All12raw runs retained.
Full-material stress GPU95/99=1.251/1.297ms vs forwardRGBA8 1.157/1.183ms;
compact and packed RGBA identical, no actual close-up validation.

Decision: change G4 beyond leaf-only caching; retain cache as conventional control.
Cards849,920bytes and visibility targets524,288 dominate the current renderer.
Next unproven bet: joint source-instance/visibility hierarchy with exact fallback.
Initial assessment5m48s; decision19m10s. Frozen model8/24 vs controls, unchanged.
Frozen8images unchanged; +8inactive buffer bytes counted; format/typepass.
Failures preserved; no full suite; G1–G7 unchanged/unmet.
[Full evidence](neural-shaded-appearance.md#bounded-source-tiles-recover-speed-then-confront-the-memory-floor).
Evidence build/neural-evidence/g4-leaf-tiles-v1 on Pop!_OS.


### 2026-10-27 — Source hierarchy and bounded visibility

AMD RADV RENOIR/Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57, Pop!_OS.
Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Parent fcc7d35d plus exact archived sources; solo, sequential, 64px,
60 warmup + 240 measured frames. Initial five-way assessment 12m15s; decision
23m09s, after exactly two optimizations. No training or new shaded truth.

Full source hierarchy preserves eight shaded outputs exactly and cuts requested
live memory from 1,875,660 to 1,239,652 bytes (33.91%). Saturated median rises
0.275 to 0.320ms; paced GPU p95/p99 rises 0.924/1.019 to 1.190/1.217ms.
Eight-row targets save another 458,752 bytes, but full-geometry repetitions cost
1.467ms saturated median. Conservative culling only lowers that to 1.433ms,
5.21x the cached control. Its live allocation is 781,252 bytes.

A 16-cubed source shading volume reduces live allocation to 601,580 bytes, but
passes only 1/24 quality cases, versus 24/24 for exact source methods.
PSNR 38.552 vs 40.296dB; FLIP 0.022787 vs 0.013002; coverage remains identical.
Saturated median 1.459ms; paced GPU p95/p99 2.309/2.437ms.
Actual DRM/PSS snapshots show no demonstrated physical residency saving.

Decision: change representation; retain exact source compilation as conventional
control. Retire repeated full-geometry bands and uniform response volume for G4.
Next bet: submit only intersecting source-cluster ranges; use equally informed
conventional routing. All 26,560 source cards match; all 22,176 wood vertices
inside bounds. Frozen fit/model and eight defaults unchanged; +24 inactive buffer
bytes counted; format/type pass. Failed results preserved; no full suite.
G1–G7 unchanged/unmet.
[Complete evidence](neural-shaded-appearance.md#source-hierarchy-exact-reconstruction-helps-repeated-visibility-work-does-not).
Evidence build/neural-evidence/g4-source-hierarchy-v1 on Pop!_OS.


### 2026-10-27 — Source work lists reject band reuse for G4

AMD RADV RENOIR/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57, Pop!_OS;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Parent54aaf879 plus archived sources, solo/sequential, 64px,
60warmup+240measured frames. Initial assessment376.4s, decision875.8s;
exactly two optimizations, no training/new shaded references.

Nine variants all24/24 against3strong controls; all72RGBA comparisons exact.
Stable range selection removes70.65–72.01% of leaf submissions and63.07% of wood
submissions versus8full bands, but complete cost remains too high.
Reconstructed8-row saturated median1.153ms; batched1.112ms; two32-row bands0.511ms
versus cached full-frame0.278ms. Final paced GPU95/99=1.654/1.848ms versus1.058/1.085ms.
Final source32 requested live986,124bytes versus1,875,712control:47.43% less,
83.32% more saturated GPU time. Physical residency savings unproved.
With identical routing, stored32 uses1,623,540bytes and0.484ms, still no16x ratio.

Decision: retire band reuse on this AMD G4 path. Archive implementations and restore
the exact prior runtime; no new default allocations. Native range/argument checks
cover154ranges,8bands,2views,4lighting repeats. No redundant checks of unchanged
runtime/profiler; no full suite. Frozen fit unchanged,8/24; G1–G7 unchanged/unmet.
Next diagnostic: separate opacity and radiance errors in existing shaded captures
before choosing joint visibility/radiance residuals at a matched conventional byte budget.
[Complete evidence](neural-shaded-appearance.md#source-work-lists-selective-submission-still-fails-the-band-strategy).
Evidence build/neural-evidence/g4-source-worklist-v1 on Pop!_OS.


### 2026-10-28 — Rendered residuals improve averages but fail G4

AMD RADV RENOIR/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57, Pop!_OS;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Parent4e07e0c1 plus archived sources. Solo/sequential,64px,60warmup+240measured.
First executable comparison 799.3s;
decision 1558.9s; exactly two targeted experiments.
3,600 rendered-loss steps; no material-moment fitting or new shaded truth.

Oracle substitutions identify joint coverage/radiance problems, not runtime wins.
Final perceptual CNN passes14/24 versus frozen8/24 and conventional linear13/24;
CNN and first linear both5/12 on view excluded from fitting. Final CNN means
40.530325dB/0.01234041FLIP/0.00174393LPIPS; worst boundary p99=1.414pixels.
Lighting alone changes CNN coverage by0.066285, despite fixed geometry.

Complete paced GPU95/99: CNN0.672/0.674ms, linear0.406/0.407ms,
source cache1.042/1.086ms, frozen0.074/0.269ms. Saturated medians:
0.16116/0.103/0.27652/0.06812ms respectively. Single runs, no paired-speedup claim.
CNN requested engine representation/scratch384,420bytes versus1,466,092source
cache and316,564frozen; all-live794,020/1,875,692/726,164. Only3.814x requested
engine compression, no demonstrated16x actual resident saving. New full-image
correction scratch65,536bytes plusweights is included. DRM/PSS snapshots recorded.

Decision: change representation; retire joint RGBA post-correction. Separate
geometry-only coverage from conditional visible-surface radiance, using equally
informed conventional and learned controls. Preserve failed models, weights and
sources; restore exact prior runtime.32GPU/Torch outputs agree within5.96e-8;
prior8frozen+8cache captures unchanged. Reuse default checks, no redundant suite.
G1–G7 unchanged/unmet.
[Full report](neural-shaded-appearance.md#rendered-error-attribution-change-representation-retire-joint-rgba-correction).
Evidence build/neural-evidence/g4-rendered-error-attribution-v1 on Pop!_OS.


### 2026-10-28 — Geometric opacity heads preserve lighting invariance but lose quality

AMD RADV RENOIR/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57, Pop!_OS;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Parent702ddab3 plus archived sources; solo/sequential,64px,60warmup+240measured.
First complete comparison327.1s,
decision605.9s; zero follow-up optimizations.
Two1200-step rendered-coverage fits; no material-moment fitting/new shaded truth.

Equal-input/equal-byte33-parameter polynomial and ReLU4 opacity predictors run
inside the primitive vertex path, radiance/support frozen. Both preserve exact
lighting-invariant coverage and add128requested bytes with no image scratch.
Both fail quality:5/24 vs frozen8/24, zero/12 on view excluded from fitting.
Polynomial39.867820dB/0.01522767FLIP/0.00200208LPIPS; neural39.451052dB/
0.01581269/0.00231367. Neural worst boundaryp99 rises1.414 to2pixels.

Complete paced GPU95/99 polynomial0.282/0.286ms, neural0.280/0.283ms,
frozen0.269/0.270ms, source1.084/1.088ms. Saturated medians
0.07036/0.06976/0.06820/0.27780ms. Requested engine bytes316,676candidate/
316,548frozen/1,466,076source, only4.630x; no demonstrated16x physical saving.
DRM/PSS and complete CPU/completion costs recorded; no paired speedup claim.

Initial parity failure preserved; one differing wood sample was resolved through
native readback before fitting. ActualGPU/Torch predictors agree within6.855e-7
across1897primitives,2views,2models.8frozen+8cache images unchanged.
Retire scalar-opacity correction; restore prior runtime and reuse default checks.
Next: source-guided support/visibility changes at smaller payload, judged by
shaded images with lighting invariants and equally informed conventional controls.
G1–G7 unchanged/unmet.
[Full report](neural-shaded-appearance.md#geometric-opacity-predictors-invariant-silhouettes-do-not-recover-shaded-quality).
Evidence build/neural-evidence/g4-geometric-coverage-v1 on Pop!_OS.


### 2026-10-28 — Authored response removes a cache; visibility dominates memory

AMD RADV RENOIR/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57, Pop!_OS;
Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Parent124dd389 plus archived sources; solo/sequential,64px,60warmup+240measured.
Initial comparison102.2s,
decision950.0s; exactly two targeted experiments.
No new shaded references; polynomial solves and2400neuralsteps only after
owner-only actual shaded failure identified missing clump interactions.

Full authored field after visibility:24/24, all8RGBA byte-identical to sourcecache;
212,472fewer requestedGPUbytes than exact-response hierarchy. Full-field all-live
1,027,180bytes vs fastsource1,875,660(45.24% less), saturated median0.60796ms vs
0.28000(117.13% more). Paced95/99=1.561/1.654ms vs1.088/1.093ms.
Physical savings unproved. Per-pixel reuse worsens full field to0.79568ms.

Matched12,320byte degree4polynomial/ReLU17 codecs pass7/24and5/24againstcurrent
cache(neural4/24againstforwardsource); cost0.60844/0.50616ms saturated.
Neither wins. Both source labels/inputs/bytes identical; no rendered selection.
All9variants72captures have exact samecoverage;6variants48RGBA exact.
All26560GPUcodec outputs agree withNumPy/Torch within1.595e-6.

Retain exact authored field as optional conventional control; retire owner-only,
reuse and codecs. Retained field/default8images and native memory each match
prior results; format/typepass, no new default allocation/full-suiteclaim.
VisibilityID/depth524,288bytes is84.9% of remaining617,580enginebytes:
next architecture question is visibility storage, not another response model.
G1–G7 unchanged/unmet.
[Full report](neural-shaded-appearance.md#authored-response-after-visibility-exact-reconstruction-beats-the-tested-codecs).
Evidence build/neural-evidence/g4-source-response-v1 on Pop!_OS.


### 2026-10-28 — Packed visibility reduces allocations; visible responses rarely repeat

AMD RADV RENOIR/Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57, Pop!_OS;
Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Parent c77aff17 plus archived sources; solo, sequential, 64x64, 60 warmup + 240 measured.
Initial assessment 166.9s; decision 711.7s. Two follow-ups; no training/new shaded references.

Packed depth16/source-ID16 retains hardware coverage and passes 24/24 against three
controls. Six sample winners change across two views; all 88 coverage captures match.
Mean PSNR 40.295900 dB versus native 40.295971 dB. Packed full field uses 420,980 engine
bytes / 830,580 all-live versus native-field 617,585 / 1,027,185. Complete saturated
median 0.63764 versus 0.60912 ms; fast stored-source 0.27540 ms. Paced packed-field
p95/p99 1.499/1.566 ms. No demonstrated 16x physical saving.

Discard after atomic update has no consistent full-cost benefit. A 12,288-byte visible
response cache hits only 9–12% of leaf samples and worsens full-frame medians to
0.66764 ms native / 0.69588 ms packed. Both optimizations are retired and preserved.

Retain packed visibility as an optional conventional control, disabled by default;
zero new default allocations. Default eight images/allocations and retained sixteen
packed images/live allocations match prior measurements; format/type checks pass.
Next: fuse tile-local visibility/shading rather than cache low-reuse responses.
G1–G7 unchanged and unmet.
[Full report](neural-shaded-appearance.md#packed-visibility-smaller-allocations-little-response-reuse).
Evidence build/neural-evidence/g4-packed-visibility-v1 on Pop!_OS.


### 2026-10-28 — Retire fused compute visibility after exact quality recovery

AMD RADV RENOIR/Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57, Pop!_OS;
Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Parent 9487c46b plus archived sources; sequential, 64x64, 60 warmup + 240 measured.
Initial assessment 163.6s; decision 1161.3s. One follow-up, no training/new references.

Source worklists and 4,096 shared bytes per workgroup remove the full-frame sample
image. Initial analytic paths pass 20/24; snapped coverage plus interpolation
recovers 24/24 against four controls and sixteen exact packed-control images.
Full-field saturated median 1.49664 versus packed hardware 0.64160 ms; paced p95/p99
2.337/2.418 versus 1.490/1.542 ms. Requested live bytes 765,816 versus 830,580,
only 7.80% less; engine bytes 356,216 versus 420,980. No 16x physical-memory claim.

Worklists contain 52,714/52,760 entries within 65,536 capacity, with valid IDs,
duplicate-free rows and consistent prefixes/cursors. The local visibility/shading
kernel dominates paced cost. Skip a second memory-only trial and retire this path.
Restore all runtime sources byte-for-byte to 9487c46b; reuse verified default checks.
Preserve failed quality results and the corrected stable-list readback diagnostic.
G1–G7 unchanged and unmet.
[Full report](neural-shaded-appearance.md#fused-source-visibility-exact-images-recover-complete-cost-fails).
Evidence build/neural-evidence/g4-fused-source-v1 on Pop!_OS.

### AMD source-conditioned sample integration (2026-10-28)

On RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0 compilerde9b81e5, parent4a99ffb6 plus archived sources:
centroid4/8 cost0.44052/0.51460ms saturated GPU but pass only1/24 shaded cases.
Selecting actual source points8 passes16/24 current/15/24 forward at0.629ms,
versus exactfield0.638ms and fastsource0.28140ms; all48coverage images exact.
Source8 paced GPU95/99=1.58658/1.63398ms versus exactfield1.48228/1.55041.
All candidate requestedlive830,580bytes unchanged; physical DRM/PSS snapshots
do not establish an appearance-memory reduction. One follow-up,0training/newrefs;
retired all new runtime. G4unmet. Completed references and existing checks reused.
See [full quality/cost/memory evidence](neural-shaded-appearance.md#source-conditioned-sample-integration-retires-response-sharing).

### AMD exact spatial hierarchy retirement (2026-10-28)

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0 compilerde9b81e5, parenta26bc0d7 plus archived sources.
Six64pxvariants: all24/24quality,32hierarchyRGBAexactpackedcontrol,48coverageexact.
Compactfullfield live636,324requestedbytes versuspacked830,580; physical16xunproved.
Tight-footprint GPUmedian36.71596ms/pace95/99=37.12236/37.16383ms versuspacked
0.64360ms/1.49463/1.54513ms. Fastnative0.27908ms. One follow-up roughlyhalves
initial traversal cost but fails decisively. Retire;0training/newrefs;runtime restored.
[Complete cost, quality and memory evidence](neural-shaded-appearance.md#source-space-visibility-hierarchy-exact-quality-unacceptable-cost).

### AMD wood-depth integration retirement (2026-10-28)

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0 compiler de9b81e5, parent f586361b plus archived sources.
Five 64px variants: frozen hard/CDF both 8/24, conventional hard/CDF both 5/24
against each of four source controls. Frozen saturated GPU0.06100 ->0.07376ms;
paced p95/p99 0.24960/0.25230 ->0.29860/0.30254ms. Mean PSNR+0.00225dB;
16 coverage images exact; engine/live allocation316,532/726,132bytes unchanged.
Native control0.27752ms, engine/live1,466,060/1,875,660bytes. Actual asset-specific
physical16x reduction unproved. Retire; no follow-ups/training/newreferences.
[Complete quality, runtime, physical memory and focused checks](neural-shaded-appearance.md#conditional-depth-at-wood-no-useful-rendered-advantage).

### AMD unconditional response-mixture retirement (2026-10-28)

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0 compilerde9b81e5, parentc359b103 plus archived sources.
Frozen fourcomponent9/24 vsoriginal8/24; conventional6/24vs5/24.
Frozen saturatedGPU0.09144vs0.06144ms; requestedlive847,540vs726,132bytes.
One unreduced source-response follow-up reaches9/24at0.28616ms/1,945,112livebytes;
native0.27936ms/1,875,660bytes. No quality-matchedmemory or neural win.
40coverageexact,24controlRGBAexact,24mathprobes. Inactive selector runs preserved,
excluded and repaired. No training/newrefs; runtime restored.
[Quality, complete cost and physical memory](neural-shaded-appearance.md#source-response-mixtures-retire-a-single-colour-per-aggregate).

### AMD joint source partition retirement (2026-10-28)

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0 compilerde9b81e5, parent4a7d04b9 plus archived payloads.
Spatial/joint median trees0/24. Source-balancedLloyd3/24 vsestablishedspatial5/24
andfrozen8/24, despite43.4%lower source response residual; coverageMAE worsens.
Same316,532engine/726,132all-live requestedbytes. JointGPU0.06872ms vscontrol
0.06804ms; no quality-memory-runtime win. One follow-up, no neuraltraining/newrefs.
24controlRGBA exact; allcoverage light-independent; runtime restored.
[Full quality, runtime and physical memory evidence](neural-shaded-appearance.md#joint-source-partitioning-better-response-statistics-worse-rendered-gates).

### AMD conditional material comparison (2026-10-28)

RADV RENOIR1002:164c/Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 compiler de9b81e5, parent 5a1aa8db plus archived variants.
Frozen conditional field derivatives pass14/24 vs 8/24 original; conventional 9/24
vs 5/24. Complete GPU 0.10108 ms vs 0.06120 frozen  / 0.27488 native; requested-live
773,064 vs726,132 frozen /830,580 packed-field /1,875,660 native bytes.
Mean image quality improves, coverage stays exact and ten cases still fail.
Physical DRM/PSS recorded, but 16x asset appearance residency remains unproved.
Decision: change coverage/visibility representation; preserve cached derivatives
as a component. Two follow-ups, zero training/new references; default restored.
[Full quality, runtime and physical memory evidence](neural-shaded-appearance.md#conditional-unlit-material-useful-gain-change-the-g4-representation).

### AMD source boundary mesh retirement (2026-10-28)

RADV RENOIR 1002:164c/Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 compiler de9b81e5, parent 60e6b323 plus exact archived variants.
Fine source-boundary mesh 17/24 quality cases, 8.75952 ms GPU, 801,772 live requested
bytes; compact informed field 24/24, 0.64060 ms, 830,580 bytes. One coarser follow-up:
1/24, 1.78924 ms, 760,812 bytes. No joint quality-memory-runtime advantage.
40 captures; original controls byte-exact; source compiler checks passed.
Retire; no second sweep, training or new references. Runtime restored.
[Full quality, cost and physical memory evidence](neural-shaded-appearance.md#source-boundary-meshes-retire-the-per-leaf-geometry-expansion).

### AMD compact learned coverage retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 compiler de9b81e5; parent 14e4285d plus archived variants.
At 64px, vectorized neural coverage: 14/24 cases, 6.30516 ms complete GPU span,
749,184 live requested bytes. Conventional fitted grid: 16/24, 0.61752 ms,
748,924 bytes. Exact source program: 24/24, 1.89936 ms, 748,156 bytes.
Paced GPU tails, silhouettes and actual whole-client DRM/PSS also recorded.
No joint advantage; retire after one frozen-weight optimization. First comparison
5.52 minutes; decision 16.40 minutes. Runtime restored; failed results preserved.
[Complete quality, runtime and memory evidence](neural-shaded-appearance.md#compact-learned-coverage-retire-per-fragment-decoding).

### AMD clump visibility retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 compiler de9b81e5; parent 63d67522 plus archived variants.
One layer: 0/24 shaded cases, 0.82956 ms complete GPU, 656,844 live requested bytes.
Four layers: 0/24, 0.86428 ms, 919,908 bytes. Compact source: 24/24, 0.63828 ms,
830,596 bytes. Coverage exact in both candidates, selected materials incorrect.
Rebuild every frame included; actual DRM/PSS snapshots recorded. Retire after one
follow-up; no training/new references. Failed results preserved, runtime restored.
[Full quality, cost and memory evidence](neural-shaded-appearance.md#clump-visibility-tiles-exact-silhouettes-insufficient-surface-information).

### AMD source-guided sampling retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 compiler de9b81e5; parent b8e8a036 plus archived sources.
Four 32-frame analytic clips, 192x108, all scales. At 4x balanced FLIP 0.265405,
LPIPS 0.317812, PSNR 15.556 dB vs regular 0.243244/0.253246/16.369 dB;
opaque temporal error +40.0%. 1080p case1 complete GPU median/p95:
regular 21.97056/22.26636 ms, uniform 31.57704/31.72848, balanced 33.42048/33.57992.
All methods requested live/peak 89,493,544/97,787,944 bytes. Actual whole-client
DRM/PSS and completion costs recorded; no asset-specific physical saving.
First assessment 7.95 minutes; decision 20.46 minutes. No follow-up, training or
reference expansion. Retire both scattered paths; preserve all failed results and
restore exact prior runtime. G1–G7 unchanged/unmet.
[Complete comparison and limitations](neural-adaptive-sampling.md).

### AMD persistent surface cache and retained jitter preparation (2026-10-28)

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0 compilerde9b81e5; parentb3332131 plus exact archived sources.
Four192x10832-frameclips at1x-4x;1080p4xcase1runtime. Two-slotcache14.46528ms,
81,391,420livebytes but4xFLIP+10.2%,temporal+68.2%. Four-slot+table14.83664ms,
114,761,532bytes vsregular+table18.49892ms/89,493,804bytes;2xFLIP+37.28%.
Retirecache aftertwofollowups; no training/newreference method orcases.

Retain shared jitter preparation in fixed non-indexed source decoder only.
FinalGPUmedian/p95=18.49648/18.80460ms vsoriginal21.96008/22.28536;
live/peak=89,493,784/97,788,184B,240netaddedbytes,no ordinary-rendererallocation.
All1024pairedRGBA and1152finalfixed/frozenimage/reference/mask files exact.
Whole-clientDRM/PSS andcompleteCPU/completionrecorded;singlecomparisons,not
sustainedG6or16xappearance-memoryclaims. Goals unchanged/unmet.
[Full quality, costs, failures and verification](neural-surface-cache.md).

### AMD coverage/radiance factorization (2026-10-28)

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent6a102762 plus archived sources.
Four public192x10832-frameclips at1x–4x; timing1080p4xcase1.
Regular4xFLIP .24324395/LPIPS .25324590/opaque temporal .02631498;
selected factorization .17217554/.19584559/.01482404. Same source parameters,
colour budget and history. Complete GPU18.49432→15.97144ms; final explicit
raw-factor 15.95700/18.09632ms median/p95.
Live/peak 89,493,784/97,788,184B unchanged. Physical DRM/PSS and
CPU/completion recorded separately. No16x compression, shadedG4, general3D,
sealed validation or sustainedG6 claim. One follow-up; no training/newreferences.
[Full quality, boundary errors, costs and failures](neural-coverage-radiance.md).

### AMD exact visibility with compact material lookup (2026-10-28)

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent83a2f998 plus archived variants.
Existing64px tree,2views×4lights×3backgrounds; AA64references unchanged.
Frozen/conventional full material under exact source visibility pass4/24;
source-normal/depth follow-up raisesboth to21/24. Fullfrozen GPU.35120ms,
914,044liveB vs compactsource.63812ms/830,580B andnative.27572ms/1,875,660B.
Colour-only.64276ms/898,868B stillfails3cases andpacedcost. No16x quality-matched
memory or neuralwin. All56captures,24exactpriorcontrols,32exactcandidatealpha,
complete CPU/GPU/completion,DRM/PSS andfailedvariants preserved. Runtime restored.
[Decision and full measurements](neural-shaded-appearance.md#exact-surface-visibility-with-compact-materials-retire-aggregate-response-lookup).

### AMD sparse source directional field (2026-10-28)

Correction: inherited CPU targets used leaf coverage as field blend. GPU measurements remain valid; see the [corrected source-target comparison](neural-shaded-appearance.md#adaptive-response-trees-correct-the-targets-retire-the-cost-tradeoff).

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent250fcd5c plus archived variants.
Existing64px tree,2views×4lights×3backgrounds; allsourcevariants24/24 numericcases.
Pointcutoff8 omits70.69%normal/exponential terms butGPU.62000ms is nearlyexact-owner
.62396ms; exactfield.63800ms. Ownerlist imagesexact vs cutoff8 but.74136ms and
832,684liveB vs830,580B. No worthwhile jointwin or16xmemoryclaim. One follow-up;
no training/newreferences. Source-pointbounds,completeGPU/CPU/completion,DRM/PSS
andfailedinit preserved. Appearance runtime restored exactly toparent.
[Full comparison and decision](neural-shaded-appearance.md#sparse-source-field-evaluation-quality-preserved-execution-retired).

### AMD compact neighbor-field residuals (2026-10-28)

Correction: inherited CPU targets used leaf coverage as field blend. GPU measurements remain valid; see the [corrected source-target comparison](neural-shaded-appearance.md#adaptive-response-trees-correct-the-targets-retire-the-cost-tradeoff).

RADV RENOIR 1002:164c/Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0/compiler de9b81e5; parent 76b78aab plus archived variants.
Existing 64 px tree, 2 views×4 lights×3 backgrounds. Both 7,040 B fits fail all 24 cases with
exact source alpha. Neural 36.7506 dB/0.44136 ms/837,612 live B versus exact field
40.2959 dB/0.64056 ms/830,580 B; fastest cache 0.27780 ms/1,875,660 B.
First comparison 4.20 minutes, decision 7.21 minutes; zero follow-ups/new references.
Retired after all-card GPU agreement. Complete CPU/GPU/completion and actual DRM/PSS
preserved. No paired speedup/16× physical memory/G4 claim. Runtime restored.
[Full comparison](neural-shaded-appearance.md#neighbor-field-residuals-retire-compact-owner-local-response-fitting).

### AMD shared coverage/radiance support tables (2026-10-28)

RADV RENOIR1002:164c/Ryzen 5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent5c61df01 plus archived variants.
1080p,4×,case1,32 warm-up/240 measured frames; three alternating pairs.
GPU median15.95852→10.77440ms (32.52% saving); same512 tested RGBA frames.
Requested live89,493,784→92,013,064B; all tables/motion/scratch/fallback counted.
Final cleaned implementation10.77568ms; actual DRM/PSS and
completion recorded separately. Two follow-ups, no training/new reference cases.
Predicted-half-motion failure preserved; final selected/frozen/cut checks pass.
Conventional fixture gain only; G4 and sustained G6 remain unqualified.
[Full measurements](neural-coverage-radiance.md#shared-support-tables-retain-exact-output-with-lower-complete-gpu-cost).

### AMD cooperative observation-gather retirement (2026-10-28)

RADV RENOIR1002:164c/Ryzen 5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent147049ed plus archived variants.
Four existing 192×108 clips, 32 frames, all scales: both candidates' images exact.
Complete 1080p/4×/case1 GPU median/p95: fragment10.78208/12.14824 ms,
direct compute13.10052/13.93748 ms, cooperative47.71356/50.73932 ms.
All live/peak requested92,013,064/100,307,464B; cooperative adds13,312B shared/group.
Actual whole-client DRM/PSS and completion tails preserved. No joint advantage.
First comparison7.10min; decision8.87min.
Zero follow-ups/training/new references. Failed build and variants preserved;
runtime restored exactly. No neural/G4/sustained-G6 claim.
[Full comparison](neural-coverage-radiance.md#cooperative-observation-gather-retire-shared-decoding).

### AMD adaptive response and corrected-target retirement (2026-10-28)

RADV RENOIR1002:164c/Ryzen 5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent28ea20bf plus archived variants.
Existing64px tree,2views×4lights×3backgrounds. Corrected full/budget trees24/24:
GPU0.63988/1.36124ms, live1,479,420/843,852B. Matched cache0.37568ms/1,043,052B;
exact field0.63832ms/830,580B. Corrected7040B neural4/24, polynomial6/24.
Both837,612liveB; neural0.44104ms, polynomial0.49860ms. No joint win.
Inherited CPU parser misread leaf coverage1.65 as field blend1.10; prior GPU
measurements stand, but affected target-based interpretations are superseded.
First comparison6.23min; decision22.89min.
Two follow-ups,2,400 corrective steps/head, no new references. Actual DRM/PSS,
all costs, source/decoder checks and failed results preserved. Runtime restored.
[Full comparison and correction](neural-shaded-appearance.md#adaptive-response-trees-correct-the-targets-retire-the-cost-tradeoff).

### AMD source contributor routing retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 / compiler de9b81e5; parent bf9314f1 plus archived variants.
Existing 64-pixel tree, two views × four lights × three backgrounds.
Owner/spatial masks pass 24/24 shaded gates with exact source alpha but cost
0.69940 / 0.73768 ms versus stable full field 0.63848 ms. One follow-up applies
direct summation fairly: full 0.59024 ms, owner 0.66040 ms, spatial 0.69468 ms;
requested live 830,580 / 830,924 / 838,748 bytes. Matched cached source:
0.34460 ms / 1,043,052 bytes; native source: 0.28416 ms / 1,875,660 bytes.
Direct full-field paced GPU p95/p99 1.82223 / 2.03297 ms versus matched cache
1.22550 / 1.26566 ms. No paired or sustained tail improvement claimed.
Frozen fit 0.06840 ms / 726,132 bytes still passes only 8/24.
Actual DRM/PSS snapshots, all requested allocations, full frame costs and failed
results retained. First comparison 3.60 minutes;
decision 13.82 minutes. No training or new references.
Retire masks, restore runtime; no joint G4 advantage.
[Full comparison](neural-shaded-appearance.md#source-contributor-routing-preserve-quality-retire-the-lookup-cost).

### AMD sparse optical source bricks retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 / compiler de9b81e5; parent 3a945ec6 plus archived variants.
Existing static 64-pixel tree, two views × four lights × three backgrounds.
Corrected 16³/32³/64³ volumes pass 0/24 gates; PSNR 26.78/30.69/35.89 dB,
GPU median 0.16816/0.29108/0.53292 ms, live 669,408/787,936/1,549,792 B.
Matched source passes 24/24: 40.30 dB, 0.34568 ms, 1,043,064 B.
32³/AA4 gains only 0.12 dB, costs 1.05260 ms / 1,377,760 B and still passes 0/24.
64³ paced GPU p95/p99 1.33754/1.44686 ms versus matched 0.35947/0.90365 ms.
Full cell decode/lighting/cache, visibility, wood, updates and composite included;
decoded buffers and larger upload ring charged. Actual DRM/PSS retained separately.
Initial wrong projection is preserved; corrected analytic traversal and half decode
pass. First corrected comparison 6.44 minutes,
decision 10.71 minutes. Two follow-ups; no training/new references.
Retire homogeneous optical cells; restore runtime; no joint G4 advantage.
[Full measurements](neural-shaded-appearance.md#sparse-optical-bricks-fast-coarse-rendering-insufficient-surface-fidelity).

### AMD exact surface blocks retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 / compiler de9b81e5; parent d4628d37 plus archived variants.
Existing static 64-pixel tree, two views × four lights × three backgrounds.
All four block variants pass 24/24 gates and reproduce packed source RGBA exactly.
One balanced-scheduling follow-up lowers full-field GPU median 13.25096 to
3.91400 ms; matching hardware field 0.64852 ms. Requested live 619,968 versus
830,580 B. Balanced cache 3.71784 ms / 832,440 B; matching cache
0.34576 ms / 1,043,052 B. Field paced GPU p95/p99 4.97893/5.07260 ms versus
1.78904/1.88645 ms. Shared scratch rises 4,096 to 12,292 B per workgroup.
Full decoding, block routing, depth ordering, wood, material response, updates
and composite included; all buffers/caches counted. Whole-client DRM/PSS
observations retained separately; no asset-attributed 16× residency claim.
First assessment 14.24 minutes, decision
23.10 minutes. No second follow-up, training or new references.
Retire this software rasterization architecture; restore parent runtime.
[Full quality/runtime/memory measurements](neural-shaded-appearance.md#exact-surface-blocks-preserved-quality-excessive-software-rasterization-cost).

### AMD hardware block proxy retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 / compiler de9b81e5; parent f973d593 plus archived variants.
Same static 64px tree, two views × four lights × three backgrounds.
Full-frame and corrected eight-row proxies reproduce source RGBA exactly, 24/24.
Full-frame field GPU median 7.37296 ms / 931,252 live requested B.
Corrected banded field 9.21288 ms / 644,532 B versus source
0.64120 ms / 830,584 B: 22.40% fewer requested bytes, 14.37× GPU cost.
Corrected banded cache 8.04348 ms / 857,004 B versus matched
0.35888 ms / 1,043,056 B. Full decode/intersection/wood/order/updates/composite
charged; actual DRM/PSS observations separate, no asset-attributed 16× claim.
Initial band-coordinate defect and failed results preserved; repaired outputs exact.
First corrected comparison 12.30 minutes;
decision 13.70 minutes. One repair, no training/new references.
Retire rectangle proxies with per-fragment source traversal; restore runtime.
[Full quality/runtime/memory evidence](neural-shaded-appearance.md#hardware-block-rectangles-exact-visibility-expensive-fragment-traversal).

### AMD source material basis retention (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 / compiler de9b81e5; parent 4e78b3ae plus archived sources.
Four public 192×108 clips, 32 frames, 1×–4×: at4× raw-factor FLIP0.17217554,
LPIPS0.19584559, mean PSNR18.10427dB; guided material basis FLIP0.00200035,
LPIPS4.85658e-8, PSNR87.14079dB. Adds authored material-region inputs;
this is a conventional specialization, not a fixed-input/neural win.
Full1080p4×case1 GPU median/p95: raw-factor10.75508/12.03020ms,
guided1.60788/1.90288ms. Requested live/peak92,013,084/100,307,484B
versus48,045,912/56,340,312B. Actual guided DRM VRAM/GTT145,371,136/
18,608,128B; browser PSS537,577,472B, whole-client, not asset residency.
Three paired guidance changes+1.43584/-0.21874/+1.61796% are inconclusive;
retain for fixed cold missing-colour quality with unchanged allocation.
Targeted cases19/53 improve worst-frame FLIP0.03487/0.04820 to0/0.000305.
All source samples, selection, coverage integration, common textures/scratch,
motion and composite counted. This fixture has no wood/sorting workload.
Initial assessment13.94min; decision35.74min.
Two follow-ups, zero training, two justified existing-catalog reference cases.
Frozen models/goals unchanged. 3,200 parity files and cut/source checks pass.
[Full comparison and limitations](neural-coverage-radiance.md#source-material-basis-retain-guided-observations-for-the-supported-family).

### AMD direct source density retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 / compiler de9b81e5; parent02a84e9f plus archived variants.
Existing64px tree, two views × four lights × three backgrounds, AA64 references.
Full/half/quarter source density: PSNR40.29590/36.24121/33.47724dB,
FLIP0.0130032/0.0201182/0.0273694, LPIPS0.00249059/0.00541399/0.01412255.
Both reduced levels0/24 numeric gates. Worst boundary p99:2/2/2.236px.
Field full/half/quarter saturatedGPU0.64056/0.59596/0.57240ms,
all830,580requestedliveB. Cache0.34564/0.30080/0.28076ms,
live1,043,052/936,812/883,692B. Quarter-field pacedp95/p99
1.75356/1.95366ms versus full1.51103/1.56699ms; no paired gain claim.
Full decode, original cutouts, ordering, wood, exact field/cache, updates and
composite counted. Whole-clientDRM/PSS preserved; no16xresident claim.
First comparison7.06min; decision8.53min.
No follow-up, training or new reference.64captures,16field/cache exact pairs,
24prior-control exact images. Initial cache-tail guard defect preserved/fixed
before reduced cache runs. Runtime restored; existing engineLOD unchanged.
[Full measurements and architectural decision](neural-shaded-appearance.md#direct-source-density-retire-area-preserving-thinning-for-g4).

### AMD hardware response grid retirement (2026-10-28)

RADV RENOIR 1002:164c / Ryzen 5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent37102f0b plus archived variants.
Same64px tree,2views×4lights×3backgrounds, completedAA64references.
11³/22³Rgba16Float normal-depth grids:10,648/85,184B, PSNR33.95858/38.66374dB,
FLIP0.04768960/0.02221810, LPIPS0.01062023/0.00342961, gates0/24and2/24.
One raw-gradient/signed-distance follow-up:PSNR33.45534/38.53724dB,
gates0/24and1/24. All32grid alpha images exactly match source.
22³normal-depth fullGPUmedian/p95 0.36056/0.47876ms versus exactcache
0.34600/0.47376ms; pacedp95/p99 1.23669/1.26564 versus1.21102/1.27494ms.
Live/peak915,764/932,148B versus1,043,060/1,059,444B. Grid11live841,228B.
Actual22³DRM VRAM/GTT17,698,816/9,936,896B; PSS488,910,848B, whole-client.
Source geometry, cutouts, wood, visibility, gridquery, shading, updates and
composite charged. Static gridcooking excluded from steadytime but storage/peak
included; dynamicedit rebuild unmeasured. No paired or16×appearance claim.
First comparison7.98min; decision12.89min.
Onefollow-up, no training/newreference. Failedbinding/build preserved.
64captures,24prior controls exact; runtime restored.
[Full measurements and decision](neural-shaded-appearance.md#hardware-response-grids-exact-silhouettes-insufficient-material-fidelity).


### AMD spatial radiance source comparison (2026-10-28)

RADV RENOIR1002:164c / Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parentb39f17c2 plus archived variants.
Two new192×108,32frame affine/bilinear radiance clips over existing geometry,
1×–4×. Palette/observed-bilinear/direct-source FLIP0.27222473/0.00252797/
0.00152334; LPIPS0.01576336/1.23050e-7/1.24333e-8;
meanclipPSNR22.19894/79.35112/84.84180dB. GPU foreground geometry moment
exact across256frames/5,308,416pixels; shaded edge errors reported separately.
Full1080p4×case72 GPUmedian/p95 1.09688/1.25856,1.79748/1.89284,
1.43692/1.49072ms. Requestedlive/peak48,045,912/56,340,312,
48,021,760/56,316,160,25,212,040/33,506,440B.
Direct actualDRM VRAM/GTT111,140,864/18,608,128B,PSS539,652,096B;
whole-client,not perasset. All applicable sampling,refresh,motion,visibility,
integration,common storage and composite included; this fixture has no wood/sort.
Firstassessment16.59min; decision20.34min; onefollow-up, no training.
Direct integration exploits the same authored source; no privileged neural inputs.
Retain opt-in diagnostic controls; change architecture toward exact source terms
plus only expensive residual approximation. No paired/G4/worldleadership claim.
[Full evidence and limitations](neural-coverage-radiance.md#spatial-radiance-compile-known-formulas-before-estimating-coefficients).


### AMD source-domain field bounds (2026-10-28)

RADV RENOIR1002:164c / Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parentdfef2053 plus archived variants.
Existing64pxtree,2views×4lights×3backgrounds,AA64references. Source-region
cutoff4mask retains16.43of44clumps onaverage;24/24shaded numeric gates.
PSNR40.28407dB,FLIP0.01312640,LPIPS0.00248835 versus fullfield40.29590/
0.01300320/0.00249059. All32candidatealpha images exact to source.
Five paired completeGPU median changes:-21.53,-20.69,-19.86,-21.54,-21.83%.
Geometricmean saving21.10%,95%exact pairedbootstrap20.37–21.66%; CPU inconclusive.
Single fullfield/cutoff4/exactcache GPU0.63832/0.50308/0.34760ms;
pacedGPU p95/p99 1.50256/1.52333,1.36733/1.40551,1.20235/1.23112ms.
Requestedlive830,580/830,924/1,043,052B; cutoff4peak847,308B.
Cutoff4actualDRM VRAM/GTT17,678,336/10,035,200B,PSS487,397,376B,whole-client.
All source geometry,cutouts,wood,visibility,field/shade,updates andcomposite
counted; staticmaskcook excludedsteadytime butstorage/peak included.
Twofollowups, no training/newreferences; firstassessment7.09min,decision14.99min.
Retain opt-in bounded field; exactcache faster/larger. NoG4/16x/global claim.
[Full comparison and limitations](neural-shaded-appearance.md#source-domain-field-bounds-retain-a-conventional-gpu-speed-tradeoff).


### AMD source bounds across additional views (2026-10-28)

RADV RENOIR1002:164c / Ryzen 5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent2cc22613 plus archived variants.
16 existing public validation views, 64px, four lights, three backgrounds.
AA8 screen passes: cutoff4 178/192, cutoff5 191/192, cutoff6 192/192.
Existing CLI AA32/64 confirmation: cutoff4 loses0.44766dB and reaches
1.100013LPIPS; cutoff5 stillfails1.058506LPIPS; cutoff6 passes6/6selectedcases.
All192candidate alpha images exactly match source. No broad AA64 qualification.

Cutoff6 five-pair median GPU saving4.35% vs full field; geometric3.51%,
conditional pairedbootstrap95%CI2.20–4.63%. CPU inconclusive.
Single fullfield/cutoff6/cache GPUmedian0.60500/0.57744/0.35216ms;
pacedp95/p99 1.49421/1.53206,1.47600/1.51526,1.42566/1.48716ms.
Requestedlive830,580/830,924/1,043,052B; cutoff6peak847,308B.
Cutoff6actualDRM VRAM/GTT17,678,336/10,035,200B,PSS488,007,680B,
whole-client observations, not per-asset physical savings.
All source geometry, decoding, cutouts, visibility, wood, field/shade, updates
and composite counted; static maskcook excluded steady time but storage/peak
included. Dynamic edits unmeasured. Two follow-ups; no training; four justified
reference captures. First assessment12.61min; decision18.09min.
Change representation for G4; retain stricter cutoff6 diagnostic. Earlier21.53%
cutoff4speed result remains true on its two views but fails broader quality.
[Full measurements and limitations](neural-shaded-appearance.md#broader-views-overturn-the-aggressive-cutoff).


### AMD source-compiled far-field aggregates (2026-10-28)

RADV RENOIR1002:164c/Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e5; parent2e32bb90 plus archived variants.
Same64px tree;36AA64 shaded comparisons,192additional-viewAA8screen rows.
Compact cutoff4quadratic and cutoff2eight-region linearlog both36/36AA64 against
native/compactcache/fullfield controls. Regional191/192nativeAA8screen; the sole
borderline case passesAA32/AA64. Compact192/192. All288candidatealpha images exact.
FullGPUmedian compact/regional/cache/fullfield0.49672/0.44004/0.35248/0.60264ms.
Five-pair median savings vsfullfield17.93%/26.55%; geometric18.03%/26.58%,
conditional95%pairedbootstrap17.61–18.46%/26.34–26.80%. CPU inconclusive.
Tables7,392/22,880B vs212,480Bexactresponsecache; totalrequestedlive
837,964/853,452B vs1,043,052Bcache; peaks854,348/869,836B.
Actualcompact/regional DRM VRAM17,694,720/17,678,336B,
GTT9,953,280/10,035,200B, PSS489,110,528/490,124,288B, whole-client observations.
All geometry,decode,cutouts,visibility,wood,near/aggregate/cache,shade,frameupdates
andcomposite counted. Staticcook excludedsteadytime butstorage/peak charged;
dynamic sourceedit rebuild unmeasured. No16×physicalresident/worldleadership claim.
First assessment9.71min; decision30.68min. Two follow-ups, no training,
four justified reference captures; one layout repair and one helper fix preserved.
Retain both conventional diagnostic recipes; defaults unchanged.
[Full measurements and limitations](neural-shaded-appearance.md#source-compiled-far-fields-recover-quality-and-useful-gpu-savings).

## Source population viability: compact table sharing and hidden resident cost (2026-10-28)

Solo Pop!_OS AMD RADV RENOIR, Wrela 0.9.0/de9b81e. Ten26,560-card layouts over one 44-clump crown, two difficult views/lights, three backgrounds; AA 32→AA 64 convergence passes all 20 cases. All controls share wood/cutouts/clumps/visibility scratch. Complete320×128 ten-crown frames include decoding/placement, visibility/depth ordering, wood, updates, copies and composite. Static cooking excluded steady time, retained allocations counted. Not the fixed-canopy10× density gate.

| Method | Saturated GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Native exact cards | 2.58168 / 2.61652 | 10,327,756 / 10,491,596 | 26,271,744 / 9,936,896 |
| Compact exact cache | 3.29660 / 3.33384 | 3,758,188 / 3,922,028 | 18,231,296 / 10,199,040 |
| Exact full field | 6.01572 / 6.04880 | 1,633,396 / 1,797,236 | 18,038,784 / 10,297,344 |
| Compact cutoff 5 | 5.44260 / 5.53416 | 1,640,780 / 1,804,620 | 18,046,976 / 10,297,344 |
| Exact cache + owner table, retired | 3.09440 / 3.13384 | 3,784,748 / 3,948,588 | 101,040,128 / 10,199,040 |

Cutoff 4 and regional 2 fail1/60 and 5/60 rows. First follow-up cutoff 5 passes all 60 plus36 legacy rows (90 distinct); silhouettes match the compact exact control. Five paired complete GPU runs show9.63% saving versus full-field (geometric 95%CI9.564–9.735%). It remains65.1% slower than compact caching, at 2.290× lower requested allocations. Second follow-up owner table is byte-exact and 6.04% faster than exact cache but rejected for80+MB resident VRAM growth in all three variants. Removing its GPU-cooked implementation restores18,042,880 B resident VRAM,10,297,344 B GTT and 1,640,780 B requested live. Exact driver allocation cause is unproved. DRM/PSS are whole-client observations; do notsum UMA pools.

First assessment 23.19 minutes; exactly two optimizations, zero training. Frozen models and G1–G7 unchanged/unmet. Decision: change representation toward source-compiled response residuals; retain cutoff 5 as a bounded diagnostic. [Full quality, pacedGPU/CPU completion, resident observations and evidence](neural-source-population.md).

## Material-statistic packing fails the joint G4 gate (2026-10-28)

Solo Pop!_OS AMD RADV RENOIR, Wrela 0.9.0/de9b81e, Mesa 26.1.6, Chrome 154.0.8037.57. Ten 26,560-card layouts, complete 320×128 frames; equal source information, visibility, wood, cutouts and completed shaded references. First assessment 27.14 minutes, decision 37.77 minutes, two follow-ups, zero training/new references.

| Method | Quality passes / 60 | Saturated GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: | ---: |
| Native exact cards | 60 | 2.56868 / 2.60796 | 10,327,756 / 10,491,596 | 26,533,888 / 10,100,736 |
| Exact normal/attenuation cache | 60 | 3.27896 / 3.32620 | 3,758,188 / 3,922,028 | 17,768,448 / 10,035,200 |
| 32 bits: 12+12 normal, 8 attenuation | 59 | 3.29120 / 3.32296 | 2,695,788 / 4,820,644 | 26,427,392 / 10,297,344 |
| 32 bits: 12+12+8, chunked construction | 59 | 3.28712 / 3.33744 | 2,695,788 / 2,859,628 | 18,034,688 / 10,231,808 |


Uniform 16-bit formats pass only 3/60 and 0/60. Precision reallocation improves 32-bit 56/60 to 59/60, still failing LPIPS. Bounded construction preserves 160 parent images and fixes a resident penalty; it does not produce a quality-passing renderer. Full cache replacement plus rendering costs 4.50912 ms native / 5.01492 exact / 6.78132 chunked exact / 7.11080 chunked compact. Requested peaks: 18,826,996 / 5,883,044 / 5,948,580 / 3,856,532 B. Observed resident VRAM: 26,443,776 / 34,652,160 / 34,902,016 / 26,259,456 B.

Complete frames include placement/decoding, depth ordering, visibility, wood, updates, copies and composite. Static cooking is excluded steady timing and measured through replacement with old caches still live. This measures invalidation cost, not edited-source correctness. GPU results are descriptive 240-frame runs after 60 warmups. Paced tails, completion, PSS and startup observations are in the report; whole-client UMA pools must not be summed.

Retire uniform per-card packing; all engine/studio code restored. 720 captures and failures preserved; G1–G7/frozen models unchanged. Next bet: source-certified adaptive shading with exact coverage and sensitive-material fallback. [Full comparison and evidence](neural-response-statistics.md).

## Exact sample lanes: faster conventional rendering at unchanged quality (2026-10-28)

Solo Pop!_OS AMD RADV RENOIR; Wrela 0.9.0/de9b81e, Mesa 26.1.6, Chrome 154.0.8037.57. Ten 26,560-card layouts over one crown, 320×128 complete frames. Same visibility, source, wood and material; existing converged references, no training/new truth. First assessment 16.82 minutes; decision 26.43 minutes; two follow-ups.

| Method | Saturated GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Native serial | 2.56848 / 2.60780 | 10,327,756 / 10,491,596 | 26,271,744 / 9,936,896 |
| Native, sixteen lanes / 64 KiB staging | 1.82084 / 1.88512 | 10,393,292 / 10,557,132 | 26,480,640 / 10,199,040 |
| Native, shared duplicate suppression (retired) | 1.97984 / 2.03584 | 10,393,292 / 10,557,132 | 26,542,080 / 9,936,896 |
| Compact exact cache, serial | 3.29456 / 3.33116 | 3,758,188 / 3,922,028 | 18,038,784 / 10,199,040 |
| Compact, sixteen lanes / 64 KiB staging | 2.37780 / 2.40796 | 3,823,724 / 3,987,564 | 17,776,640 / 10,100,736 |
| Compact, shared duplicate suppression (retired) | 2.55376 / 2.58196 | 3,823,724 / 3,987,564 | 18,235,392 / 10,100,736 |


Five alternating pairs: native 29.6196% lower complete GPU time (geometric 95% CI 28.6226–29.8305%); compact 27.7395% (27.6959–27.8667%). CPU results inconclusive. Full replacement plus rendering improves 4.50252→3.74344 ms native and 5.00860→4.11744 ms compact; old caches remain live during construction. All 640 candidate RGBA images match their controls; all 60 scored comparisons pass. Large staging and duplicate suppression are retired, with failures retained.

Whole-frame cost includes placement/decoding, visibility/depth ordering, wood, shading, scratch, updates, copies and compositing. Requested live/peak counts every observed resource; whole-client DRM/PSS observations are not per-asset attribution or physical peaks and UMA pools must not be summed. Retained fresh residency: native VRAM/GTT 26,542,080/10,199,040 B; compact 17,780,736/10,199,040 B. The winner adds 65,536 B global and 1,024 B workgroup storage.

Continue AA4 sixteen-lane shading; AA2 and frozen-default image/resource checks are exact. G1–G7/frozen models unchanged, no 16× memory or world-leading claim. [Full quality, paired samples, paced tails, refresh, residency and evidence](neural-sample-lanes.md).

## Source fields with sample lanes: steady rejection, replacement-cost opportunity (2026-10-28)

Solo Pop!_OS AMD RADV RENOIR; Wrela 0.9.0/de9b81e, Mesa 26.1.6, Chrome 154.0.8037.57. Ten 26,560-card layouts, 320×128 complete frames; 240 frames after 60 warmup. Same source, visibility and sixteen-lane scheduler. First joint assessment 15.08 minutes, decision 28.43, exactly two follow-ups.

| Method | Saturated GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Native cards | 1.80384 / 1.83880 | 10,393,292 / 10,557,132 | 26,476,544 / 10,035,200 |
| Compact exact cache | 2.37644 / 2.40916 | 3,823,724 / 3,987,564 | 18,235,392 / 10,100,736 |
| Exact source field | 3.10472 / 3.13224 | 1,698,932 / 1,862,772 | 18,239,488 / 10,035,200 |
| Cutoff-5 quadratic field | 2.98664 / 3.01860 | 1,706,316 / 1,870,156 | 17,788,928 / 10,199,040 |


All 800 captures preserve their corresponding parents, including silhouettes; all methods pass 60/60 scored rows. Same-leaf sharing and visible queues slow complete frames despite exact images. Queue adds 424,964 B. Retire both; renderer/engine/reconstruction restored byte-exact to parent.

Full response replacement plus rendering: exact field 3.11124 ms, native 3.73940 ms, compact cache 4.10316 ms. Five alternating paired comparisons:

- Exact field versus native cards: **17.020% lower complete GPU time**, geometric paired-log 95% interval **16.943–17.151%**; CPU result inconclusive.
- Exact field versus compact exact cache: **24.326% lower complete GPU time**, geometric paired-log 95% interval **24.274–24.523%**; CPU result inconclusive.

This rebuilds unchanged source and omits actual edited geometry and selective invalidation. Do not generalize to ordinary editing. Complete rendering includes placement, visibility/depth, wood, shading, queue/clear, updates, copies and composite. Requested resources counted; DRM/PSS snapshots are whole-client, not per-asset or physical peak. Do not sum UMA pools.

No 16× residency or G4 win. Change toward source-aware selective response-cache invalidation; test actual edits against equally informed controls. No training/new references; goals/frozen models unchanged. [Complete quality, tails, replacement, failures and evidence](neural-field-lanes.md).

## Actual source edits: conventional source algebra and preparation tradeoff (2026-10-28)

Solo Pop!_OS AMD RADV RENOIR; Wrela 0.9.0/de9b81e, Mesa 26.1.6, Chrome 154.0.8037.57. Ten 26,560-card layouts, 320×128 complete frames, one translated clump toggled each frame. 240 frames after 60 warmup; equal source, ownership, changed-clump identity and material information for all controls. First joint assessment 23.09 minutes; decision 62.31; two candidate follow-ups, no training.

| Method | GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Original native full update | 3.49828 / 3.53948 | 10,393,292 / 10,557,132 | 26,476,544 / 10,199,040 |
| Compiled raw native full update | 2.92860 / 2.97140 | 10,394,700 / 10,558,540 | 26,300,416 / 10,035,200 |
| Compiled raw compact full update | 3.46316 / 3.49920 | 3,823,724 / 3,987,564 | 17,793,024 / 10,133,504 |
| Direct source field | 3.10864 / 3.14004 | 1,698,932 / 1,862,772 | 18,051,072 / 10,133,504 |
| Native half invariant | 2.20608 / 2.31236 | 12,519,500 / 12,683,340 | 26,505,216 / 10,199,040 |
| Compact f32 invariant | 2.59572 / 2.63144 | 8,073,324 / 8,237,164 | 22,249,472 / 10,035,200 |
| Compact half invariant (retired) | 2.59176 / 2.62652 | 5,948,524 / 6,112,364 | 26,386,432 / 9,936,896 |


- Compiled raw native full update versus Original native full update: **16.167% lower complete GPU time**, geometric paired-log 95% interval **16.089–16.250%**; CPU result inconclusive.
- Native half invariant versus Compiled raw native full update: **25.403% lower complete GPU time**, geometric paired-log 95% interval **24.324–25.833%**; CPU result inconclusive.
- Compact f32 invariant versus Compiled raw native full update: **11.669% lower complete GPU time**, geometric paired-log 95% interval **11.611–11.739%**; CPU result inconclusive.
- Compact f32 invariant versus Compiled raw compact full update: **25.345% lower complete GPU time**, geometric paired-log 95% interval **25.220–25.404%**; CPU result inconclusive.

All valid methods pass 90/90 rendered rows; silhouette coverage is unchanged. Stale negative fails all 60 edited rows. Preparation+update+render costs 3.46548 ms native half and 3.82876 ms compact f32, making full updates better for one-off edits. Repeated-edit costs exclude this initial preparation; no automatic reuse policy or arbitrary edit-target qualification.

Whole frames include updates, placement, depth visibility, wood, shading, scratch and compositing. Requested live/peak includes all observed resources; DRM/PSS is whole-client residency, not per-asset attribution or physical peak. Do not sum UMA pools. Compact half's smaller buffer produces higher actual VRAM than f32 (fresh 26,386,432 versus 22,052,864 B); retire it.

Continue native half and compact f32 invariant caches as opt-in paths plus fair full recomputation. 960 captures, 20 necessary edited reference cases, 48 retained/drift/default checks; failed source preserved. Goals/frozen models unchanged; no G4/world-leading claim. [Joint quality, complete cost, preparation, actual memory and evidence](neural-source-edits.md).

## Changing and simultaneous source edits; batched capture workflow (2026-10-28)

Solo Pop!_OS AMD RADV RENOIR, Wrela 0.9.0/de9b81e, Mesa 26.1.6, Chrome 154.0.8037.57. Ten 26,560-card layouts, 320×128 complete frames. Five source states: base, three individual translations, all three. A 40-frame phase/view/light cycle; 240 frames after 60 warmup, one worker. Equal source/ownership/edit/visibility inputs for controls. First assessment 33.13 minutes, decision 63.31, two candidate followups, no training.

| Method | GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Native full raw update | 2.93024 / 2.98128 | 10,394,700 / 10,558,540 | 26,300,416 / 10,199,040 |
| Compact full raw update | 3.47028 / 3.50928 | 3,823,724 / 3,987,564 | 18,259,968 / 10,297,344 |
| Direct source field | 3.11240 / 3.14348 | 1,698,932 / 1,862,772 | 17,784,832 / 10,199,040 |
| Native target-specific rebuild | 3.56492 / 3.64740 | 14,644,300 / 14,808,140 | 30,756,864 / 10,133,504 |
| Compact target-specific rebuild | 3.90148 / 3.94776 | 8,073,324 / 8,237,164 | 22,253,568 / 10,297,344 |
| Stale native (invalid) | 1.86212 / 1.98672 | 10,394,700 / 10,558,540 | 26,476,544 / 10,035,200 |
| Native immutable f32 | 2.25176 / 2.30528 | 14,644,300 / 14,808,140 | 30,560,256 / 10,035,200 |
| Compact immutable f32 | 2.60140 / 2.72984 | 8,073,324 / 8,237,164 | 22,249,472 / 10,199,040 |
| Native immutable Oct12 | 2.21216 / 2.30952 | 13,581,900 / 13,745,740 | 26,763,264 / 10,035,200 |
| Compact immutable Oct12 (retired) | 2.59712 / 2.74012 | 7,010,924 / 7,174,764 | 26,189,824 / 10,199,040 |


- Native immutable Oct12 versus Native full raw update: **24.342% lower complete GPU time**, geometric paired-log 95% interval **23.766–24.576%**; CPU result inconclusive.
- Compact immutable f32 versus Native full raw update: **11.575% lower complete GPU time**, geometric paired-log 95% interval **11.337–11.672%**; CPU result inconclusive.
- Compact immutable f32 versus Compact full raw update: **24.977% lower complete GPU time**, geometric paired-log 95% interval **24.910–25.186%**; CPU result inconclusive.

Valid methods pass 90/90 shaded rows with unchanged alpha; stale negative fails 72 edited rows. All updates, visibility, depth ordering, decoding, wood, scratch and composition included. Initial preparation is separate: native Oct12 3.28980 ms, compact f32 3.65432 ms including update/full frame, so full recomputation wins one-off edits. Requested bytes include caches/fallback; physical DRM/PSS is whole-client snapshot, not per-asset or peak. Do not sum UMA pools. Compact Oct12 retired for higher actual VRAM; native Oct12/compact f32 retained opt-in.

Matched 32 captures: 30.573 s/eight legacy builds versus 3.877 s/one batch, 7.89× local workflow improvement with exact images/resources. Full 400-image batches take 6.071/6.381 s. This is not a rendering speedup. 352 retained archive comparisons, 80 history comparisons and eight default comparisons pass. Initial raw paced GPU files for compared controls were overwritten by the comparison tool; initial aggregate reports and complete pair samples remain. Use isolated baseline copies for future comparisons.

1,760 original captures, 21 necessary new edited reference cases plus nine reused; failed results preserved. Next neural block isolates encoder capacity before changing input/output representation. G1–G7/frozen models unchanged. [Joint evidence, bounds and replay](neural-changing-edits.md).

## Neural capacity and exact conventional source-history bypass (2026-10-28)

Solo Pop!_OS AMD RADV RENOIR, Ryzen 5 5500U; Wrela 0.9.0/de9b81e, Mesa 26.1.6, Chrome 154.0.8037.57. Full analytic renderer at 4× reconstruction to 1080p/4K: 120 measured frames after 60 warmup, one worker. Quality: eight whole cases excluded from fitting, 32 frames at all scales at 192×108, frozen metrics/targets. Wider model: 8,076 weights versus 1,260, with the same inputs/head and 256 training updates; CPU training took 183.47 s. First joint assessment: 26.07 min; decision: 35.75 min.

| Method / width | Complete GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
| --- | ---: | ---: | ---: | ---: |
| Wider neural / 1920 | 13.58652 / 14.14452 | 56,364,404 / 64,658,804 | 103,436,288 / 19,001,344 | 573,840,384 |
| Wider neural / 3840 | 53.86832 / 57.10364 | 224,326,004 / 257,503,604 | 318,009,344 / 43,884,544 | 704,864,256 |
| Frozen small neural / 1920 | 3.22668 / 3.41228 | 56,323,508 / 64,617,908 | 94,515,200 / 18,509,824 | 589,079,552 |
| Frozen small neural / 3840 | 12.45480 / 13.02804 | 224,285,108 / 257,462,708 | 351,571,968 / 43,393,024 | 667,756,544 |
| Original untrained source-history / 1920 | 3.15624 / 3.36560 | 56,315,920 / 64,610,320 | 94,507,008 / 18,427,904 | 543,590,400 |
| Original untrained source-history / 3840 | 12.14584 / 12.59902 | 224,277,520 / 257,455,120 | 351,563,776 / 43,311,104 | 733,302,784 |
| Fixed raw-source history / 1920 | 18.49556 / 18.81872 | 89,493,804 / 97,788,204 | 127,705,088 / 18,427,904 | 587,612,160 |
| Fixed raw-source history / 3840 | 73.69632 / 75.24704 | 356,988,204 / 390,165,804 | 450,736,128 / 43,311,104 | 682,889,216 |
| Production temporal / 1920 | 6.68184 / 6.78400 | 48,021,584 / 56,315,984 | 95,133,696 / 18,427,904 | 588,565,504 |
| Production temporal / 3840 | 26.47468 / 26.82112 | 191,099,984 / 224,277,584 | 284,823,552 / 43,311,104 | 684,032,000 |
| Direct conventional source-history / 1920 | 2.02632 / 2.27716 | 48,021,584 / 56,315,984 | 95,125,504 / 18,427,904 | 543,860,736 |
| Direct conventional source-history / 3840 | 7.87388 / 8.18004 | 191,099,984 / 224,277,584 | 284,819,456 / 43,311,104 | 688,799,744 |


Wider median 4× FLIP improves only 0.813% versus small, with two FLIP and three motion regressions. Retire this model; preserve its weights and failed shader builds. Memory includes unused fixture resources; physical DRM/PSS is a whole-client snapshot, not asset attribution or peak. Do not sum UMA pools.

The conventional source-history bypass removes the unused encoder and cells without changing any of 1,024 rendered images or their targets/masks. Requested live/peak savings: 8,294,336 B at 1080p and 33,177,536 B at 4K. Five alternating pairs:

- 1920 output width: **34.582% lower complete GPU time**, geometric paired-log 95% interval **34.075–35.014%**; CPU comparison inconclusive.
- 3840 output width: **35.266% lower complete GPU time**, geometric paired-log 95% interval **35.045–35.320%**; CPU comparison faster.

Fresh final retained builds:

| Clean retained build | GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
| --- | ---: | ---: | ---: | ---: |
| 1920 | 2.09824 / 2.34388 | 48,021,584 / 56,315,984 | 95,125,504 / 18,427,904 | 543,171,584 |
| 3840 | 7.86376 / 8.42936 | 191,099,984 / 224,277,584 | 318,382,080 / 43,311,104 | 667,675,648 |


No quality qualification follows from preserving a blurry image. G1–G7 remain unmet. One candidate followup plus a cold-input diagnosis; no further training. All 72 final cleanup/default images are exact; formatting and type checks pass. [Complete quality, resident memory, failed results and reproducibility](neural-capacity.md).

## Shared source-field grids, retired (2026-10-28)

Solo Pop!_OS AMD RADV RENOIR, Ryzen 5 5500U; Wrela 0.9.0/de9b81e, Mesa 26.1.6, Chrome 154.0.8037.57. Ten-layout AA4 320×128 mosaic, five source edit states, 240 frames after 60 warmup, one worker. Quality uses 30 completed references on three backgrounds. No training/new references; one candidate followup.

| Method | Saturated median / p95 ms | Paced p95 / p99 ms | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
| --- | ---: | ---: | ---: | ---: | ---: |
| Native immutable Oct12 | 2.24624 / 2.36788 | 3.04277 / 3.10952 | 13,581,900 / 13,745,740 | 26,501,120 / 10,199,040 | 512,014,336 |
| Compact immutable f32 | 2.59420 / 2.72716 | 3.95355 / 4.05676 | 8,073,324 / 8,237,164 | 22,052,864 / 10,035,200 | 492,166,144 |
| Direct stabilized field | 3.12112 / 3.15124 | 4.45830 / 4.57109 | 1,698,932 / 1,862,772 | 17,788,928 / 10,100,736 | 511,159,296 |
| Direct bounded-raw field | 3.05524 / 3.09360 | 4.42856 / 4.55481 | 1,698,932 / 1,862,772 | 18,046,976 / 10,035,200 | 512,728,064 |
| 4-cell-axis guarded grid | 3.06944 / 3.09704 | 3.97715 / 4.03246 | 1,810,860 / 1,974,700 | 17,797,120 / 10,100,736 | 492,165,120 |
| 8-cell-axis guarded grid | 3.07724 / 3.10504 | 4.06527 / 4.22658 | 2,393,772 / 2,574,052 | 17,793,024 / 10,035,200 | 492,418,048 |
| 8-cell-axis, guard removed | 2.53540 / 2.68880 | 3.46317 / 3.57360 | 2,393,772 / 2,574,052 | 17,793,024 / 10,199,040 | 494,153,728 |
| 4-cell-axis, preparation each frame | 3.48252 / 3.51880 | 4.14308 / 4.25920 | 1,810,860 / 1,974,700 | 18,006,016 / 10,297,344 | 495,052,800 |
| 8-cell-axis, preparation each frame | 5.13500 / 5.18372 | 5.88046 / 6.00398 | 2,393,772 / 2,574,052 | 18,010,112 / 10,231,808 | 496,363,520 |


Guarded grids pass 90/90 quality rows, but accept zero / 1.320% of all authored leaf queries. The finer grid without rejection passes only 49/90, costs 12.9% more saturated GPU time than fastest native, and has paced p95/p99 ratios 1.138/1.149. Retire this representation; no new rendering win or G4 qualification. Source guards are sampled checks, not continuum bounds.

All requested resources and actual whole-client DRM/PSS snapshots are included; initial preparation is separate above. No UMA summation, physical peak or appearance-only attribution. Paced grid tails are lower than direct-field tails in these unpaired runs; no small-win or dominance inference. Complete decision, including preparation and strengthened raw-field baseline, finishes 13.48 minutes after implementation start, with an explicitly estimated additional nine-minute inspection allowance.

3,600 images preserve alpha; 800 preparation images and 528 prior baseline images are byte-identical. Retained renderer restored exactly; failures preserved. [Full quality, runtime, residency and replay evidence](neural-base-grid.md).


## 2026-10-28: nonlinear source lighting; uniform radiance cache retired

[Complete report](neural-leaf-lighting.md). Parent421f740c; same Pop!_OS AMD5500U/
RADVRENOIR,Mesa26.1.6,Chrome154.0.8037.57,Wrela0.9.0/compilerde9b81e52.
120frames after60warmup, oneworker. All cache generation/current-light refresh,
visibility/material integration, decode and composite included. This analytic
layer fixture uses production leaf_response, but has no wood/sort/GI work and
does not measure a complete3D foliage pass.

At4×, cases74/75 saturated GPUmedian1080p/4K:
direct2.25112/8.86216 and3.29396/13.08708ms;
bilinear1.44356/5.65384 and1.95304/7.61416;
cubic2.31348/9.10400 and3.32324/13.22728.
Unpaired runs; no confidence-qualified small difference. Quality192×108,32frames,
fourcases: direct FLIP0.00196–0.00394, bilinear0.0100–0.0948,
cubic0.00595–0.0703. Full-resolution quality remains unqualified.

Requested live1080p/4K: direct25,212,056/99,861,656B;
both caches27,285,640/108,156,040B. Actual deduplicated DRMVRAM:
direct60.93–69.85/185.25–218.81MB, bilinear69.80–69.87/227.13MB,
cubic69.87/227.13MB. GTT/browserPSS, requestedpeaks and pacedGPUtails in report.
No per-asset/physicalpeak claim; do not sum UMApools.

Decision29.65minutes, after23.10-minute first assessment and one followup:
retire both uniform caches; no new joint rendering win. Retain nonlinear diagnostic
and source-direct control, preserve failures,264image/target/mask parity checks.
No training, old models and G1–G7 unchanged/unmet.


## 2026-10-29: source terrain ceilings retired after production measurement

[Full report and preserved failures](neural-probe-ceilings.md). Parent61c534f4;
solo Pop!_OS Ryzen5 5500U, AMD RADV RENOIR, Mesa26.1.6, Chrome154.0.8037.57,
Wrela0.9.0/compilerde9b81e52. One worker, 1920×1080, 60 frames after600warmup.
Fixed equal probe work and resolution; all clearing rendering, preparation,
caches, fallbacks and composition included. No training or independent transport truth.

| Method | Complete GPU median / p95 ms | Probe median / p95 ms | Paced complete p95 / p99 ms |
|---|---:|---:|---:|
| Unchanged baseline | 27.66524 / 29.55736 | 1.80284 / 3.00688 | 29.44713 / 30.42032 |
| Per-ray global ceiling | 27.97544 / 29.57108 | 1.86372 / 3.00028 | 29.52476 / 30.11182 |
| Cached global ceiling | 27.71396 / 29.42020 | 1.85272 / 3.02020 | 29.45787 / 30.24935 |
| Per-column local ceiling | 27.69688 / 29.44572 | 1.88600 / 2.93348 | 29.62742 / 30.14961 |

| Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Unchanged baseline | 405,005,112 / 582,183,284 | 551,976,960 / 20,180,992 | 808,082,432 |
| Per-ray global ceiling | 405,005,112 / 585,120,968 | 551,936,000 / 20,099,072 | 813,744,128 |
| Cached global ceiling | 405,005,116 / 585,120,972 | 551,936,000 / 20,099,072 | 853,451,776 |
| Per-column local ceiling | 405,010,232 / 582,188,404 | 552,001,536 / 19,902,464 | 803,638,272 |

All 60 measured frames update probes. Default adaptive runs had zero updates after
the initial bake and cannot support a GI speedup claim. Final local image is exact;
initial/cached each differ by one byte by one unit, also seen in independent repeat
variability. Alpha exact. Initial flat/hilly fixtures preserve 1,296 lighting words
and all RGBA bytes per variant, but nominal 2–6% probe gains do not transfer.

Requested allocations include scratch/caches/fallbacks; actual DRM/PSS snapshots
follow660pacedframes, not physical peaks/per-asset memory. Do not sum UMA pools.
Unpaired small tail/cold differences do not establish dominance. First joint
assessment23.16minutes, decision51.65minutes, exactlytwofollowups: retire all
candidates and restore runtime byte-exact to parent. G1–G7 unchanged/unmet.


## 2026-10-29: cloud far-sunlight cache retired

[Complete report](neural-cloud-cache.md). Parent91c97c8d. Solo Pop!_OS Ryzen5 5500U,
AMD RADV RENOIR, Mesa26.1.6, Chrome154.0.8037.57, Wrela0.9.0/compilerde9b81e52.
One worker, 1920×1080,60frames after600warmup. Fixed equal resolution/work,
all rendering retained. One lattice-resolution followup; no training.

| Method | Complete GPU median / p95 ms | Clouds median / p95 ms | Paced complete p95 / p99 ms |
|---|---:|---:|---:|
| Original source march | 27.71420 / 29.40692 | 6.23936 / 6.93252 | 29.62749 / 30.19913 |
| 128×64×128 far cache | 27.06012 / 28.84800 | 5.52452 / 6.30872 | 28.88872 / 29.74425 |
| 256×128×256 far cache | 27.18664 / 28.86080 | 5.67980 / 6.46024 | 28.65976 / 29.55769 |

| Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Original source march | 405,005,112 / 582,183,284 | 552,005,632 / 20,262,912 | 801,325,056 |
| 128×64×128 far cache | 413,393,720 / 590,571,892 | 560,386,048 / 20,180,992 | 809,989,120 |
| 256×128×256 far cache | 472,113,976 / 649,292,148 | 619,085,824 / 20,180,992 | 803,874,816 |

Coarse/fine keep two near sunlight samples exact but interpolate the six farther
terms from a source-coordinate cache. They fail the existing2/255 image guard on
7.02%/4.36%of pixels, maximum63/57. Contrast-band errors and repeat variability are
reported. This is sampler comparison, not an independently converged transport test.

Observed cache generation4.48328/23.19544ms, extra8/64MiB requested and physical
residency. Actual DRM/PSS are snapshots, not physical peaks; do not sum UMA pools.
Dynamic refresh frames remain unqualified. Unpaired nominal savings are not a win.
Initial joint assessment15.69minutes, retirement22.58minutes. Runtime restored to
parent; all failed sources/results preserved. G1–G7 unchanged/unmet.


## 2026-10-29: cloud weather bounds retired

[Full report](neural-cloud-bounds.md). Parent60e2fbec; solo Pop!_OS AMD5500U/
RADVRENOIR,Mesa26.1.6,Chrome154.0.8037.57,Wrela0.9.0/compilerde9b81e52.
1920×1080,oneworker,60frames after600warmup, equal fixed resolution/probe work.
All scene rendering and row-mask preparation included.

| Method | Complete GPU median / p95 ms | Cloud median / p95 ms | Paced complete p95 / p99 ms |
|---|---:|---:|---:|
| Unchanged source | 27.67216 / 29.44744 | 6.22624 / 7.07352 | 29.47345 / 30.31655 |
| Single-level intervals | 27.76088 / 29.46692 | 6.24152 / 7.03040 | 29.50551 / 30.37371 |
| Weather hierarchy | 27.92436 / 29.55908 | 6.39500 / 7.11812 | 29.71243 / 30.72155 |
| Shared row masks | 28.00516 / 29.62692 | 6.52288 / 7.25752 | 29.67519 / 30.70949 |

| Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Unchanged source | 405,005,112 / 585,120,968 | 551,976,960 / 20,180,992 | 805,894,144 |
| Single-level intervals | 405,037,880 / 582,216,052 | 551,927,808 / 20,099,072 | 805,415,936 |
| Weather hierarchy | 405,701,432 / 585,817,288 | 551,968,768 / 20,099,072 | 849,928,192 |
| Shared row masks | 405,742,392 / 582,920,564 | 551,989,248 / 20,099,072 | 834,298,880 |

Images are exact for hierarchy/rows, one channel byte differs by1 for single-level,
within observed replay variation. Cold bound generation0.49644/0.48336/0.48896ms
is measured separately; actual changing-weather refresh frames are unqualified.
Requested bytes count all scratch/caches/fallbacks; DRM/PSS are whole-scene snapshots,
not physical peaks. No UMA summation or small unpaired dominance claim.

Initial assessment11.85minutes, retire all three in35.79minutes after two followups.
GPU bound tests and negative controls preserved; runtime restored toparent.
Subsequent counter runs are not timing/quality results: they replace colour.
They identify46.413%of fine queries rejected at shape thresholding and no work-unit
advantage for simple compaction. No new rendering win; G1–G7 unchanged/unmet.


## 2026-10-29: source-support cloud acceleration retained

[Full report](neural-cloud-support.md). Parent e42f2d16; Pop!_OS AMD5500U /
RADV RENOIR, Mesa26.1.6, Chrome154.0.8037.57, Wrela0.9.0/compiler de9b81e52.
1920×1080, one worker,60frames after600warmup, equal fixed resolution/probe work.

Final corrected executable, three alternating pairs: complete GPU medians
27.74020 → 26.15880ms,
paired saving 5.70% (three-pair log-bootstrap95%
interval 5.37–5.93%);
cloud medians 6.25456 → 4.73592ms.
Earlier five-pair prototype kept separate. No population/presentation claim.

| Method | Final paced complete p50 / p95 / p99 ms | Cloud p50 / p95 / p99 ms |
|---|---:|---:|
| baseline | 27.84158 / 29.61088 / 30.41396 | 6.36486 / 7.18129 / 7.46191 |
| candidate | 26.28088 / 27.82283 / 28.59330 | 4.81578 / 5.22001 / 5.60141 |

256KiB added, all source/fallbacks retained. Explicit-update validation residency:

| Run | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| baseline-update | 405,005,112 / 582,183,284 | 551,976,960 / 20,180,992 | 804,996,096 |
| support-32-update | 405,267,256 / 582,445,428 | 551,997,440 / 20,262,912 | 852,608,000 |

DRM/PSS are whole-client snapshots, not per-cache physical peaks; no UMA summation.
Cold table generation2.58444ms; supported same-value noise recook at frame620:

| Method | Complete update frame ms | Noise generation ms | Support rebuild ms | Cloud ms |
|---|---:|---:|---:|---:|
| baseline-update | 34.08588 | 6.86492 | 0.00000 | 5.70904 |
| support-32-update | 34.96984 | 6.88524 | 1.99020 | 4.51772 |

Final image max difference1byte. Original-query audit:
25,166,123queries,11,867,986certified empty,zero false rejection. Changed-bound test:
131,072queries; thin-column failure preserved/repaired; reject-all control fails.
Sampler parity is not converged transport, semantic silhouette or broad motion
qualification. First assessment6.45minutes; two
optimizations rejected, all failures saved. G1–G7 unchanged/unmet.


## 2026-10-29: sunlight reuse of shape support retired

[Full report](neural-cloud-light-support.md). Parent2b6f47c4 already includes the
retained view-ray gain. Same Pop!_OS AMD5500U/RADVRENOIR, Mesa26.1.6,
Chrome154.0.8037.57, Wrela0.9.0/compilerde9b81e52. 1920×1080,one worker,
60frames after600warmup, equal fixed resolution and recurring probe work.

| Method | Complete GPU median / p95 ms | Cloud median / p95 ms | Paced complete p95 / p99 ms |
|---|---:|---:|---:|
| Retained view support | 26.04872 / 27.77300 | 4.68700 / 5.19472 | 27.65298 / 28.79247 |
| All eight light queries | 26.71472 / 28.89400 | 5.38516 / 6.09120 | 28.45048 / 29.23467 |
| Far six light queries | 26.53576 / 28.04832 | 5.12176 / 5.75236 | 28.38363 / 29.23905 |
| Distant four light queries | 26.37524 / 28.09772 | 4.97780 / 5.57404 | 28.03521 / 28.90495 |

| Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Retained view support | 405,267,256 / 585,383,112 | 551,907,328 / 20,099,072 | 802,246,656 |
| All eight light queries | 405,267,256 / 582,445,428 | 551,907,328 / 20,180,992 | 808,064,000 |
| Far six light queries | 405,267,256 / 582,445,428 | 551,911,424 / 20,262,912 | 798,598,144 |
| Distant four light queries | 405,267,256 / 585,383,112 | 551,907,328 / 20,099,072 | 805,586,944 |

No added cache or omitted source/fallback. DRM/PSS are whole-client snapshots,
not per-cache physical peaks; no UMA summation. Existing construction/invalidation
and prior measured update penalty remain. No new update qualification.

Actual-query audit:4,926,936 light queries,731,785 certified empty,zero false skips.
Eligible near rejection0.0109%, far24.109%; all-query rejection14.853%.
Counter-only timings/images are invalid rendering evidence. Rendered candidate
differences max2bytes, exceeding the1byte baseline replay variation; not exact
parity, converged transport or semantic silhouette/motion qualification.

Retire all8/far6/distant4 variants after two targeted followups; first joint
assessment4.73minutes, decision14.45.
Unpaired development evidence, no broad dominance claim. Runtime restored exactly
to2b6f47c4. All failed results saved; G1–G7 unchanged/unmet.


## 2026-10-29 — Dense source-coordinate radiance charts: retired

See [the source-chart report](neural-source-chart.md). Solo Pop!_OS AMD RADV
RENOIR, Wrela 0.9.0/compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23,
Chrome 154.0.8037.57, parent a829ee61 plus archived candidate. Six existing
192x108 clips, 32 frames, four scales; 1080p/4x profiles on flat and nonlinear
leaf cases, 120 frames after 60 warm-up. One quiet GPU queue; scoring afterward.

Chart: 4x mean FLIP 0.213524, temporal 0.032675, requested live 114,549,584B.
Raw-factor: 0.170361 / 0.013622 / 92,013,116B. Complete medians chart
4.97408/4.68804ms versus raw 10.79672/17.88788ms on cases1/74.
Direct source costs1.79612/2.25320ms, 25,212,072B requested live, and is more
accurate. Physical residency, tails, cold/reveal/boundary errors and source-input
differences are reported. Unpaired development points, not production/G4 gates.

First complete comparison 30.91 minutes; retire at
33.13 minutes with zero followups. No training or new
reference method. Runtime restored exactly; all failures retained in
build/neural-evidence/source-chart-v1. Four batch runs capture3,072 frames in
97.89s, excluding profiles/scoring; no unbatched speedup claim.


## 2026-10-29 — Cloud interval hierarchy: retired

See [the interval report](neural-cloud-intervals.md). Solo Pop!_OS AMD RADV
RENOIR, Wrela0.9.0/compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23,
Chrome154.0.8037.57. Parent ccd785d1 plus archived candidates. Fixed1920×1080
clearing, one worker,60frames after600warmup; quiet GPU queue, CPU assessment after.

Complete medians26.50504/26.48944/26.49236ms for16³/8³/weather-first versus
26.10764baseline; clouds5.17724/5.11784/5.10400ms versus4.71184.
All inspected image differences ≤1byte, no quality-matched speedup. Requested
live adds65,536/36,864/36,864B; actual DRM/PSS and cold table creation recorded.
Final query audit certifies22.425% of25,166,123 original samples empty with zero
positive-density false skips; this does not imply lower GPU cost.

First joint assessment 12.56min, retire at
21.99min after two followups. Unpaired development evidence;
no converged transport, changed-weather-tail, silhouette or G1–G7 qualification.
All runtime restored to parent; failures preserved in build/neural-evidence/cloud-interval-v1.


## 2026-10-29 — Temporal identity shortcuts, AMD, retired

See [the complete temporal comparison](neural-temporal-identity.md) for source,
runtime, quality, residency and limitations. Parent 388ca647, pinned Wrela 0.9.0
compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23; AMD RADV RENOIR / Ryzen 5 5500U,
Mesa 26.1.6, Chrome 154.0.8037.57. Fixed 1920×1080 clearing, one worker,
60 measured frames after 600 warmup; separate 660-frame allocation runs.

Saturated complete / temporal medians: baseline 26.14876 / 5.06712 ms;
identity 25.93984 / 4.92956; stationary-texel 25.72540 / 4.72864;
exact-texel 26.42412 / 5.34344. Unpaired development points, no small-speedup
confidence claim. Initial shortcuts change up to 98/37 channel bytes.
Exact-texel matches 768 analytic frames exactly and full clearing within one byte
but is slower. All live requested storage 405,267,256 B; whole-client DRM/PSS,
scratch/caches/fallbacks and transient requested peaks are preserved in evidence.

First joint assessment 8.64 min, retirement
25.56 min, exactly two followups. Four batched runs capture
3,072 frames in 95.27 s excluding full-render measurement/scoring;
no unbatched control or batching speedup claim. Retire all shortcuts and restore
runtime. No training, new reference algorithm or G1–G7 qualification.
