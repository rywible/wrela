# Vesper creature craft delivery — 2026-09-11

The five capability areas now have shared source representations, agent operations,
compiler/runtime integration and native authoring validation. Vesper exercises
them in a published asset and a twelve-second performance. This is a substantial
authoring-tool delivery, not certification of AAA artwork or a complete production
character pipeline.

## Implemented and exercised

| Area | Reusable tools | Vesper use |
| --- | --- | --- |
| Anatomy | Persistent spatial brushes, mirrored strokes, landmarks, conforming refinement, coverage and fold rejection | Brow/cheek shaping and refined body |
| Rig and deformation | Additional/replacement joints, spatial four-weight fields, pose-driven correctives, bounded landmark fitting, deformation audits | Three-joint spine, reparented neck, body weights, neck corrective and fitted head pose |
| Whole-body choreography | Pose/contact phrases, spline interpolation, layered arrangements, sparse footstep planning, mass/support diagnostics and bounded static support assistance | Gather, coil, release and sequential recovery footfalls |
| Reference and motion editing | Capture, retime, mirror, explicit joint-map retargeting, reference annotations, synchronized video/image review, trails and contact timelines | Captured original phrase, authored twelve-second arrangement and comparison with previous renderer footage |
| Groom and costume | Pinned guide chains, cloth control grids, procedural clumping/curl/variation, projected skin-following stitches | Mane, beard, extra beard chains and mantle seams; arbitrary cloth panels exercised in the isolated native fixture |

All edits use revision-checked atomic transactions with strict schemas and budgets.
Rejected edits preserve the prior source. Source, undo, checkpoints, paused replay,
file watching and the game runtime share the implementation. The Python interface
provides semantic operations rather than requiring whole-document replacement.
See [interface contract](../CREATURE_CRAFT.md) and
[friction ledger](../CREATURE_CRAFT_PLAN.md).

The published source is `Games/Sanctuary/Authoring/Assets/vesper.json` (669,144 bytes).
The delivery checkpoint is `.soundstage/projects/sanctuary/checkpoints/vesper-craft-delivery.json`;
the resumable study is `.soundstage/studies/vesper-craft-delivery.json`.

## Actual renderer evidence

These are local generated artifacts, not concept illustrations:

- [Interactive close-up review](../../.soundstage/studies/vesper-craft-close-20260911-212343-e51fa6/index.html):
  158 quarter/side captures, event-aware motion diagnostics, source and metadata.
- [Complete performance viewer](../../.soundstage/studies/vesper-craft-performance-20260911-213402-1fdf64/index.html)
  and [movie](../../.soundstage/studies/vesper-craft-performance-20260911-213402-1fdf64/performance.mp4):
  twelve seconds, 30 fps, 360 actual 1920×1080 frames.
- [Matched appearance comparison](../../.soundstage/studies/vesper-craft-lighting-20260911-213007-ed9840/index.html):
  front, quarter, back and above under seven environmental conditions plus softbox.
- [Wet-material review](../../.soundstage/studies/vesper-craft-wet-20260911-213149-341e8d/index.html)
  and [shared style board](../../.soundstage/studies/style-board-20260911-213311-08398b/index.html).
- [Final paused capture](../../.soundstage/captures/vesper-craft-delivery-5c38f787.png)
  and [metadata](../../.soundstage/captures/vesper-craft-delivery-5c38f787.json).
- [Sanctuary integration capture](../../.build/vesper-craft-integration-20260911-214910/garden/runtime/captures/vesper-craft-garden-40952241.png).

The native Craft audit, Motion frame/restart controls and Behavior scenario were
exercised. The visitor reached Performing with attention 1.0 after ten seconds.
The review was inspected in native WKWebView, including contact seeking, enlarged
frames and reference-video seeking at 9.600 seconds. The complete film was opened
in the native viewer and representative frames were inspected directly. Sanctuary
was inspected in its actual native window with the same published source.

Final capture source fingerprint:
`fe21840cb86b905e5a64a8e7c5bbc6ac395c1778b0c9d02fcadeaa64421f32bd`.
Film source fingerprint:
`2877838a0ad83ad1d64e54ccd13b951c5315e234ac881cf8bd31549cfb44c50e`.
The source changed afterward for status publication and duration metadata; the
rendered creature source and shaders stayed the same. Both shader fingerprints:
`df84a130b1f97320a694ffab2acb158f7e1d6979c993a390a06a7cea4280d1dd`.
Full-detail creature geometry is 591,540 triangles.

## Engineering validation

