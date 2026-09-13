# Saved-time nature gestures — September 12, 2026

Status: source implementation prepared for the root agent's build, native controls,
motion strips and integration review. No renderer appearance or live performance
acceptance is implied. The authoring agent ran no build or GPU work.

## Production source and ownership

`Games/Sanctuary/Project/NatureMagicPresentation.swift` owns the authored growth
pose, invitation ripple pose, single procedural effect mesh, cached runtime
adapter and Soundstage registrations. `LivingWorldPresentation.swift` applies the
same growth transform to existing grove/reed/flower meshes. Every tree batch gets
the same transform, preserving trunk/foliage attachment. `SanctuaryProject.swift`
registers the two new subjects. The root agent owns the separate
`SanctuaryExperience.swift` argument wiring.

The entry point now receives the saved `expedition.state.age` as `playSeconds`
and the production `sim.localWaterHeight` closure as `waterHeight`. The latter
resolves overlaps with natural water and ecology; it is not reconstructed using
only the newly created garden patch.

Existing `HabitatGarden.Patch.createdAtPlaySeconds` is authoritative. A gesture
event subtracts its saved creation time from saved expedition age in **Double**,
then converts only the short elapsed interval to Float for evaluating poses.
Legacy nil timestamps produce identity growth and no ripple. Invalid or future
timestamps also produce settled presentation; normal save validation already
rejects future timestamps. There is no wall clock, frame counter accumulator,
random generator state or persistence schema added by the animation.

`NatureMagicPresentation.activeEvents(garden:playSeconds:)` returns events in the
first 1,000 milliseconds. Each event exposes `patchID`, `createdAtPlaySeconds`,
`elapsedMilliseconds`, and the exact UInt64 deterministic seed. Its `diagnostics`
dictionary represents the ID/seed as decimal strings to avoid JSON number
precision loss. This is read-only derivation from production state, not another
authoritative event journal.

## Authored response

Plant growth lasts exactly one second including a deterministic 0–120 ms stagger
derived from patch seed and element index. It starts at 0.025 vertical scale and
0.16 lateral scale, grows with zero initial spring velocity, allows at most
1.065 vertical overshoot, and blends to exact settled identity. Lateral scale
never exceeds one. Damped pitch/roll are bounded by seven degrees. The grounded
origin never translates upward; this is a stylized emergence/contact gesture,
not a biological growth or rigid-body solver.

Grove, reed and flower placement RNG consumption is unchanged. Existing sample
positions, yaw and final scale remain the same. The new pose multiplies between
each grounded placement and its original batch transform. Mature and legacy
plants take a direct identity fast path. Existing tree source, shader wind and
shared look are preserved.

Water invitations reuse one 512-triangle torus with a local radius of one metre
and an 8 mm tube. At most four nearby active water patches emit two ring instances
each, so additional production cost is bounded at **one uploaded mesh, eight
render items, 4,096 source triangles**, with no shadow draws for the effect.
The two crests expand with a short stagger, then settle under the supplied water
surface before the one-second event ends. There is no alpha/glow pass or extra
water surface. The ripple radius is capped at 1.4 m; it is a local invitation
gesture even when the authored patch itself is much larger.

Each active patch samples the production water height at its center and four
nearby offsets to orient the effect to a local tangent. Missing/nonfinite center
water suppresses the effect. Missing/nonfinite tangent samples use the center
height. This planar local approximation may intersect strongly curved/sloped
water farther from its center; it is not a new water simulation or shoreline.

No mesh compilation, GPU upload or terrain/world rebuild occurs in these per-frame
pose/effect functions. Native world updates triggered by the underlying authored
terrain/water edit remain owned by the existing production system and should be
measured separately from steady pose cost.

## Soundstage studies

| Subject | Clips | Shared source |
| --- | --- | --- |
| `nature-growth` | `grow`, `settled` | Existing `SanctuaryProceduralCraft.flower()` and the production `growthPose` |
| `nature-water-invitation` | `invite`, `settled` | The production ring mesh and `ripplePose`, over the studio's known zero-height plane |

The 1.35-second studio duration includes a 350 ms settled hold. Studio playback
repeats according to its existing clip controls; the game gesture itself does
not repeat. Rehearsal behavior advances the same event sampling time and settles.
No alternate test-only growth/ripple implementation exists.

Both subjects expose the saved patch ID. Growth also exposes plant index; water
exposes the local invitation radius. The scalar studio patch-ID control is exact
through 16,777,215; production events retain all 64 bits. Native diagnostics expose
the full game seed, and the studio inspection reports four exact 16-bit seed words
plus elapsed milliseconds. Studio controls do not represent every possible
64-bit lifetime patch ID; production replay remains authoritative for those saves.

When a ripple is inactive, production emits no render item. The studio's fixed
part list places an extremely small positive-scale ring beneath the plane, so
transforms remain nonsingular and the clip can seek freely. Both use the same
pose result; studio retention of the tiny hidden part is an adapter limitation.

## Native review requested

