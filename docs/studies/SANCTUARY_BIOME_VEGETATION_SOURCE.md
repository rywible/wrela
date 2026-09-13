# Biome vegetation source studies — September 12, 2026

Status: second iteration built by the root agent and inspected in six selected
actual native captures. Willow and palm are suitable for provisional scene
integration to judge context; neither has an approved baseline or final-art
acceptance. Broadleaf remains an isolated authoring study and needs further work.
The root agent coordinates builds and serial native/GPU work.

The existing creek and rainforest captures were inspected directly:

- `.build/test-runs/20260912-173345-render-91bd3/native-2/runtime/captures/explore-creek-129dda30.png`
- `.build/test-runs/20260912-173345-render-91bd3/native-7/runtime/captures/explore-rainforest-238ea50b.png`

Both reuse the same flattened opaque crown and columnar trunk. The rainforest
foreground exposes the lack of separate botanical silhouette most clearly.
Creek framing places trees much farther away; it cannot establish close material
quality. Terrain/water defects visible in these images are separate work.

## Authored candidates

`Games/Sanctuary/Project/BiomeVegetationDesign.swift` owns three native procedural
recipes. `SanctuaryProject.swift` registers separate subjects and preserves the
existing tree generator and published tree source.

| Subject | Source design | Default growth height / spread / trunk radius | Base triangles, wood + foliage |
| --- | --- | --- | --- |
| `biome-willow` | Seven unequal scaffold branches, 49 irregular hanging sprays, 1,344 pointed blades | 5.6 / 6 / 0.27 m | 6,470 + 16,128 = 22,598 |
| `biome-palm` | More curved tapered trunk and crown sheath, thirteen pinnate fronds, 351 blades | 7 / 6.2 / 0.19 m | 2,940 + 4,212 = 7,152 |
| `biome-broadleaf` | Nine rounded foliage aggregates at three unequal heights, 540 smaller oriented edge blades | 5.6 / 6 / 0.27 m | 2,268 + 12,096 = 14,364 |

These are source topology counts, not measured draw counts. Growth height and
spread are semantic guide dimensions: leaves/root flares can extend beyond them.
Soundstage reports the actual compiled bounds. Changing the four scalar controls
does not increase topology counts. Height ranges 3–10 m, spread 3–9 m, trunk
radius 0.12–0.48 m, and dimensionless droop 0.65–1.25. A deliberately loose
source-derived enclosure over these ranges is `[-8, -1, -8] ... [8, 12, 8]` m
before wind: Bezier centers remain within their control hulls, lobe radial
modulation stays in 0.855–1.145, and all tube/leaf offsets are bounded. This is
an analytical source envelope, not a measured compiled bounds or collision claim.

Each design produces two batches: wood uses material 4, foliage uses material 8
and explicit double-sided rasterization. Each leaf is a curved twelve-triangle
surface with a pointed root and tip, broad vertex tint and a central fold. There
are no imported models, image textures or alpha cutouts. Broadleaf now includes
nine capped, irregular parametric foliage volumes (624 triangles each) rather
than trying to establish a whole crown using oversized flat leaf fans. These
represent unresolved small foliage as rounded groups; they are an intentional
aggregate approximation, not individually modeled interior leaves.
Colors use the Sanctuary muted foliage/warm bark direction and shared scene look.
No light/exposure/material shader change was made for these subjects.

Wood uses capped transported-frame tubes. Branch bases overlap inside their
parent tube. This establishes visible attachment but is not a welded manifold
union; the models do not supply collision solids. Leaf blades are deliberately
open surfaces. Willow/palm leaf roots are sampled directly on their stem curves;
broadleaf edge roots use the same envelope function as their foliage aggregate.

`Design.batches(name:)` requests existing simplification LODs: wood ratio 0.45,
error 0.035 m; foliage ratio 0.5, error 0.025 m. These are targets, not promised
reductions. Actual retained triangle counts, thin-tip behavior and visual changes
must be checked after compilation; disconnected thin blades can constrain reduction.