- [Quick suite](../../.build/test-runs/20260911-212424-quick-1cc2d/index.html):
  105 Swift tests, harness contracts, architecture boundaries and both game smoke scenarios passed.
- [Final native craft suite](../../.build/test-runs/20260911-215127-authoring-99d04/index.html):
  31 checks passed, including transactions, rejection, fitting, guides/cloth,
  cached-source equality, arrangement duration, undo and exact paused/future pixels.
- Existing [workshop](../../.build/test-runs/20260911-212809-authoring-c3e0c/index.html),
  [authoring](../../.build/test-runs/20260911-214449-authoring-2bee9/index.html),
  [creatures](../../.build/test-runs/20260911-212829-authoring-1b270/index.html) and
  [dynamics](../../.build/test-runs/20260911-212840-authoring-a1b42/index.html) suites passed.
- [GPU harness](../../.build/test-runs/20260911-212807-gpu-check-09d0e/index.html) passed.
  The separate production renderer check measured maximum CPU/GPU creature
  deformation disagreement of 2.5981063e-7; see
  [renderer log](../../.build/vesper-craft-integration-20260911-214910/renderer.log).
- [Isolated integration](../../.build/vesper-craft-integration-20260911-214910/result.json):
  seven checks passed, including rich source loading, real file-watcher edits,
  malformed edit rejection, exact restoration and game execution without GPU errors.
  The player's default save was preserved.

The event-aware motion audit reduced peak sampled joint acceleration from
440.46 to 34.07 m/s² after repairing discontinuous support transfer. The dense
121-grid-plus-contact-events audit measured peak speed 2.642 m/s and maximum
outside-support distance 4.83 mm. The body deformation review measured maximum
edge stretch 1.2245× and minimum triangle area ratio 0.6915×, with no reversed
triangles in the sampled poses. These are sampled engineering diagnostics;
they do not establish continuous collision safety or convincing acting.

## Short live performance checks

Measured at native 1920×1080 on Apple M4 with ordinary live updates included.
Developer-triggered capture/rebuild work is separate. Overlapping GPU stage
intervals are not summed.

| Session | Duration | Submitted fps | GPU median / p95 / maximum | Frame interval p95 / maximum |
| --- | --- | --- | --- | --- |
| Soundstage, normal counters-off mode | 12.02 s | 59.50 | 5.47 / 12.64 / 17.78 ms | 22.52 / 40.43 ms |
| Sanctuary, live clouds and wind, counters enabled | 12.59 s | 59.96 | 14.45 / 15.11 / 17.04 ms | 18.70 / 35.71 ms |

Reports: [Soundstage](../../.soundstage/profiles/vesper-craft-final-20260911-215255.json),
[Sanctuary](../../.build/vesper-craft-integration-20260911-214910/garden/runtime/profiles/vesper-craft-garden-20260911-214940.json).
Sanctuary presented 60.04 fps. Its atmosphere CPU update maximum was 0.65 ms;
whole-frame GPU maxima during air, lighting and sky update phases were 15.77,
15.42 and 17.04 ms respectively. Frame hitches remain. These short observations
do not establish sustained locked 60 fps or crowd performance.

Authoring exposed full-source JSON status serialization on the main thread as
friction: an earlier short editor sample submitted 49.92 fps. Source caching,
background serialization, coalesced 2 Hz full heartbeats and 10 Hz command polling
removed that avoidable work from the UI thread. The protocol still returns full
source and acknowledges captures only after their files are durable. Timing
variation between samples prevents attributing every difference to this change.

## Remaining artistic and tool friction

Vesper is still a stylized prototype. The mask is puffy, the limbs are simple
tubes, the mane often reads as thin wire, and the costume ornament lacks a
convincing construction hierarchy. The movement has authored progression and
better contacts, but it is a small performance vocabulary rather than a complete
combat creature. Some outdoor indirect-light views are dark across the shared
scene; no creature-specific exposure compensation was applied.

The implemented support assistance is bounded kinematic correction against a
lumped mass model, not force-based dynamic balance. Guide/cloth constraints are
not a complete garment simulator. Explicit-map motion transfer is not
anatomy-aware retargeting, and reference review does not reconstruct motion from video.

Remaining capabilities include topology-changing sculpt/remeshing, muscle and
flesh deformation, richer surface/material authoring, anatomy-aware motion
transfer, cloth pattern sewing, surface continuous collision, hair self-contact,
measured hair scattering, and skinned LOD/crowd optimization. These limitations
are recorded so future work addresses real friction rather than treating an API
name or passing test as proof of world-class quality.
