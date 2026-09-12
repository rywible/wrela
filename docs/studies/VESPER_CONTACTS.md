# Vesper: contact envelopes and surface deformation

11 September 2026. This iteration fixes demonstrated authoring blind spots in hair
and cloth. It does not establish Elden Ring quality. The mask, limbs, feet and
material language remain stylized; the motion repertoire and anatomical deformation
still need substantial work.

## What the tools now make possible

Physical guide spans can collide with moving body capsules, with linearly tapered
contact radii. Bend constraints and cloth diagonals remain separate from physical
contact edges. Closest clearance for a tapered span is a convex one-dimensional
minimization, bounded by twenty bisections; equal radii use the finite-segment
solution. This is spatial contact at substeps, not temporal CCD.

One-coefficient positional friction opposes tangential movement relative to a
moving capsule's surface. Capsule translation and axis rotation contribute surface
motion; spin about the capsule axis is not represented. Ground response uses a
local normal but retains vertical clearance and sampled height-field contact.
It is approximate on sharp steps. The formulation is informed by section 6.1 of
[Unified Particle Physics](https://matthias-research.github.io/pages/publications/flex.pdf).

`guideRadii` and `guideFrames` are atomic source edits. They reuse compiled geometry,
participate in undo/study replay, and apply through the same player in the game.
Surface frames fit a local affine differential to guide neighbours, retaining
stretch and shear in the cloth tangent plane and normal thickness. Degenerate
or singular patches fall back to rotation. They do not certify a fold-free cage.
Adjacency is cached per source, and rest lengths are computed once per physics tick.

`secondaryReport` separately reports particle, span and pinned penetration, with
offending guide IDs. Pins are diagnosed, never silently moved. The native Dynamics
panel exposes span contact, friction and these measurements; penetrating spans
appear magenta in guide inspection.

`surfaceContacts` checks every compiled vertex after the renderer's four-weight
skinning against registered capsules and the rehearsal height field. It returns
the part, vertex, position, body proxy, depth and influencing guides. The default
selection includes every secondary-skinned batch; a named part can be inspected
explicitly. One sample inspects the current pose; 2–121 scan the registered clip.
The native surface-audit button and four-view viewer expose the same diagnostics.
These audits are explicit operations, outside the live frame loop.

## What changed in the creature

Vesper has 135 guide nodes, with 133 authored contact envelopes and 55 surface
frames for the cloak and its trim. The torso proxies now better cover the visible
upper body. The cloak's resting crown and mane roots were sculpted through guide
offsets. Friction is 0.4, shape compliance 0.15, with four substeps and eight
iterations at 60 Hz. Its existing surface materials and performance curves were
preserved.

A dense audit exposed a second problem after guide contact was working: the cloth
between clear controls could fold inward under rigid guide rotations. Surface
frames corrected that interpolation. The returned skin influences then identified
the remaining front-edge patch, allowing a targeted envelope correction instead
of inflating every cloth control.

The [matched motion review](../../.soundstage/studies/vesper-contacts-motion-20260911-184120-4ff90d/index.html)
contains 72 actual 1080p captures: before/after, nine times and four views. Both
variants were evaluated with the same current implementation and their own saved
source. The old variant retains its smaller torso proxies; the new variant uses
the larger fitted proxies.

| Diagnostic | Before source | Current source |
|---|---:|---:|
| Maximum guide-span penetration, 61 poses | 82.615 mm | 0.172 mm |
| Maximum dense surface/proxy penetration, 61 poses | 346.171 mm | 0 mm |
| Maximum structural strain | 1.96% | 2.53% |

A separate [121-pose dense audit](../../.soundstage/studies/vesper-contacts-final-20260911-184750-b7499e/surface-audit.json)
checks 143,995 vertices per pose across mane, beard, mantle and trim; no vertex
penetration was found. The [guide audit](../../.soundstage/studies/vesper-contacts-final-20260911-184750-b7499e/guides-audit.json)
reports zero pinned penetration and 14.32% maximum strain in the deliberately softer
bend/shear links, distinct from the 2.53% structural strain. The troublesome phases
were also inspected at 18.2/19.4, 30.2/31.4 and 42.2/43.4 seconds; their dense audits
are saved beside the movie and found no proxy penetration.

This does **not** prove triangle-interior clearance, self-collision, hair/cloth
clearance, whole-body dynamics, volume preservation or continuous-time collision.
No solver pushes the kinematic body back. Vesper's garden specimen remains a
rendering rehearsal rather than a combat NPC or a player-blocking actor.

## Visual review and reproduction

- [12-second, 30 fps recording](../../.soundstage/studies/vesper-contacts-final-20260911-184750-b7499e/performance.mp4), made from 360 exact-time 1920×1080 renderer captures. It is not a live benchmark.
- [Lighting review](../../.soundstage/studies/vesper-contacts-lighting-20260911-184406-899d97/index.html): front, quarter, back and above under noon, golden hour, sunset, afterglow, overcast, rain, indoor and softbox lighting.
- [Shared style board](../../.soundstage/studies/style-board-20260911-184552-2d1087/index.html), including the provisional tree, stone, seed and sky references.
- [Frozen current review](../../.soundstage/studies/authoring-20260911-185137-f2d1a5/index.html), pinned as **Vesper contact and surface reference**. The named checkpoint is **Vesper contact and surface review**.
- [Replay study](../../.soundstage/studies/vesper-contact-final.json).

The native review viewers were opened and exercised, including before/after at the
bow and the full lighting contact sheet. Motion stepping/playback, the Dynamics
controls and surface audit, and the calm/running visitor Behavior controls were
exercised. The movie was played in QuickTime. The published asset was reloaded and
inspected in an isolated Sanctuary session, preserving the player's default save.

Visible cloak holes during the bow are absent in the reviewed current images.
The remaining visual weaknesses include simple limb/foot anatomy, a rounded mask,
brush-like fibre clumps, sparse movement vocabulary and weak readability in darker
outdoor lighting. Material exposure was not adjusted per object to disguise that
shared-lighting weakness.

The frozen capture records source digest
`6601ba195dd5dd1d5149b300071848b7f880ac4a34c2920bb52bc89a14f8ad2d`
and shader digest
`83c3daddfd8c6715bc62605f10950b2285156316d49f966a450fb64a152ecbdc`.
Each archived image has its study and renderer metadata.

## Validation and short live measurements

[Quick validation](../../.build/test-runs/20260911-183649-quick-acd2d/index.html)
passed all 87 unit tests and the registered headless scenarios. The new tests cover
span contact with clear endpoints, tapered clearance against a dense reference,
friction on static/moving surfaces, pinned diagnostics, legacy decoding, dense skin
contact, affine stretch/shear and singular-frame fallback. Module boundaries and
whitespace checks passed.

Eight isolated native suites passed: dynamics, workshop, authoring, creatures,
creature-tools, performance, Cave and expedition. Dynamics additionally verifies
atomic rejection, geometry reuse, exact undo images, reverse replay, saved future
replay and the dense 121-pose clearance check. These checks validate reproducibility
and the stated cases, not a AAA-quality certificate.

| 20-second live run, native 1080p | Studio | Sanctuary specimen |
|---|---:|---:|
| Submitted frames/second | 60.00 | 60.08 |
| GPU median / p95 | 11.23 / 14.50 ms | 14.36 / 15.02 ms |
| CPU simulation median / p95 | 4.15 / 4.95 ms | 4.34 / 4.77 ms |
| Maximum frame interval | 29.04 ms | 17.68 ms |

[Studio profile](../../.soundstage/profiles/vesper-contacts-live-20260911-184951.json)
and [Sanctuary profile](../../.build/vesper-integration-20260911-185211/runtime/profiles/vesper-contacts-garden-20260911-185327.json)
include normal live updates. Both reported thermal state 0 and no GPU errors.
Submitted FPS is distinct from presentation timing. The studio's occasional longer
frame intervals remain visible in the report. Cold source rebuilds, seeking and
explicit dense audits can cause separate authoring hitches; these profiles measure
ordinary playback after the standard three-second warm-up. No long thermal soak
was performed. The previous session's broader desktop/GPU slowdown did not recur
in this short garden run; this does not establish its cause or a general performance
improvement attributable to these tools.

- [dynamics](../../.build/test-runs/20260911-183925-authoring-1ef73/index.html)

- [workshop](../../.build/test-runs/20260911-183946-authoring-e6133/index.html)

- [authoring](../../.build/test-runs/20260911-183954-authoring-1546a/index.html)

- [creatures](../../.build/test-runs/20260911-184006-authoring-a96d8/index.html)

- [creature-tools](../../.build/test-runs/20260911-184017-authoring-2a527/index.html)

- [performance](../../.build/test-runs/20260911-184026-authoring-a4cf6/index.html)

- [cave](../../.build/test-runs/20260911-184036-host-84a84/index.html)

- [expedition](../../.build/test-runs/20260911-184045-host-b65b4/index.html)

## Final collinear-contact correction

Final review found a degeneracy when tapered guide and capsule axes overlap on a
line: returning the first zero-distance point could miss the wider end. The solver
now uses the segment tangent cone to choose the appropriate one-sided derivative.
New assertions cover full overlap, partial overlap and reversed taper. All eight
focused secondary-motion tests and the
[isolated dynamics suite](../../.build/test-runs/20260911-185926-authoring-8678b/index.html)
passed again, and all three app bundles were rebuilt.

The [final frozen-frame comparison](../../.soundstage/studies/vesper-contact-robustness-20260911-190034-7d3c13/index.html)
is byte-identical to the earlier captured frame: PNG SHA-256
`fcaf46da1af826c186f493082b8fc24b73dd52151353a8763ca579533d881b0b`.
The final build's source digest is
`a3762bcaaa298284d39a18838efe286366c7bda3066db56bc9fb1d3aba6e72d4`;
its shader digest is unchanged. Both complete 121-pose diagnostic reports were
also rerun and are unchanged, archived beside that comparison. The movie and live
timings above were recorded before this final degeneracy correction.