## Shared integration contract

`BiomeVegetationDesign.make(.willow/.palm/.broadleaf, parameters: [:])` returns
`Design { wood: Mesh, foliage: Mesh }`. Both meshes contain local source color.
Production instances should supply a neutral or small variation tint, rather
than multiplying the existing dark per-biome color over the authored palette.

Wood and foliage must receive the **same instance transform**. The current
regional `coreScale`/`canopyScale` split should not be reused for these designs.
Their physical attachments and the renderer's existing height-based deformation
for material kinds 4 and 8 agree when instance transforms agree. There is no new
branch dynamics, independent leaf flutter or biological growth simulation.

These registrations have not replaced regional meshes. Root/native acceptance
precedes integration. CPU collision remains owned by SanctuaryContent and needs
separate consideration if visual trunk placement differs materially from its
existing source; no presentation-only shape should quietly become collision truth.

## Review handoff

For each subject, select Sanctuary explicitly, inspect catalog, select subject,
select softbox, pause and save a named study before iteration. Compare front,
quarter, back and above under noon, golden, sunset, afterglow, overcast, rain and
indoors. Save a style board against working tree/stone/seed references. Inspect
the PNGs and the `workshop-study` HTML viewer. Check lower canopy gaps, attachment
at branches, leaf underside shading, tip sparkle and shadows. Resume live wind
and rotate the view before accepting. After publication/integration, inspect the
actual creek/rainforest/meadow viewpoints and report live update hitches separately
from steady frame cost.

## First native inspection and second iteration

All six quarter/above PNGs listed in
`.build/sanctuary-native-20260912/flora-first-review.json` were inspected directly.
Captures reside under `field-flora-stage/workspace/.soundstage/captures/` in that
run directory. These are rendered evidence of iteration one, not iteration two.

- Willow quarter `biome-willow-quarter-7940403e.png` and above
  `biome-willow-above-22a190d2.png`: recognizable weeping shape, but regular bare
  scaffold spokes, thin repeated curtains and similarly terminated strands.
  Iteration two varies scaffold reach/height, introduces 49 unequal sprays,
  staggers the opposing leaves and changes their orientation. Blade width grows
  modestly; the palette is unchanged.
- Palm quarter `biome-palm-quarter-a61f75b6.png` and above
  `biome-palm-above-bbc475cb.png`: recognizable in both views; adequate above-view
  coverage, but thin lower frond tips and a nearly straight, uniform trunk.
  Iteration two increases trunk curvature/taper, adds a short crown sheath,
  modestly widens lower leaflets and shortens terminal blades. Frond count stays 13.
- Broadleaf quarter `biome-broadleaf-quarter-f135ae8e.png` and above
  `biome-broadleaf-above-08cac282.png`: rejected. Giant opposed blades fused into
  fern-like horizontal ladders, exposing almost the whole scaffold and a cut
  trunk end. Iteration two removes those fans, places nine unequal rounded foliage
  groups over branch ends, and anchors 540 blades around their surfaces with
  differing orientations. Default blades are approximately 0.22–0.30 m long,
  compared with approximately 0.6 m before. The trunk ends within the upper groups.

The dark trunk/leaf underside rendering affects all three first captures. This
iteration makes no exposure, shared look or shader change to compensate for it.

Required next native comparison: replay each first study for matched quarter and
above noon captures, especially broadleaf crown mass and underside continuity.
Then inspect broadleaf front/back under softbox and noon to expose accidental
stacking or disconnected foliage spheres, and willow front/back to check that
curtain gaps remain deliberate. Compare all required lights/views from the
general handoff before acceptance. Resume wind and orbit to verify no branch/leaf
attachment separation. Check palm crown sheath and trunk curve in left/right views.

Second-iteration risks still requiring actual renderer inspection: broadleaf
aggregates may look too smooth or spherical; willow can still show repetitive
leaf rhythm; palm leaflets may overlap too much from above. More willow triangles
and broader overlapping foliage increase potential raster/shadow cost, so short
live measurements remain required before regional population use. No measured
performance improvement is claimed.

