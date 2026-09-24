# Creature development runbook

Build the creature in clay, prove its skeleton and movement, then develop its surfaces. Keep clay review available throughout production. Materials, fur, lighting, and camera composition must not conceal defects in the underlying creature.

This is the working process for humans and agents using Wrela. It complements the [implementation plan](architecture/creature-authoring-plan.md) and the [public authoring operations](../packages/authoring/CREATURES.md). A gate records evidence and a design judgment; passing numerical tests alone does not pass an art gate. Agents can continue reversible experiments independently, but must not describe an unreviewed result as approved.

## 1. Define what success looks like

Write a short brief before generating geometry:

- Role in the game, physical size, expected camera distance, and movement vocabulary.
- Three defining silhouette features and the emotional impression they should convey.
- Body plan: support limbs, manipulation limbs, face, spine, tail, wings, or other appendages.
- Intended departures from real anatomy, and how each is meant to function.
- Protected features, allowed asymmetry, likely close-ups, and expensive details worth preserving.
- A small encounter that will test the design: what the player reads, avoids, and responds to.

Choose neutral review cameras and lighting once. Include front, side, rear, three-quarter, face, and important joints. Save physical scale, ground plane, exposure, viewport size, compiler quality, and source revision with every comparison. Start with fixed targets for contact error and attachment clearance appropriate to this creature. Performance targets need a named device and scene; they are not promises until measured.

**Exit:** the creature has a recognizable purpose and a falsifiable review brief. A collection of attractive surface adjectives is insufficient.

## 2. Block out silhouette and skeleton together

Use a small number of primary masses, joint centers, limb segments, and swept forms. Work in opaque neutral clay, without coat, surface patterns, bump, emission, or wet highlights. Use a separate silhouette view. Keep the ground and a scale reference visible.

Place the skeleton while defining these forms. The bones should explain the posture and articulation: where the shoulder moves, where the jaw hinges, how the foot supports weight, and where a tail can bend. For a fantastic creature, write down the intended mechanism instead of forcing human anatomy onto it.

Inspect both solid clay and a skeleton overlay. An overlay reveals rig placement; solid clay reveals whether the rig is being hidden inside an implausible volume. Review both sides of intentional asymmetry. Turn the creature rather than relying on a single attractive angle.

**Exit:** silhouette, proportions, support, major joint locations, and distinctive features read at the gameplay camera. Record the clay views and the anatomical decisions. Do not spend time on pores, scales, individual hairs, or polished materials yet.

## 3. Prove articulation before detailing

Make a short pose laboratory: neutral stance, crouch, extension, twist, jaw opening, head turn, compression, and an extreme but supported pose for every important chain. Test nearby limbs and overlapping anatomy; automatic weights must not let one limb pull its neighbor.

Inspect joint motion in clay with the skeleton visible, then hide the skeleton and look again. Check shoulder and hip collapse, changing limb volume, elbow and knee folds, jaw/cheek separation, floating eyes or teeth, and stretched attachments. Test combinations, not only one joint at a time.

Fix defects at the appropriate level:

| Observation | First place to inspect |
| --- | --- |
| Wrong silhouette even in rest pose | Authored mass, sweep, patch, proportions |
| Bending at the wrong location | Joint placement and hierarchy |
| Another limb follows this joint | Anatomical influence rules and exclusions |
| Pinching only in a particular pose | Binding distribution, then a local corrective field |
| Scar or accessory drifts after an edit | Chart, anchor revision, attachment rule |
| Tiny feature vanishes or becomes a lump | Compiler realization and feature sampling |

Do not repair a source-shape defect by changing the lighting. Do not add a corrective for a misplaced joint until the placement has been considered. Preserve deliberate exceptions explicitly.

**Exit:** the declared pose range works in clay. Unresolved poses have localized diagnostics and a bounded supported range; they are not silently accepted.

## 4. Make the clay creature perform

Author idle, attention, walk, run, start, stop, turn, anticipation, strike, recovery, hit response, and the required end state. Begin with readable poses and timing. Add contact tracks and adaptation after the performance communicates intent.

Review the same movement at real speed and at selected fixed ticks. Show root trajectory, contact targets, joint positions, and contact error alongside the clay capture. Exercise flat ground, slope, a step, target offsets, interruptions, and direction changes where the creature is expected to operate.

Use IK to adapt a coherent gait. If every stride requires large corrective movement, reconsider stride length, phase, root travel, and pelvis motion. Separate intentional sliding, airborne motion, stance, and impact in the authored contacts. A mathematically planted foot does not establish convincing weight.

