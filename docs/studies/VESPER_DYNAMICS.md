# Vesper: guide dynamics and material authorship

Working iteration, 11 September 2026. This improves the reusable authoring stack;
it does not establish an Elden Ring fidelity match. Vesper still has simplified
anatomy, an obviously procedural face and a single ceremonial phrase rather than
a production locomotion/combat set.

## Tools exercised

- Registered guide graphs, deterministic XPBD distance constraints, gravity,
  damping, artistic shape attraction and one-way capsule/ground projection.
- Instance-owned fixed-step replay with a two-second settling preroll, reverse
  seeking and cached checkpoints. Gameplay and Soundstage use the same player.
- Curved guide grooms with varied strand length, curl, spread and density. Stable
  guide IDs can be sculpted through source transactions without editing Swift.
- Continuous four-control surface bindings and differential normal offsets for
  cloth decorations. Sweep profiles allow noncircular limbs and ridged horns.
- Ordered local tint, roughness, metallic and shading-relief fields; no UVs or
  geometry rebuild. Recipes, effects and finish fields remain authored source.
- Native Dynamics controls, guide/strain display, machine-readable diagnostics,
  revision-checked source transactions and synchronized motion review.
- Ground queries in moving upright actor frames; translated/yawed/uniformly scaled
  contact frames and rehearsal-space contact trajectories.

Vesper uses 135 guide particles: 64 for the mane, 16 for the beard, 55 for fabric.
The geometric groom has 768 mane fibres and 192 beard fibres, with independently
varied lengths and curved cross sections. It is opaque geometry with conventional
surface shading, not a physically measured hair scattering model. Cloth and trim
use the same continuous grid field. The first front medallion can still be partly
occluded near the shoulder; attachments are approximations, not exact deformed
triangle constraints.

The initial dynamics pass failed visibly: lower mane roots passed through the
neck, and differently moving spine pins overconstrained the cloth. Reauthoring
the root arc and retaining two central fabric pins reduced maximum sampled
structural strain from roughly 45% to 1.77%. This is why the rendered review and
the offending-guide diagnostics belong in the same workflow.

## Evidence collected

- [Four-view motion review](../../.soundstage/studies/vesper-dynamics-final-motion-20260911-173718-e6e573/index.html):
  36 actual captures, nine times, four synchronized views. Native slider/Next
  controls were exercised and the 7.3-second sweep inspected at full resolution.
- [Close quarter view](../../.soundstage/captures/vesper3-closednose-7c200481.png).
- [Shared quick checks](../../.build/test-runs/20260911-172109-quick-a28f9/index.html):
  81 Swift tests, harness policy checks, module boundaries and both games' smoke
  scenarios passed.
- [Rendered dynamics/finish validation](../../.build/test-runs/20260911-171410-authoring-a5c4c/index.html):
  guide edit, material edit, rejection, undo, rewind and saved future image checks
  passed before the final surface-binding refinement. The final source was then
  checked again in the runs below.

The working phrase sampled at 61 times has 1.77% maximum structural strain,
2.57% maximum softer bend strain, 0.759 m maximum displacement from authored
guides and effectively zero particle penetration (0.00015 mm numerical residual).
The separate contact report samples 121 times with no observed planted slip,
reach residual or body-proxy overlap. These are discrete sampled observations.

A warm 17-field material edit reused all 22 draw batches in 16.6 ms; a combined
mask/eye/groom sculpt reused 19 and rebuilt three in 109.8 ms. These are developer
editing costs, not steady rendering times. The compiled recipe changes take a
build/restart; source material and guide changes do not.

## Physical and visual limits

The dynamics solver uses XPBD distance constraints and vector shape attraction,
followed by hard particle projection. Contact is one-way. There is no cloth
triangle contact, hair self-collision, friction, continuous collision detection,
body reaction, torque simulation, physical twisting, muscle simulation or balance
controller. A particle outside a capsule does not imply that every intervening
fibre or triangle is outside the rendered body. Guide frames approximate normals
with linear blend skinning. A static preroll is not a periodic loop bake.

