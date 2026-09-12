# Creature craft implementation and friction ledger

Requested outcome: five reusable, agent-native capabilities for anatomy sculpting,
rig/deformation, whole-body choreography, reference/motion editing, and groom/costume
authoring. Vesper is the workshop specimen. Source belongs to assets; compiled
geometry and runtime buffers are derived. This document tracks implementation and
evidence, not a declaration of production readiness.

## Delivery gates

1. Sculpt: persistent strokes, anatomical landmarks, symmetry, local refinement,
   coverage/fold diagnostics, stable skin propagation, atomic edit/replay.
2. Rig: source-authored additional joints, spatial skin weights, pose-driven
   corrective fields, stress-pose deformation diagnostics, same game renderer.
3. Choreography: authored pose phrases and contact trajectories, layered timing,
   support/mass diagnostics, explicit limits of kinematic assistance.
4. Motion: reusable phrases, retiming, mirroring/retargeting, reference annotations,
   synchronized review and motion trails; preserve contacts across transitions.
5. Groom/costume: source-authored guide groups, clumping/curl/length variation,
   surface-attached seams/trim, deterministic generation and deformation.

Each gate needs rejection tests, actual native operation, preserved studies and a
Vesper use case. Existing content must load unchanged; partial implementation is
recorded as partial. Tests cannot certify artistic quality.

## Friction encountered

| Friction | Consequence | Implementation response | Evidence/status |
| --- | --- | --- | --- |
| Whole source replacement for a local artistic decision | Repeated JSON surgery and fragile IDs | Revision-checked semantic operations with discovery and localized reports | Implemented; native atomic rollback, stale revision and undo/replay checks pass |
| Compact translation fields cannot carve or flatten forms | Anatomy remains primitive | Persistent spatial brush strokes with symmetric counterparts and mesh refinement | Implemented; coverage/fold rejection and symmetry/refinement tests pass |
| Rig joints tied to generated mesh parts | Flexible spines require recipe surgery | Additional/replacement joints in authored source | Implemented; Vesper has a new three-joint spine and reparented neck |
| Four influence weights fixed by recipe | Joint collapse difficult to repair | Spatial weight editing plus pose-driven corrective fields | Implemented; Vesper body uses two weight fields and a neck-driven corrective |
| One scalar score over a fixed clip | Choreography cannot be composed/reused | Pose/contact phrase library and layered arrangement | Implemented; captured reference plus authored pose/contact phrase and arrangement |
| Contact diagnostics omit mass and performance arcs | Grounded feet can accompany weightless acting | Sampled support, speed/acceleration and reference diagnostics | Implemented; contact-event audit exposed and drove support-transfer repairs |
| Groom style compiled into one generator | Species-specific changes and repetitive fibres | Shared source-driven groom/trim compilation | Implemented; Vesper mane/beard and seams use shared compilers; arbitrary guide/cloth fixture passes |

## Research

