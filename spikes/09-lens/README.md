# Spike 09: the lens

*Throwaway code that answers one question with measurements. It isn't the studio, and it isn't the compiler. There's no wrela compiler yet, so this spike works on WGSL field files directly and does by hand, in JavaScript, what D-105 asks the compiler to provide.*

## The question

D-105 says the source is the truth and every studio tool is a lens that reads and writes it, through two compiler queries: **provenance** (which expression and literal made this output) and **parameter gradients** (how an output changes with each literal). The authoring experiment (D-095, `experiments/agent-authoring/`) found that agents author placeholder creatures because placing and shaping forms by editing numbers is imprecise, and both agents asked for named parts, part isolation, probes and metric views.

**Can a browser tool edit a field file by direct manipulation, fast enough to feel direct, while the file stays the truth?** Concretely, on the wolf from the authoring experiment (286 lines, ~600 float literals):

1. **Lift the literals:** find every numeric literal and named `const`, record its source span, rewrite the shader to read them from a parameter buffer, and write edits back by splicing new values into the spans.
2. **Click to source:** click a pixel, get the part that won the union there and the literals ranked by |∂d/∂literal|, with their source lines. Isolate and tint views per part.
3. **Drag to edit:** drag a surface point, solve for a small change to the most influential literals with damped least squares, write it back.
4. **Fit to a reference:** optimize a chosen subset of literals so the side silhouette matches a target image, with known ground truth.
5. **An agent API:** the same operations as plain calls, usable headless.
6. **A human UI** at <http://127.0.0.1:8417/09-lens/>.

## Success criteria, written before measuring

Latency is wall-clock in the page, from the call to the result in JavaScript, on the M4 in Chrome. Other spikes share the GPU while this is built, so numbers are **indicative** until re-run on a quiet machine.

| Measure | Target |
|---|---|
| Click to source (pixel → part + ranked literals + source lines) | **≤ 50 ms** median |
| One drag-solve update (gradients, solve, upload) | **≤ 33 ms** median for 30 Hz; ≤ 16 ms ideal |
| One drag update including the re-render of the view being dragged | reported; same targets |
| Fit to reference, to convergence | reported; a hypothesis: ≤ 10 s for ~10 literals |
| Write-back (splice, verify by re-lifting) | ≤ 50 ms |

Quality targets, set before measuring:

| Measure | Target |
|---|---|
| **Lift coverage** | Every float literal in the subject file is lifted, or reported with a reason |
| **Lift round trip** | Lift, then write back with no edits: byte-identical source |
| **Lift correctness** | The lifted shader renders like the unlifted one: mean difference ≤ 0.5/255 and ≤ 0.1% of pixels off by more than 8/255 |
| **Part at a click** | Correct for ≥ 9 of 10 scripted clicks on known landmarks |
| **Top literal at a click** | Belongs to the clicked part's code or a `const` it uses, for ≥ 8 of 10 |
| **Drag accuracy** | The target point is on the new surface within **1 mm**, after write-back rounding |
| **Drag locality** | ≤ 8 literals change; surface points away from the drag (outside the falloff radius) move **≤ 0.5 mm RMS, ≤ 2 mm max** |
| **Minimal diffs** | Only the changed literals' characters change; no reformatting; changed lines = lines holding changed literals |
| **Fit** | IoU ≥ 0.99 against the ground-truth silhouette; the changed literals recovered within **2 mm**; untouched literals in the subset drift ≤ 2 mm |
| **Robustness** | No NaNs or crashes with all ~600 literals selected, with literals shared across parts, and with zero or collinear gradients |

## Method

What was built. Items marked **(added)** came in after the first measurements, when the first version of the solver produced bad edits. The criteria above didn't change.

### 1. Lifting (`lift.js`)

- **Tokenizer and shallow parser.** A WGSL tokenizer and a shallow parser of the module: `const` declarations, functions with their parameters and bodies, and everything else. There's no type checker.
- **Every float literal** in the subject file gets an id and a source span. A unary minus directly before a literal is part of its span, so a value can change sign without breaking the syntax.
- **Module-scope `const`s can't read a buffer,** because WGSL needs a constant expression there. So each use of a `const` whose value holds a literal is expanded in place to its rewritten initializer: `J_HEAD` becomes `(vec3f(lens_litbuf[15], …))`. A literal in a `const` is then shared by every use, which is what editing the source would do.
- **Reads come from a read-only storage buffer.** Each literal becomes a static read of a read-only storage buffer. Two modules are compiled from the same text:
  - **fast**, for rendering, silhouettes and probes
  - **finite-difference**, where each read also adds a per-invocation perturbation of one literal
  
  Editing a value is a buffer write, never a recompile.
