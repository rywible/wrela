# Creature craft

The `craft` extension in an `AssetSource` contains reusable source for sculpting,
rigging, pose corrections, choreography, references, grooming and costume seams.
Swift/JSON source is authoritative. Generated meshes, palettes and review frames
are disposable. The game and Soundstage evaluate the same asset source.

This is an implemented workshop, not certification of AAA art or a complete DCC.
The limitations below are part of the interface contract.

## Start an agent session

Select a project and subject explicitly. Save a study before a substantial edit.

```sh
scripts/stagectl project sanctuary
scripts/stagectl subject vesper
scripts/stagectl rig softbox --z -1.5
scripts/stagectl creature --mode procession --seconds 0
scripts/stagectl pause true
scripts/stagectl saveStudy before-craft.json
scripts/stagectl craftInspect
```

`craftInspect` returns the source revision, strict JSON schema, actual part vertex
counts, joint definitions, secondary guide nodes, contact-chain IDs and clips.
`docs/CreatureCraft.schema.json` is the exported schema. Its structural validation
is shared by file loading and transactions. Semantic/geometric checks additionally
reject missing references, invalid bounds, sampled folds, and uncovered edits.

`Tools/AgentTools/creature.py` supplies a small Python interface:

```python
from creature import CreatureWorkspace
studio = CreatureWorkspace()
with studio.edit() as edit:
    edit.landmark('left-cheek', 'mask', [-.56, 2.99, -2.03])
    edit.sculpt_landmark('cheek-plane', 'left-cheek', brush='flatten',
                        radius=.32, strength=.15, direction=[0, 0, -1], mirror=True)
    edit.skin('chest', 'body', [0, 2, -.4], [1, 1, 1],
              {'body': .4, 'neck': .6}, strength=.5)
```

The context manager submits one revision-checked transaction. An exception in the
body submits nothing. Stale revisions are rejected; there is no automatic retry
that could overwrite another edit. A native Undo restores the complete study.
Source publication remains explicit through `studio.save()` / `assetSave`.

The full status document is preserved. Immutable source encoding is cached until
an accepted source change; heartbeat and response serialization run off the UI
thread. Commands poll at 10 Hz, full heartbeats publish at 2 Hz, and stale
heartbeats are coalesced. Acknowledgements are atomically written after the edit
and any required GPU capture complete. Shutdown drains writes before marking
the session stopped.

The equivalent `stagectl craft request.json` envelope is:

```json
{
  "expectedRevision": "revision returned by craftInspect",
  "operations": [
    {"op": "upsert", "collection": "landmarks", "value": {
      "id": "left-cheek", "part": "mask", "position": [-0.56, 2.99, -2.03], "note": "Cheek plane"
    }}
  ]
}
```

`upsert` replaces one identified record in place; it does not silently reorder
sculpt or material operations. `remove` requires an existing key. `set` replaces
`refinement`, `arrangement` or `balance`; null clears the latter two. An invalid
operation aborts the whole transaction. Compilation occurs before publication.

## Sculpt and anatomical form

- Landmarks identify named bind-space points and retain design notes.
- Spatial strokes use polyline coverage independent of control-point density.
  Brushes are grab, inflate along an authored direction, flatten, crease and
  adjacency-based smooth. X reflection includes displacement directions.
- Compact surface edits and strokes are ordered. A missed primary surface or
  sampled reversed/collapsed geometry is rejected with the responsible stroke.
- Per-part midpoint refinement supports two levels with shared-edge caching and
  interpolated skin weights. Source strokes survive refinement because they
  address spatial regions, not generated vertex numbers.
- Geometry caches retain unaffected parts. Motion and annotation changes reuse
  geometry; a new groom only replaces the selected generated part.

Limits: refinement preserves topology and does not add new holes or reconnect
anatomy. Smoothing uses indexed adjacency and respects existing seams. Fold tests
sample vertices/triangles; they do not certify global injectivity or prevent every
self-intersection. Landmarks are explicit design anchors, not automatic anatomical
recognition. `surfaceProbe` locates the actual compiled surface for placement.

## Rig, weights and corrective shapes

