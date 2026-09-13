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
| Edit a silhouette at broad scale while retaining detail | Global extraction resolution and flat list of strokes; coarse triangles can reject valid local work | Coarse/medium/detail passes; local adaptive extraction/refinement and correspondence-aware remeshing | Bounded local refinement and optional field projection implemented; tests cover edge continuity across attribute seams and skin weights. Optional regional octree extraction now compiles the joined Vesper head under strict geometry audits; binding recovery is explicit |
| Join head/neck anatomy without a seam | Separate part surfaces and changing rigid/skinned policies | Field composition across semantic parts with local detail budgets; explicit rebind | Retained working checkpoint joins head/body fields and preserves their joint palette; actual front/quarter/side seam improvement inspected |
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
Capture tokens also expire across app/compiler sessions; replay a study and
capture again before selecting its newly compiled geometry.

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

`lens(vertical_degrees)` changes the actual studio perspective (8…100° vertical),
compensating camera distance to retain apparent scale at the target. Use
`frame_visible(part)` afterward for exact geometry fitting. Rendering, sky rays,
picking, native camera fitting and study replay use the same lens. Old studies
retain the original 1.05-radian lens. The native Parts inspector exposes the value.
This reduces perspective distortion for proportion review; it is not orthographic
projection or reference-camera calibration. Garden projection remains unchanged.

`edit.fit_anatomy(source, controls, targets, iterations=24)` fits named source
parameters to base-field surface membership. Each control has `id`, delta bounds
`minimum`/`maximum`, and `terms=[{element,parameter,scale}]`. Linked terms express
symmetry or coordinated changes without species-specific solver code. Supported
parameters are centre/radius/rotation, capsule endpoints, additive blend and cut
rounding. All terms in one control use one unit. Bounds contain zero and limit
each physical parameter change to ±0.5 m or ±45°. A parameter belongs to one
control. Targets are `{id,position,tolerance}` in bind metres.

The bounded nonlinear solve reports each target's initial/final `abs(F)/|gradient|`
residual and each control's delta/bound saturation. Conflicts reject the entire
transaction, including earlier operations. Source compilation and persistent
correspondence recovery follow the same ordinary path as a manual anatomy edit.
Surface membership does not preserve a tracked material point's tangential
position, all points in a region, topology or collision. Disable sculpt passes
and surface edits on the part before fitting; downstream deformations are not
part of this base-field objective. Always inspect new actual rendered geometry.

`edit.refine_local(key, part, center, radius, edge_length, maximum_new_vertices)`
authors a local refinement recipe. Conforming triangle splits satisfy its edge
length inside an ellipsoidal footprint, propagate across shared edges (including
exactly coincident attribute seams) and carry skin/vertex attributes. The added-
vertex budget rejects atomically. Select again from a new capture afterward.
By default this preserves the initial piecewise-linear surface.

Optional `projection={maximumDistance, tolerance}` resolves the refined mesh back
to its base anatomy field. The inner 70% of the ellipsoid meets field tolerance;
the outer 30% feathers to the existing representation. Projection can change
edge lengths after subdivision. Distance, convergence and triangle orientation
are checked before publication. This supports anatomy before guide/garment
deformation, preserves topology and interpolated weights, and cannot recover
missing components or replace adaptive extraction.

`projection.retriangulate=true` explicitly permits local representation recovery
before projection. The existing vendored meshoptimizer removes redundant edges
within `min(maximumDistance * .1, edgeLength * .25)` estimated geometric error;
exterior vertices, attribute seams and varying skin/color/groom are locked.
Up to eight deterministic edge-flip passes then improve triangle quality.
Unused vertices are removed with the same remap for skin. Projection convergence
and orientation checks still apply. This is a bounded local mesh rebuild, not
an adaptive field extractor or a self-intersection certificate.

Anatomy cuts accept optional `cutBlend` in field metres. It rounds the transition
with smooth subtraction, including analytic field normals, conservative bounds,
region/material/skin blending and numerical Metal verification. Existing cuts
decode unchanged. `blend` continues to control only additive blending.

The first six-point fit took 17.9 s. Reusing unchanged extracted anatomy and the
actual preview mesh reduced it to 0.96 s on the M4. Adding region protection took
1.10 s. These are authoring latencies, not game runtime measurements.

## Remaining substantial limitations

- Region masks are spatial ellipsoids, not semantic/geodesic face sets. Narrow
  transitions can leave unwanted ridges despite exact interior preservation.
