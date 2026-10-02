# Spikes

*Throwaway code that answers one question each with measurements. None of it is the start of the engine or compiler. Results feed `docs/design/decisions.md`.*

| Spike | Question | Status |
|---|---|---|
| [01-grazer](01-grazer/) | Can hand-written field code draw a herd of creatures in budget? (triangles: extraction, skinning, per-pixel shading) | Done: passes (D-089, D-096) |
| [02-raymarch](02-raymarch/) | Can the same herd be drawn with no triangles at all? | Done: ties at herd distance, 2.2× worse in close-ups |
| 03-forest | Can a ray-marched forest look beautiful in budget? | In progress |
| 04-vista | Can a world be cooked on device and ray-marched to the horizon while the player moves? | In progress |
| 05-light | Can fields give soft shadows, sky occlusion and bounce light in budget, including indoors? | In progress |
| 06-crowds | What do hundreds of moving things, and live world edits, cost in a ray-marched world? | In progress |
| 07-hero | Can a furred hero creature fill the screen with believable joints? (ray marching's worst case from 02) | In progress |
| 08-water | Can lakes and rivers with true reflections and refraction be ray-marched in budget? | In progress |

Spikes 03–08 test the hardest cases for a pure ray-marched renderer in the flagship game: a beautiful open world, forests first, then landscapes, towers and creatures. Volumetric clouds and atmosphere are left out on purpose: they're ray-marched in shipped games already (Horizon's Nubis, for example).

## The frame budget (a hypothesis to test, not a decision)

The reference device is the MacBook Air M4 (D-096). One frame at 1080p is 16.7 ms. A first split for the flagship's forest scene:

| System | Slice (ms) | Spike |
|---|---|---|
| World geometry: terrain, rock and architecture, to the horizon | 3.0 | 04 |
| Vegetation: grass, trees, foliage | 4.0 | 03 |
| Lighting: sun shadows, sky occlusion, bounce light | 3.5 | 05 |
| Creatures and other moving things | 2.5 | 06, 07 |
| Water | 1.0 | 08 |
| Sky, atmosphere, clouds, fog | 1.0 | (known) |
| Temporal accumulation, upscaling, tone mapping, UI | 1.0 | — |
| Slack | 0.7 | |

**Verdict rule for spikes 03–08,** at native 1080p on the M4, from GPU timestamps under sustained load:

| Result | Verdict |
|---|---|
| Within the slice | **Pass** |
| Up to 2× the slice | **Inconclusive:** optimization or reallocating budget might save it |
| Over 2× the slice | **Fail:** use the spike's named fallback |

Every spike also reports its cost at half resolution per axis (960×540), the usual lever for a temporal upscaler. A spike that doesn't implement upscaling doesn't claim upscaled quality.

## Protocol

1. **Write the README first:** the question, the kill criteria (with this budget), the fallback if it fails, the method, and the key parameter to sweep. Criteria don't change after measuring. If they must, say so and why.
2. **Measure the limit, not just a point.** Sweep the parameter that matters (tree count, coverage, view distance, instance count, …) and report the curve, so we learn where it breaks.
3. **Check correctness against a brute-force reference** of the same content: smaller steps, tighter tolerances, more samples. Report the mean difference and the share of pixels off by more than 8/255, as spike 02 does. A fast number with a wrong image doesn't count.
4. **Look at it.** Save screenshots of every scene. Beauty is part of the question.
5. **Timing:** GPU timestamp queries per pass, 30 warm-up frames, then 90 back-to-back frames, medians (see `01-grazer/main.js`). Also run 60 Hz pacing with a busy-wait loop. Timestamps are quantized to ~65.5 µs.
6. **Self-contained:** each spike lives in its own directory. Copying code from other spikes is fine; changing them isn't.

## Running a spike

```bash
python3 spikes/serve.py 8417
```

```bash
spikes/headless.sh 03-forest '#run' 600
```

- `headless.sh` runs the page in headless Chrome stable on the real GPU, with a throwaway profile.
- The page must PUT `results/DONE` when it finishes.
- The page's console output lands in `results/console.log`, so WGSL errors are visible.
- Pages save results by PUT into their own `results/` directory (`serve.py` allows that).
- `#run` measures everything and saves `results/run-<timestamp>.json` plus screenshots. Keep a full run under ~3 minutes on a quiet GPU.
- `#quick` renders each scene once and saves screenshots, for development.

**Timings taken while other GPU work runs are indicative only.** Spikes 03–08 are built in parallel, so their final measurements are run one at a time on a quiet machine.