`rigChain` creates 2…16 named joints along explicit pivots under a chosen parent;
`reparent` names children moved under its last joint. Other joint records can
replace existing definitions or add controls without requiring mesh parts. The
whole hierarchy rejects duplicate IDs, cycles and missing parents; it supports
64 authored joints, independently of each draw's 64-entry skin palette.

Spatial skin fields mix up to four named influences into existing bindings.
Normalization and deterministic reduction retain the four strongest influences.
A previously rigid part acquires an explicit binding when edited. Unaffected
vertices retain their original bindings.

Pose correctives are compact displacement/dilation fields activated by a chosen
local authored joint angle. They run before skinning in the ordinary vertex,
mesh and shadow paths. Analytical field Jacobians transform normals. The CPU
inspection path evaluates the same fields; `--check-renderer` numerically compares
that result with the production Metal deformation function.

`posedCorrective` selects a point on the final posed surface and fits a requested
visible-space displacement into an ordinary source corrective. Supply `key`,
`part`, `point`, `offset`, scalar `radius`, explicit angular `driver`/`axis`/
`start`/`end`, and optionally `time` and `maximumProjection`. The response includes
the selected source region, bind center, activation, selection distance and fit
residual. Selection is fixed to one triangle during fitting. A missed surface,
inactive driver, singular inverse or continuously fold-prone translation rejects
the complete transaction. Guide-deformed parts use their guide controls instead.
The fit constrains the selected point; inspect the surrounding surface and other
times before accepting the artistic result.

Rigid skin palettes use dual-quaternion skinning; affine palettes retain the documented fallback. See [DEFORMATION.md](DEFORMATION.md). Correctives are spatial fields, not an
unrestricted dense blendshape editor or muscle simulation. The driver observes
local authored rotation, not a reconstructed post-contact anatomical joint angle.
Affine guide frames preserve cloth stretch/shear; rigid-only skinning would lose
that information. Stress reports include stretch, compressed triangle area and
orientation reversals, but cannot decide whether anatomy looks convincing.

## Choreography and performance editing

A phrase stores poses, contact paths and reference annotations. All pose samples
share a joint set. Offsets use metres, rotations use right-handed XYZ degrees,
world up is Y and forward is -Z. Interpolation is selectable: deliberate eased
holds, linear interpolation or a time-aware cubic spline. Rotations blend on the
quaternion sphere. Spline intermediate keys carry continuous velocity; endpoints
have zero velocity. Scale is bounded to the supported positive range.

An arrangement layers phrases over a registered game clip. Each layer has a
source interval, destination interval, fades, weight and additive/replacement
mode. Arrangements explicitly loop or hold their endpoint. Existing performance
curves remain supported underneath. Game hosts consume the arrangement too.

Contact paths encode hold/liftoff/airborne/landing intervals. A planted interval
cannot move its anchor. Blends between different anchors are reported as moving,
not falsely labeled planted. Contact IK runs after the authored body poses.

`footsteps` compiles sparse footfalls into continuous swing paths, with a specified
minimum number of supporting feet. It rejects overlap on one foot and unintended
simultaneous swings. This count is a scheduling constraint, not a balance proof.

Lumped masses and their bind-space centers supply support diagnostics. Optional
balance assistance moves one selected joint toward the support polygon through
a measured COM Jacobian, with an explicit maximum displacement. Contact IK then
reaches the unchanged foot anchors. An optional transition interval blends the
nearest points on nested support polygons as each foot takes or releases weight.
Overlapping landing/liftoff intervals stay continuous. Degenerate support lines
and points can be inspected and approached; no-support phases receive no static
assistance. Actual foot anchors stay fixed. This is a bounded correction to an
authored pose, not a physical controller or a guarantee of dynamic stability.

`fitPose` fits specified anatomical points to target positions using selected
bounded joint channels. Damped least squares and line search keep each accepted
step improving the residual. Unselected channels remain exact. By default an
unreachable target rejects the edit with its residual; deliberately accepting
an approximation requires `requireConvergence: false`. Fitting preserves the
contact plan; it does not invent footfalls or acting decisions.