- Fitting evaluates selected target errors and full-surface displacement/folds;
  it does not infer anatomy, aesthetic quality or collision equivalence.
- Adaptive field extraction and connecting separately authored surfaces still
  need deeper compiler work. Local projection recovers existing field curvature,
  but cannot repair incorrect large forms or missing connected components.
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

`sculpt-rounded-cuts-20260912` contains matched 0 / 6 / 15 mm cut-rounding trials.
The 15 mm image visibly improves the eye and ear rims; small undersampled patches
remain. Its full submitted triangle count falls from 1,608,012 to 1,317,228, and
the edit takes 1.05 s. This is studio authoring evidence, not a game-frame budget
claim. CPU/GPU rounded-cut field error is 1.43e-6 over 4,096 points on Apple M4.
Local field projection passes native exact-replay tests. The first Vesper trial
rejected an eye-rim triangle: a cell-face sliver reversed when one corner moved
about 0.08 mm. Edge flips resolved that location but exposed a neighbouring
triangle that collapsed to 3.6% of its previous area. Both diagnostics and exact
inputs are preserved in `sculpt-field-recovery-20260912` and
`sculpt-retriangulation-20260912`; no rejected geometry was published. Bounded
local edge collapse also rejected a neighbouring face. The later orientation
fallback trial (`sculpt-orientation-recovery-20260912`) still rejected triangle
151444 after 5.60 s. Its original/projected positions and field-normal alignment
are archived. This difficult local recovery remains unresolved; no rejected
geometry or source was retained.

Further recovery found a face already oriented against the base field before
projection. Dual extraction now considers alternative quad diagonals and reports
sampled inverted faces separately from oriented edge incidence. Either failed
check uses the existing tetrahedral fallback. `inspect()['anatomy'][...]['extraction']`
exposes the cached source extraction provenance, counts and limits; it may be
unavailable after cache eviction. This is not a claim that either extractor
certifies self-intersections or collision equivalence.


`sculpt-lens-review-20260912` archives 20° front, quarter, side, back and above
renders with their replay studies. The native value control was exercised.
Narrower perspective reduces muzzle exaggeration but confirms oversized round
cheeks, shelf-like brows, jagged cuts and the disconnected-looking jaw. It improves
review reliability, not the authored anatomy. Native lens tests verify changed
pixels/picking, atomic rejection, exact undo and exact saved-study replay.

The orientation report layout exposed stale Swift build products. An isolated
clean build passed the crashing scene-batch test; prior debug/release caches were
archived under `.build/swift-cache-before-orientation-20260912/`, then both rebuilt.
The clean full suite passed 177 tests. With source fitting, 180 tests pass.

Shared Sanctuary actual-render regression check:
`.build/test-runs/20260912-101833-render-ecbd6/index.html` (PNG inspected).
Five-second live Apple M4 workload:
`.build/test-runs/20260912-101858-perf-8b945/index.html`.
GPU median/p95/max: 13.929/14.598/15.111 ms; frame intervals:
16.667/17.618/17.644 ms; CPU simulation: .362/.569/1.823 ms.
Normal live updates are included. Developer rebuilds and captures are separate.
This is an existing game regression check; Vesper is not integrated or measured
in that scene, and this does not satisfy the character performance target.


`sculpt-source-fit-20260912` archives 20 / 40 / 60 mm cheek requests using five
bounded controls linked across left/right anatomy. The 20 and 40 mm requests
compiled in 3.22 and 3.15 s; the 60 mm request rejected in .19 s with cheek,
brow and forehead conflicts and the saturated zygomatic-radius control. All
trials restored the original source. The 40 mm candidate's requested cheek
surface residuals are .463 / .115 mm, with held brow/nose/forehead/ear targets
inside their 2 mm tolerances. Those are implicit surface-membership residuals,
not tracked landmark displacement. Native source-fit tests cover actual changed
geometry/pixels, exact undo, strict numeric types and rejection of preceding
operations when a later fit conflicts.

Actual quarter, front and side candidate images were inspected; the latter two
are in `sculpt-source-fit-front-20260912` and `sculpt-source-fit-right-20260912`.
They show reduced lower-cheek bulk without the earlier masked-brush mound.
They also retain the anatomically incorrect forehead, orbit, muzzle and jaw.
This demonstrates a more direct structural editing workflow, not acceptable
finished anatomy. Native suite: `.build/test-runs/20260912-102852-authoring-42bfe/index.html`.

