# Source primitive visibility controls

The source geometry provides a much stronger visibility control than the
current learned volume on this public material diagnostic. At full card
count, unfitted ellipses reduce mean coverage error by 61.05%
against the fixed BC control on four stored directions and 95.16%
on four off-grid directions. Both full-count kernels pass all five fixed
material screens in both groups. **This is an uncompressed conventional
control, not a learned advantage or a G1–G7 result.**

All captures, calculations and evaluation run on Pop!_OS, AMD Ryzen 5 5500U /
Radeon Graphics, RADV RENOIR. The five changed Wrela files add diagnostic
exports and component captures. Production rendering and shipped weights
do not change.

## Source and independent controls

The new commands are:

```bash
wrela tool appearance --timeout 900 --blob-budget 3221225472 -- primitives 7
wrela tool appearance --timeout 900 --blob-budget 3221225472 -- source 7 0.375 0.1875 64 0 64 --part wood
wrela tool appearance --timeout 900 --blob-budget 3221225472 -- source 7 0.375 0.1875 64 0 64 --part leaves
```

`primitives SEED` exports the existing canonical description, 26,560 card
records and independent CPU centres, then card rotations/pattern indices,
actual GPU and CPU wood triangle vertices, the actual RGBA8 leaf atlas,
and each pattern's filtered values, leaf indices and hard cut on
128², 256² and 512² midpoint grids. It omits the volume exports.
`source --part all|leaves|wood` selects a component diagnostic;
`all` is the existing full source. It cannot be combined with `--pixel`.
Only the full tree remains the quality target.

Seed 7 repeats all twelve export files exactly; seed 11 preserves the
wood/pattern data while changing card placement. A final capture after all
Wrela edits repeats all twelve files exactly. The old `geometry 7` command
repeats all six earlier files exactly. The first complete-tree component
capture matches the prior reference byte for byte.

Independent calculations reconstruct the pinned hash, rotations, filtered
RGBA8 pattern sampling and authored cubic wood geometry. GPU/CPU wood
vertices differ by at most 1.908e-6; independent float64 reconstruction
differs by at most 1.644e-6, below the fixed 1e-4 absolute + 1e-5 relative
limit. Card rotations differ by at most 4.450e-7. Filtered pattern red
differs by at most 5.402e-8; decoded leaf indices and hard cuts agree
exactly, including every sampled threshold-band value.

All sixteen patterns pass the 256²→512² checks: area fraction changes
≤0.001; centroid and raw second-moment components change ≤0.002.
The existing 8×8 pattern-area estimator is reproduced exactly; its
difference from the 512² estimate ranges from −0.02699 to +0.05563
absolute area fraction. That source-integration error remains visible;
the production estimator has not been silently replaced.

An independent barycentric CPU rasterizer reproduces actual source wood
in all four public off-grid directions. At 32×32 samples per pixel,
coverage MAE is at most 1.669e-6, normal-component MAE at most 1.833e-6,
and there are zero 0.5-coverage threshold disagreements. All eight
leaf-only/wood-only 32→64 sampling checks pass, and the repeated wood
capture is byte exact. Wrong axis, pattern, rotation, view, image flip
and visibility-order controls produce detectable changes.

The first CPU wood replay failed because empty-background normals divided
by zero. Its source, output and failure log are retained in
`failed-empty-normal/`. Guarding the zero denominator fixes that defect
without changing any replay or quality limit. Initial Wrela type errors
and their sources are also retained.

## Kernels and visibility

Both untrained models keep every source card and all 7,392 actual wood
triangles. Each card's local mean and covariance come from its actual
512² hard-cut pattern, transformed by its rotation and width.

The uniform ellipse has Mahalanobis radius squared 4 and constant opacity
`area / (4π sqrt(det(covariance)))`. It exactly preserves planar area,
centroid and covariance. Source-derived opacity is between 0.707 and 0.841.

A Gaussian using the original covariance would require peak opacity
between 1.414 and 1.681. The Gaussian control therefore widens covariance
until peak opacity is at most 0.99, retaining area and centroid but
deliberately changing covariance. Its support ends at Mahalanobis radius
squared 18, with the finite-support mass correction included. Independent
512² numeric integrals check both kernels' area and second moments.

