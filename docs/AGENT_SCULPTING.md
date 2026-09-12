# Agent sculpting workflow

This is a workflow development effort prompted by the user's rejection of the
clay anatomy. Better controls must be demonstrated with visible edits; they do
not certify anatomical judgment or character quality.

## Established workflows examined

- [Blender sculpting](https://www.blender.org/features/sculpting/): direct surface brushes, masking, dynamic topology and multiresolution.
- [Blender visibility, masks and face sets](https://docs.blender.org/manual/en/4.3/sculpt_paint/sculpting/introduction/visibility_masking_face_sets.html): isolate the intended surface and protect neighbouring structures.
- [Blender multiresolution](https://docs.blender.org/manual/en/4.4/modeling/modifiers/generate/multiresolution.html): change broad form while viewing higher resolution detail; separate viewport, sculpt and render resolution.
- [ZBrush planar/trim/polish](https://help.maxon.net/zbr/en-us/Content/html/user-guide/3d-modeling/hard-surface/planar-trim-polish/planar-trim-polish.html): working planes derived from the surface or view, with different adding/cutting behaviours.
- [ZBrush sculpt layers](https://help.maxon.net/zbr/en-us/Content/html/user-guide/3d-modeling/sculpting/3d-layers/3d-layers.html): independent sculpt passes with visibility/intensity control and morph-based local recovery.
- [Houdini VDB reshape](https://www.sidefx.com/docs/houdini/nodes/sop/vdbreshapesdf.html): masked level-set dilation/erosion, opening/closing and explicit narrow-band management.

These are technique references. Wrela owns the source and runtime; no external
application or imported model becomes the production pipeline.

## Observed gaps and order of work

| Task the agent needs to perform | Existing friction | Required capability | Evidence status |
| --- | --- | --- | --- |
| Select a cheek seen in a render | `surfaceProbe` requires a guessed bind coordinate and finds a vertex | Pixel ray against the actual posed render items, barycentric bind/source correspondence, stale-frame rejection | Six actual cheek/face selections in 0.44 s; native rejection suite added |
| Establish a plane without moving the opposite side | Spherical falloff, one guessed direction, no protected regions | Surface-aligned elliptical footprint, scrape/fill/clay/pinch, scoped/protected masks | CPU field checks pass; native intent fit held 9,276 protected vertices exactly, but the narrow feather produced a visible ridge and was rejected |
| Compare two strengths and turn a pass off | Ordered strokes can be removed, but lack grouped intensity and preview reports | Named sculpt layers; deterministic baseline/variants and exact restore | Native 0 / 0.5 / 1 captures and replay studies saved; source restored exactly; 0.14–0.74 s layer updates |
| Edit a silhouette at broad scale while retaining detail | Global extraction resolution and flat list of strokes; coarse triangles can reject valid local work | Coarse/medium/detail passes; local adaptive extraction/refinement and correspondence-aware remeshing | Bounded conforming local refinement implemented; tests preserve surface, edge continuity and skin weights. Adaptive field extraction/topology changes remain unresolved |
| Join head/neck anatomy without a seam | Separate part surfaces and changing rigid/skinned policies | Field composition across semantic parts with local detail budgets; explicit rebind | Unresolved |
| Judge proportions consistently | Hidden finery still affects bounds; perspective/focus drifts | Fixed reference-aligned views, visible bounds, crop/landmark annotations and matched multiview review | Saved comparison frame locks framing/placement/light scale; visible-only deformed-surface framing implemented and native control exercised. Reference alignment still unresolved |
| Know what an edit actually did | Compile success and a later image are separate | Touched/protected displacement, selected source, latency, fold/miss diagnostics; links to paired renders | Native reports include target residuals, full-surface displacement and protected-vertex motion; normal change report added after the ridge failure |

## Agent-first contract

Observe an immutable actual-render frame → select pixels/regions → receive
surface frame, source identity and local dimensions → submit a bounded named
sculpt pass → inspect matched alternatives → retain or reject the pass. All
mutations use current source revisions and the ordinary atomic compile path.
Screen pixels are authoring input, never authoritative geometry. Stored source
contains fields, metre coordinates, masks, layer controls and provenance.

Do not resume Vesper finishing just because these APIs exist. Exercise them on
the difficult cheek/neck/shoulder edits and record what still obstructs the work.

## Public loop

`CreatureWorkspace.observe()` captures the engine image and a selection token.
`select(observation, pixels, part=...)` returns barycentric bind positions,
normals, source/region identity and local triangle size. A source edit invalidates
the selection. This selects opaque geometry, not groom alpha or material relief.

`select(..., explain=True)` also returns the contributing base fields and their
local parameter responses. A positive response means outward normal motion per
metre (or degree) of parameter increase. This uses the composed field's chain
rule, with local finite differences for primitive controls. It exposes buried
controls and subtractive contributions, not just the region's winning ID.
Responses are first-order estimates; branch switches, large changes and later
sculpt/deformation passes require a new render. Singular field gradients produce
no response prediction.

`edit.sculpt_selected(...)` builds a surface-aligned field layer from that result.
`edit.fit_sculpt(...)` fits a bounded set of compact fields to motion/preservation
targets on the actual compiled bind triangles. Conflicting requests reject the
whole transaction. `protect` volumes retain their inner ellipsoid exactly; an
optional `innerFraction` controls the feather to their outer radius (default .7).
This is bind-space fitting, not a posed deformation or physics solve.

`compare_sculpt_layer(key, new_folder, opacities=(0,.5,1))` archives actual PNGs,
metadata and replay studies under a fixed comparison frame, then restores the
original source with a revision check. It never accepts an alternative. Another
editor's revision prevents restoration over their changes; `original.json`
remains available for explicit recovery. The study must already be paused.

`preview_sculpt(part, controls=..., protect=...)` displays editable influence in
orange and protection in blue on the native deformed mesh. It is reversible
study state and is excluded from published character source.
`explore_sculpt(intents, new_folder)` evaluates at most eight independent requests
against the same baseline, archiving accepted renders and rejected diagnostics,
and retaining no candidate. Requests can set an explicit maximum surface-normal
change as well as displacement and target tolerances. Those are geometric limits,
not aesthetic approval.

`explore_edits([{key, operations}], new_folder)` extends the same bounded review
to ordinary craft transactions, including anatomy changes. It archives accepted
renders and rejected diagnostics and restores the entire baseline craft with a
revision check. `explore_sculpt` uses this common path. Source alternatives are
experiments, never implicit publication or approval.

`frame_visible(part=None)` frames current deformed, visible mesh surfaces without
using hidden finery or a registered motion envelope. It holds the resulting
comparison frame; `sculptFrameLock(enabled=False)` releases it. This changes the
camera target/distance while preserving subject placement and lighting scale.

`edit.refine_local(key, part, center, radius, edge_length, maximum_new_vertices)`
authors a local refinement recipe. Conforming triangle splits satisfy its edge
length inside an ellipsoidal footprint, propagate across shared edges and carry
skin/vertex attributes. The added-vertex budget rejects atomically. Select again
from a new capture afterward. This preserves the initial piecewise-linear
surface; it is not field reprojection, adaptive extraction or a topology change.

The first six-point fit took 17.9 s. Reusing unchanged extracted anatomy and the
actual preview mesh reduced it to 0.96 s on the M4. Adding region protection took
1.10 s. These are authoring latencies, not game runtime measurements.

## Remaining substantial limitations

- Region masks are spatial ellipsoids, not semantic/geodesic face sets. Narrow
  transitions can leave unwanted ridges despite exact interior preservation.
- Fitting evaluates selected target errors and full-surface displacement/folds;
  it does not infer anatomy, aesthetic quality or collision equivalence.
- Local field extraction and connecting separately authored surfaces still need
  deeper compiler work. Local tessellation supports detailed deformations but
  cannot repair incorrect large forms or recover missing extraction features.
- The HTML alternatives viewer was generated and its source inspected. The
  browser blocked its local-file URL; actual PNGs were inspected directly.
  Do not claim its new variant controls have been exercised in a browser.

Further technique examined: [de Goes and James, Regularized Kelvinlets (2017)](https://graphics.pixar.com/library/Kelvinlets/paper.pdf). Analytical elastic displacement fields and derivative constraints are relevant to broader form edits. This is research for subsequent deformation work, not an implemented or validated Wrela capability.

## Evidence from the difficult edit

`.soundstage/studies/sculpt-intent-exploration-20260912/` archives four bounded
requests against one unchanged source. Point preservation accepted a 45 mm
request with 16.45° maximum normal change. Wide region protection accepted at
26.04° but left an unwanted mound in the image. Tighter protection rejected the
45 mm request at 52.07° against its explicit 30° limit. A 20 mm alternative
accepted at 24.78°. None was retained. Numerical acceptance does not select the
artistic winner, and the clay face remains anatomically unconvincing.

The native influence preview made the overbroad protected eye region visible.
Widening its feather removed a sharp ridge but did not remove the unwanted
shape being held. This exposed an error in the requested edit as well as a tool
limitation. The native Parts inspector also showed an obsolete recipe pivot;
it now reads/edits the authored craft joint and anatomy material when present,
with no shadowed recipe override. Native rejection/undo tests cover this path.

The actual Vesper local-detail trial rejected a 3.25 mm patch at its 20,000-vertex
budget (the next pass needed at least 36,384). A 5 mm patch accepted in 1.39 s;
the selected triangle edge fell from 8.51 to 4.26 mm. A surface-aligned scrape
compiled in 0.69 s. Its matched layer captures are in
`sculpt-plane-comparison-20260912`; the plane change is modest and does not repair
the face. The trial layer and refinement were removed after review. A preflight
cost estimate remains desirable; current rejection reports a lower bound.

The source explanation found four contributors at the cheek point: cranium,
frontal, masseter and zygomatic. The zygomatic coefficient was .471 versus .272
for the reported owner (masseter). Conservative stretched fields also make
some radius responses counterintuitive within blends. `sculpt-source-alternatives-20260912`
archives two ordinary field edits, each about 3 s. Reducing blending exposes an
incorrect separate brow plate; neither alternative is acceptable anatomy.
The explanation removed guesswork about which source to change; it did not
validate the chosen facial construction.