The reference extractor now attempts bracketed roots on canonical lattice edges.
Triangulation retains the linear reference's connectivity. Shared edge points
are held back together if any incident triangle would reverse/collapse; after
16 monotone passes, any unresolved recovery retains the full linear reference.
Root positions never increase the point's absolute field residual. Extraction
reports expose `referenceRefinedVertices` and `referenceHeldVertices` (canonical
edge points, before attribute splitting). Sphere/rounded-cut tests cover improved
field agreement, closed edge incidence, triangle orientation and deterministic
replay. The Vesper trials below exercise this recovery; these are not self-intersection
or missing-component guarantees.


After the edge-root change, the clean build passed all 181 Swift tests. The hard
Vesper mask reports 219,948 refined canonical edge points and 160 held points.
The rounded-base edit took 5.06 s; its separate local projection still rejected
triangle 137649 in 2.60 s. Without local remeshing, the 2.5 mm pass required at
least 102,299 extra vertices (above its 90,000 budget); the 5 mm pass rejected a
different sliver. These remain archived failures, with unchanged source.

`sculpt-root-matched-20260912-104548-67b492` replays the previous 15 mm rounded-cut
study against frozen pixels. Both actual PNGs were inspected: the rim improvement
is modest, with remaining serrations. This compares the combined compiler changes
since that capture, not an isolated root-solver ablation. The HTML source was read;
the browser restriction on local-file review remains in force.

Shared checks after root recovery pass: workshop
`.build/test-runs/20260912-105101-authoring-2403a/index.html`, authoring
`.build/test-runs/20260912-105111-authoring-fc87d/index.html`, Sanctuary render
`.build/test-runs/20260912-105139-render-69cde/index.html`, Cave native host
`.build/test-runs/20260912-105236-host-6e924/index.html`. Actual Sanctuary and Cave
PNGs were inspected. Boundaries pass; saves and existing review baselines remain
isolated from these harness runs.

Latest five-second live game measurement is
`.build/test-runs/20260912-105147-perf-fb010/result.json` (use its recorded `end`,
not the later restored `status.json`). GPU median/p95/max: 13.538/14.701/16.259 ms.
Frame intervals: 16.666/17.280/43.610 ms. The 43.610 ms interval is already present
in the first post-reset sample and remains retained in the report; its cause is
unattributed. CPU simulation max is 1.137 ms. Live atmosphere phases are included;
per-stage counters overlap and are not summed. This is not hitch-free 60 Hz proof,
and the workload does not contain Vesper.


## Continuous section envelopes