Cards are sorted front to back. A leaf contribution behind the nearest
wood triangle is rejected, and surviving leaf contributions are alpha
composited before the wood contribution. Source colour, crown normal,
toward and crown depth use the measured material encoding. This is a
view-facing two-dimensional kernel on a three-dimensional source centre,
not a reproduction of [Kerbl et al.'s 3D Gaussian Splatting](https://repo-sam.inria.fr/fungraph/3d-gaussian-splatting/).
No paper code, assets, training data or weights are imported.

The complete-tree primitive integration passes 6/16 checks at 8→16,
12/16 at 16→32 and 16/16 at 32→64 spatial samples per pixel.
The limits remain coverage MAE ≤0.001, coverage-error p99 ≤0.01,
linear RGB MAE ≤0.0005 and normal-component MAE ≤0.002.
Worst final values are 0.0003722, 0.004043, 0.00003745 and 0.0003806.
Earlier failed settings remain in the record.

## Full-count quality

The reference is the existing converged full source at 64×64 output
pixels, with hard leaf cuts, crown normals, flat bark and quantized
material targets. It is not production lighting or continuous leaf truth.
The fixed BC control uses two BC7 atlases plus independent BC5 toward/depth;
off-grid queries use the previously frozen reprojection. The volume plus
scalar model uses its nominal 32-ray/2×2 setting. The primitive controls
use 64×64 spatial samples. These are quality comparisons at unequal cost,
not speed comparisons.

The four stored directions are 37, 103, 185 and 242; the four off-grid
directions are the existing public cases. All sixteen seed/view scores
are retained. Seed 11 is compared with unchanged seed-7 representations,
so it is a changed-instance stress test, not asset-conditioned generation.
No claim covers all 256 stored directions.

Four stored directions, mean absolute errors of coverage-weighted moments:

| Control | Coverage | Linear RGB moment | Normal moment | Toward moment | Crown-depth moment |
|---|---:|---:|---:|---:|---:|
| Fixed BC | 0.003223348 | 0.001155048 | 0.029452563 | 0.002844412 | 0.000151159 |
| Volume + scalar | 0.015759998 | 0.001934764 | 0.023326990 | 0.009819634 | 0.000398774 |
| 26,560 ellipses | 0.001255631 | 0.000116909 | 0.001287552 | 0.000746455 | 0.000037115 |
| 26,560 Gaussians | 0.002320183 | 0.000218657 | 0.002352630 | 0.001411993 | 0.000068204 |

Four public off-grid directions, the same quantities:

| Control | Coverage | Linear RGB moment | Normal moment | Toward moment | Crown-depth moment |
|---|---:|---:|---:|---:|---:|
| Fixed BC | 0.025882901 | 0.002766674 | 0.034725454 | 0.015425762 | 0.000571803 |
| Volume + scalar | 0.016379418 | 0.002008220 | 0.023660087 | 0.010108929 | 0.000418904 |
| 26,560 ellipses | 0.001252439 | 0.000116947 | 0.001275214 | 0.000757939 | 0.000037503 |
| 26,560 Gaussians | 0.002436846 | 0.000227491 | 0.002397372 | 0.001493961 | 0.000072445 |

Each material screen requires mean error ≤1.05× BC and the worst view
≤1.10× BC. All five screens pass for each full-count kernel in each group.
These are development screens, not the G4 perceptual, temporal or resident
memory gates.

![Full source primitive controls at stored direction 103](figures/neural-appearance-primitives.png)

## Count ladder and native LOD controls

Half and quarter counts use the native source prefixes, at 13,280 and
6,640 cards. The 3,320-card control selects every other card in the
coarsest prefix, spanning its clump shares; it is not a nonexistent native
level 3. Width and local offset scale by the square root of the count
reduction, covariance by the reduction, retaining expected source area.
Opacity and material values are unfitted.

The generalized evaluator exactly reproduces both earlier full-count
images in its identity control. Every image passes physical moment bounds;
an unscaled-width negative control changes coverage. Sampling results:
46/48 at 16to32, 48/48 at 32to64. Selected spatial grid: 64×64.
Native hard-cut level-1 and level-2 GPU captures also pass all sixteen
32→64 integration checks and use the same source wood.

| Control | Cards | Stored coverage MAE | Off-grid coverage MAE | Stored screens | Off-grid screens |
|---|---:|---:|---:|---:|---:|
| ellipse | 13,280 | 0.011084152 | 0.011209176 | 1/5 | 5/5 |
| gaussian | 13,280 | 0.010873575 | 0.011043908 | 1/5 | 5/5 |
| ellipse | 6,640 | 0.017402736 | 0.018358856 | 1/5 | 1/5 |
| gaussian | 6,640 | 0.017001278 | 0.017888624 | 1/5 | 3/5 |
| ellipse | 3,320 | 0.023856093 | 0.025091473 | 0/5 | 0/5 |
| gaussian | 3,320 | 0.023406959 | 0.024291974 | 0/5 | 1/5 |
| native hard cut | 13,280 | 0.011177704 | 0.011336625 | 1/5 | 5/5 |
| native hard cut | 6,640 | 0.017632648 | 0.018552572 | 1/5 | 1/5 |

None of the six reduced-count primitive controls passes all five stored-view
screens. Half and quarter counts pass only the normal-moment screen there;
both eighth-count variants fail all five. Half-count variants still pass
the off-grid screens, showing why an unseen-view mean alone is insufficient.
These quality failures remain after all 48 final integration checks pass.
Native hard-cut LODs show similar losses: half-count stored coverage MAE
is 0.011178 versus 0.011084 for ellipses; quarter-count is 0.017633 versus
0.017403. This suggests reducing the source point set is the main loss
here, rather than the moment-kernel approximation alone. This inference
is limited to this tree, fixed material diagnostic and 64-pixel footprint.
A screen can fail its worst-view limit even when its average beats BC.

Fewer primitives cover larger regions. In the eight-view count ladder,
candidate leaf sample tests stay nearly constant as count falls because
expected area is retained. Gaussian support tests about seven times the
ellipse footprint. These are counts of rectangular sample tests in an
independent CPU implementation; they are not GPU instructions, shading
work or whole-renderer latency.

![Source count ladder and native hard-cut controls at the same stored direction](figures/neural-appearance-primitive-counts.png)

## Decision and reproducibility

Retain these source-derived controls when fitting any compact learned
representation. The next meaningful candidate must beat the unfitted
model at the same capacity, the native hard-cut controls where applicable,
and a strong quality-matched atlas. Report all stored and unseen-view
failures. Count positions, shape/material data, wood representation,
sorting, tile lists, caches, scratch and fallback in resident memory.

No packed primitive model, trained primitive model or GPU primitive
runtime exists in this checkpoint. CPU/capture elapsed observations
include reference evaluation or compilation/cooking/readback/file writes.
They do not establish G4 speed or G7 generation time. Production materials,
motion, lighting, 8–512-pixel footprints, unseen asset families, browser
whole-pass measurements and the full G1–G7 validation remain.

Focused format/type checks for the changed tool and fixture, `suite allow`,
independent controls and diff checks pass. No full-suite or PR-readiness
claim is made.

Evidence lives on Pop!_OS under
`/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/`:

| Artifact | SHA-256 |
|---|---|
| `appearance-primitives-v1/manifest.json` | `2a232b363a4f279ae0cb3a70d9c000e327933436989d262d8b223220883e2d39` |
| `appearance-primitive-budget-v1/manifest.json` | `7d55fc50d287fc0457b4d2ff2026f9cf91063bb2c4abda015d260238e34bc55c` |
| `appearance-primitives-v1/final-source.tar.gz` | `b333ba10f3c1fb3902c52a47ec7e1c21343b9d45af25cdb90bc2640003e7a453` |
| `appearance-primitives-v1/external-scripts.tar.gz` | `54933ac1ce22ba48aaf2cd7d1a9cc034263bc6128130f6509f77d79e3d72490d` |

The manifests hash the raw outputs, every external capture dependency,
failures, plans, scripts and figures. The source archive overlays the
recorded parent commit. Wrela 0.9.0 pins compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`; NumPy is 2.2.6.
All original goals and prior failures remain intact.
