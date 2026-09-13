# Sanctuary contact-shadow footprint and bias study

Status: closed with no renderer change justified by the available native
evidence. The bounded CPU experiment and one-variable diagnostic patch packet
remain archived for reproduction; they are not an active implementation task.
Latest 0130 bridge evidence identifies a material mapping defect, and the
Soundstage tree source meets its floor. Neither establishes a shadow regression.
No new shadow stack or visual baseline approval follows from this study.

## Concrete defect and provenance

The actual 1920×1080 image
`.build/sanctuary-native-20260912/normal-coat-2257-native-opening-company-play/runtime/captures/native-bench-contact-4478337b.png`
has weak/separated-looking contact below the nearest thin bench leg. Its broad
shadow does not meet the leg with a convincing contact patch. The Sunhare's
opening images have a similar ambiguity in their soft foot shadows. The bench
seat's separate radial material defect is owned and repaired by the placement
author; changing that material does not establish contact correctness.

Capture source digest is
`cdfc2ca7806804f06a942317d74b2eb7a487d08c4d1462b3064334fdeaf4b15d`, shader digest
`673f7613b313b6c5fa31fb5ff399a54583a8c72567cbd536055093a996a4e979`.
The scene sun is (-0.4640331, 0.5877852, -0.6627079): altitude 36°, azimuth -35°.
The capture reports published-cache sun coherence. Keep its camera, light and
geometry fixed when comparing shadow variants.

The placement author's source audit places the bench at
(0.78509414, 1.410673, 20.149673) m, yaw π/2, scale 1. Source capsule bottoms
are Y=1.395673 m, at world Z=19.469673/20.829673 m. Source terrain triangle
interpolation gives Y=1.404460456/1.416394791 m: approximately 8.787/20.722 mm
penetration. These are source arithmetic, not measured compiled foot extrema.
Do not lower the bench to compensate for this screenshot. Sunhare's actual
compiled flat-plane sweep passed with minimum sole Y about -32 µm; production
opening capture root/support equality is additional root evidence, not per-paw
slope certification.

## Current renderer contract

Read-only source locations:

- `Engine/FieldEngine/FrameComposer.swift:15`: world-anchored orthographic matrix;
  game descriptor at line 44 uses half-width 65 m, near 1, far 250, distance 110.
- `Engine/FieldEngine/CameraMath.swift:23`: orthographic scale is 1/size in X/Y,
  therefore full span is 130 m and light-depth range is 249 m.
- `Engine/FieldEngine/Rendering/MetalRenderer.swift:383`: one 2048² depth32Float
  map. At line 456 raster depth bias is `(1, slopeScale: 1, clamp: 0.001)`.
- `Engine/FieldEngine/Resources/Surface.metal:336`: screen derivatives reconstruct
  receiver depth gradient in shadow UV. Bias is `.00005 + (abs(gx)+abs(gy))/2048`.
  Each comparison includes receiver-plane correction at its sample position.
  Outdoor compact filtering pairs a nominal 5×5 bilinear box into nine fetches;
  the finite-light widened path retains 25 fetches. The CPU experiment mirrors
  the compact branch's actual positions/references, without assuming equivalence
  to 25 fetches when receiver depth differs across samples.

Construction explicit instances bypass automatic LOD choice. No current evidence
identifies a missing bench shadow caster or warrants a geometry LOD repair.

## Scale and bias equations

Let T=130/2048=0.0634765625 m per light-plane texel, D=249 m depth range.

| Quantity | Source-derived value |
| --- | ---: |
| One shadow texel | 63.477 mm |
| Nominal five-tap light-plane footprint | 317.383 mm |
| Nominal three-tap footprint | 190.430 mm |
| Maximum capsule diameter | 110 mm = 1.733 texels |
| Ground cross-section at the two penetrations | approximately 59.65 / 86.02 mm |
| Receiver constant bias | .00005 × D = 12.450 mm in light depth |
| Raster bias clamp upper bound | .001 × D = 249 mm in light depth |

The ground cross-section uses the bottom hemisphere equation
`diameter = 2 sqrt(r² - (r-penetration)²)`, r=55 mm. It approximates the local
terrain by a horizontal slice; the shallow actual slope perturbs that section.
This puts contact width near one texel even before filtering. Five sample
centres span four texel intervals; each bilinear fetch also reaches neighboring
texels. The nominal five-texel width is a scale summary, not an exact compact
support boundary at every fractional texel coordinate. Projection onto a
grazing receiver further stretches the world-ground footprint.

For unit light vector L, light-plane bases R/U and receiver normal N, light depth
derivatives are `ddu = dot(N,R)/dot(N,L)` and
`ddv = dot(N,U)/dot(N,L)` (texture V uses the flipped up basis). Therefore:

```text
receiver bias in light metres = .01245 + T (abs(ddu) + abs(ddv))
model raster bias = min(.249, representativeFloatULP + T max(abs(ddu),abs(ddv)))
```

The second equation is an analytic tangent proxy for the slope of the rasterized
primitive, not a measurement of the M4 rasterizer. Actual caster triangles have
their own slopes; the receiver's slope must not be substituted for them when
claiming an observed displacement. Apple's API applies bias after clipping and
affects both depth testing and stored depth; its positive clamp limits positive
bias. [Apple Metal depth bias](https://developer.apple.com/documentation/metal/mtlrendercommandencoder/setdepthbias(_:slopeScale:clamp:)).

At 36° sun on a flat plane, the model receiver bias is 99.818 mm light depth,
and same-plane raster bias is about 87.383 mm. Their sum is only an illustrative
plane-to-plane comparison threshold: it corresponds to about 110.034 mm along
the plane normal, not a measured foot lift or screenshot gap. The 249 mm raster
clamp is likewise a maximum, never an unconditional displacement. At 12° and a
Z slope of -0.25, `N·L≈.00737`; receiver-plane bias grows to 9.737 m light depth.
This is a near-grazing conditioning limit, not evidence that that slope exists
below the captured bench.

These mechanisms match the known bias tradeoff: insufficient bias produces
self-shadowing, while excessive bias separates a shadow from its caster.
Low-resolution projection and grazing surfaces make both problems harder.
[Microsoft's shadow-depth artifact discussion](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/common-techniques-to-improve-shadow-depth-maps).
PCF averages comparison outcomes; a wide kernel smooths an undersampled edge
but cannot restore a missing sub-texel contact region. Hardware bilinear shadow
filtering already combines adjacent comparison results.
[NVIDIA/Pixar GPU Gems, shadow-map antialiasing](https://developer.nvidia.com/gpugems/gpugems/part-ii-lighting-and-shadows/chapter-11-shadow-map-antialiasing).

## Reproducible CPU experiment

```sh
python3 Tools/Experiments/SanctuaryContactShadows/experiment.py \
  --output .build/sanctuary-native-20260912/contact-shadow-cpu
```

Outputs are `results.json`, `contact-profiles.csv`, and the engineering scale
diagram `footprint.svg`. No app/GPU, source mutation, imported fixture mesh or
player-save access occurs. Source hashes accompany the result. The depth cache
is bounded to 32,768 samples. The source bench is analytically intersected after
captured yaw; a local plane is fit through the two published support samples,
with unknown X slope explicitly zero. This model excludes compiled triangle
error, exact hardware float rounding, foliage, Sunhare skinning, ambient light,
tone mapping and human perception. It cannot certify the actual rendered gap.

Two 121-point profiles run downstream from the feet, and a 16-position texel
phase sweep isolates one leg. Selected model outputs for the less-occluded
foot at relative world Z=-.68 m:

| Model | Mean occlusion, 60–160 mm from foot axis | First ≥25% occlusion from axis |
| --- | ---: | ---: |
| Existing paired 5×5 + current bias | 23.06% | 125 mm |
| Existing filter, raster slope scale 0 | 27.06% | 90 mm |
| 3×3 filter, all bias unchanged | 36.08% | 60 mm |

These distances start at the axis, not the capsule edge, and include seat/back
occlusion; they are not screenshot gap measurements. For an isolated leg sampled
80 mm downstream, changing texel phase gives occlusion ranges 10.00–24.00%
(reference), 12.00–28.00% (raster slope zero), and 16.67–38.90% (3×3). Narrower
filtering strengthens contact but increases this example's phase variation.
This supports a controlled filter diagnostic, not unconditional sharpening.

## At most two native diagnostic variants

Use one fresh matched reference capture under the currently selected material,
then the following variants separately. Keep exposure, sun, projection span,
shadow resolution, mesh instances and material fixed. Do not combine changes.

1. **Compact outdoor 3×3 PCF:** only the `spread <= 1.0001` branch changes to
   receiver-relative offsets x/y=-1...1, one texel apart, division by 9. Retain
   `sc.z + dot(gradient,offset) - bias`, and leave the finite-light branch and all
   raster bias untouched. This retains nine comparison fetches, zero extra maps,
   zero extra passes and zero extra allocations. It tests contact dilution by
   the kernel. It may worsen edge aliasing or temporal variation.
2. **Raster slope isolation:** restore existing filter and change only
   `setDepthBias(1, slopeScale: 1, clamp: .001)` to slopeScale 0. Keep constant 1
   and clamp unchanged, and preserve receiver-plane bias. This has no added
   passes, texture fetches or allocations. It tests whether caster slope bias
   causes separation; it is a diagnostic and can reveal acne on steep or thin
   geometry. Do not remove all bias or assert this is safe globally.

For both, root should use the same bench destination and opening Sunhare, first
paused with repeat capture, then a short ordinary camera move. Examine near-foot
contact and adjacent floor at front/quarter/side; the Sunhare must retain visible
flight clearance at hop apex and planted support at contact. Contact must not be
manufactured by darkening albedo or ambient light. Check illuminated terrain,
bench seat/back, curved hare cheeks/ears and grass for new acne/bands, and include
the previously failing Alpine slope with unchanged geometry. A representative
low sun and moderate uphill/downhill slope challenge the bias reduction.

M4 cost gate: native 1920×1080, one Wrela render window, same short live workload
and baseline/candidate sample durations; report GPU p95/max and presented max,
including normal cache updates. Targets remain GPU p95 ≤16.7 ms and presented
max ≤33.4 ms. Compare the shadow/scene counters without adding their overlapping
intervals. Retain outliers and device/thermal/source fingerprints. No GPU speedup
is predicted from reducing 25 nominal samples to nine here: the existing outdoor
filter already uses nine hardware fetches. An acceptable contact change must
also preserve exact paused replay and not introduce new visible shimmer during
the short camera move.

## Decision

A narrow diagnostic in the existing shadow path is warranted: source contact
does not explain the apparent lift, while actual map/filter scale and slope bias
are large relative to the foot. The 3×3 variant is the stronger CPU prediction;
raster slope isolation distinguishes a second plausible contributor. Neither
variant is production-approved before the matched native slope/acne/cost check.
Do not add cascades, contact decals, screen-space shadows, an alternate renderer,
or a creature-specific shadow exception from this evidence. Geometry and shared
game/studio pose equations remain unchanged.

Latest local execution: 0.0544 s CPU, 242 profile locations, 16 phase locations
per variant and 432 planar comparison sanity locations. All planar sanity
locations returned zero false occlusion. The fitted native plane gives receiver
bias 99.048 mm and model plane raster bias 86.075 mm in light depth. These
numbers describe the analytic experiment only; no shader check, Swift test,
native benchmark or renderer visual approval was performed by this worker.

## Archived decision and 2337 follow-up

The completed CPU experiment remains archived at the output paths above; no
additional model run is needed before the native diagnostic. The latest actual
`local-nap-2337-water-construction-rejected-6d09f947.png` still shows bench feet
perceptually separated from a broad shadow patch after the separate material
repair. This matches the class of failure modelled here, but its different
camera/scene state prevents a quantitative comparison against 4478337b. The new
capture source digest is
`ce1cc3d5a1ef604b7c030523caffc2bc278af8a571d521272918bf786b41e961`, shader digest
`2d3f3d0debd4e192e4df828a4f0720cd5fd86a4f4e3d00f249e4352689a3ed05`.
The observed separation is not evidence that its source feet moved upward.

Root's reproducible handoff is
`Tools/Experiments/SanctuaryContactShadows/NATIVE_DIAGNOSTICS.md`. A/B now exist as
separate patch files against frozen 2337 source, with complete hashes and an
atomic apply/revert runner. Both variants affect outdoor frames only. B retains
slope scale 1 for frames without the outdoor shadow descriptor. The native
fixture helper restores the saved production snapshot, checks the exact camera,
and produces repeated matched captures on an already-running isolated app.
It is root-operated and has not been run against a native app by this worker.

Patch application was checked on temporary copies only: A/B each apply, repeat
apply, inspect, revert and repeat revert successfully; both restore their exact
reference file hashes. Combining A+B and applying after source drift are rejected
before writes. Receipt:
`.build/sanctuary-native-20260912/contact-shadow-patch-check.json`. Shared renderer
files and the immutable 2337 candidate remained unchanged. The next blocker is
root's actual shadow A/B, not authority to implement a new stack or propagate
creature design changes.

## Reproducibility and source-limit review, 2026-09-13

The actual reopened bench image
`local-nap-2337-manual-reopened-585c7637.png` was inspected at its native 1920×1080
resolution. Its visible near leg ends against a broad, softly defined dark patch;
the strongest shadow continues down-left. A distinct hard contact patch remains
unclear. This is consistent with the filter/bias uncertainty, but the dark ground
at the leg does **not** support claiming a measured empty gap or a newly lifted
foot. Its source/shader digests equal the earlier 2337 receipt. PNG SHA-256 is
`43703f8882e63421a98e256ae24a722fe96ce74420e18116f8c4976aad2d51ee`.
The two source penetration values remain the applicable geometric evidence;
this inspection supplies no reason to lower the bench.

The actual `groom-parity-0017-tidepool-reopened-764df721.png` was also inspected.
It contains a nearly black sky, low-contrast foreground and distant rock/shore
silhouettes; no nearby bench or readable creature sole contact is available.
Metadata has sun altitude −12°, coherent published scene sun, and outdoor ambient
floor (0.0063, 0.0098, 0.014). It therefore cannot diagnose daylight directional
shadow separation. Do not infer a new caster/bias defect from this image or
compensate its exposure for this comparison. Its PNG SHA-256 is
`7ede0ec1f5ef805f9b49b994572ccc700841236190feabd9f3342e23e7ddf924`;
source digest `aa8c8f7ec480235ea9d99fe2a295651f5251006c305cd107a3ff1823fcc029c0`.

The primary references were checked again. Apple's depth-bias API documents the
post-clipping operation, signed clamp, and effect on depth testing/writes; it does
not specify the model's representative float-ULP formula or establish measured
M4 caster displacement. Keep the model's constant term and tangent slope labelled
as proxies. [Apple Metal depth bias](https://developer.apple.com/documentation/metal/mtlrendercommandencoder/setdepthbias(_:slopeScale:clamp:)).
The Microsoft source supports the acne/separation and projection-conditioning
tradeoffs, but its Direct3D examples are not a Metal implementation contract.
[Microsoft shadow-depth artifacts](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/common-techniques-to-improve-shadow-depth-maps).
The NVIDIA/Pixar chapter supports comparison filtering and hardware interpolation;
its historical GPU examples do not measure M4 cost or prove that paired taps
with different receiver references equal an unpaired filter. A fixed PCF radius
also does not constitute a physically varying contact penumbra.
[GPU Gems chapter 11](https://developer.nvidia.com/gpugems/gpugems/part-ii-lighting-and-shadows/chapter-11-shadow-map-antialiasing).

The native helper previously omitted restoration of `sceneLook` and returned
success even when repeated PNG bytes differed. It now restores the full saved
shared look, checks the **captured** camera/light/sky/wetness/exposure/look/ambient
floor, requires known coherent sun/cache flags and stable source/shader digests,
and exits nonzero after preserving a failed comparison report. A wrong app can
still produce internally consistent captures: root must check its sealed variant
manifest. The helper's protocol paths were reviewed against `GameBridge` and
the capture metadata writer; the revised helper has not been run against an app
by this worker. The CPU experiment was not rerun. No new shader variant, renderer
change, benchmark result or visual approval is introduced by this review.

## 0130 bridge and studio-tree separation

Inspected actual `ear-groom-0130-bridge-cast-649d267b.png` and the immutable
`sanctuary-ear-groom-20260913-0130` source. The bridge deck has an obvious radial
starburst in its brown surface response. That source assigns material 4 to the
deck/rails; `Materials.metal` computes bark color and bump from
`atan2(local.z,local.x)`. A flat horizontal deck therefore inherits a cylindrical
bark coordinate singularity. This is direct source evidence for the surface
pattern, not evidence of a missing caster or detached contact shadow. The bridge
bank supports are not usefully exposed by this close view; water and the bridge
obscure the terrain contact region. The Sunhare in the background is not a
paused sole-contact measurement.

The **single recommended diagnostic** from these images is to validate the
already-authored bridge material-only correction (main uses material 7) against
that same captured view: camera (0.014510762, 3.1025672, 23.285015), yaw 0, pitch
−0.68, noon sun altitude 55°/azimuth −35°, exposure 1, fixed shared look and saved
simulation. Geometry, shadow projection and all biases must stay unchanged.
The radial deck pattern should disappear without losing the rail/caster
silhouettes. This is a source-material comparison, not an A/B approval of either
shadow packet variant. No additional shadow diagnostic is justified by the
starburst itself.

Also inspected actual studio `style-board-20260913-013334-26c651/frame-003.png`
and the flora owner's `working-tree-floor-contact-0130-review.md`. The lowest
compiled tree vertex is translated to ground Y=0 in this bind/wind-zero study;
the capsule's rounded bottom rises around a tangent point. This differs from the
game's buried root. No positive geometric gap was established. The visible
trunk shadow reaches the small root region, but this still does not quantify
caster/receiver bias.

The tree's captured `workshop.extentMetres` is 6.902527809. Its **studio** single
subject shadow uses half-width 1.2×extent, so full span is 16.566066742 m and
texel width is approximately 8.089 mm (nominal five-tap footprint 40.445 mm).
The depth interval is 11.99×extent, about 82.7613 m. Do not substitute this
study's numbers with the game's 130 m/249 m intervals, or apply the bench model's
predicted separation to the studio tree. Existing native bench shadow A/B remains
pending; neither new image resolves it or warrants changing tree anatomy.

Bridge and tree captures both report source digest
`d4dcee44f7554bab4096f997820d6b927cddabdbc5a35a15003a5e37c7b15eca` and shader digest
`2d3f3d0debd4e192e4df828a4f0720cd5fd86a4f4e3d00f249e4352689a3ed05`.
No renderer edits, CPU experiment rerun, build or GPU run accompanied this review.
