# Experiment: can an agent author a creature as a field?

*D-067 item 3, answering audit finding T4. Thesis 1 says an agent-native authoring surface can carry a studio. This is the cheapest test of that: no wrela code, existing tools, a render-and-look loop.*

## The tool

`fieldview/` is a small Rust + wgpu program that renders a WGSL distance field headlessly:

```bash
fieldview/target/release/fieldview creature.wgsl out.png [--size 512] [--center x,y,z] [--radius r]
```

- **Input:** a WGSL file with `fn field(p: vec3f) -> f32` (metres, +y up, ground at y = 0, the creature faces +z) and optionally `fn albedo(p: vec3f) -> vec3f`. The helpers in `fieldview/src/lib.wgsl` are available (`sd_round_cone`, `sd_ellipsoid`, `smin`, `noise3`, …).
- **Output:** a 2×2 contact sheet:
  - top-left: side view from +x, so the front is on the left
  - top-right: front view from +z
  - bottom-left: three-quarter view from above
  - bottom-right: top-down view

  The ground is checkered in 0.5 m squares for scale.
- **What it prints:**
  - compile errors with the line number in the author's file
  - volume and implied mass
  - bounds
  - how many **separate pieces** there are (floating parts)
  - whether anything is **sunk below the ground**
  - whether the creature is **cut off by the framing**
  - the field's **gradient near the surface**, where values above ~1.2 cause sphere-tracing artifacts

  A picture doesn't show those reliably.

Build it with `cargo build --release --offline` in `fieldview/`. A render takes ~70 ms.

## Protocol

- **Two agents, run independently,** each with one brief (below) and no human input. Each gets at most **15 renders**.
- **They aren't shown** the spike's grazer or any other reference field.
- **Each must:**
  - organise the creature as parts attached to a skeleton, with joint positions listed, so it could be skinned the way sketch 01 does it
  - keep `notes.md`: what changed each iteration and why, and what was hard
  - save its renders as `iter-NN.png` and the final one as `final.png`
- **The briefs:**
  - **A, the grazer:** sketch 01's creature, from its prose description only. Comparable with the spike's hand-built version.
  - **B, a wolf:** a familiar animal, so judging realism doesn't depend on imagination.

## Judging criteria, written before seeing results

1. **Reads as intended** from all four views: silhouette and proportions. Yes / partly / no.
2. **Sound as a field:** one piece, feet on the ground, no visible artifacts, near-surface gradient ≤ ~1.2.
3. **Secondary form:** joints, muscle masses, head features (eyes, ears, nostrils), hooves or paws.
4. **Fits the pipeline:** parts in bone space with a skeleton, so it can be skinned and pruned per part, as opposed to one monolithic expression.
5. **Process:** renders used, errors hit, and whether the diagnostics changed what the agent did.
6. **Verdict:** blockout / placeholder / shippable mid-tier / hero.

**Bias:** the judge (Claude) is the same model family as the authors. Look at the images yourself before trusting the verdicts.

## Results (2026-10-01)

| | Wolf | Grazer |
|---|---|---|
| Final | [`wolf/final.jpg`](wolf/final.jpg) | [`grazer/final.jpg`](grazer/final.jpg) |
| First render | [`wolf/iter-01.jpg`](wolf/iter-01.jpg) | [`grazer/iter-01.jpg`](grazer/iter-01.jpg) |
| Renders used | 15 (4 of them close-ups) | 14 (3 close-ups) |
| Agent time and tokens | ~21 min, ~207K | ~23 min, ~235K |
| Field | 286 lines; 21 joints | 300 lines |
| Diagnostics, re-run by the judge | 1 piece, on the ground, near-surface gradient max 1.15 | 1 piece, on the ground, near-surface gradient max 1.03 |
| Volume | 0.095 m³, about 100 kg. A real wolf is 40–55 kg. | 0.905 m³, about 950 kg |

### Against the criteria

| Criterion | Wolf | Grazer |
|---|---|---|
| 1. Reads as intended | **Yes** from the side, front and three-quarter views; top-down is a plain tube | **Partly.** Side and three-quarter views read as a large woolly horned grazer. From the front it reads as a llama; the agent added horns, which the brief didn't ask for, to stop it reading as a camel. |
| 2. Sound as a field | **Yes** | **Yes** |
| 3. Secondary form | **Partly.** Ears, eyes, nose, ruff, paws with claws and a saddle-patterned coat. Legs are soft tubes with a weak hock, and the body is about twice a wolf's mass. | **Partly.** Horns, ears, eyes, nostrils, cloven hooves, a tail tuft, and joints that bend the right way. The forms are inflated, with few bony landmarks, and the hindquarter is one smooth bean shape. |
| 4. Fits the pipeline | **Mostly.** 21 joints with parents, and parts built in bone frames. The jaw has no joint. | **Partly.** Limbs, head, neck and tail are in bone frames. Torso pieces and colour and wool masks are in world space, so it isn't skinnable as written. |
| 5. Process | Used the gradient diagnostic to find a bad primitive on render 1 | Cropped existing renders to stretch the budget; diagnosed the same bad primitive from the gradient number |
| 6. Verdict | **Placeholder:** a good blockout that's recognisable at once. Not shippable. | **Blockout to placeholder** |

### What the runs show

1. **First drafts carry most of the quality.** Both first renders were already recognisable. The next 13–14 renders refined detail and fixed mistakes, but didn't move either creature up a class. The limit looks like the agent's ability to place and shape forms precisely through numbers, not the number of iterations.
2. **Both agents hit the same primitive bug independently.** The library ellipsoid's bound is discontinuous at its centre, so small ellipsoids near the surface spike the gradient (max 3.1–3.6). Both found it through the gradient diagnostic and replaced it. Spike 01's probe found the same thing (D-092).
3. **Both asked for the same features, and all of them are general** (D-050):
   - **named parts:** a part tree that diagnostics can point at, that can be rendered alone, or tinted
   - **joints and frames as values,** with hierarchical transforms, so parts are written in their joint's frame
   - **per-part channels:** material and displacement attached to parts rather than to world-space masks
   - **cheap geometric assertions**

   That's close to sketch 01's model: parts on a skeleton (D-031), channels per part (D-002). It's evidence that the sketch's structure is the one authors want.
4. **Feedback resolution is the bottleneck.** At a whole-body framing, a hoof is ~10 px and an eye ~3 px. Both agents spent renders on close-ups or improvised crops, and both asked for probe/measure tools and orthographic views on a metric grid.
5. **Errors were silent and non-local.** Widening the wolf's muzzle buried its eyes, unnoticed for six renders. The grazer's wool displacement coated its horns because of evaluation order.

### What this says about thesis 1 (T4)

**Neither confirmed nor refuted.**
- Unaided agents produce placeholder creatures as fields in about 20 minutes each. That's useful for prototyping, and as a starting point for refinement.
- The gap to hero quality is large, and more iterations of the same loop don't obviously close it.
- **Before claiming more,** re-run with:
  - the tools the agents asked for: named parts, part isolation, probes, metric views
  - reference images
  - a human doing art direction between rounds

  Then see whether quality moves up a class (D-095).

**Bias:** the judge is the same model family as the authors. The images are in this directory; look at them before trusting the verdicts.
