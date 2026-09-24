# Creature authoring experiments: executable CPU evidence

Date: 21 September 2026. These experiments evaluate source coordination and bounded numerical behavior. They do **not** establish finished creature quality, real-time budgets, or completion of the creature implementation plan.

The two independent source studies are Ash Warden (quadruped, layered coat, anchored scar and armor) and Reed Penitent (asymmetric biped, pinned cloth patches, mounted charm). Their public recipes and six authored motion scenarios remain inspectable data.

## Reproduce

```sh
bun tools/creature-review.ts --output=output/creature-authoring-cpu-report.json --capture-sources=output/creature-review-sources
bun test ./tools/creature-review.test.ts ./tools/creature-capture-recipes.test.ts ./packages/model/src/creature-fixtures.test.ts ./packages/runtime/src/creature-review.test.ts ./games/creature-study/src/rules.test.ts
```

This command does not launch a browser or touch the GPU. The JSON records exact character and scenario source keys, sample counts, localized diagnostics, raw measurements, unresolved visual evidence, and observational CPU timings. Timings are one local execution, without warm/cold controls, and must not be read as a benchmark distribution.

The numerical example below used character keys `486a0f78cf590f2f` (Ash Warden) and `6a83413911a53695` (Reed Penitent). Source changes require a new report.

## E1: surface correspondence and mounted components

The public authoring session proposes a 25% shoulder-width increase with explicit protected regions, adopts the candidate, and reverses that exact transaction. Ash Warden protects the head and jaw. Reed Penitent protects its two cloth regions after a stricter head-preservation attempt is explicitly rejected. The experiment resolves scar, groom, and armor anchors both before and after the edit. It also prepares the actual mounted field components, rather than checking only semantic labels.

Observed results:

- All chart anchor identities, anatomical regions, revisions, and chart coordinates were retained.
- Ash Warden's mounted pauldron moved 9.61 cm and scar relief moved 9.35 cm. Their component dimensions stayed unchanged.
- The widened pauldron had 3.669 cm clearance against its local chart tangent plane, exceeding its authored 3 cm minimum. Scar relief clearance was 0.049 cm.
- Reed Penitent's charm moved 1.74 cm and retained 0.3 cm local tangent-plane clearance.
- Increasing the shoulder chart topology revision without correspondence repair invalidated its anchors explicitly.
- Adopting and reverting restored the original character source exactly.

The biped exposed an important assumption: its shoulder region owns an off-center neck joint (`x = 0.07 m`). Widening that region moves the neck pivot, so a promise to preserve the dependent head is false. The stricter public operation correctly rejects it. The experiment retains that rejection and names its narrower protected cloth scope. Completing the original head-preserving biped edit requires an explicit anatomical ownership or coordinated-control revision; this runner does not silently weaken the constraint or claim that task completed.

A bounded closest-chart projection of the original world point is retained as a comparison. It searches the same anatomical region; it is not a universal nearest-triangle baseline. Tangent-plane clearance does not certify clearance from every part of the body. Lit scar continuity and armor clearance in motion still require matched captures.

**Decision:** retain chart-based source correspondence and explicit invalidation. Do not replace a failed correspondence with an unrelated nearest surface.

## E2: shape vocabulary and realization

The runner compiles the same authored charts at review quality, records vertices/triangles and preparation time, and reports source patch thickness beside the legacy global field spacing. The biped has explicit thin cloth patches and a front-facing charm mounting patch; these retain explicit surface coordinates.

This is partial numerical evidence. It does not compare matched images of primitive-only and mixed realizations or establish lower authoring effort. A chart vertex existing is not proof that its feature is visible, correctly shaded, or well deformed. Compiler tests separately exercise very thin closed patches and topology ambiguity.

**Decision:** keep sweeps and patches as explicit feature-preserving source, with compiler fidelity diagnostics; defer claims of visual superiority until matched clay and pose review.

## E3: envelope binding, anatomical restrictions, and correctives