[Houdini Skin](https://www.sidefx.com/docs/houdini/nodes/sop/skin.html) and
[Cross Section Surface](https://www.sidefx.com/docs/houdini/nodes/sop/crosssectionsurface.html)
provide surfaces across authored profile curves. Wrela retains fields as source:
`primitive: "loft"` accepts 2–16 named parallel elliptical sections, in local
metres, transformed by the element's `center` and `rotation`. Its legacy `radius`
is fixed to `[1,1,1]` and `end` to zero. Each section has `id`, increasing `z`,
2D `center` and positive 2D `radius`. The ends are planar caps, not automatic
rounded tips. Component-wise monotone cubic Hermite interpolation passes through
each section without radius overshoot. This is not an arbitrary NURBS loft,
branched skeleton skinning, or a freely twisting cross-section surface.

The source fitter accepts `section.<id>.radius.x/y`, `section.<id>.center.x/y`
and `section.<id>.z`; source explanation exposes their local normal response.
Element translation and rotation remain available. Material coordinates retain
normalized XY and section interval/fraction. Changing section identities/order
requires rebinding; changing section dimensions preserves compatible coordinates.
Ordinary atomic craft transactions, alternatives, checkpoints and replay apply.

Loft values are general implicit values, not metre distances. The type propagates
through field composition. Sparse extraction uses conservative interval exclusion
for implicit graphs, with the existing fast distance bound for older fields.
Generated Metal evaluates the same cubic coefficients. Field visibility/capsule
queries use signed distance bounds certified by empty/interior cubes for these
graphs, a slower conservative path; compiled creature collision representations
remain the runtime approach. This does not establish physical realism.

The initial CPU tests cover profile membership, analytic gradients, sampled
interval enclosure, geometric closure across cap normal splits, source round-trip,
section correspondence, source fitting and conservative visibility. Subsequent native replay, GPU agreement and actual Vesper validation are recorded below.


Section-envelope validation, 12 September:

- Native edit/undo/strict rejection/replay passes in
  `.build/test-runs/20260912-113202-authoring-a3163/index.html`.
- Generated Metal initially failed on an unparenthesized negative station value;
  the failed `20260912-112339-gpu-check-95ccc` report is preserved. The corrected,
  locally scaled field passes in `20260912-113159-gpu-check-f4cc1`: 4,096 loft
  samples, maximum CPU/GPU disagreement 9.54e-7 on Apple M4.
- All 185 Swift tests pass (`.build/section-loft-local-all-tests.log`). The
  subsequently added capsule-wall check passes in the focused four-test loft
  suite (`.build/section-loft-collision-test.log`). The sweep remains on the near
  side of the wall; this checks conservative contact handling, not dynamics.
- The source field now scales by each evaluated section's smaller radius, with
  the derivative and interval product included. A remote narrow tip no longer
  weakens the entire envelope's blend behaviour. The global minimum radius is
  retained only for the exterior enclosure bound.
- Adding/reordering section IDs requires controlled rebinding. Explicit recovery
  starts from the old physical anchor position rather than reinterpreting its
  section index in the new topology. A regression test exercises insertion before
  an existing attached section.

Actual front/quarter/side PNGs in `sculpt-section-envelope-*-20260912` and
`sculpt-section-refinement-*-20260912` were inspected. Initial compile: 5.17 s;
refined source: 4.53 s. The second trial connects the nose and chin more coherently,
but remains smooth, simplified and unconvincing as lion anatomy, with a poor neck
transition and incomplete orbital structure. The refined envelope is retained
only as a working sculpt checkpoint, `vesper-envelope-sculpt-base-20260912`.
`vesper-before-envelope-sculpt-20260912` preserves the previous study. Published
assets remain unchanged.

`sculpt-envelope-planes-20260912` applies three actual-pixel-selected, mirrored
scrapes. Strength .55 accepts in 1.28 s but leaves an obvious cheek patch; .9
rejects a sampled fold above the eye in .25 s. Neither plane pass is retained.
`sculpt-envelope-rim-20260912` then tries 3 mm local field projection: plain and
retriangulated trials both reject the same orbital sliver in 1.29 / 1.48 s.
The envelope does not solve this representation limitation. A full changed-patch
orientation diagnostic accompanied projection failure. The subsequent recovery
and corrected continuous area check are described below.

### Projection recovery and several-view alternatives

The orbital failure was reproduced and audited over the whole changed patch.
Pre-simplification improved coarse triangles but made their field projection
worse. Explicit `retriangulate: true` now permits deterministic local diagonal
repair in both original and projected representations, followed by a retry from
the original topology if the reduced topology fails. Indexed attribute seams and
exterior topology remain fixed; flips do not move vertices or their channels.

The endpoint-normal test was also mathematically inappropriate: a triangle can
rotate over 90 degrees without collapsing, while agreeing endpoint normals can
hide a collapse during motion. `TriangleMotion` bounds squared triangle area
throughout linear vertex travel using a quartic Bernstein enclosure. Ambiguous
bounds reject; the area threshold remains 0.0025 of the initial squared area.
Final changed faces must also face outward relative to the authoritative field.
These are individual face checks, not a global self-intersection certificate.

`sculpt-envelope-continuous-recovery-20260912` accepts the real Vesper orbital
patch in 2.34 seconds. Baseline and candidate PNGs were inspected: the jagged
upper rim becomes smooth. The bead-like eye and absent lid structure remain
anatomical defects. Earlier failed studies, including the original-topology
attempt with three large normal rotations but zero inward faces, are retained.
All 188 Swift tests pass in `.build/sculpt-continuous-all-tests.log`.

`CreatureWorkspace.explore_edits(..., views=[dict(key='front', orbit=3.14159,
elevation=.12), ...])` now captures each source candidate in 1–8 specified views
after a single source compile. Each view has its own matched baseline metadata;
captures retain independent replay studies. Changes to settings between views
stop the comparison. The original camera and source are restored when unchanged
by another editor. The camera command accepts a bounded `viewName` label for
exact restoration. This is a comparison workflow, not an automated aesthetic
score: the agent must inspect the actual images from every requested view.


The native 60-check sculpt suite, including multi-view camera restoration and
exact pixel replay, passes in `20260912-121152-authoring-b2b3e`. Actual eyelid
alternatives are archived in `sculpt-orbital-wrap-20260912`; enlarged globes
protruded through the forehead, so none were retained.
`sculpt-orbital-seating-20260912` tests three depths and removes that protrusion,
but still has crude lid construction. `sculpt-continuous-head-20260912` uses the
existing `replaces` and weighted anatomy interfaces to unite head and body.
Inspected front/quarter/side images remove the hard neck seam but expose coarse
facial extraction. Both 14 mm and 9 mm projected head refinements reject actual
collapsed/inward faces in `sculpt-continuous-head-detail-20260912`. Source is
restored after each experiment; no published asset is changed.

Compilation now skips reference-lattice blocks whose conservative field interval
excludes zero, caches canonical edge normals, and prepares anatomy primitive
transforms/loft coefficients once for repeated skin/material queries. A first
regional-expression shortcut introduced tiny floating-point changes; it was
removed. Canonical whole-field values reproduce all nine original study images
with zero decoded-pixel differences in `sculpt-continuous-head-canonical-20260912`
(`pixel-comparison.json`). The same two structural edits improve from 12.63/11.13
to 8.11/6.65 seconds. These are short authoring measurements, not runtime FPS.
The faster but nonidentical intermediate results remain in
`sculpt-continuous-head-sampling-20260912`. All 189 Swift tests passed before the
canonical-value correction; the focused sparse/dense equivalence check passes
after it. Broader validation continues with subsequent compiler work.

## Local source-volume resampling

`AnatomySource.sampling` optionally selects conforming volume resampling before
surface extraction. `SurfaceSampling` records a `baseEdgeLength` in metres,
0–16 optional ellipsoidal `regions` (`id`, `center`, `radius`, `edgeLength`) and
`maximumTetrahedra`. The Python transaction helper is
`sample_anatomy(sourceID, base_edge_length=.04, regions=[...], maximum_tetrahedra=500000)`.
It composes with pending declarative source edits; place it before deferred
source fitting in the same transaction. Omission retains the existing extractor.
Changing sampling preserves field identity and semantic source coordinates.

The extractor builds an interval-pruned isotropic octree to the requested local
edge length. Each retained leaf connects its centre to triangle fans on its six
faces. Canonical dyadic face partitions and shared axis-edge breakpoints make the
boundary triangulation agree where coarse/fine cells meet, including edge-only
contacts. All scalar samples and final edge roots use the canonical whole field.
This can recover components missed by a coarse grid's corner signs. It is distinct
from `detailPatches`, which subdivide/project an already extracted surface.

[Fang et al., Extracting Geometrically Continuous Isosurfaces from Adaptive Mesh Refinement Data](https://web.cs.ucdavis.edu/~hamann/FangWeberChildsBruggerHamannJoy2004.pdf)
informs the use of conforming cell-boundary subdivisions and cell-centre pyramids.
Wrela triangulates each boundary patch and samples new points from its source
field; it does not use the paper's interpolation of supplied AMR values. These
are extraction cells, not a material simulation. Surface normals are sampled
diagnostics, not a global self-intersection or geometric-error certificate.
Explicit volume/depth budgets reject the entire edit.

The preceding longest-edge-star implementation was tested against
[Arnold and Mukherjee's discussion of tetrahedral conformity](https://www-users.cse.umn.edu/~arnold/papers/bistetima.pdf),
but was not their marked-tetrahedron algorithm. Actual Vesper tests exposed
excessive propagation beyond the requested head region. Correcting nonconvergent
capsule enclosures, splitting only longest edges and using isotropic starting
cells did not make the original one-million-cell requests fit. All rejected
inputs remain in the `sculpt-adaptive-head*` studies. The octree implementation
replaces that path; it does not retain a second selectable extraction algorithm.

Native source inspection reports the algorithm, volume/surface counts, octree subdivisions, transition faces,
held/refined roots, sampled inward faces and extraction milliseconds. Tests cover
closure across density transitions, recovery of a small separate component,
repeated deterministic compilation, an implicit loft with a rounded cut, semantic
coordinate/skin preservation and budget rejection. The 67-check native sculpt
suite passes in `20260912-124140-authoring-6c998`, including pending-edit composition,
undo, strict rejection and exact replay. All 192 Swift tests pass before the
subsequent capsule-enclosure correction.

The capsule enclosure now intersects its endpoint/AABB bounds with the exact
capsule's 1-Lipschitz centre enclosure, so shrinking query boxes converge to the
field value. Focused tests cover that correction and the octree's local density,
closure, source/skin preservation and deterministic extraction. Actual Vesper retesting on the replacement octree path is recorded below;
validation results above predate the final corner-classification change.

The final octree stage retains only sign-crossing tetrahedra, using exactly the
polygon extractor's existing no-polygon condition. `maximumTetrahedra` bounds
this retained volume; the same count also bounds retained octree leaves.
`SamplingReport` includes octree subdivisions, transition faces, recovery passes,
volume vertices, triangles, held roots, oriented geometric edge incidence and
sampled inward faces. The audit welds exact positions across attribute seams and
uses double-precision area tests to distinguish tiny valid triangles from exact
collapse. Failed orientation can request up to two local refinement passes under
the original volume budget; unresolved defects reject the entire transaction.
This still does not certify vertex links or global self-intersections.

Actual `sculpt-adaptive-head-crossing-20260912` accepts the original 10 mm head
request at 793,498 tetrahedra / 992,164 surface triangles in 18.79 s, with zero
sampled inward faces. Its 6 mm alternative rejects the unchanged one-million
budget. All three accepted views were inspected: the seams and jagged rims
improve, but the mask-like anatomy and bead eyes remain wrong.
`sculpt-adaptive-head-allocation-20260912` compares 20 mm head sampling (8.97 s,
448,410 triangles, six sampled inward faces) against focused eye/ear/nose detail
(14.10 s, 760,214 triangles, zero sampled inward faces). All six candidate images
were inspected. These are authoring costs before the subsequent stricter audit,
not runtime performance measurements. `explore_edits` now archives native
compilation diagnostics alongside each accepted candidate's cost and images.

Shared quick validation passes in `20260912-131730-quick-f2375`: 195 Swift tests,
harness checks, boundaries and both games' smoke scenarios. Native and actual
Vesper replay validation continue on the stricter audited build.

The stricter Vesper replay (`sculpt-adaptive-head-audited-20260912`) recovers the
20 mm candidate in two passes, but rejects the focused candidate: four exact
zero-area faces, failed geometric edge incidence and one inward face were hidden
by the earlier report. The compiler now removes only exactly zero-area faces,
then performs the full edge/orientation audit; removal never bypasses that gate.
Failed edge locations can also request bounded local recovery. Tiny positive-area
triangles remain intact. Focused CPU tests pass; native replay follows.

`sculpt-adaptive-head-canonical-corners-20260912` now accepts the focused source
under the full audit, without recovery: 608,994 retained tetrahedra, 760,038
triangles, closed oriented geometric edges, zero remaining degenerate triangles
and zero sampled inward faces. Fourteen near-zero grid corners are classified
consistently within four coordinate ULPs; the maximum linearized displacement is
0.000000954 m. Exact-zero endpoints retain their canonical coordinates instead
of being reconstructed through subtract/add interpolation. The classifier is a
floating-point representation policy, not a change to authored field functions
or a certified geometric-distance bound. It is recorded in the compile report.
176 resulting zero-area faces are removed before the full audit. Ten focused
sampling/reference/orientation tests pass in `.build/adaptive-canonical-corner-tests.log`.

All three actual candidate PNGs and the native app screenshot were inspected.
The surface is smoother but still reads as a simplified mask. The working-only
checkpoint is `vesper-continuous-sculpt-base-20260912`; no asset was published.
The first actual-pixel eye-rim fit (`sculpt-visible-orbit-fit-20260912`) rejects
its bounded spherical globe request in 0.32 s. The four selected rim points would
require a 0.272 m sphere radius, exposing an incorrectly constructed opening.
The next authored alternatives align the orbital section axis with the face.


`sculpt-orbit-alignment-20260912` rejects both rotated aperture lofts under the
unchanged geometry audit. `sculpt-coupled-eye-sockets-20260912` instead derives a
socket and globe from one centre/radius. The 75 mm globe compiles and all three
actual views were inspected; the 85 mm alternative rejects two remaining sampled
inward faces. The accepted candidate is also discarded artistically: a 12 mm
exposed cap leaves tiny circular eyes buried in the face. This establishes a
construction failure independently of meshing. Both trials restore the retained
working source and preserve their exact candidate inputs and rejection reports.

Native sculpt validation passes on the final octree/canonical-corner build in
`20260912-135229-authoring-2c51b`. Its HTML report and final actual PNG were
inspected; that restored tree image is a harness artifact, not Vesper evidence.
