# Actual0237 coverage calibration decision

All23 actual1920×1080 PNGs were inspected. Receipt and per-image/metadata hashes:
`.build/sanctuary-native-20260912/coverage-0237-review/review.json`.
Reviewed integer ROIs, transfer qualification and measurements are adjacent in
`rois.json`, `measurement.json`, and `material-comparison.json`.

| Authored duty | Native flat mean | Native lifted mean |
|---|---:|---:|
| .25 | .256 | .265 |
| .50 | .505 | .500 |
| .75 | .744 | .738 |
| .94 | .937 | .937 |

These are channel-averaged **calibrated display inversion estimates**, not HDR
readback. All five near/native pairs passed the predeclared black/gray/white
checks: black0, unclippedwhite, independent gray mismatch1.03 encoded levels
against a3-level limit. Native sample ROIs have112 interior pixels per patch;
near have7200. Near .94 estimates are .932 flat/.933 lifted. Native tip means
fall to .332/.338 at .94, and close views visibly resolve many strand openings.
Source density ordering survives the unresolved footprint (~3.052 periods/pixel).
Missing A2C and broken unresolved mean filtering are not supported explanations
for the0212 native tile appearance at interaction distance.

The softbox emitter is visible above-left in native/far captures despite rigView
false. At native distance it lies y400..409, while measured ROIs start y527.
The actual single quarter-resolution horizontal/vertical bloom pass has maximum
11.218-texel offset plus bilinear/extract support, conservatively <56 full pixels
per axis. Thus that emitter cannot contribute to these ROIs; diagnostic values
<=1 do not enter bloom extraction. Near front has no visible HDR emitter. This
local argument is retained with the ROI config. It is not a claim that all
captures or all scene pixels have zero bloom. PNG alpha is forced255; it does
not report the sample mask. Far/grazing chips lack robust interior area and are
not used for linear estimates.

The native analytic dry/wet PNGs are byte-value identical (maximum pixel
RGB difference0). Ordinary kind13 changes with wetness (board display mean
absolute difference .04593). Near ordinary flat/lifted interior display means
are .8110/.7567, versus analytic .89508/.89515. Those ordinary values are not
coverage; lifted source normals and lighting introduce additional contrast.
No missing normal is inferred. Hard polygon side boundaries remain visible in
analytic views before this extra shading response.

A separate unresolved draw defect blocks the4m front case: the left planar
patch disappears in **all three modes, including opaque reference**, while the
lifted patch remains. Both reappear at70° orbit; near/native front show both.
No coverage conclusion is drawn from the missing patch. Root assigned its
read-only rendering diagnosis to world_content; this author changes no culling,
depth or common renderer behavior.

## One evidence-backed correction

Keep the exact0212 positions, indices, normals, phases, period, site selection,
352patches, parents, material and defaultoff. Change only per-vertex duty across
source width: center .94, linearly falling toward both sides. Existing retained
center columns make that envelope piecewise linear without new vertices. Edge
value .0001 preserves the groom sentinel (zero means ordinary opaque surface),
and is negligible at the measured unresolved footprint. The tip mask remains
the same. No increase in patch count, density, geometry lift or sheen is made.

This isolates the demonstrated lack of side coverage attenuation. It is not a
fur identity claim: smooth exposed substrate, overlapping correlated masks,
raised interior shading and wet plastic response may remain. If the same30
actual Sunhare images still show tiles or smooth clay, reject again; do not
promote the default or hide it through an exposure change.

The existing four real compiler tests now also sample every retained left/right
edge through production GroomCoverage at .05,1,3.052,8-period footprints, keeping
positive sentinel and <.003coverage; center coverage remains >.7 and tips<.01.
Original root/exclusion/parent/default compatibility and1mm/budget guards remain.
No Swift build, test execution or GPU work was run by this author. Root must
compile the correction and repeat `groom-acceptance.json` (30captures), including
front/quarter/left dry/wet, sidehop, blink and exact reload. Cost profiling follows
only a visible improvement. Existing populated eye-level p9532.937ms is a failing
60Hz baseline; a provisional +.20ms incremental allowance is not headroom or
whole-game certification.

Replay the pixel measurement using bundled Python:

```sh
/Users/ryanwible/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3 Tools/Experiments/SanctuaryFur/measure_coverage.py --config .build/sanctuary-native-20260912/coverage-0237-review/rois.json --out .build/sanctuary-native-20260912/coverage-0237-review/measurement.json
```
