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