Runtime replay needs a reconstructible pose history. Vesper's time-driven clip
satisfies this; a changing gameplay brain or moving placement needs an appropriate
history adapter before using it as a general simulation replay contract. Height
fields assume upright actors with yaw and positive uniform scale; they are not
arbitrary oriented collision surfaces. The skin palette now allows 64 transforms
per draw, not an unlimited skeleton or automatic crowd LOD system.

Surface relief affects shading only. It cannot carve silhouettes or change depth
and collision. The finish fields are authored patterns rather than curvature-
aware physical weathering. All colors go through the shared scene look and the
existing renderer's authored-RGB conversion; no per-object exposure adjustment
was introduced.

Research: [XPBD](https://matthias-research.github.io/pages/publications/XPBD.pdf)
and [Small Steps](https://matthias-research.github.io/pages/publications/smallsteps.pdf).
These support the solver design, not an artistic quality claim.

## Final material sweep

[Dry](../../.soundstage/studies/vesper-dynamics-final-dry-20260911-173515-b253bd/index.html)
and [wet](../../.soundstage/studies/vesper-dynamics-final-wet-20260911-173608-412be3/index.html)
reviews retain four views under noon, golden hour, sunset, afterglow, overcast,
rain, room light and softbox light. The native all-views board and enlarged image
controls were exercised. A thin nasal patch produced depth artifacts and was
replaced with a closed form. The indoor sweep initially paired indoor shading
with an outdoor fixture; the review script now preserves the room fixture.
The earlier dry/wet archives with that mismatch are superseded by these reports.
Golden hour, overcast and rain remain very dark under the shared outdoor look;
this is an unresolved scene-lighting readability issue, not an approved look.
The mask remains stylized, paws simplified, and the mane can read as a stiff
curtain in motion despite the guide simulation.

## Final verification

All nine isolated native suites passed on the final source:

- [validate-dynamics](../../.build/test-runs/20260911-174228-authoring-77ddc/index.html) — passed.
- [validate-creature-tools](../../.build/test-runs/20260911-174237-authoring-5aded/index.html) — passed.
- [validate-performance](../../.build/test-runs/20260911-174245-authoring-12d39/index.html) — passed.
- [validate-creatures](../../.build/test-runs/20260911-174253-authoring-2bd01/index.html) — passed.
- [validate-workshop](../../.build/test-runs/20260911-174305-authoring-3c68d/index.html) — passed.
- [validate-authoring](../../.build/test-runs/20260911-174313-authoring-d0621/index.html) — passed.
- [validate-review](../../.build/test-runs/20260911-174327-authoring-ee8af/index.html) — passed.
- [validate-cave](../../.build/test-runs/20260911-174334-host-9faaa/index.html) — passed.
- [validate-expedition](../../.build/test-runs/20260911-174346-host-a66ec/index.html) — passed.

The native Motion controls were stepped and played; Behavior was restarted and
advanced with both calm and running visitors. Dynamics enable/disable and guide
inspection were exercised earlier in this iteration. The review regression now
captures an actual indoor study, verifies its room rig, and checks exact document
restoration. No default player save was used by these tests.

[Final movie](../../.soundstage/studies/vesper-dynamics-final-20260911-173757-985065/performance.mp4):
12 seconds, 360 actual 1920×1080 captures, 30 fps playback. Opened and played to
completion in QuickTime. This is an offline capture, not evidence of live frame rate.

[Studio live profile](../../.soundstage/profiles/vesper-dynamics-studio-20260911-174006.json):
20.4 seconds, 59.6 submitted fps, 4.29 ms median / 6.11 ms p95 GPU time,
2.23 ms median / 3.56 ms p95 CPU simulation. Frame intervals had 18.53 ms p95,
32.34 ms p99 and 35.65 ms maximum. Thermal state was nominal. Presentation
rate is unavailable from the OS, so this is not a locked-60 presentation claim.

[Shared style board](../../.soundstage/studies/style-board-20260911-174043-4711d8/index.html)
was opened natively; noon and sunset candidate/reference/sky images were inspected.
The references are provisional and visibly simpler than the intended fidelity target.

## Renderer specialization and unresolved garden budget

Finish fields now have dedicated vertex, mesh and grass shader variants. Ordinary
scene shaders compile that machinery out with a function constant. All candidate
pipelines must compile before hot reload installs them. The final renderer check
passed on Apple M4 (diffuse irradiance maximum numerical error 0.006604582).
The dynamics check now renders and reverses a material edit through both the mesh
and ordinary vertex paths.

[Matched shader comparison](../../.soundstage/studies/vesper-specialized-verify-20260911-175652-d6c34e/index.html)
was inspected in the native split viewer. Its before/after PNGs have identical
SHA-256 hashes (`b0a99a2c5a46c896c4cc20973296a9aaf022b5f68eac9241b5171d83f510f3dd`).
The new variants preserve those frozen pixels. The film and full lighting sweep
above precede this specialization; their source and shader fingerprints remain
recorded rather than being relabeled as captures from the new shader.

These targeted native suites passed again after specialization:

- [validate-dynamics](../../.build/test-runs/20260911-175520-authoring-81aa8/index.html) — passed.
- [validate-workshop](../../.build/test-runs/20260911-175531-authoring-6bd69/index.html) — passed.
- [validate-authoring](../../.build/test-runs/20260911-175541-authoring-431e5/index.html) — passed.
- [validate-cave](../../.build/test-runs/20260911-175556-host-9f587/index.html) — passed.
- [validate-expedition](../../.build/test-runs/20260911-175608-host-12de9/index.html) — passed.

The final [garden integration capture](../../.build/vesper-integration-20260911-174920/runtime/captures/vesper-garden-420dd0d5.png)
uses published source after `reloadAssets` in an isolated save/workspace. Native
movement was exercised in the preceding integration run. The specimen is a
rendered rehearsal in the game; it is not a complete combat NPC, and its authoring
collision proxies do not establish player blocking or combat collision behavior.

Live garden results **fail the 60 Hz target**. These short measurements include
cloud and wind updates; GPU stage intervals overlap and are not summed:

- [Initial new-tools run](../../.build/vesper-integration-20260911-174417/runtime/profiles/vesper-dynamics-garden-20260911-174552.json):
  36.8 submitted fps, 44.65 ms median / 58.77 ms p95 GPU, 68.34 ms maximum frame interval.
- [Specialized shader run](../../.build/vesper-integration-20260911-174920/runtime/profiles/vesper-specialized-garden-20260911-175002.json):
  47.4 submitted fps, 32.89 ms median / 41.70 ms p95 GPU, 44.48 ms maximum frame interval.
- [Same session without Vesper](../../.build/vesper-integration-20260911-174920/runtime/profiles/specialized-garden-without-vesper-20260911-175209.json):
  41.9 submitted fps, 36.15 ms median / 47.69 ms p95 GPU.
- [Pre-authoring shader control](../../.build/vesper-integration-20260911-174920/runtime/profiles/pre-authoring-shader-comparison-20260911-175335.json):
  41.3 submitted fps, 38.80 ms median / 51.86 ms p95 GPU, without Vesper. The checked-in
  HEAD shader was temporarily loaded for this scene-only comparison; the complete
  edited shader was then restored byte-for-byte and reloaded successfully.
- [After releasing inspection bindings](../../.build/vesper-integration-20260911-174920/runtime/profiles/vesper-inspection-released-20260911-175440.json):
  38.7 submitted fps, 39.76 ms median / 54.05 ms p95 GPU, 68.59 ms maximum frame interval.

[Stage counters](../../.build/vesper-integration-20260911-174920/runtime/profiles/vesper-stage-costs-20260911-175020.json)
show substantial scene and shadow intervals as well as live sky work. The earlier
roughly-59-fps garden result from the previous iteration did not reproduce, even
with the pre-authoring shader and no specimen. Desktop compositor and capture
service CPU activity were high; thermal state varied between nominal and fair.
These observations do not prove the cause of the slowdown or a causal speedup
from specialization. A clean comparison remains necessary. No rendering updates,
resolution or quality settings were disabled to hide this result.

Current working source is saved in `Games/Sanctuary/Authoring/Assets/vesper.json`.
The checkpoint is **Vesper dynamics review** and the frozen pixel reference is
**Vesper guides and finish reference**. These record a working prototype, not an
artistic approval. [Current close capture](../../.soundstage/captures/vesper-dynamics-current-d0f816c4.png)
is from the final shader. The Elden Ring fidelity objective remains incomplete.
