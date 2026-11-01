# Lookdev

*How a look-changing step is judged (docs/plan-world-class-2.md §0.5 G-look, §B1.8). **Proposed** until Ryan edits it.*

A step that changes the image on purpose is judged four ways, all required: the stills before and after (`wrela tool lookbook --stills`); the error against a reference render and the artifact measures (`wrela tool lookbook --quality`); and a blind judgement on this rubric. The pixel diff is a cheap screen that says where a change landed; it decides nothing.

## The instrument

`wrela tool lookbook -- <out> --quality [--controls] [--shots a,b,c] [--size WxH]` measures each named shot (all of the lookbook's by default), driven on the native host's GPU with the frame's own keys for it (`engine::frame::Frame::key`):

- **F10** freezes the frame's time: the wind, the clouds' drift, the water's drift and the particles hold, and a game that reads `Frame::held_time` and `Frame::frozen` (the Last Green, the clearing) holds its own clocks and reads no new sim snapshots. So only what the renderer does can move a pixel.
- **F11** holds the eye where the game aimed it; pressed again it dollies the held eye along its look at 0.5 m/s (`DOLLY` a frame), and a third time lets it go. The **backquote** pauses the dolly where it is, and resumes it.
- **F12** draws **the reference**: `Quality::reference()`, every knob at its most (the scene at the output's size, unpaced, the cascades at 4096² drawn whole, probes of 128 rays), temporal AA's history off and the jitter on an 8×8 grid, so the mean of 64 of its frames is a 64-sample supersample. F12 again puts play back. Either way the frame waits for its caches (an arrival), then prints `reference at frame N` or `play at frame N`.

A shot's run: chosen as a player chooses it and settled for 5 s; time frozen (at 60 s) and the eye held; 240 frames to settle the still view, then **play**'s still (the frame as a player sees it), and 64 more frames; the reference there; play again, and a 10 s dolly; then the eye back at the dolly's start, and the dolly again, paused at each quarter: the moving frame there, play settled there for 64 frames, and the reference there. From those, per shot, into `quality.json`, with the play still and the reference beside it as PNGs:

| Measure | What it is | Better |
|---|---|---|
| PSNR | play's still against the reference, on luma, dB | higher |
| SSIM | Wang et al. 2004, 8×8 windows on luma | higher |
| sharpness | the mean absolute luma step between neighbours of play's still over the reference's | nearer 1 |
| flicker | the mean over pixels of luma's variance over the 64 frames after the play still, the camera still | lower |
| pop | the largest mean absolute luma difference between two frames in a row of the dolly | lower |
| still_pop | the same over the 64 still frames: pop's comparator, in pop's units | lower |
| trail | at the dolly's four stops, the median over rows of how many pixels from the sharpest edge of play settled there the moving frame's luma differs from it without a break, by more than 2/255 and than twice the row's median difference; the most of the four | lower |
| lag PSNR | at each stop, the moving frame against play settled there (in `quality.json`, with `stops_psnr`, the moving frame against the reference there) | higher |
| clipped | the share of the play still's pixels at 1.0 in any channel | lower |

Trail and lag compare the moving frame with play settled at the same place, not with the reference: the reference's light differs from play's (its finer shadows, its probes' more rays: 1 to 1.5 dB each on `02-beech-high-forest`), by more than 2/255 nearly everywhere, which would hide any lag. The measures are read together, each beside its comparator. A change that lowers flicker while PSNR falls by more than 0.5 dB fails: smoothing isn't quality.

## What the reference bypasses, and what it can't

| System | The reference | So play against it measures |
|---|---|---|
| Temporal super-resolution (TSR) | replaced by a 64-sample supersample | the approximation |
| Shadow caching | the finest maps, drawn whole each frame | the approximation |
| Probes | the same algorithm, more rays | convergence, not bias |
| Fog, cascades | the same algorithm at higher settings | convergence, not bias |

Where the reference measures convergence, its error can't show a bias the algorithm has; the rubric and Ryan carry those.

## Negative controls

`--controls` runs the instrument's own tests on `02-beech-high-forest` (a detail-rich shot, so a blur can't hide on flat ground), into `controls.json`, and the tool fails if one does:

- **Blur:** the play still blurred 3×3 loses at least a tenth of its sharpness and some SSIM, although the blurred frames' flicker falls. Its PSNR is printed beside it, and doesn't decide: a blur takes play's jitter noise away as well as its detail, and where play is already softer than the reference and its light differs, the two cancel (on this shot at 1080p, 22.37 dB before and 22.43 after).
- **Ghost:** a run with temporal AA's blend forced to 0.02 (the game's `lookdev` export) lengthens `trail` past the plain run's and to at least 4 px, and lowers the mean lag PSNR over the dolly's stops; `pop` may stay or fall.
- **Sharp motion:** the reference's sharpest edge is within 1 px of the play still's in width: the frozen-time supersample keeps a sharp edge sharp.
- **Quarter:** the play still at a quarter of its resolution loses at least 3 dB: the instrument reacts.
- **Detail removal:** a run with half the grass's cover placed (the `lookdev` export) in play and reference both, which PSNR may reward. Its standing in `controls.json` is "degraded" whatever its PSNR says, so a step can't cite a PSNR gain from removed detail. It's the blind judge's calibration pair.

## The rubric

Five rows, each scored 0 to 3 per shot, before and after, and a preference between the two:

| Row | 3 is |
|---|---|
| light | the light's direction reads; falloff and bounce are where the eye expects them |
| colour | the palette is in harmony, saturation is held, the distance hazes toward the sky |
| value | a full range of values; the forms read by value alone (judge the still desaturated) |
| depth | overlap, scale cues and atmosphere separate near from far |
| artifacts | no aliasing, flicker, leaks, popping or tiling is seen |

A separate agent judges, in a context of its own: shown each pair in an order it isn't told, it scores both stills, writes one sentence per row and says which it prefers. With `WRELA_HERO_DIR` naming the private hero set (docs/citations.md), it views those first; that set is held to the same eligibility as the public references (docs/plan-world-class-3.md §B3.1): 23 public-domain paintings (CC0) and 52 Studio Ghibli stills under the rights holder's published invitation, which Ryan accepted on 2026-10-28. No other image in it is ineligible. It's calibrated once per step on the detail-removal pair: if it prefers the degraded still, its scores don't count for that step. The scores, the preferences and the order key go in the PR. Agents alone judge (docs/plan-world-class-3.md D38): their tally is the verdict.

## The reference review

The vale's look is judged against freely licensed photographs and footage of its real subjects (`references/vale.wrela`, docs/citations.md): G-reference (docs/plan-world-class-3.md §0.5, §B4). `studio/compare` lays out the comparisons and tallies the judgements; it doesn't judge.

```bash
wrela tool studio/compare --write ws=. --read refs=$WRELA_REFERENCE_DIR -- build/vale-b5b --a build/vale-b5b/stills --set evaluation --seed 5 --refs-dir $WRELA_REFERENCE_DIR
```

```bash
wrela tool studio/compare --write ws=. -- build/vale-b5b --tally
```

The references stay outside the repository, so the tool runs as a package with their directory granted (`refs`): a workspace's declared tool's roots stay inside the workspace.

- **The mode** is attributed: each reference's credit, licence, source and the conditions its page discloses are shown beside it, so the judge knows which image is the photograph. The order of the pairs and of each pair's sides comes from a seed. No claim of independence follows from the random order or from fresh contexts.
- **The rubric** is the five rows above, 0 to 3 for each image. The judge allows for different scenes and source conditions, and judges the relevant visual quality. "Equal" means both images are within one point on every row: a judge who says equal of images further apart is read as preferring the one scored higher, and a file that says equal of images further apart whose totals tie is refused until it says which.
- **Rights-aware access.** A reference stands in a panel only if its licence permits a local copy, an agent judge's view and a display's resizing, named exactly (`rights::PERMITTING`): public domain, CC0, or CC BY or CC BY-SA at a published version, with its creator, licence URL and source page to credit. Anything else (a non-commercial or no-derivatives licence among them) is link-only and refused, and the tool never fetches. Each copy is checked against its SHA-256 in the manifest, and a missing or changed copy is refused. The judges read `pairs.json`; `key.json` (which image is the vale's) is kept from them; the review page (`index.html`) stays under `build/`.
- **Calibration** uses two pairs made only from the vale's own stills: an original against its 3×3 blur passed over it eight times (about a 2.3 px Gaussian: one pass, as the plan named it, is lost at the size an agent is shown a 1080p image, and five of ten judges viewing honestly rated it equal), and against its quarter-resolution copy brought back to its size, every image of them under a neutral name (`controls/<16 hex>.png`: which is the original is the key's alone), each original's copy with a red bit flipped in one pixel in 97 (the same to the eye, but no file's bytes or pixels another's: a judge comparing files can't find the original by its match). A judge's answers are read by their pairs' ids (`control`), not their order; a missing, repeated or unknown id is refused. A judge that prefers a worse copy is rejected and replaced; every attempt is kept.
- **The tally** (`--tally`) needs eight valid judges, each by a name of its own (a copy of one judge's file is no second judge), who judged every pair; a missing or unviewable judgement leaves it incomplete, never shrinking the denominator. Three rules, all required:
  - overall: at least 50 % of the judgements prefer the vale or rate the pair equal;
  - per pair: no pair is lost worse than 6 to 2;
  - majority: at least 6 of 12 pairs are won or equal by the judges' median (4 of 8 on the development panel).
  It writes `tally.json` and `tally.md` (each pair's preferences and rubric means, the rules, the verdict, the rubric's rows ranked by how much the vale lost them) for docs/bench.md's "Public visual references". Changing a reference (any field of its record: what a judge is shown, its rights, its copy) or a shot (any field: its framing too) changes the manifest's or the shots' hash, and the tool refuses a panel whose hashes aren't the frozen ones: a changed source or shot makes a new baseline.
- **The judges** are eight agents in fresh contexts. Each is given the rubric, the pairs' images (its own access to the files), their credits and conditions, and nothing of what the implementer expects, and is told to judge by viewing alone (no checksums, bytes or pixel arithmetic). Each writes `judges/<name>.json`.

## The motion review

The vale's walking creature against freely licensed footage of a real deer walking (G-motion,
docs/plan-world-class-3.md §F9): shot 10's three seconds (`wrela tool studio/motion --write ws=.
-- <out> --package examples/vale --frames-only trail`) and the reference's (`references::vale`'s
`vale-motion-walk`, the seconds its record names), through the reference review's rules: its
attributed mode, rights-aware access, calibration and complete tally. Five rows, each 0 to 3 for
each clip, a sentence each, and a preference:

| Row | 3 is |
|---|---|
| weight | the animal's mass reads in its motion: its body rides its legs and settles onto each footfall |
| timing | the gait's rhythm reads as an animal's: its feet's order, its pace, how it changes |
| contact | its feet plant and leave without sliding, skating or sinking |
| secondary motion | its head, ears and tail follow the body's motion and settle |
| readability | the motion reads at a glance: what the animal is doing and where it's going |

- **The clips** are strips of frames sampled at 10 a second over three seconds of walking (no
  frame is interpolated), each shown whole and cropped round the animal; the footage's frames are
  extracted and cropped, which its credit notes. The judge records what differs between them
  (the animal, its path, the camera, the frame rate, the animation method) and judges the motion,
  not the scene.
- **Calibration**: the vale's strip against itself held on every third frame (a stutter: 3.3
  frames a second) and against itself blurred (the stills' eight passes): a judge that prefers
  either degraded strip is replaced; every attempt is kept.
- **The gate**: eight valid judges, each judging the pair; they prefer the vale's motion or rate
  it equal in at least 40 % of their judgements. "Equal" means within one point on every row, as
  the stills' review reads it. The agents' tally is the verdict (D38).

## Judging a style

A look other than the physical one (docs/plan-world-class-3.md Part D, `content/styles`) has no
"before" to be preferred to. Its G-look is (1) its twelve stills; (2) and (3) the instrument's
measures against the look's own reference (F12 at the look's own settings), which say
convergence and coherence, not taste; (4) the blind judge on the rubric without a pair, and,
where the look was fitted to a set of paintings (`wrela tool fit --style`), the pair (still,
painting) on one added row, *style* (3 is "the same hand").

**Coherence** (§D7): a look's marks are anchored in 3D and sized in 2D (`engine::marks`), and the
instrument measures what that leaves on the screen. `wrela tool lookbook -- <out> --package
examples/vale --fast --shots dev --look <type> --bars <physical-run>` builds the vale in the
look, measures the eight development shots' strips (`--fast`: below), and holds each to the physical look's bars
(**proposed**): flicker and pop at most 1.5 times the physical look's on the same shot, trail at
most its plus a pixel. The physical baseline is in docs/bench.md ("D7a, the marks").

**The same in every run** (2026-10-30): F10 and F12 freeze the frame's time at 60 s
(`engine::frame`'s `FROZEN_AT`), and a still view settles 240 frames before it's measured
(`quality::SETTLE`). Frozen at the frame's own time, and settled 64 frames, a shot's measures
varied from run to run by up to a fifth, and some fell into one of two states (dawn in the
hollow's flicker 29 or 48 ×10⁻⁶): the wind and water were frozen wherever that run's load had got
to, and loading work finished during the measured frames on some runs and not others. Now four
runs of that shot in two sessions read the same, to the digit, and two runs of all twenty
shots in two sessions read the same pop on every shot and the same flicker on thirteen; the
flicker of seven still differs from run to run (06, 07, 09, 11, 14, 18 and 19: another old oak's
4.7 or 41.4 ×10⁻⁶). So a look is held to the bars on those seven by each look's median of three
runs (`--runs 3 --shots ...`). Every number before is the earlier instrument's.

**The fast run** (2026-10-30, `--fast`): coherence alone, a shot in about 10 s and the twenty in
about 3.5 minutes (the full run took about 3 minutes a shot). 960 × 540; the shot looked for
every 30 frames until it's shown, then time frozen and the eye held and 240 frames settled; the
still's flicker over 32 frames; the dolly's pop over its first 150; and the trail where the dolly
ends, paused and settled 60 frames. No reference, so no PSNR, unless `--psnr` takes one at the
still (and settles again before the dolly). A frame costs about 12 ms at that size, and the GPU
is the limit (two runs at once took longer than one after the other), so frames are what a run
saves. Its numbers are its own: a look is held to the physical look's fast run. While the
instrument holds the frame's time (F10, F12) the cook planner plans from the code's record of
each kind's cost, not the device's timings (`engine::cook::Planner::steady`): planned from the
host's timings, a cost near a rung of its ladder took one rung on one run and the next on
another, and a shot's held frames did different work. With it, a run of the development shots
repeats within 3 % (twelve of sixteen exact), and the twenty within 2 % but for a few shots up
to 13 % (the fall, the meadow at evening) and a trail's pixel; so a record is each look's median
of three (`--runs 3`), and hill-climbing takes single runs. The settle can't be shorter: at 120
frames, the shore at evening's flicker read 31.6 or 37.0 ×10⁻⁶.

**A record measures only what can decide it** (2026-10-30). With `--bars`, `--near 0.2` takes a
shot's other runs only where its first leaves it undecided: a measure more than a fifth over its
bar is a miss, every one more than a fifth under is a hold (a trail by a fifth or a pixel), and a
run's spread (2 %, 13 % at most) can't change either. On the records, one run decided 13 to 20 of
each look's twenty shots, each as its median of three did. And a fast run keeps each trail's two frames (`-moving.png`,
`-settled.png`, about 1 MB each), so a change to the trail's measure is `--retrail` on the run's
directory, every shot's trail measured again from them (the median of its runs'), nothing drawn.
A change to an engine or a look still needs its looks drawn again, but only those it can move.

**A trail is behind an edge** (2026-10-30): a pixel is off the settled frame by more than 2/255
*and* by more than twice its row's median difference. At the fall's last stop the whole sky is
lighter moving than settled, 2.5% in the physical look and 3% in the cel (the settled view's
history is the mean of its frames, the moving one's a clipped running blend, and the sky's samples
are noisy), and a row whose sharpest edge was the horizon ran on across it to the row's end: the
physical look's median 6 px held because just under half its rows were such rows, and the cel's
flat ground made it just over half, 527 px. The sky's lag is the engine's, both looks', and the
lag PSNR still shows it; it is no trail. The physical bars were measured again with this.

**A still waits for its shot** (2026-10-30). The wood under cloud is at (−250, 250), far from
where the vale starts, and its content streams in: at B5 it had arrived by the still's frame 301;
on 2026-10-30 it was black there in every look (it showed 90 to 330 frames later, by the clock's
pace), and two agent judges were handed a black still before it was caught. `--stills` now looks
again every 30 frames until the shot shows and settles `SETTLE_FRAMES` more (printing how late);
a shot shown by frame 301 is the frame it always was (the old oak's still byte-identical). A shot
whose measures fail (`quality::measure`) is loaded once more on a fresh page.

**A trail can rest on one row.** Only rows with a step of 0.1 in luma count, and a soft frame has
few: on another old oak's fast run the dolly ends on bark, and one row qualifies (row 533, at a
small dark thing in the corner), so the shot's trail is that row's run, 27 to 29 px in the
physical look from one run to the next. A look's trail there moves with whatever shades that
corner (Painterly's 30: docs/bench.md, "D4–D7b"). The bar isn't widened after the fact; a later
instrument should count its rows and say when too few qualify.