1. Select Sanctuary and inspect catalog. Load `nature-growth`, choose softbox,
   pause, select `grow`, and inspect exact times 0, 0.08, 0.20, 0.40, 0.70 and
   1.00 seconds. Preserve a study and motion strip; inspect front and side for
   contact, overshoot and lateral motion. Exercise native Motion controls.
2. Repeat `nature-water-invitation` / `invite` at 0.02, 0.20, 0.45, 0.70 and
   0.95 seconds under noon and an indoor light, from quarter and above. Check
   visibility, thin-crest aliasing and the return below the surface.
3. In an isolated named expedition slot, plant each vegetation kind and invite
   shallow water through production actions. Inspect actual near-ground context
   and existing natural-water overlap. Compare the event's saved timestamp,
   elapsed milliseconds and seed with rendered phase.
4. Save within the first second, reopen/restore at the same saved age, and compare
   a paused capture before advancing. Seek the recording and repeat paused capture
   for deterministic pose/pixels. A legacy nil-timestamp patch must remain settled.
5. Measure a short live interval including the edit and subsequent steady frames;
   preserve the edit/update hitch separately. Check four simultaneous nearby
   water invitations obey the eight-ring cap. No player's default save should be
   used, and no reference should be silently accepted.

Remaining unverified risks: the growth spring may feel too abrupt for large grove
trees; thin PBR rings may be hard to see under the current dark-side lighting;
the local water tangent can fail on pronounced curvature; shader wind and
cosmetic growth do not change the immediate full-size collision representation.
These are candid limitations for review, not reasons to alter physics or exposure.

## Frozen fingerprints

SHA-256 before root/native review:

| File | SHA-256 |
| --- | --- |
| `NatureMagicPresentation.swift` | `4ba55628a8a2828c9550665a51fa689e3a9f68ecc5491e2c5c44ffe4d28db26d` |
| `LivingWorldPresentation.swift` | `2bbc8c7de3a2121dca9fd07ec657aa139b3e19ec452a0b1b75c466a9623463df` |
| `SanctuaryProject.swift` | `c591d978c871d60d5495e52f42845ff5ad94e825aa1ba0f9e403902e12b235f7` |
| `Engine/FieldEngine/Resources/Surface.metal` | `0470de2d0965c2a6b060d2a8536c454ce81f527240a0f42e4133450e0c794975` |
| `Games/Sanctuary/Authoring/Materials.metal` | `1e1f2a798c3ffc3f417397362a8e603e23cde6972afe7963fa49e54870bb202e` |
| `Games/Sanctuary/Authoring/ArtDirection.json` | `2993f512b120c5c878e6791ff17e882c3f339271e8e9983057db3cef260ea799` |

The shader hashes describe the shared team workspace. This implementation does
not edit shader source or project art direction. Actual capture metadata remains
the authority for source/shader fingerprints used by a native study.

## First native review, September 12 at 19:10

Built Soundstage and Sanctuary serially; numerical renderer check passed on Apple
M4 (diffuse0.006604582, deformation3.591767e-7, groom4.3570995e-5). Evidence:
`.build/sanctuary-native-20260912/grass-shadow-renderer-check/report.json`.
Native binary source `b2ad1bf925bf5cbf0d2ba36510ffcf4be9dca8a4c40c1adac8b5871c730b607d`,
shader `c0e2cdc1fddecb595b4bb21a669bbe9cfc8860e89071639be3d281af1679ba33`.
These predate ground-contact and botanical scene changes now in source.

Root inspected real growth side captures at0.08/0.20/0.40s and the native HTML
motion-strip viewer (front/left), including Play sequence. The plant grows from a
fixed ground pivot with a restrained settle. Water rings were inspected at0.40s
quarter/above, disappearance at0.95s, and the native front/left strip. Native Motion
restart, next frame, play and pause worked. Unlike the earlier browser attempt,
Soundstage's own embedded review window opened the HTML and was inspected directly.
No baseline was accepted. Native app session7226 closed normally.

Game production water action followed by12fixed ticks saved at200.0000104ms. Disk
slot reopen preserved decoded Expedition state exactly, as well as patchID1,
seed1428778502690932921 and elapsed animation time. Full image comparison was not
bit exact: only a distant10×2pixel bounding region differed, by at most1/255 in
red/blue; effect/camera/sky-time state matched. Report retains this difference.
Native action picker/Apply undid flowers, then water; third empty undo displayed
its rejection and preserved the exact simulation snapshot.

Reports: `nature-motion-first-review.json`, `nature-water-first-review.json`,
`creek-shadow-nature-review.json`, `native-nature-undo.json` under the native root.
Replay scripts: `run-nature-first-review.py` (initial run races its asynchronous
motion review; wait for `reviewRunning=false` before changing subjects),
`run-creek-nature-review.py`. Source review mutation during an active capture can
be overwritten when the study restores its document; this is a remaining tooling
workflow defect, not silently accepted behavior.

Visual limits: thin rings remain somewhat like outlines on actual invited water;
water patches have plain circular edges. Generic flowers can currently be grown
through shallow water; richer plant adaptation is unfinished. Regional ground is
still excessively bare and flat in color. These gestures are provisional context
integration, not AAA appearance or enjoyment acceptance. Session52719 closed;
no native app left running at this checkpoint.
