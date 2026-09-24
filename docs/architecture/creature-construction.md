# Anatomical construction and patterned garments

`applyBipedAnatomy(character, recipe)` builds supported semantic biped anatomy around existing rig pivots. Build, shoulder breadth, pelvis breadth, head scale and crown reach control coherent volumes rather than independently stretching bones. Overlapping deltoid, trapezius, pectoral, knee and calf forms feed the existing smooth field composition. Hands and branching crown use explicit tapered surface charts so narrow digits and tips survive the field sampling grid. The shoulder chart remains available for correspondence but no longer adds a second visible round torso. Existing source is cloned, and landmarks, attachment frames, joint locations and contacts remain stable.

`applyBipedWalk(character, recipe)` bakes two-bone foot arcs, alternating lateral weight transfer, double-support hip lowering, arm counter-swing and upper-body counter-rotation into ordinary editable keys. Duration, stride, support fraction, foot lift and weight shift are bounded recipe inputs. Persistent contact windows and footstep events use the same phase boundaries. The source bake uses an explicit forward knee pole; it does not rely on a nearly straight chain choosing an arbitrary bend. Contact review still runs the complete runtime, including physical root motion.

`authorCreatureMantle(character, recipe)` emits normal authoring transactions. Shoulder width, back depth, length, flare, collar offset and pattern-gore count define one curved shoulder-to-hem surface. Its four fitted corners remain anatomical landmarks; a smooth `controlOffsets` grid stores the interior shape relative to those corners. Geometry, anchors and the physical cloth lattice evaluate that same surface. Editing fitted corners changes the base surface without losing the interior drape. Region proportion edits scale the offsets with the corners.

The mantle owns one 289-particle cloth lattice. Collar pins and UV-based shoulder stays share a dedicated supporting-joint frame, so motion cannot pull independent gores apart. The softly gathered skirt and curved hem hang below this shoulder section. Full stiffness and 32 bounded constraint iterations keep the source's 3.5% extension allowance satisfied in the reviewed walking samples. Anatomical pelvis, abdomen, chest, neck and head collision proxies replace the original tall capsule: its 250 mm radius intersected the 125 mm fitted collar and caused severe pin/collision conflicts. Collision remains enabled.

The old six-panel construction is preserved in the rejected capture at `output/authoring-lookdev/1790115555797-30193/source/output/character-lookdev/v1/`. It showed a box-shaped back, fractured yoke and visible vertical gaps during walking. The continuous patch addresses that observed failure; a subsequent render is still required for visual acceptance.

The shared alpine Warden remains a quadruped. `createCharacterLookdev({ bodyMass, chestDepth, legStrength })` provides a separate recipe around its existing four-legged rig. The shared scene receives the default tucked torso, stronger limb taper, connected girdles and removal of redundant torso surface automatically. Its anchors and groom correspondence remain available; the biped joint recipe is never applied to the quadruped.

## Reproduction and held-out variants

Run `bun tools/character-lookdev.ts --fitting --variant=pilgrim --revision=v2`. Repeat with `broad-keeper` and `long-mantle`, using distinct revision directories. These variants change anatomical build, garment pattern dimensions and walk timing together through the same construction functions; they are not individual hand-edited meshes. The tool saves thirteen hardware-rendered views including clay anatomy with garments hidden, front/back silhouettes, garment closeups and left/right support poses, plus authored source and a real-runtime `motion-review.json`.

CPU review before visual acceptance measured:

| Recipe | Interactive triangles | Maximum planted horizontal slip | Maximum contact residual |
| --- | ---: | ---: | ---: |
| Pilgrim | 25,680 | 4.79 mm | 0.445 mm |
| Broad keeper | 26,964 | 3.36 mm | 0.272 mm |
| Long mantle | 25,140 | 3.95 mm | 0.370 mm |

All three completed the full clothed runtime walk without unavailable or conflicting contact diagnostics. Across 175 walking ticks, the continuous mantle had zero strain-limit residual and zero pin error for every variant; final sampled extension ratios were 1.0137, 1.0155 and 1.0142 against the authored 1.035 allowance. Tests cover unchanged source and pivots, distinct anatomical output, valid compiled variants, continuous loft fit, anchor propagation, undo/export, finite geometry and planted-foot constraints. These measurements establish coherent construction and contact behavior; final silhouette, facial structure, cloth/body intersections and motion weight still require judging the captured images.

The revised shared Warden compiles to 39,652 interactive triangles with no compiler errors. Its complete runtime walk measures 1.61 mm maximum planted horizontal slip and 23.49 mm maximum contact residual, within the existing 25 mm review allowance, with no unavailable or conflicting contact diagnostics. This residual is close to the allowance and remains a useful constraint for further body-motion edits.