- **Not lifted, with reasons:**
  - integer literals, which can be used where only an integer is allowed (the wolf's are fbm octave counts); telling those uses apart needs type checking
  - literals in the shared library `lib.wgsl`
  - literals in contexts that need a constant expression
- **Parts:** each call, in the body of `field`, to a function defined in the subject file that returns `f32`. Each is wrapped in `lens_tap(k, …)`. The winning part at a point is the one with the smallest value. This needs no naming convention.
- **Static provenance:**
  - A call graph over functions and `const`s says which literals can reach `field`: 388 of 606. Only those get gradients.
  - It also counts each literal's static evaluation sites. sd_ell's `1.0` has 19.
  - It marks **helpers**, functions called from three or more functions. Automatic drag selection skips their literals.
- **Write-back** splices new values into the recorded spans, in the literal's own style (decimals, exponent, suffix). It then re-lifts the new source and checks two things: the token structure is unchanged apart from literals, and every value reads back as intended.

### 2. Gradients: GPU central differences, not AD

∂d/∂θ comes from central differences on the GPU, with h = 2·10⁻⁴·max(1, |θ|). One kernel evaluates M points × (K literals + 1) invocations, chunked to about 1M field evaluations per submission. Forward-mode AD wasn't attempted: it needs a type-aware transform of the whole field, which is the compiler's job. The drag uses the gradient of the distance estimate d/|∇d| rather than of d **(added)**, at 14 field evaluations per literal instead of 2. Without it, the field's Lipschitz safety factors (`0.9 * (…)`, `0.88 * (…)`) were levers the solver could pull to shrink |d| at an off-surface target without moving the surface. It did pull them.

### 3. Click to source

One submission does the whole query:
1. A probe kernel traces the clicked pixel's ray and refines the hit with four unrelaxed steps.
2. It records every part's value and writes the hit point.
3. The gradient kernel differentiates `field` with respect to all 388 literals that reach it.

Then there's one readback. Literals are ranked by |∂d/∂θ| and influence is grouped by source line.

### 4. Drag: damped least squares on the parameter gradients

A drag moves a target point t on the plane through the clicked point p₀ that faces the camera. Gauss–Newton runs on

  W·Σᵢ wᵢ (r(pᵢ + δ) − r₀ᵢ)²  +  W_A·Σₐ wₐ (r(a) − r₀(a))²  +  λ²·|θ − θ₀|²

over K selected literals, where r = d/|∇d| and δ = t − p₀. Each update warm-starts from the last and is damped toward the values at the start of the drag. The pieces:
- **The clicked point** has weight 1. **(added)** A ring of six surface points 6 mm around it has weight 0.02 each, the **patch**. On its own, the single-point constraint is often met by a bulge: the brief's literal version doubled the ear-tip radius.
- **(added)** If the field is mirror-symmetric in x, checked numerically on surface samples, the mirror image of the patch gets rows that follow the mirrored drag. The tolerance is 1 cm; the wolf's unmirrored fur noise makes it asymmetric by 5 mm. Without these rows, the solver could break symmetry by moving midline `.x` literals.
- **Anchors** are ~260 surface samples from four views. Their weight rises from 0 within 3 cm of the drag (or 1.5× its length) to full at 3× that radius. Points whose gradient over the chosen literals equals the clicked point's are exempt, as is the mirror region: the source moves them together anyway.
- **Literal selection.** **(added, the default)** Once the drag has a direction, the top K = 8 literals are chosen by |Σᵢ ∂rᵢ/∂θ · rᵢ|, the move's steepest-descent direction. The brief's selection, top K by |∂d/∂θ| at the click, is kept as `select: 'influence'`. Helper literals are skipped unless asked for. `literals: [...]` overrides both.
- **Weights:** W = W_A = 10⁴ and λ² = 1. Tuned on the first seven drags; see the held-out set.
- **On release:**
  - literals carrying under 5% of the move are pruned, and the rest are re-solved
  - **(added)** each changed literal is rounded to the source's own decimals, largest change first, and the others are re-solved to compensate; the last one gets as many extra decimals as keep the rounding error under 5% of its change
  - the error is re-measured with the rounded values
  - locality is measured on a separate, denser set of surface samples
  - **(added)** separate pieces are counted on an 8 mm grid, as `fieldview` does, so a drag that detaches a part is flagged

### 5. Fit to a silhouette

- **Target:** `subjects/wolf-target.wgsl`, a copy of the wolf with a longer neck (`J_HEAD` +4.0 cm up, +4.5 cm forward) and bigger ears (`EAR_H` 0.086 → 0.110).
  - Its orthographic side silhouette (256², 6.7 mm per pixel) is rendered and saved as a PNG.
  - The fit reads the PNG back, so it doesn't see the truth.
- **Loss:**
  - For each pixel, m is the minimum of `field` along the pixel's ray: sphere steps outside, half-depth steps inside, then a golden-section refine. The pixel is inside the silhouette when m < 0.
  - Pixels on the wrong side of the target, or within half a pixel of its boundary, get a hinge residual in metres. That's a distance loss computed from the field itself, not from a distance transform of the image.
  - IoU is reported, not optimized.
- **Gradient:** by the envelope theorem, ∂m/∂θ = ∂d/∂θ at the minimizing point. The Jacobian comes from the same gradient kernel, evaluated at the active pixels.
- **Solver:** Levenberg–Marquardt with accept/reject, and an optional prior toward the source values.
- The d/|∇d| residual was tried here as well. It stalled the fit at IoU 0.95: it's ill-conditioned at deep interior minima. So the fit uses raw m.

### 6. Views

Contact-sheet views like `fieldview`'s:
- The side, front and top views are **orthographic**, with a 10 cm metric grid, as the agents asked. The three-quarter view is in perspective.
- **Modes:**
  - `shaded`
  - `parts`: false colour by winning part
  - `isolate`: one part's field alone
  - `tint`: one part coloured, the rest grey
  - `influence`: ∂d/∂θ of one literal over the surface; orange where raising it moves the surface out, blue where in
  - `silhouette`
- **(added)** Rays are culled to the creature's padded bounds, which renders 1.2× faster. 44 of 1M pixels differ by more than 8/255 from the unculled render, at grazing silhouettes, because the march starts at a different point.

### 7. GPU safety (added after the 2026-10-01 incident, `spikes/README.md`)

- Renders go in 256² tiles (128² for the influence view), silhouettes in row bands of ~16k pixels, gradients in chunks of ~1M field evaluations, and probes in 2048 rays. Each gets its own submission, with `onSubmittedWorkDone` between them.
- Kernel loops are capped: 400 march steps, 96 shadow steps.
- Pipelines compile one at a time.
- A whole `#run` holds the GPU lock for about 18 s.

## The agent API

Every operation is a JSON command. `lens.py` runs commands headless through `spikes/headless.sh`, so they queue for the GPU lock. Two modes:

```bash
# A command file: load once, run every command, print the outputs as JSON.
python3 spikes/09-lens/lens.py run job.json [--write]

# A live session: ~25 ms per cheap command. It holds the GPU lock while open, so stop it when done.
python3 spikes/09-lens/lens.py start creature.wgsl --session wolf
python3 spikes/09-lens/lens.py call wolf '{"op": "query", "point": [0, 0.87, 0.81]}'
python3 spikes/09-lens/lens.py call wolf '{"op": "write"}' --write    # saves over creature.wgsl
python3 spikes/09-lens/lens.py stop wolf
```

A job file is `{"source": "field.wgsl", "commands": [...]}`, with paths relative to the job file. See `examples/agent-job.json`. With `--write`, a `write` command saves the new source over the job's source file; otherwise the new source is only returned. Images come back as absolute paths under `spikes/09-lens/results/`. The field file must define `fn field(p: vec3f) -> f32` in metres, +y up, ground at y = 0, facing +z, as `fieldview` expects; `fn albedo(p: vec3f) -> vec3f` is optional.

| Command | Arguments | Returns |
|---|---|---|
| `info` | | Lifted and unlifted literals with reasons, the parts, compile time |
| `literals` | `filter`: a selector, or `{fn, const, line, inField}` | The literal table: id, name, value, line, evaluation sites |
| `render` | `view`: `side`, `front`, `top`, `threeq` or `sheet`; `size`; `mode`: `shaded`, `parts`, `tint`, `isolate`, `influence` or `silhouette`; `part`; `literal`; `shadows`; `name` | A PNG path, plus each view's geometry (metres per pixel, axes) |
| `isolate` | `part`, `view`, `size` | A render of that part's field alone |
| `query` | `{view, pixel, size}`, `{point}` (snapped to the surface) or `{ray: {o, d}}` | The winning part and every part's value; literals ranked by \|∂d/∂θ\|, with how far the surface moves per unit; the source lines by share of influence |
| `probe` | `point` | The field value, inside or not, the winning part, every part's value. No snapping. |
| `ray` | `o`, `d` | The first surface hit: a tape measure |
| `project` | `point`, `view`, `size` | The pixel |
| `move` | `from`: `{point}` or `{view, pixel}`; `to`: `{delta}`, `{point}` or `{view, pixel}`; `k` (8); `literals` (selectors); `anchors`, `patch`, `symmetry` (all true); `select` (`residual` or `influence`); `falloff` (m); `steps` | The literal changes, the error in mm after rounding, locality (far, near and tied RMS and max in mm), piece counts before and after |
| `set` | `literal`, `value` | |
| `reset` | | Discards unsaved edits |
| `fit` | `target`: `{png}` or `{source}`; `literals`; `view` (`side`); `res` (256); `maxIter`; `prior` | Fitted values, IoU before and after, the convergence log |
| `pieces` | `cell` (m) | How many separate pieces the shape has |
| `diff` | | The diff the current edits would write |
| `write` | | Writes back, verifies by re-lifting, returns the new source and the diff |

**Literal selectors:**
- an id, or a name: `J_HEAD.y`, `EAR_H`, `TOE_IN.z`, or `sd_neck:105:39` for a literal inside a function (function, line, column)
- `const:J_HEAD`: every component
- `fn:sd_neck`: every literal in the function
- `part:sd_head`: every literal the part's code can reach
- `@105`: every literal on line 105
- `@105:0.122`: the literal on line 105 with that text

**How an agent would use it.** For instance, "make the ear tip reach 1.085 m": find the tip with `ray`, then `move` with `literals: ["EAR_H"]`. The lens computes the value, and `diff` and `write` record it. This was the most precise use measured: 0.33 mm off, nothing else moved. `query` answers "what made this pixel"; `isolate`, `pieces` and `probe` catch the silent, non-local errors the authoring agents hit.

## The human UI

Open <http://127.0.0.1:8417/09-lens/> in Chrome.
- **Click** the creature: the part and ranked literals appear, with the source lines highlighted.
- **Drag** from the creature to move that surface point. The target stays on the plane that faces you, and the solve runs live.
- **Click a literal** in the table or the source: the view switches to its influence view, and the literal can be edited by number.
- **Write back** saves `results/ui-edited-wolf.wgsl` and offers a download. The page can't write to the subject file: the server only accepts PUTs into `results/`.
- **Fit demo** fits the side silhouette to `subjects/wolf-target.wgsl`.

`#uitest` drives the same UI with synthetic pointer events, headless: click, drag the nose 30 mm, select a literal, write back, switch views and modes. It passes (`results/uitest.json`, `results/uitest-canvas.jpg`). Nobody has used the UI by hand yet.

## Results (2026-10-01)

**Setup:**
- MacBook Air M4, Chrome 154 headless (via `headless.sh`), WebGPU.
- Final run: `results/run-2026-10-02T01-43-30-730Z.json`. Lifting cost: `results/perf-2026-10-02T01-42-50-729Z.json`.
- Both were taken under the GPU lock, so no other spike's page was on the GPU, but other agents' CPU work was. **Indicative** until re-run on a quiet machine.
- Early development runs, with ~10 spikes on the GPU at once, were 3–15× slower.

### Latency

| Measure | Target | Measured (median, p90) | Result |
|---|---|---|---|
| Click to source: pixel → part, 388 literals ranked, source lines | ≤ 50 ms | **0.9 ms**, 1.0 ms (36 clicks); first query 2.6 ms | **Pass** |
| Drag-solve update: one Gauss–Newton step over ~270 points, K = 8, upload | ≤ 33 ms; 16 ideal | **1.1–1.2 ms**, p90 ≤ 1.8 ms (7 drags × 15 updates) | **Pass** |
| Drag update including the 512² preview render (no shadows) | same | 1.2 + **18.6–20.5 ms** ≈ 20–22 ms | **Pass** for 30 Hz; misses 16 ms. The render is the bottleneck, not the solve. |
| Start a drag (query, patch, anchors, symmetry check) + choose literals | | 6–7 ms + 4 ms | |
| Finish a drag (prune, re-solve, round, locality, piece count) | | 58–91 ms, on release | |
| Fit to reference, 3 or 9 literals, to convergence | hypothesis ≤ 10 s | **100 ms** (7 iterations) and **136 ms** (8); about 300 ms including stall detection | **Pass** |
| Fit, 138 literals | | 1.1 s (C), 1.75 s with a prior (D) | |
| Write-back: splice 11 literals + verify by re-lifting | ≤ 50 ms | **5.4 ms**, 11 ms; saving the file 2.5 ms | **Pass** |
| Lift the wolf / compile cold / compile cached | | 6.8 ms / 2.2 s / ~50 ms | Edits never recompile |
| CLI session round trip (query, probe, ray) | | 23–49 ms; a move 266 ms | |
| CLI batch job (13 commands) | | ~1 s of page time; the wall clock is dominated by the GPU-lock queue (15 min in the test) | |

**The cost of lifting** (`#perf`, side view 512² with shadows):

| Variant | ms | × unlifted |
|---|---|---|
| Unlifted, the source as written | 26.1 | 1.0 |
| Lifted, literals in a storage buffer (the default) | 41.0 | 1.6 |
| Lifted structure, literals baked back in as constants | 26.1 | 1.0 |
| Lifted, literals in a **uniform** buffer | 240.7 | **9.2** |
| The finite-difference module (the influence view) | 91.3 | 3.5 |

The same 606 static reads are nine times slower from a uniform buffer than from a storage buffer on this device (Chrome → Dawn → Metal). Why isn't known. A hypothesis: the compiler hoists the uniform loads out of the march loop, and spills registers.

### Quality, against the criteria

| Criterion | Target | Measured | Result |
|---|---|---|---|
| Lift coverage | all, or a reason | 606 float literals lifted (75 in `const`s, 531 in functions; 388 reach `field`). 7 not lifted: integers, the fbm octave counts. 24 library literals not lifted, by design. 27 of 27 `const`s. | **Pass** |
| Lift round trip | byte-identical | identical | **Pass** |
| Lift correctness | ≤ 0.5/255, ≤ 0.1% > 8/255 | 0/255 mean, 0% (max difference 1/255) over a 1024² sheet | **Pass** |
| Part at a click | ≥ 9/10 | **12/12** | **Pass** |
| Top literal in the clicked part | ≥ 8/10 | **11/12** | **Pass** |
| Drag accuracy after rounding | ≤ 1 mm | **13/13** drags: 0.0003–0.63 mm | **Pass** |
| Drag: ≤ 8 literals change | ≤ 8 | 1–3 in every drag | **Pass** |
| Drag: far points ≤ 0.5 mm RMS and ≤ 2 mm max | both | RMS: 10/13. Max: 6/13. Both: 6/13 (held out: 2/6). | **Fail** |
| Minimal diffs | | Only literal characters change; same line count; structure verified by re-lift; changed lines = lines holding changed literals (1–3 per drag, 3–40 characters) | **Pass** |
| Fit IoU | ≥ 0.99 | 0.9997 (A), 0.9996 (B); 0.9996 and 0.9993 at 512² | **Pass** |
| Fit recovery | ≤ 2 mm | A: 0.32, 0.17, 0.27 mm. B: 0.42, 0.11, 0.84 mm. | **Pass** |
| Fit distractors | ≤ 2 mm | B: 4 of 6 under 1 mm; `HEAD_SCALE` 1.130 → 1.134, `HEAD_PITCH` −0.0013 rad (unitless, so mm doesn't apply) | **Pass** |
| Robustness | no NaN or crash | All 388 field literals selectable; zero, collinear and shared gradients handled; no NaN anywhere | **Pass** |

**Click to source** (`results/side-clicks.jpg`):
- All 12 landmarks land on the right part.
- **The one miss is a real finding.** At the middle of the tail, the most influential literal is `0.010` on line 208, the amplitude of the belly and britches fur tufts in `field`. Its world-space "britches" mask also covers the tail. That's the kind of silent, non-local effect the authoring agents struggled with, and the lens surfaces it on a click.
- **The ranking favours translations along the viewing axis:** `J_HEAD.x`, `J_CHEST.x`. That's accurate, since moving the part sideways moves that visible surface most, but it's rarely what an author wants. Residual-ranked selection, which scores literals against the requested motion, fixes this for drags.

**Drags** (default settings: patch, residual selection, anchors, symmetry, K = 8):

| Drag | Literals changed (the diff) | Error mm | Far RMS / max mm | Notes |
|---|---|---|---|---|
| Ear tip up 15 mm | `EAR_H` 0.086 → 0.111, `EAR_BASE.y` 0.040 → 0.029, ear-tip radius 0.012 → 0.0131 | 0.40 | 0.08 / 0.92 | Valid but odd: it lengthens the ear 25 mm and sinks it 11 mm into the head. With `literals: ["EAR_H"]`: 0.086 → 0.099, 0.33 mm off, 0 far motion. |
| Nose forward 12 mm | nose pad offset 0.188 → 0.198, y 0.006 → 0.008, radius 0.011 → 0.01134 | 0.004 | 0 / 0 | One line changed |
| Belly down 10 mm | `J_CHEST.y` −5 mm, ribcage y-radius +7 mm, tuft amplitude 0.010 → 0.0093 | 0.035 | 0.78 / 6.8 | The ribcage is a big primitive; its flank moves too |
| Tail tip down 20 mm | `J_TAIL3.y` 0.270 → 0.251 | 0.20 | 0.01 / 0.14 | One literal |
| Back of thigh back 10 mm | hamstring cone ends, tuft amplitude | 0.17 | 0.47 / 3.3 | |
| Ribcage side out 10 mm | ribcage x-radius 0.130 → 0.139, `J_CHEST.x` 0 → 0.001 | 0.33 | 1.83 / 9.3 | With `falloff: 0.15`: far 0.0001 / 0.0015, and the flank counts as near or tied |
| Ear side up 15 mm (tangential) | tip radius, ear base x and z | 0.38 | 0.003 / 0.04 | **Ill-posed:** see below |
| *Held out:* forepaw toe forward 8 mm | `TOE_IN.x`, `TOE_IN.z`, toe radius | 0.04 | 0 / 0 | `TOE_IN` is shared by the fore and hind paws (`sd_paw`); the hind toes moved too, flagged as tied |
| *Held out:* hock back 10 mm | `J_HOCK.z` −6 mm, Achilles cone | 0.06 | 0.16 / 2.2 | |
| *Held out:* withers up 10 mm | mane ellipsoid offset and radius | 0.03 | 0.70 / 6.4 | The click hit the mane (`sd_neck`) |
| *Held out:* croup down 10 mm | croup y-radius −26 mm, centre +13 mm | 0.10 | 0.47 / 3.5 | Keeps the croup's underside in place |
| *Held out:* cheek ruff out 8 mm | cheek ruff offset and radius | 0.63 | 0.08 / 0.81 | |
| *Held out:* chest front forward 10 mm | prosternum radius 0.090 → 0.108 | 0.0003 | 0.24 / 2.3 | One literal |

**Ablations** (`dragAblations` in the run JSON), on the ear tip, nose and belly:
- **The brief taken literally** (a point constraint, top K by \|∂d/∂θ\|): the ear tip becomes a bulb (tip radius 0.012 → 0.024) rather than moving up. That's the bulge problem the patch fixes.
- **Anchors off** (plain damped least squares on the target alone): the ear and nose drags move the whole head instead (`J_HEAD.y`, `J_HEAD.z`), with far motion of 7.8–9.0 mm max.
- **A rigid patch** (ring weight 1): the ear sinks 55 mm and grows 66 mm, with far motion of 10 mm max.
- **K from 4 to every field literal** gives nearly the same edits after pruning. Only 20 non-helper literals have any influence near the nose: smooth unions have exactly zero gradient outside their blend radius.

**Failures and limits found:**
- **Locality fails for big primitives.** The ribcage, mane, croup and chest are 20–60 cm. A parametric edit has the extent of the primitive it changes, and a fixed 9 cm falloff counts most of it as "elsewhere". A larger falloff, a user setting like a brush radius, makes the ribcage drag local. The criterion conflates unwanted changes with the natural extent of the edited primitive.
- **Tangential drags are ill-posed.** Dragging a point along the surface (the side of the ear, upward) is unobservable at that point for an implicit surface. The solver meets the constraint with small sideways tweaks; the ear doesn't get taller.
- **A large drag on a small primitive detaches it.** Nose 30 mm forward: the nose-pad ellipsoid leaves the muzzle, 0.06 mm "accurate". The piece count catches it (1 → 2, 22 ms); the solver doesn't prevent it.
- **Automatic literal choice is valid but not always the author's.** Tuning the weights on the first seven drags fixed the worst cases. The held-out six fared about as well on accuracy (0.0003–0.63 mm) and worse on far max (2/6 under 2 mm). An author or agent naming the literal is precise.
- **Fitting many literals overfits.**
  - C (138 head and neck literals) matched the silhouette worse than A (IoU 0.9915) and recovered nothing: the true literals were 40–55 mm off, and 121 others changed by more than 1 mm (`results/fit-C.jpg`; the ears become needles).
  - A prior (D) gets IoU 0.9992 and `EAR_H` within 3.8 mm, but not the head position. Other head literals can mimic it from the side.
  - A silhouette constrains only a few degrees of freedom. The subset has to be small and right.

**Write-back:**
- Four drags then wrote 11 literals on 8 lines (60 characters), verified by re-lift (`results/edited-wolf.diff`).
- The written file, compiled from scratch, renders identically to the in-memory state (0/255).
- The CLI's fit and write wrote `J_HEAD = (0.0, 0.952, 0.637)` and `EAR_H = 0.110`, which is the ground truth once rounded to the file's 3 decimals.

## What this says

**For agents (hypothesis, not tested).** This should remove two of the bottlenecks the authoring experiment found, but whether it moves creature quality up a class needs the experiment re-run with these tools (D-095).
- **Precision:** "put this surface here" or "match this silhouette" becomes a sub-millimetre call that returns a one-line diff. No more mental arithmetic on the formulas.
- **Silent non-local errors:**
  - `query` names the part and literal behind any pixel.
  - `isolate` and `pieces` catch buried or floating parts.
  - The influence view answers "what does this number do?"
- **Most useful mode:** the agent picks the literal and the lens computes the value (`move` with `literals`, `fit` with 3–9 literals).
- **Riskiest mode:** automatic literal selection on big drags.

**For humans:**
- Interaction is fast enough: about 1 ms to solve and 20 ms to preview at 512².
- Clicking to see the part and its source line, and the influence view, look useful.
- Dragging chooses literals for you. The choices are sometimes surprising, but every result is a small readable diff, and undo is a buffer write.
- Nobody has used the UI by hand. It's minimal: no gizmos, no brush-radius control, no constraint locks.

## What a real compiler must provide

1. **Stable literal identities.**
   - The spike names literals by `const` component or by function:line:col, and re-lifts after every write to recover spans.
   - Names that use line:col break as soon as a line is inserted above.
   - The compiler should give each literal an identity that survives unrelated edits, and map it to a span on demand.
2. **Lifting as a compilation mode, with types.**
   - Literals become runtime parameters with no recompile.
   - It should know which literals are integers or structural, and carry value ranges (a radius ≥ 0), which the solver can't see today.
   - Hypothesis: specialization would keep the render cost of lifting near zero. Bake the literals that aren't being edited as constants (1.0× here), keep the edited ones live (storage reads: 1.6× for all 606), and re-specialize when the edit set changes.
3. **Provenance.**
   - The part (top-level call) that wins at a point, and the expression within it. Here it's a tap on each call in `field`.
   - Static reachability (which literals can affect an output).
   - Evaluation-site counts, to tell shared helpers from local constants.
4. **Parameter gradients**, as forward-mode AD, ideally in vector mode over many literals:
   - Central differences cost 2 evaluations per literal (14 for d/|∇d|), have f32 noise, and need a careful step size.
   - AD would also define gradients through `min`, `smin` and branches.
   - It should give the gradient of the distance estimate d/|∇d|, which needs second derivatives, because a plain ∂d/∂θ rewards scale factors that don't move the surface.
5. **Structural facts the solver can use:**
   - symmetry (`mirror_x`), detected numerically here and loosely, because the fur noise isn't mirrored
   - which literals are Lipschitz safety factors
   - per-part extents, for a sensible default falloff
   - connectivity (the piece count), as a cheap assertion (D-095's agents asked for these)

## Caveats

- **One creature, one author.** Seven drags were tuned on and six held out. The weights could be overfit to the wolf.
- **Locality is sampled.** It's measured on ~260 surface samples from four views, about 8 cm apart, so small parts can be missed. The hind toes in the toe drag were caught only by gradient coupling.
- **Unit-blind ranking.** Literal units are unknown (metres, radians, scale factors), so ranking and damping mix units.
- **Tied points are a heuristic.** "Tied" covers points whose gradient over the chosen literals is within 15% of the clicked point's, or near its mirror image.
- **Some brief items weren't done.** Forward-mode AD wasn't built. The fit uses a field-distance hinge loss, not a distance transform of the image. Only side-view silhouettes were fitted.
- **The judge's bias.** The same model family wrote and judged this. Look at `results/*.jpg`.

## Layout

| File | What |
|---|---|
| `lift.js` | Tokenizer, literal lifting, shader rewrite, part taps, call graph, write-back, diff, verification |
| `lens.wgsl` | Kernels: tiled render, probe, gradients, silhouette minimum, grid samples |
| `lens.js` | The engine: GPU resources and every operation (`query`, `move`, `fit`, `render`, `pieces`, …) |
| `api.js` | The agent API: a JSON command executor over `lens.js` |
| `main.js` | The page: human UI, `#quick`, `#uitest`, `#perf`, `#api=…`, `#serve=…` |
| `run.js` | `#run`: scripted tests of every operation |
| `lens.py` | CLI for agents: batch command files, or a live session |
| `examples/agent-job.json` | A sample command file |
| `lib.wgsl` | Copy of `fieldview`'s helper library |
| `subjects/` | `wolf.wgsl` (a copy of the authoring experiment's wolf) and `wolf-target.wgsl` (the fit target) |
| `results/` | `run-*.json`, `perf-*.json`, `uitest.json`, `edited-wolf.diff`, screenshots (PNG ignored by git; JPEG copies kept) |
| `jobs/` | Scratch space for `lens.py` (ignored by git) |

## Running it

```bash
python3 spikes/serve.py 8417              # already running in this workspace
spikes/headless.sh 09-lens '#run' 300     # ~18 s of GPU time: results/run-<timestamp>.json, screenshots, DONE
spikes/headless.sh 09-lens '#quick' 120   # ~3 s: render, query, two moves, write-back, fit
spikes/headless.sh 09-lens '#uitest' 120  # drives the human UI with synthetic pointer events
spikes/headless.sh 09-lens '#perf' 180    # the cost of lifting, five shader variants
```

JPEG copies of the key screenshots: `sips -s format jpeg -s formatOptions 80 -Z 1280 in.png --out out.jpg`.

## Quiet-machine rerun (2026-10-02)

Run file: `results/run-2026-10-02T03-52-09-332Z.json` (serial, after a cool-down, Chrome 154 headless). Every non-timing field matches the 01:43 run.

| Metric | README value | Quiet value |
|---|---|---|
| Click to source, median / p90 / first query | 0.9 / 1.0 / 2.6 ms | 1.0 / 1.3 / 4 ms |
| Drag-solve update, 7 drags: medians, worst p90 | 1.1–1.2 ms, ≤ 1.8 ms | 1.2–1.5 ms, ≤ 1.8 ms |
| Drag update incl. 512² preview, per drag | ≈ 20–22 ms | ≈ 20.5–22.4 ms (render 19.2–21.1) |
| Start a drag + choose literals / finish, 13 drags | 6–7 + 4 ms / 58–91 ms | 6.7–12.6 + 3.8–7.4 ms / 72–116 ms |
| Fit A / B / C / D to convergence | 100 ms / 136 ms / 1.1 s / 1.75 s | 101 ms / 138 ms / 1.05 s / 1.77 s |
| Write-back splice + verify, median / p90 | 5.4 / 11 ms | 5.5 / 7.0 ms |
| Lift / compile cold / compile cached | 6.8 ms / 2.2 s / ~50 ms | 7.5 ms / 2.2 s / 54.5 ms (`freshCompileMs`) |
| Lifting cost (`#perf`); CLI session and batch | 1.6× storage, 9.2× uniform; 23–49 ms | not in the run JSON; not re-measured |
| Passing quality rows (lift, clicks 12/12 and 11/12, drag error 0.0003–0.63 mm, fit IoU 0.9997 / 0.9996, recovery, robustness) | Pass | identical values |
| Far points ≤ 0.5 mm RMS and ≤ 2 mm max | 6/13 (held out 2/6): Fail | 6/13 (held out 2/6) |

No verdict changes under the README's own criteria: every latency target still passes (the preview still misses the 16 ms ideal), and the quality numbers are identical, so drag locality still fails.
