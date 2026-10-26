# Lookdev

*How a look-changing step is judged (docs/plan-world-class-2.md §0.5 G-look, §B1.8). **Proposed** until Ryan edits it.*

A step that changes the image on purpose is judged four ways, all required: the stills before and after (`wrela tool lookbook --stills`); the error against a reference render and the artifact measures (`wrela tool lookbook --quality`); and a blind judgement on this rubric. The pixel diff is a cheap screen that says where a change landed; it decides nothing.

## The instrument

`wrela tool lookbook -- <out> --quality [--controls] [--shots a,b,c] [--size WxH]` measures each named shot (all of the lookbook's by default), driven on the native host's GPU with the frame's own keys for it (`engine::frame::Frame::key`):

- **F10** freezes the frame's time: the wind, the clouds' drift, the water's drift and the particles hold, and a game that reads `Frame::held_time` and `Frame::frozen` (the Last Green, the clearing) holds its own clocks and reads no new sim snapshots. So only what the renderer does can move a pixel.
- **F11** holds the eye where the game aimed it; pressed again it dollies the held eye along its look at 0.5 m/s (`DOLLY` a frame), and a third time lets it go. The **backquote** pauses the dolly where it is, and resumes it.
- **F12** draws **the reference**: `Quality::reference()`, every knob at its most (the scene at the output's size, unpaced, the cascades at 4096² drawn whole, probes of 128 rays), temporal AA's history off and the jitter on an 8×8 grid, so the mean of 64 of its frames is a 64-sample supersample. F12 again puts play back. Either way the frame waits for its caches (an arrival), then prints `reference at frame N` or `play at frame N`.

A shot's run: chosen as a player chooses it and settled for 5 s; time frozen and the eye held; 64 frames to settle the still view, then **play**'s still (the frame as a player sees it), and 64 more frames; the reference there; play again, and a 10 s dolly; then the eye back at the dolly's start, and the dolly again, paused at each quarter: the moving frame there, play settled there for 64 frames, and the reference there. From those, per shot, into `quality.json`, with the play still and the reference beside it as PNGs:

| Measure | What it is | Better |
|---|---|---|
| PSNR | play's still against the reference, on luma, dB | higher |
| SSIM | Wang et al. 2004, 8×8 windows on luma | higher |
| sharpness | the mean absolute luma step between neighbours of play's still over the reference's | nearer 1 |
| flicker | the mean over pixels of luma's variance over the 64 frames after the play still, the camera still | lower |
| pop | the largest mean absolute luma difference between two frames in a row of the dolly | lower |
| still_pop | the same over the 64 still frames: pop's comparator, in pop's units | lower |
| trail | at the dolly's four stops, the median over rows of how many pixels from the sharpest edge of play settled there the moving frame's luma differs from it by more than 2/255 without a break; the most of the four | lower |
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

A separate agent judges, in a context of its own: shown each pair in an order it isn't told, it scores both stills, writes one sentence per row and says which it prefers. With `WRELA_HERO_DIR` naming Ryan's private hero set (docs/citations.md), it views those first. It's calibrated once per step on the detail-removal pair: if it prefers the degraded still, its scores don't count for that step. The scores, the preferences and the order key go in the PR; Ryan's verdict is final.