Limits: balance assistance is static and kinematic. No ground reaction forces,
inertia-driven body motion, muscle forces, momentum conservation, ragdoll or
whole-body collision solver is implied. Fitting local landmarks does not infer
a skeleton from video. Endpoint hold/spline choices and layer transitions need
visual inspection; an unconstrained artistic spline can overshoot.

## Reference and motion library

- `capturePhrase` samples the current named clip into reusable pose/contact source.
- `retimePhrase` rescales source time and reference markers, preserving active
  arrangement source ranges.
- `mirrorPhrase` reflects X using explicit joint/contact mappings.
- `retargetPhrase` maps joint identities and scales translations/contact positions;
  anatomical differences can then be fitted with bounded landmark targets.
- References record a locator, source time, phrase time, notes and optional measured
  landmark coordinates. They are data, never executable instructions.

This is an explicit reference/retargeting workflow. It does not automatically
extract motion capture from arbitrary footage or resolve radically different
anatomies without authored correspondence and pose fitting.

## Source anatomy and correspondence

`craft.anatomy` replaces an existing named part or creates a new one from ordered
ellipsoid/capsule additions and cuts. Each volume has a stable ID, semantic region,
linear color and normalized joint weights. Internal structures remain queryable
source volumes without changing the exterior. Runtime extraction, material/weight
partitioning and four-influence compilation share this source. Existing sculpt,
weight fields, correctives and seams then operate on the compiled surface.
Optional `skinBlend` (0…1 m, default zero) widens the transition of skin influences
independently of geometric union radii; zero preserves the original partition.
This control requires posed clay review to select the appropriate transition.

The agent client exposes `anatomy`, `anatomical_element`, `anatomy_anchor` and
`rebind_anatomy`. `anatomyAnchors` records primitive coordinates and semantic region
identity instead of transient vertex numbers. A compatible edit updates saved
anchors by bounded projection. Structural changes with saved anchors reject
atomically until the same transaction explicitly supplies a recovery policy;
removed elements require an explicit replacement map. A failed projection cannot
silently jump regions. Accepted craft responses include changed sources, rebind
requirements, preserved-anchor counts and projection residuals. Raw anchor records
are validated against the current mathematical surface on load.

`bindAnatomy` creates an explicit persistent consumer link. `anatomyBindings`
captures the anchor's current position/normal and addresses a `guideChain`,
`guideNode`, `joint`, `landmark`, `skinField`, `corrective` or `surfaceLayer` target.
These are **bind-space source-edit bindings**. They preserve correspondence when
authoritative anatomy changes and the runtime representations are rebuilt.
Guide bindings move the actual secondary rest graph and compiled groom geometry;
registered guide meshes receive the same rest-shape displacement. A whole chain
can optionally follow the surface normal. Rigid joint attachments follow anchor
translation, while localized skin, corrective and finish fields follow the
surface point. Resolving a binding never modifies its original source positions,
so repeated rebuilds do not accumulate drift. The compiler's semantic weights
regenerate skin influences from named volumes independently of topology.

```python
with studio.edit() as edit:
    edit.anatomy_anchor('shoulder-root', 'torso-source', surface_point)
    edit.bind_anatomy('shoulder-mane', 'shoulder-root', 'guideChain', 'shoulder-lock',
                      follow_normal=True)
    edit.bind_anatomy('shoulder-mark', 'shoulder-root', 'surfaceLayer', 'scar-finish')
```

Each target accepts one binding. Binding a whole guide chain and one of its nodes
at the same time rejects. Removed anchors or target controls also reject until
the corresponding bindings are explicitly removed/reassigned. Normal following
changes control positions; it does not rotate axis-aligned finish-field extents
or infer anatomical rig retargeting. During performance, guide roots still follow
their explicitly authored rig joint and secondary graph. A surface anchor does
not copy the body's skin weights, evaluate a deformed surface attachment, or
promise that a root tracks a blended/skinned point across poses. Bind-space
correspondence and posed attachment motion must be reviewed separately.

An anatomy source can list `replaces` output names when a continuous compiled
surface replaces several former batches. The old joints remain available for
skin weights and contact chains, while those legacy meshes are omitted in both
game and editor. No source may omit another authored anatomy output, and duplicate
replacement ownership rejects.