The production wolf shoulder chart is correspondence-only: it carries coordinates without adding a second visible skin shell. For this closed-volume experiment only, an explicit measurement clone realizes that chart as a surface. Both binding methods operate on **identical generated chart vertices** with the same compact distance-to-bone envelopes and four-influence linear skinning. The baseline removes anatomical restrictions. The experimental binding applies the authored region/joint rules. An identical torso pose is evaluated with and without motion of unrelated limb roots, exposing how much those limb controls move the shoulder.

| Measurement | Ash Warden | Reed Penitent |
| --- | ---: | ---: |
| Mean disallowed shoulder weight, unrestricted envelope | 0.386 | 0.467 |
| Mean disallowed shoulder weight, anatomical binding | 0 | 0 |
| Maximum unrelated-limb shoulder displacement, compressed pose, unrestricted | 65.98 cm | 19.24 cm |
| Same displacement, anatomical binding | 0 cm | 0 cm |
| Maximum intended corrective displacement | 2.25 cm | 8.32 cm |
| Corrective displacement outside its shoulder region | 0 cm | 0 cm |

The exaggerated diagnostic pose is deliberately useful for exposing cross-region contamination; it is not a claim that all ordinary frames exhibit that displacement.

The runner also records signed-volume ratios for the closed shoulder chart. In the compressed pose, unrestricted/anatomical/corrected ratios were approximately `0.954 / 1.002 / 1.007` for Ash Warden and `0.850 / 1.000 / 1.015` for Reed Penitent. This is a limited chart-volume proxy, not full skin volume or a score of anatomical plausibility. A corrective increasing volume is not inherently better.

All six authored CPU pose/contact scenarios passed their declared tolerances on both sources. Contact reports compare unsolved and constrained joint motion, measure fully planted slip and floor clearance, and retain failures with source IDs/times. They also inspect foot clearance during clips with no active contact intervals. Missing contact solvers and stale anchors are explicit failures.

**Decision:** anatomical restrictions prevent the measured unwanted coupling. Retain local correctives as editable artistic controls. Require clay pose sweeps before accepting the resulting shoulders, jaw, or gait.

## E4: two groom products, nested detail, and a no-groom control

E4 compiles the full character so opt-in groom roots project onto the actual final body within an authored physical distance and source-region scope. Rejected roots remain diagnosed; they are not moved to unrelated limbs. Full compilation timings include body work and can reuse caches.

Ash Warden compiles 392 retained guide roots from the same source to closed opaque tufts and closed opaque ribbons. Root coordinates and canonical guide curves are identical across representations. The body projection rejects 58 guard-coat roots and 14 mane roots outside its declared source scope or distance; localized warnings remain in the report. Hero/gameplay/distant guide sets are nested (392 / 220 / 85), and each product reports a rest-guide envelope and centerline interpolation measurement.

| Product | Hero triangles | Hero bytes | Gameplay triangles | Distant triangles |
| --- | ---: | ---: | ---: | ---: |
| Opaque tufts | 25,088 | 834,176 | 5,280 | 1,020 |
| Opaque ribbons | 12,544 | 432,768 | 3,520 | 680 |
| No groom | 0 | 0 | 0 | 0 |

The ribbon hero's maximum canonical-guide interpolation deviation was about 0.70 cm; distant ribbons reached about 5.02 cm. These are source-curve measurements, not rendered pixel error. The no-groom control deliberately omits the capability and is not a matched-quality performance baseline. Reed Penitent's accepted source has no groom, so its E4 result explicitly reports not applicable.

**Decision:** retain both bounded source realizations for visual comparison. Lower memory and triangle counts do not justify promoting ribbons without silhouette, backlight, shadows, motion, and full-frame GPU measurements. Alpha cards, strand shading, and temporal equivalence are not demonstrated here.

## E5: constrained source editing versus direct edits

The public solver adjusts shoulder width to 120% of its original value and shifts one authored contact target by 8 cm. Protected regions are the wolf's head/jaw or the biped's cloth, for the E1 reason above. Each case converged in five objective evaluations and one iteration, returning two ordinary editable operations. The accepted source was unchanged until adoption.