Play the simple encounter in clay. Judge attack readability, range, commitment, recovery, player response, and apparent mass. Keep damage rules in the game and editable performance tracks in the creature.

**Exit:** motion and the encounter read without fur, texture, dramatic lighting, or camera shake. Contact and reach residuals are within declared tolerances; any intentional violations are identified.

## 5. Add secondary anatomy and local precision

Develop ears, eyelids, lips, nostrils, claws, teeth, creases, scars, and membranes after the primary motion works. Keep medium-scale anatomy distinct from microdetail. Use explicit sweeps, patches, and bounded local strokes where a global field cannot preserve a feature efficiently.

Review facial controls in clay: gaze, blink, jaw, brow, lip movement, and combinations that matter to the character. Check wet boundaries and exposed oral surfaces geometrically before shading them. Unsupported controls should remain visible as missing work.

For every small defining feature, compare its source dimensions with the compiled result. A compiler warning about lost features is a defect to resolve or explicitly accept. Increasing resolution everywhere is not the default repair.

**Exit:** the body and exposed face remain compelling in clay at close range, and defining features survive compilation and motion.

## 6. Develop material response in isolation

Start with broad material regions and restrained color. Evaluate skin, eyes, horn, metal, cloth, and fibers under neutral, raking, and back lighting with fixed exposure. Inspect albedo, roughness, normals, thickness approximations, and orientation where the implementation supports them.

Add deliberate variation that follows anatomy and history: growth direction, wear near contact, exposed scar tissue, or regional roughness. One scar recipe should coordinate its shape, color, roughness, and growth suppression through the same anchor.

Check that colors are not multiplied twice by source material and generated vertex color. Check that procedural detail stays attached during skinning and origin changes. Describe approximations honestly: a thickness-aware lighting response does not imply a full subsurface simulation or refractive eye.

**Exit:** material families remain distinguishable in several lights, and disabling them exposes the same accepted anatomy. Retain a matched clay capture with the beauty capture.

## 7. Grow and shape the coat

Author large flow and volume before density: partings, mane crest, cheek tufts, guard hairs, short coat, sparse regions, and deliberate asymmetry. Inspect the body with the groom hidden, the groom against the body in clay, and the shaded result.

Use the coat to express the design, but do not use it to conceal a broken shoulder or an unfinished face. A creature may intentionally have a coat-defined silhouette; record that choice and still verify its underlying articulation.

Check roots through proportion edits, pose corrections, scars, and chart changes. Inspect highlights, backlight, shadow coverage, and camera motion. Compare runtime detail variants at matched projected sizes and during transitions. Triangle count alone does not measure preserved coat volume or appearance.

**Exit:** the coat follows its source and movement, strengthens the intended silhouette, and remains stable at declared distances. Known coverage changes or transition pops are recorded with evidence.

## 8. Add physical response with clear ownership

Add ears, tail, guides, accessories, cloth, and articulated reactions incrementally. Test one subsystem in isolation before combining them. Authored performance remains responsible for readable gameplay timing; simulation has a declared scope.

Exercise acceleration, abrupt turn, impact, wind where supported, pause/resume, teleport, dormancy, save/reload, and replay. Check pinned cloth, fixed groom roots, collision clearance, joint limits, and the transition into and out of ragdoll. Record constraint residuals when the requested constraints conflict.

**Exit:** secondary movement supports the performance, stays bounded, and survives lifecycle operations. No simulation is considered correct solely because it moves.

## 9. Stress the authoring system

Perform a deliberate cross-system edit through public commands: widen the shoulder, shorten a limb, alter a mane, relocate a scar, or revise attack timing. Protect accepted regions and scenes. Propose the change as a candidate before adoption.

Compare the same cameras, lighting, poses, and ticks. Inspect anchors, skinning, contact adaptation, attachments, generated materials, guide placement, and cost. Reject stale candidates; record unsatisfied constraints rather than relaxing them invisibly. Undo and reproduce the accepted result.

Repeat representative edits on a different body plan. A workflow that requires hidden fixture edits or manual repair of every dependent system has exposed a tool defect.

**Exit:** the creature can be revised predictably. Save reusable source recipes, successful controls, rejected alternatives, and reasons.

## 10. Review delivery and play

Review the cooked creature in the exported runtime, including shadows, identity/picking, detail transitions, physical response, and save/replay where supported. Measure the complete scene on declared hardware with one close hero, an encounter, and the intended actor count. Record frame distributions, preparation time, owned memory, and missing-product diagnostics.