## Second native inspection — bounded review

The root's `.build/sanctuary-native-20260912/flora-second-review.json` records
15 captures. This review inspected six useful PNGs directly, all under
`flora-coherent-stage/workspace/.soundstage/captures/` in that run directory:

| Subject / view | Actual PNG | Finding |
| --- | --- | --- |
| Willow, noon quarter | `biome-willow-second-quarter-7052a2f1.png` | Fuller, recognizable weeping silhouette; uneven curtain ends improve the first version. Leaf scale and strand rhythm remain conspicuous. |
| Willow, noon above | `biome-willow-second-above-42b0c7b0.png` | Unequal branches reduce the previous spoke symmetry, but the open center and cut central trunk tip remain visible. |
| Palm, noon quarter | `biome-palm-second-quarter-84d9e3d5.png` | Clear palm identity; long bare trunk and thin fronds still dominate. |
| Palm, noon back | `biome-palm-second-back-0ddf5bc1.png` | Trunk curvature reads clearly. The small crown sheath is attached, though visually simple; lower fronds remain sparse. |
| Broadleaf, noon above | `biome-broadleaf-second-above-4dbb8c06.png` | Rounded silhouette improves on fern fans, but separate smooth masses dominate and sparse leaf ornaments resemble bumps. |
| Broadleaf, indoor quarter | `broadleaf-second-indoor-874b383d.png` | Brighter indoor lighting confirms that stacked rounded masses and sparse, faceted surface leaves are geometry/detail problems. The crown does not yet read as rich foliage. |

Decision: integrate willow and palm **provisionally** into the isolated creek and
rainforest context, preserving a single shared transform for their wood/foliage.
This is permission to evaluate context, not a baseline approval or performance
acceptance. Check leaf readability and shadow density at normal movement distance,
actual ground/collision alignment, live wind attachments and short live costs.
Complete outstanding lighting/view comparisons before final acceptance. Neither
the geometry author nor these still images establish live motion correctness.

Keep broadleaf out of the regional replacement for now. A bounded next candidate
should redistribute its current budget into approximately 16–18 smaller unequal
foliage groups, then cover them with roughly 1,500–1,600 native four-triangle
blades using the established `Mesher.leaves` approach. That replaces 540 expensive,
widely separated twelve-triangle leaves with more continuous foliage texture at
similar triangle cost. Lower group tessellation can offset the extra groups.
This is a concrete proposed next source change, not an implemented result.

Shared dark-side rendering remains apparent in the outdoor views, especially
wood and leaf undersides. No per-object exposure compensation is warranted.
Broadleaf's indoor capture is sufficient to separate its shape/detail failure
from that lighting issue. This review changes the report only: source remains
frozen at the fingerprint below; no build, native control or GPU work was run by
the reviewing agent.

## Frozen source fingerprints

SHA-256 at second source handoff (before its native review):

| File | SHA-256 |
| --- | --- |
| `Games/Sanctuary/Project/BiomeVegetationDesign.swift` | `c7fe9d230686b777d30c8ef2cacb61693952bbcc110d0b89e0a9b6b32fb2ea06` |
| `Games/Sanctuary/Project/SanctuaryProject.swift` | `574b6807b2509e75cc3b1698ea7d48019396a9462361c2a0180dcb5a8ca4ac65` |
| `Engine/FieldEngine/Resources/Surface.metal` | `0470de2d0965c2a6b060d2a8536c454ce81f527240a0f42e4133450e0c794975` |
| `Games/Sanctuary/Authoring/Materials.metal` | `1e1f2a798c3ffc3f417397362a8e603e23cde6972afe7963fa49e54870bb202e` |
| `Games/Sanctuary/Authoring/ArtDirection.json` | `2993f512b120c5c878e6791ff17e882c3f339271e8e9983057db3cef260ea799` |

The shared shader fingerprint reflects the current team workspace, not a shader
authored by the vegetation agent. Future capture metadata is authoritative for
the complete shader set actually used by a native study.