A `fields` asset can author its complete rig, anatomy, `contactChains`, phrases and
arrangement without a registered species generator. Source contact chains name an
existing parent → upper → lower → foot hierarchy and an explicit pole/sole height.
Authored phrase IDs and arrangement clips appear in the native Motion controls,
use the shared pose/contact evaluator and survive saved-study replay. Kinematic
contact solving remains distinct from body dynamics.

`editInterval` edits a selected phrase interval with offset/rotation adjustments,
per-joint delay, fade envelopes, protected times and channel bounds. It preserves
authored contact keys and rejects reversed local time or constraint residuals at
sampled 60 Hz checkpoints. Nonlooping performance controls retain the final frame.

## Groom and costume

A groom design addresses registered or source-authored secondary guide nodes. `guideChains` adds pinned chains under named rig joints; no species-generator edit is needed. It independently
controls fibre count, spread, thickness, clumping, curls, frequency, length
variation, flyaway fraction, seed and root/tip color. Parallel-transported guide
frames avoid world-up switching seams. Fibre variation is deterministic. The
compiled strands inherit two-node guide bindings and use the existing secondary
motion/collision system in both runtime and editor. An optional `envelope` object
with `coverage`, `taper`, `flatten`, `ridge` and optional `layers` (1…4) and `clumps` (1…8) builds
guide-bound clumps with filtered strand coverage and fine silhouette fibres.
Multiple layers use independent strand phases and a shorter opaque inner core.
Clumps add uneven subordinate locks while retaining the same guide palette.
Omitting the envelope preserves the previous
representation. This is a mesh approximation for unresolved inner fibres; it is
not volumetric hair scattering.

Costume seams are geometric stitches projected onto actual triangles of the
host surface. Barycentric skin interpolation makes each stitch follow the host.
Radius, spacing, lift and color are source controls. A missed path is rejected.
Sculpt fields can shape the host before seam projection.

`clothPanels` supplies an authored control grid, per-node joint attachments, pins,
constraint compliance, thickness proxy, color and tessellation. It compiles both
the deforming surface and its secondary graph with surface deformation frames.

Limits: panels have rectangular grid topology. This is not yet a sewing-pattern
simulator, strand self-contact,
hair/cloth cross-contact, cloth triangle CCD or a measured hair scattering model.
Groom envelopes and body contacts still require rendered verification.

## Review and validation

```sh
scripts/stagectl craftReport --samples 25 --part body
scripts/craft-review --samples 25 --views quarter,right --part body --label vesper-craft
scripts/validate-craft
```

Motion reports add liftoff/landing boundaries and adjacent fixed ticks to the
requested review grid, so a brief support loss is not hidden between coarse
samples. Surface audits are explicit work outside the live frame loop.
`craftReport --start 6.8 --end 7.1` restricts the interval; equal endpoints inspect
one pose. Surface audits read the compiled preview mesh, including authored
sculpting and bindings, then evaluate its ordinary runtime deformation.

The review HTML synchronizes real 1080p frames, joint trajectories, support
polygons and contact timelines. Playback uses the actual nonuniform sample times.
A reference selector synchronizes authored local video/images or explicitly
linked media through the phrase-to-arrangement timing map. Optional landmark
correspondences appear in the trajectory view. Media is never downloaded or
executed. Metadata and the exact study travel with it. The
native Craft section exposes phrase capture, support/deformation inspection,
publication and performance review. Motion and Behavior remain native controls.

The native craft validation also creates a source-only field asset with its own
rig/contact clip, adds a bound crest groom and finish layer, exercises compatible
rebind and explicit structural recovery, and compares exact paused/future pixels.
It restores the complete prior study and writes `source-anatomy-validation.json`.

The native craft validation covers a transaction spanning anatomy, rig, weights,
correctives and masses; stale revisions; nested typo rejection; later-operation
failure rollback; undo/redo; motion capture/mirroring/retiming; support scheduling;
groom/seam compilation; and exact paused/future rendering replay. Numerical tests
cover the algorithms independently. These are engineering checks, not an
artistic-quality approval.