Play the encounter again with finished surfaces. Compare it with the clay version: visual polish should improve readability and character, not obscure timing. Return to earlier gates whenever a later feature exposes an anatomical or motion defect.

**Exit:** a named reviewer records why the creature looks convincing and why the encounter works; technical evidence identifies exactly what was exercised. Neither a beauty image nor a green test suite establishes AAA quality alone.

## The repeatable agent loop

1. Inspect the source revision, current gate, brief, protected features, and relevant diagnostics.
2. State one observable defect or intended improvement and locate its source controls.
3. Make a bounded candidate with explicit read/write dependencies and preservation constraints.
4. Compile and run the cheapest useful numerical checks.
5. Compare matched clay, skeleton, silhouette, motion, and beauty evidence appropriate to the change.
6. Adopt or reject with a reason; retain source identity and evidence. Reopen affected gates explicitly.

Parallel tasks carry allowed regions/controls, dependencies, expected evidence, and protected decisions. Shared anatomy changes require coordination even when agents edit different files. Material, groom, and motion agents may explore early, but exploratory outputs do not bypass the earlier clay gates.

## Review record

For each gate retain: creature/source key; operator/compiler version; scenario and fixed ticks; cameras, lights, exposure and viewport; enabled diagnostics and overlays; measurements with units; capture paths; observed defects; decision and reviewer; accepted exceptions; and what invalidates the decision. Distinguish `not run`, `failed`, `passed numerical checks`, and `visually accepted`.

## Current entry points

The [clay iteration implementation and evidence](research/creature-clay-iteration.md) records the precision-field, local sampling, rendered-pixel repair, full-clip audit, and notebook tools, including open art defects in the current study.

Open the **Creature** panel in Studio and load **Ash Warden** or **Reed Penitent** to exercise the source tools. Use candidate proposal/adoption for design changes and **Play encounter** for the isolated interaction test. Study fixtures are development specimens, not approved finished art.

The shared agent interface is `window.wrela.creature`; use `window.wrela.discover()` to inspect the installed operations and capture options. Existing preview capture supports silhouette and rig overlays. Use the diagnostic modes reported by discovery; this document does not imply an unsupported rendering mode exists.

Run CPU source/anchor/contact experiments with:

```sh
bun tools/creature-review.ts --output=output/creature-review.json
```

Run a finite hardware development capture with:

```sh
bun tools/creature-smoke.ts
bun tools/creature-smoke.ts --biped --motion=walk --time=0.5
bun tools/creature-smoke.ts --mode=clay --hide-groom --skeleton
```

The smoke uses a small no-water scene and the shared GPU lease. It is not a performance certification. Keep the source stable during retained evidence captures; a source change invalidates the capture manifest. Coordinate shared-machine GPU work with other sessions.

For repeatable clay comparisons, use:

```sh
bun run creature:atlas
bun run creature:atlas --project=output/candidate.json --baseline=output/baseline.json
bun run creature:atlas --quick
```

The atlas writes an offline comparison page, PNGs, and `atlas.json`. It hides groom and declared mounted components and keeps front, side, three-quarter, face, shoulder, and feet cameras fixed. The full run adds six attack samples, three worst isolated-rig measurements, and three worst actual-runtime contact frames. The runtime audit samples the entire clip at 60 Hz, including physical root motion, feet, and contact residuals. Inspect both the numerical record and the sequence: a solver-only pose check missed the doubled landing target caught by this workflow. These finite captures do not replace real-speed playback, terrain tests, or a skin-intersection review.

When a small form is missing, first inspect its source ownership. Author an oriented/curved local sculpt and an explicit local sampling target. For a bounded correction grounded from a visible pixel, call `creature.repairPixel`, preview the compiled candidate at the same camera and across poses, and adopt only after review. Keep the fit residual and the image judgment separate. The detailed input contracts are in the operation reference.

Record each defect in a creature notebook before repairing it. Retain the original capture and source key; append a resolution or rejection with matched evidence rather than overwriting the observation. A later source edit reopens review. The checked-in [Ash Warden notebook](research/creature-clay-notebook.json) demonstrates resolved narrow defects alongside open anatomy and movement gates.

`bun tools/creature-benchmark.ts` records CPU simulation/extraction distributions for one and four actors of each body plan. Its report explicitly excludes rendering; use a full hardware scene benchmark before accepting a frame budget.
