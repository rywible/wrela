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

## 2026-10-25: B13 (`b137893c`), toolchain 0.6.2

The same machine, the same hour, against the baseline above (`--compare`).

| Scene | GPU span p50 / p95 / p99 ms | Interval p50 / p95 / p99 ms | Over 16.7 / 33 ms | CPU p50 / p95 ms | Native p50 / p95 ms | First frame ms | Playable ms | Pipelines ms | Loaded MiB | GPU peak MiB |
|---|---|---|---|---|---|---|---|---|---|---|
| clearing-path | 10.06 / 10.78 / 11.07 | 15.04 / 18.44 / 19.61 | 158 / 1 | 0.41 / 1.32 | 9.48 / 10.46 | 381.73 | 3475.74 | 2692.18 | 4.9 | 513.4 |
| last-green-shot | 18.70 / 19.59 / 26.89 | 18.51 / 22.91 / 36.03 | 312 / 7 | 4.63 / 5.38 | 9.09 / 14.20 | 585.20 | 11639.78 | 5326.91 | 14.6 | 1227.7 |
| last-green-walk | 12.41 / 14.66 / 17.98 | 12.91 / 23.63 / 29.46 | 111 / 7 | 3.98 / 5.07 | 10.51 / 13.83 | 592.28 | 11886.19 | 5401.41 | 14.6 | 1228.5 |
| capybara-run | 1.12 / 1.74 / 2.10 | 16.56 / 19.89 / 21.17 | 277 / 0 | 0.22 / 0.82 | 0.40 / 0.53 | 157.94 | 0.00 | 613.52 | 3.6 | 81.9 |
| capybara-run-large | 2.01 / 3.02 / 3.55 | 16.67 / 17.81 / 18.14 | 282 / 0 | 0.15 / 0.69 | 0.49 / 2.71 | 192.36 | 0.00 | 616.32 | 3.6 | 183.0 |
| herd | 4.65 / 8.19 / 12.22 | 16.67 / 18.12 / 18.71 | 290 / 0 | 0.33 / 0.94 | 1.10 / 1.30 | 157.54 | 0.00 | 329.39 | 1.6 | 112.4 |
| leapling | 2.15 / 2.38 / 2.58 | 16.67 / 17.79 / 18.15 | 226 / 0 | 0.09 / 0.45 | 0.24 / 0.27 | 106.62 | 0.00 | 110.13 | 0.4 | 0.0 |

The change from B1, as `--compare` printed it:

| Scene | GPU span p50 | Interval p95 | CPU p50 | First frame | GPU peak |
|---|---|---|---|---|---|
| clearing-path | 10.53 → 10.06 ms (−4.4%) | 17.82 → 18.44 ms | 0.78 → 0.41 ms (−47%) | 329 → 382 ms (+16%) | 476 → 513 MiB (+8%) |
| last-green-shot | 21.82 → 18.70 ms (−14.3%) | 28.65 → 22.91 ms | 5.18 → 4.63 ms (−10.5%) | 495 → 585 ms (+18%) | 1,468 → 1,228 MiB (−16.4%) |
| last-green-walk | 15.26 → 12.41 ms (−18.7%) | 19.50 → 23.63 ms | 4.66 → 3.98 ms (−14.5%) | 544 → 592 ms (+9%) | 1,469 → 1,228 MiB (−16.4%) |
| capybara-run | 1.69 → 1.12 ms (−33.8%) | 18.79 → 19.89 ms | 0.21 → 0.22 ms | 167 → 158 ms | 44 → 82 MiB (+85%) |
| capybara-run-large | 4.52 → 2.01 ms (−55.6%) | 18.43 → 17.81 ms | 0.19 → 0.15 ms | 196 → 192 ms | 98 → 183 MiB (+86%) |
| herd | 4.50 → 4.65 ms (+3.2%) | 18.52 → 18.12 ms | 0.32 → 0.33 ms | 162 → 158 ms | 112 → 112 MiB |
| leapling | 2.14 → 2.15 ms | 17.47 → 17.79 ms | 0.14 → 0.09 ms | 109 → 107 ms | 0.03 → 0.03 MiB |

The capybara's GPU memory is its parallax strips (B10.3: four layers, 1.5 by 1.25 screens each).
The Last Green's is the compressed impostors (B6).

**Repeats** (Chrome only, `--no-native`, two more runs a side, alternating B1 and B13): the
paced intervals, p95 / p99, over 33 ms.

| Scene | B1 | B13 |
|---|---|---|
| clearing-path | 18.02 / 18.85, 0; 18.51 / 19.44, 0 | 17.85 / 19.06, 1; 18.35 / 19.34, 1 |
| last-green-shot | 23.11 / 32.08, 4; 23.77 / 40.31, 7 | 23.56 / 43.54, 7; 23.32 / 37.44, 7 |
| last-green-walk | 28.12 / 32.96, 11; 28.05 / 32.97, 8 | 16.54 / 28.66, 9; 23.03 / 28.81, 9 |
| capybara-run | 18.88 / 20.14, 0; 18.02 / 18.40, 0 | 18.25 / 18.67, 0; 18.93 / 19.72, 0 |

Over the three runs a side, the clearing's and the capybara's intervals didn't change; the Last
Green's walk has 57 to 111 of its 1,200 intervals over 16.7 ms where B1 had 382 to 397, and a
p99 of 29 ms where B1's was 33.

## Whether §0.1's claim holds on the M4 Air

The claim (docs/plan-world-class.md §0.1, **proposed**): at 1080p in Chrome, each flagship scene
holds 60 fps with a 99th-percentile frame interval under 16.7 ms, no shader or streaming hitch,
playable within 5 s of a cold page open, at a visual tier that native engines run under 30 fps
on the same machine. As measured, it doesn't hold as written for any scene: no program can meet
a 99th percentile under 16.7 ms in headless Chrome (the leapling's is 18.2 ms), so the scenes
below are judged by their GPU frame against 16.7 ms and their frames over 33 ms.

- **The clearing's path:** holds 60 fps. Its GPU frame is 10.1 ms, its intervals are the
  leapling's within a ms (p99 19.1 to 19.6 ms), 0 or 1 frame of 600 over 33 ms, and it's playable
  in 3.5 s. The next lever is that one hitch.
- **The Last Green's opening (the shot):** misses. Its GPU frame is 18.7 ms, 2.0 ms over a
  refresh, so it runs at about 53 fps (interval p50 18.5 ms), with 7 of 480 frames over 33 ms; and
  it's playable in 11.6 s, 6.6 s over. The levers, by its labels' GPU times in Chrome (`wrela
  profile games/last-green --chrome --input build/gates/LG_SHOT.json --compare <B1's build>
  --pairs 3`: span 21.76 → 18.59 ms, −14.2%): the terrain's cook (4.65 ms), the vegetation
  (3.93), the probes' bake (2.69), the near cascade's strips (2.45) and the clouds (2.35); for
  the start, the pipelines' build (5.3 s of it) and the streaming behind it.
- **The Last Green's walk:** misses, by its hitches. Its GPU frame is 12.4 ms, but 7 to 9 of 1,200
  frames are over 33 ms and its p99 is 29 ms. The next lever is what those frames cook
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