- [Kavan et al., dual quaternion skinning](https://users.cs.utah.edu/~ladislav/dq/index.html):
  useful context for rigid twist collapse; affine guide frames require preserving
  their scale/shear, so a blanket rigid skinning replacement is inappropriate.

## Remaining quality bar

Vesper is visibly a prototype. Anatomical form, deformation, weight transfer,
choreographic range, groom hierarchy and costume construction all need authored
evidence. No claim of AAA quality or complete physical simulation is implied.

## Observed during Vesper authoring

- The first recovery score lifted three paws at once. The sampled support report
  exposed a 1.77 m distance from the remaining support point. Added a sparse
  footstep planner with an explicit minimum-support gate.
- A 25-frame review missed that short interval. Reports now include contact
  boundaries and neighboring 60 Hz ticks in addition to the requested grid.
- Ease-to-zero at every captured pose interrupted flow. Added time-aware position
  and quaternion splines plus continuous airborne foot trajectories.
- Craft initially missed the editor's mutation history whitelist. Native inspection
  exposed the stale Undo label; added registration and a whole-document undo test.
- Complete pose/contact source exceeded the original tiny field-recipe assumptions.
  Asset reads now allow 4 MiB, with a separate 16,384-joint-pose budget.
- Repeated source hashing would scale with the richer motion library. Cache the
  source revision until the next source assignment; keep replay keys exact.
- Raw record construction was cumbersome. Added a Python transaction interface with
  landmark-based sculpting, weight/corrective, chain, mass, phrase, groom and seam
  operations. It never retries a stale revision automatically.
- Point placement still required several coordinated joint edits. Added bounded
  landmark fitting with explicit degrees of freedom and residual rejection.

## Delivery engineering evidence

- Final quick suite: **105 Swift tests**, harness contracts, both games' smoke
  scenarios and architecture boundaries passed:
  `.build/test-runs/20260911-212424-quick-1cc2d/index.html`.
- Expanded native craft transaction/replay suite passed:
  `.build/test-runs/20260911-215127-authoring-99d04/index.html` (31 checks).
  It includes guide/cloth authoring, bounded fitting, huge-number rejection,
  nested schema rejection, geometry reuse and exact paused/future pixels.
- Production GPU deformation vs CPU inspection maximum error was 2.60e-7 in
  the shader verification. Final renderer, legacy suites, actual-render artifacts
  and short live measurements are recorded in the completed
  [delivery study](studies/VESPER_CRAFT.md).
- Native Craft audit, Motion frame/restart controls and Behavior visitor scenario
  were exercised. The visitor reached Performing / attention 1.0 after ten seconds.
- Actual-render comparison viewer was opened in native WKWebView. Contact seeking,
  enlarged frames and previous-video seeking at 9.600 seconds were inspected.
- Camera-preserving quarter/side review:
  `.soundstage/studies/vesper-craft-close-20260911-212343-e51fa6/index.html`.
  158 real frames, source, per-frame renderer metadata and event diagnostics.

## Additional friction resolved during delivery

- Overlapping landing/liftoff ramps made the earlier support projection jump when
  the active set changed. Nested support polygons now blend their nearest points
  continuously. A regression test checks the overlap boundary and fixed-step path.
  Vesper's event audit dropped from 440.46 to 34.07 m/s² peak joint acceleration;
  this is an engineering observation, not a dynamic stability or artistic score.
- A constrained head target was 6.69 mm outside the selected rotational freedoms.
  Rejection preserved the source. Adding a bounded body-height freedom allowed
  the landmark fit to converge without editing unrelated channels.
- A richer library exceeded old file-watcher size assumptions. Shared decoding
  now enforces the same 4 MiB limit for direct loading and live source reload.
- Groom shaping still required generator-owned guides. Source-authored pinned
  chains and cloth control grids now add their graph and geometry together.
- Equal-width event cells distorted visible timing and produced hundreds of tiny
  controls. Contact tracks now group intervals in real time; playback follows
  nonuniform timestamps and click-to-enlarge preserves close inspection.
- Changing review views reset a carefully framed camera. Creature reviews now
  retain the authored distance while changing projection.
- Mirrored reference landmarks retained old joint identities. Explicit mappings
  now transform both identities and positions, with collision rejection.

- Full-document status generation became a frame-time bottleneck as the motion
  library grew. Cache the immutable source separately from changing study fields;
  serialize heartbeats and acknowledgements off the UI thread, coalesce stale
  heartbeats, and publish the complete heartbeat at 2 Hz while polling commands
  at 10 Hz. Shutdown drains writes before its stopped marker. The full protocol
  and durable capture acknowledgement remain intact.
- Existing timeline metadata used a generator duration even when an arrangement
  supplied a different duration. Metadata now follows the authored performance;
  a native tempo assertion checks that the interface follows it.
- The 669,144-byte published source was loaded, edited by the real file watcher,
  rejected when malformed, and restored with identical pixels in an isolated
  session. Artifacts: `.build/vesper-craft-integration-20260911-214910`.

## Remaining limits, explicitly not certified

Vesper still has simple limb anatomy, a puffy mask, thin geometric hair, limited
costume construction and a small acting vocabulary. The tools materially improve
what can be authored and inspected; they do not certify an Elden Ring result.
Topology-changing sculpt/remeshing, muscle simulation, automatic video reconstruction,
anatomy-aware motion transfer, cloth pattern sewing, surface CCD, hair self-contact,
measured hair scattering, skinned LOD/crowd optimization and complete combat motion
remain outside this implemented contract. These are real future friction, not
features hidden behind names in the interface. Runtime measurements and visual
observations belong in the delivery study, including failures and remaining defects.