An exact direct-edit candidate performs the same two operations as a strong baseline. One recorded execution measured approximately 14.3 ms solver versus 8.1 ms direct on Ash Warden and 8.7 ms versus 5.6 ms on Reed Penitent. These observations establish no speedup; the direct task is simple and fully specified.

The contact objective matches a **source target coordinate**. It does not measure or optimize runtime foot slip. Those are separate scenario measurements.

**Decision:** expose bounded solving for coordinated source objectives, preserve direct controls, and avoid claiming an inverse-animation capability from a coordinate solve.

## E6: compiled microdetail

No dense-versus-filtered-versus-response comparison is implemented by this runner. The experiment remains open. Existing appearance fields, anisotropic material controls, or cheaper geometry are not substitutes for E6's moving light/camera/pose error measurements.

## E7: recipes, candidate motion/groom edits, adoption, and undo

For both body plans, the public family recipe sets chest width and jaw depth. The runner verifies that legacy field width and the corresponding chart width agree. It then creates a separate two-operation candidate: an authored head-performance key and a groom edit.

Ash Warden revises an existing coat layer. Reed Penitent reuses the same groom source recipe as a small, sparse collar-fiber study with explicit material, length, density, and budget adaptations. This is a candidate, not a change to the accepted biped design.

Both candidates leave accepted source unchanged before adoption, alter the intended motion/groom domains when adopted, and undo back to the exact original source. E1 separately exercises proportion adoption and transaction reversal.

**Decision:** source reuse and revision-safe experimentation are working. This short script does not measure human/agent active time, correction cycles, or labor saved over creating finished art.

## Form-first capture recipes

`createCreatureCaptureRecipe(id, "clay" | "skeleton")` produces the same accepted project and source key with presentation-only inspection settings:

- `channel: "clay"` uses the renderer's clay response.
- `hideGroom: true` hides groom draw geometry without deleting its source or changing skin displacement, appearance masks, binding, correctives, or cloth state.
- Skeleton inspection adds the `rig` overlay to that identical posed clay body.

There are front, side, back, and three-quarter cameras plus walk, coil, airborne lunge, and hit pose timings. These are pending capture manifests, not images. The source is neither restyled nor reconstructed to make the review pass.

The current form revision removes the solid segmented crest, blends cheek/brow masses into the underlying body, replaces capsule ears with tapered pinnae and inset membranes, shortens and clusters toes/claws, and authors a four-joint digitigrade hind chain. Its mane uses a dedicated dorsal correspondence patch and bounded outward projection onto the actual torso. These source changes address recorded clay defects; numerical tests alone do not establish their visual success. Camera, pose tick, geometry revision, and overlays must match when comparing revisions.

The neutral source stage uses its directional key plus a real camera-side point fill at `[3, 3, 5]`, intensity 26. The previous second directional source was not an independent fill in the current environment realization. This lighting correction changes project capture keys but leaves character geometry unchanged; clay and beauty must use the same corrected stage.

Review order is clay silhouette/anatomy, visible skeleton and contacts, deformation poses, material separation, and then coat/secondary motion. A poor clay creature should return to source edits before additional appearance work.

## Playable encounter evidence

`CreatureEncounterGame` is a separate, bounded flat-arena game module. It drives actual `RuntimeSession` motion, facing, movement targets, and physical root motion. Player movement, edge-triggered strikes, attack range and cooldown, dodge startup/invulnerability/recovery, readable creature anticipation, committed damage windows, punishable recovery, hit reactions, health, win/loss, and semantic events are deterministic at 60 Hz.

Tests include a real CPU physics/runtime integration through attack and recovery, checking that body-height offsets are respected and completed root motion does not snap back on clip changes. Idle players can lose, timed dodges avoid damage, and a close-range attack policy can win. This verifies rules and integration; whether the encounter feels good is still established by playing it.

## Remaining acceptance work

Matched clay/skeleton and moving visual evidence, comparative art judgment, actual encounter play, exported-runtime visuals, lower-power hardware, and full-frame GPU/memory budgets remain required. No experiment in this document grants an AAA-quality score or upgrades a pending visual gate to passed.
