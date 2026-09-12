# Single-agent creature workshop

The working objective is an original ceremonial dancing beast, developed by one
agent using the same source, renderer and pose functions in Soundstage and games.
Its brief: an aged ivory lion mask, sweeping horns, a long serpentine torso under
an embroidered mantle, a flowing mane and deliberate ritual movement. Stillness,
weight transfer and a delayed head/body relationship precede a sweeping attack.

## Implementation journal

- Existing foundation: project-owned Swift recipes, native semantic controls,
  atomic source replacement, undo, checkpoints, frozen render baselines, exact
  motion replay and behavior scenarios.
- Implemented reusable procedural surface construction, four-influence GPU skinning,
  deterministic authoring curves and editable performance scores.
- Creature source stays in Sanctuary's project. It is an authoring specimen;
  publishing it does not automatically place an enemy in the player's sanctuary.
- Runtime meshes and weights are derived from the source. No generated artwork
  is used as renderer evidence.

## Research decisions

[UCSD Skin chapter](https://cseweb.ucsd.edu/classes/wi20/cse169-a/readings/3-Skin.html)
explains bind-space weighted skinning and its deformation limitations. We use
four normalized influences and bind-space delta matrices. Linear blend skinning
is an explicit approximation; it is not muscle simulation or a volume-preserving
solver. Longitudinal weights are authored analytically for this swept body.

[CMU inverse-kinematics exercise](https://www.cs.cmu.edu/~cga/humanoids-ugrad-15/ass2/)
separates target placement from trajectories. Contact targets and authored timing
will be inspected together rather than mistaking endpoint accuracy for acting.

[Official visual reference](https://www.youtube.com/watch?v=qLZenOn7WUo): Shadow
of the Erdtree reveal trailer. The creature here is original; ceremonial mass,
mask-led motion and serpentine follow-through are the direction, not copied assets.

## Acceptance

Build and tests, shader ABI/render checks, deterministic score replay and atomic
rejection, native Parts/Motion/Behavior inspection, front/quarter/back/above under
all standard rigs and weather, wet/dry, a front/side motion strip and a short live
profile. Check Sanctuary after renderer changes. Report actual limits and defects.

## What is usable now

Select Sanctuary → Vesper in Soundstage. The Object tab edits the mask, horns,
mane, mantle and continuous limb volume. Parts exposes semantic anatomy. Motion offers idle and the full
procession, amplitude, tempo and secondary motion. Performance provides eight
named beats, exact seeking, a skeleton view, additive joint corrections and
source saving. Behavior includes rehearsal, visitor and startle scenarios.
Everything is available through the native interface and the local agent protocol.

The authoring loop is source → compiled mesh and weights → actual renderer →
exact-time review → local correction → saved source. An agent can change the
Swift generator as well as semantic values and performance keys. New geometry
and anatomy therefore do not require a catalogue of shapes built into the editor.
The game owns the recipe, material snippets and acting; the engine owns generic
geometry, skinning and rendering. The existing field compiler remains available
for carved and joined forms, alongside sampled procedural surfaces.

`stagectl performance` returns joint anchors, named beats, skinned surfaces and
generator contact diagnostics. `performanceKey` edits a joint channel at a time;
`performanceBeat` seeks an authored beat. `scripts/performance-film` records a
movie from exact-time GPU captures, with source and shader metadata per frame.
This movie is review evidence, never a live frame-rate benchmark.

`gardenctl specimen vesper` places a temporary animated specimen in front of the
player using the same asset source and pose evaluator. `specimen none` removes
it. It is an integration preview and does not change the expedition save.

## Boundaries of this version

Vesper is a stylized procedural ceremonial beast, not an Elden Ring fidelity
match. Its mane now uses simulated guide grooms, but the continuous skinned limbs still
have simple anatomy. Guide contact protects sampled particles against body capsules
and the ground; it does not certify the visible hair or cloth surfaces.
The twelve-second phrase has deliberate holds, four steps, a rising coil, a
mask-led sweep and a recovery. It is not a complete combat or locomotion set.

Contact correction now runs after additive animation, with individual terrain
samples in Sanctuary. Rehearsal exposes flat, sloped and stepped surfaces, actual
contact residuals, planted-foot slip and sampled capsule overlap. This is a
kinematic constraint plus collision diagnostics, without mass, force, friction,
balance, continuous collision detection or collision response. Rendered review
remains necessary: proxy clearance does not certify the visible mesh. There is
no damage, combat AI, muscle simulation, topology sculptor, skin-weight painting, automatic
motion synthesis or automatic artistic acceptance. Skinned surfaces currently
use full detail and conservative visibility, so a crowd needs further work.

This is the working authoring foundation and an end-to-end original specimen.
Achieving a production creature next requires finer anatomy and deformation,
richer secondary contact, locomotion and transitions, combat timing,
and continued visual iteration with the same review tools.

See [Vesper study](studies/VESPER.md) for measured results and retained evidence.

## Second iteration: precise edits and physical rehearsal

`stagectl authoring` returns a revision and complete editable source. `author`
accepts an atomic revision-checked transaction spanning local surface fields,
performance curves, contact offsets and collider overrides. `surfaceProbe`
locates a region on the actual compiled bind surface. A compact edit can move
and reshape the mask while carrying its eyes through the same field. Normals
follow the deformation Jacobian; weights and vertex ordering remain intact.
Missed brushes and sampled folds fail before the document changes. This is
local deformation, not topology editing or a global self-intersection proof.

The compiler retains base geometry; the authoring renderer rebuilds only draw
batches affected by an edit. One measured mask/eye revision reused 20 batches
and rebuilt two in 61 ms. This was a warm local edit, not a cold full build.
Motion keys can express arrival and departure velocity, with correct tangent
rescaling when tempo changes. Continuous weighted limbs replace the previous
separate upper/lower visual segments.

`poseReport` samples the final shared pose without changing the working scene.
`creature-review` captures synchronized front, quarter, side and back views with
an exact-time slider in the native review window. The Rehearsal tab exposes the
same contact tools to the human author. Saved studies include the fixture and
solver state for replay; fixtures are not published as part of the creature.

[XPBD](https://matthias-research.github.io/pages/publications/XPBD.pdf) informs
the next dynamics step: compliant constraints need explicit simulation state,
iterations and physical calibration. This second iteration established the contact foundation; the third iteration
below now implements XPBD distance constraints. Balance and locomotion transitions
remain separate work.

See [second-iteration study](studies/VESPER_TOOLS.md) for current evidence.

## Third iteration: guides, dynamics and surface finishing

The single-agent loop can now author three independent layers: geometry and
anatomy, acting and contact targets, and secondary response/material finish.
`SecondaryRig` registers a graph of stable guide IDs, joint attachments, masses,
contact radii, distance constraints and artistic attraction. `GuideGroom` compiles
curved, varied-length fibres and weights from guide curves. `guideOffsets` reuse
those weights to sculpt the resting groom or fabric and rebuild only affected
batches. Vesper currently exercises 64 mane, 16 beard and 55 fabric controls.

`SecondaryMotion` implements XPBD distance constraints, vector shape attraction,
velocity damping, gravity and deterministic procedural wind. Particles and opt-in physical guide spans project out of moving body capsules.
Ground contact samples the height field with a local normal; it retains vertical
clearance and is approximate on sharp terrain. Positional friction limits relative
tangential displacement using one coefficient and the normal correction. The instance-owned player settles
for two seconds, advances fixed 60 Hz ticks with configurable substeps, and caches
replay checkpoints. Reverse scrubbing and saved-study future playback reproduce
exact GPU images. Cloth trim and fabric share the same guide deformation. Guide
neighbourhoods orient the skin transforms; `guideFrames` can instead fit local
affine surface differentials for cloth. Tangent stretch and shear are retained,
with unit normal thickness and a rotation fallback on degenerate patches. The
renderer still uses four-weight linear blend skinning. `guideRadii` authors
tapered contact envelopes without changing compiled geometry.

The **Dynamics** section and `secondary` command share settings. `secondaryReport`
returns structural strain, softer bend strain, speed, displacement, penetration
and offending guide IDs. The four-view review includes these measurements. `surfaceContacts` also checks
every dense compiled vertex after skinning, identifies its body proxy, and returns
the worst vertex's influencing guides. This audit runs explicitly, outside the live
frame loop; it includes intended attachment overlap and excludes triangle interiors
and shader displacement.
Reports separately expose moving particles, physical spans and pinned controls.
Pinned violations remain visible because a solver cannot move an authored pin.
This is one-way secondary simulation: no swept CCD, cloth triangles, hair
self-collision, body reaction, volume preservation or physical twist. Capsule
translation and axis rotation contribute surface motion; spin about the axis
is not represented. A guide contact pass is not a collision certificate for the
rendered surface. The fixed
settling period is not a periodic loop bake. Runtime replay assumes its pose
sampler can reconstruct the requested history; Vesper's time-driven clip can.

`surfaceLayers` adds ordered, compact bind-space finish fields. Tint, roughness,
metallic and fine shading relief can vary locally without changing topology, UVs
or geometry buffers. Uniform, mottled, contour and grain patterns are filtered in
the fragment shader. These are authored material fields, not geometry engraving,
curvature-aware wear or a measured hair scattering model. The shared palette
supports 64 guide transforms per draw batch.

The contact sampler now pulls ground into moving upright actor frames. Translation,
yaw and positive uniform scale are supported; arbitrary tilted/nonuniform roots
are outside the height-field contract. Contact reports express trajectories in
rehearsal coordinates so planted-speed measurements include root motion.

The implementation follows [XPBD](https://matthias-research.github.io/pages/publications/XPBD.pdf)
and the substep rationale in [Small Steps](https://matthias-research.github.io/pages/publications/smallsteps.pdf).
The positional friction model is informed by section 6.1 of
[Unified Particle Physics](https://matthias-research.github.io/pages/publications/flex.pdf),
using one coefficient for both sticking and sliding bounds.
These papers describe the solver principles, not the quality of this creature.


## Third iteration: reusable creature craft

The earlier boundaries above describe the preceding tool generation. The new
[craft workspace](CREATURE_CRAFT.md) adds spatial sculpt brushes/refinement,
independent rig joints, editable weight fields and corrective shapes, reusable
pose/contact arrangements, bounded landmark fitting, support-transfer assistance,
reference playback, source-authored guide chains/cloth panels and attached seams.
These are shared tools, exercised on Vesper without adding species switches.
The [friction ledger](CREATURE_CRAFT_PLAN.md) records what authoring exposed and
which implementation changes addressed it. They do not turn Vesper into AAA art
or supply force-based whole-body physics, topology remeshing or a complete combat set.
