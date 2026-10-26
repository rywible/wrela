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
