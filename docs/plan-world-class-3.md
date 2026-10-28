# The world-class engine, part three: substance and style

*What the engine does next, after part two shipped (rywible/wrela#335, toolchain 0.9.0, 2026-10-26):
a renderer judged against cited, freely licensed photographs of the real subjects it draws,
running in the browser on a MacBook Air M4 at a sustained 60 fps; a showcase valley whose every plant, and every creature's
coat and body, is grown by simulation rather than authored; and a renderer whose style is a program
apart from
its substance, so the same grown world can be drawn physically, as cel, as paint or as ink
without a change to the engine. Written for an implementing agent who has not read the
conversation that produced it. Each step names its files, what to write, how to test it and the
gate it must pass before the next step's commit. Started 2026-10-27 at Ryan's request. §10 is
where the council's findings go.*

Ryan owns the goals. A line marked **proposed** is a default the plan chose that Ryan has not
confirmed; the agent builds the default behind a switch where one is cheap, and asks before a
default that can't be switched. A section marked **gate** is a check the branch must pass before
the next step's commit. A number the plan names as a target is an **estimate** until a gate
measures it; an idea the plan calls a **hypothesis** ships only if its own gate says it pays, and
otherwise ships switched off with the numbers that said so.

**Ryan's scope revision, 2026-10-27:** use public visuals as the quality bar and public
benchmarks as context; measure Wrela on the actual target devices. No Unreal installation,
local Unreal project, Fab asset acquisition or matching-scene capture is required. This is a
subjective look-development goal, not a controlled engine comparison. This revision supersedes
the local Unreal requirements in the historical council record (§10).

**Ryan's revision, 2026-10-28:** agents alone judge (D38), so only references licensed for an
agent's review count; freely licensed Unreal 5 imagery of these subjects barely exists (a
survey of Wikimedia Commons and the web found four CC BY videos, none of the vale's subjects at
showcase quality), so **the references are photographs of the real subjects**, freely licensed
(CC0, public domain, CC BY, CC BY-SA), not Unreal renders. A photograph is the bar a
photoreal engine imitates; the claim's wording says "photographs", never "Unreal".

**Ryan's revision, 2026-10-28 (execution):** the implementer merges and tags the 0.10.0
release itself (§0.2); the MacBook Air M4 is the only device this plan measures on (the other
devices' rows are deferred, §B6); a line that won't hold is the implementer's to keep climbing,
by its own judgement, not a stop (§0.9); and the style fit (§D8) uses freely licensed paintings,
not a painting from Ryan.

## 0. Read this first

### 0.1 What "world class" means now

Part two's claim (`docs/plan-world-class-2.md` §0.1, D10) was measured in its §B16 and missed on
every scene but the capybara (`docs/bench.md`, "Whether D10 holds"): the Last Green's floor at
16.6–18.1 ms a GPU frame against 12, 888 MiB against 512, 15.5 s to playable against 5; the
clearing slower than it was (10.5 → 12.65 ms). The lighting it built (probe visibility, the
distance volume, froxel fog) is the part of the image Ryan judged most improved; the materials
and the ground's clutter are what the stills lack against a scanned-content engine. The claim
this plan builds toward, **D27, proposed**:

> On a MacBook Air M4 in Chrome at 1080p, the vale (§0.4 D28: a grown valley, this plan's
> showcase) has visual quality competitive with a fixed, cited selection of freely licensed
> photographs and footage of the real subjects it draws, judged by subject and visual rubric
> across twelve fixed Wrela shots, while the vale holds a sustained 60 fps: its GPU frame under 12 ms at the median,
> no frame of 1,200 paced frames over 33 ms, the paced 99th percentile under 20 ms and the 1,200 frames
> begun within 20.4 s with one frame in flight, measured after ten minutes of play; its GPU memory under 512 MiB; playable within 5 s of a cold page open and
> 3 s of a warm one. Every plant in the vale is grown by simulation from a species' genome and
> its site's history, none authored; every creature's coat, proportions and musculature are grown
> from a genome whose authored part is its body plan. The vale renders through four styles
> (physical, cel, painterly, ink) with no change to `engine/` between them. Lower tiers for
> slower machines are built and measured on the M4; other devices' rows are a later plan's.
> Public engine benchmarks, when useful,
> are cited separately with their original conditions; they imply no same-device speedup.

The axes are three, and they are measured apart so one can't hide another: **fidelity** (the
reference review, §0.5 G-reference), **performance on the device** (the sustained bench,
G-sustained;
the tiers, measured on the M4) and **what the content costs** (bytes shipped, memory, and that it is grown:
G-grown). "Comparable fidelity with outstanding performance on commodity hardware, in the
browser" is Ryan's wording of the goal; D27 separates its subjective quality judgement from
its measured performance targets. The sources need not share our scene, hardware, settings or
resolution. Neither a review score nor a public benchmark establishes parity with any engine or
with reality, or a performance advantage; the review records how far the vale is from
photographs of what it draws, subject by subject. `Tier::Epic` is Wrela's quality tier, not a claim about the public
references' scalability settings.

What the plan bets on, and why each bet is wrela's to make and not Unreal's:

1. **The engine knows what everything is.** A field says where a branch is, how thick, where its
   leaves are, and its exact distance and curvature everywhere; a mesh engine rediscovers
   structure at run time (Nanite's clusters, Lumen's cards and distance fields). Every part of
   this plan reads that structure instead of approximating it: the styles read curvature and
   growth history, the lighting reads the exact field, the levels of detail come from the field
   at a coarser scale.
2. **The compiler sees the whole program.** A renderer specialized to one program's switches and
   one style builds none of the paths it doesn't use (§C). Unreal's permutation system
   approximates this; a compiler that computes constants by running the program's own code
   (`spec/language.md` §13) does it exactly.
3. **Content from simulation.** Scanned content can't be matched by authoring; the plan doesn't
   try. What reads as real in nature is that everything has a cause: a branch bent toward
   light, moss where water lingers, a gully where the stream cut. The `land` package already
   simulates the causes at the forest's scale (centuries of competition, erosion, soil, the
   herd: `land/grow.wrela`, `land/landform.wrela`, `land/soil.wrela`); this plan grows each
   organism from those causes too (§E, §F).
4. **Style apart from substance.** Film compositing assembles a look from decomposed light
   (AOVs); StyLit (Fišer et al. 2016) transfers a painting onto a render guided by its
   light-path buffers. The engine emits the substance (what's there, how light reaches it) and a
   style is a program over it (§D). A cel style is not a tone ramp on a physical material.

### 0.2 The three repositories and their pull requests

| Repository | Checkout | Pull requests |
|---|---|---|
| `rywible/wrela-compiler` | `~/projects/wrela-compiler` | **Part A.** One PR titled `Release 0.10.0: <summary>`, per `docs/release.md` there. The implementer merges and tags it once its checks pass (Ryan's revision, 2026-10-28). A fault found after the tag is a 0.10.1, never a replaced release. |
| `rywible/wrela` | `~/projects/wrela` | **Parts B (its engine half), C, D, E, F, G, H.** One PR per part, each from a branch named `engine/wc3-<part>` off `main`, each merged before the next part's branch is cut. Part H's PR holds the final gates and the records. |
| `rywible/wrela-others` | `~/projects/wrela-others` | **Part B, §B7 only.** One PR fixing Part C of plan two's JavaScript harness. Its measured numbers go into `docs/bench.md` by hand. No Unreal project or capture scripts are built; the public-reference manifest and reviews live in `wrela`. |

**Where the plan starts.** `main` at a55a6001 (rywible/wrela#335 merged 2026-10-26) pins
toolchain 0.9.0. Part A releases 0.10.0 first; Part B's first commit moves the pin. While Part A
is a branch, `WRELA_TOOLCHAIN=~/projects/wrela-compiler` runs it in place of the pin
(`docs/toolchain.md`). The toolchain knows nothing about the engine (the layering rule): every
Part A feature is a general language, GPU or host capability. Nothing in Part A names a tree, a
style or a creature.

**One part, one PR, one scope.** The repository's rule (`AGENTS.md`) is one PR for one requested
scope of work; this plan's parts are those scopes. A part's PR is reviewed as part two's was
(`/simplify`, then the council through `codex exec`, then an xhigh code review, every verified
finding fixed, then merged by the implementer) before the next part's branch is cut from `main`.

### 0.3 How to work

- **Read first:** this section whole; §8's worked examples (the shapes the steps expect);
  `docs/plan-world-class-2.md` §0.3–§0.5 and §0.9 (the working rules and gate kinds this plan
  reuses by name) and its §10 (what the council changed last time and why); `docs/lookdev.md`
  (the instrument every look step is judged by); `docs/bench.md` (every number so far, with its
  command); `docs/toolchain.md`; `docs/toolchain-gaps.md`; `games/last-green/design.md`;
  `docs/citations.md` (its rules: learn, don't copy; no generative model makes anything);
  `spec/language.md` §13 (constants computed at build), §14 (derivations: gradient, interval,
  filtered; they nest, so a Hessian is the gradient of a gradient), §18 (the GPU; `const`
  parameters of entry points at its §18's "const parameters"), §23 in the compiler; and
  `AGENTS.md` in each repository.
- **One commit per numbered step,** in the order given, or in a part's **Order within** line
  where it has one (the numbers are the plan's map; the line is the commits' order), each titled
  with the step's heading. A
  step that says "gate" ends with the gate's result in its commit message, numbers included.
- **Both repositories' rules hold:** one PR for one requested scope; run as few tests as possible
  to validate a change (a step names its tests; run those, then `wrela suite` once per part
  before its PR); cite your sources: each outside method in §9 gets a row in `docs/citations.md`
  when its code lands, and a method the plan doesn't name gets a row too.
- **Nothing lands that the gates don't cover.** A step whose gate fails is fixed before the next
  step; it is never skipped and never "fixed later". A **hypothesis** step whose gate fails ships
  switched off, with the measurement in its commit and in `docs/bench.md`, and the plan goes on.
- **Estimates are labelled.** A cost, a speedup or a budget is `estimated` in code and prose until
  a gate measured it; then the measured number replaces it, with the device and the command.
- **Don't invent design.** The looks, the games' feel and their lore are Ryan's. Where a step
  touches them, build what the plan says and mark anything beyond it **proposed**. A style's
  palette, a creature's body plan, the vale's shot list past §B1's rules: **proposed**, and
  listed in the PR's "For Ryan".
- **Keep the docs true:** `docs/toolchain-gaps.md` (each gap this plan closes moves to "Fixed",
  naming 0.10.0; each gap a step hits is recorded there before the step works round it),
  `spec/language.md` (every new capability), `docs/toolchain.md` (new commands), `docs/bench.md`
  (every measurement), `docs/citations.md`, `docs/lookdev.md` (the instrument's new measures and
  the panel), `games/last-green/design.md` and `examples/vale/design.md` (the contracts: every
  **proposed** number lives there).
- **Use the tools:** `wrela same <pkg> --against <saved build>` shows a change kept what a
  program does; `wrela pipelines <pkg> --json` counts pipelines and their tiers; `wrela tool
  bench` records (with `--sustained`, new, §C3); `wrela tool lookbook --quality` judges a look
  step; `wrela tool passes` shows a frame's passes; `wrela tool compare` (new, §B4) runs the
  reference review; `wrela tool grow` (new, §E6) grows and steers; `wrela tool fit --style` (new,
  §D8) fits a style to a set of freely licensed paintings. The lints are on.
- **Pixels are a first screen in a look step, never the verdict** (plan two §0.3, restated
  because it was the rule most often misread): a pixel diff locates a change; it discards
  nothing. A step that changes the image on purpose is judged by G-look; a step that changes none
  holds its pixel gate.
- **Measure on an idle GPU, and for the sustained gates on a warm one** (§C3): nothing else
  building, ten minutes of the walk before the measured frames. Report the GPU span, the CPU
  frame and the paced intervals as the separate things they are.

**What counts as a shortcut.** Each of these has happened in an earlier plan or was caught by a
council; each is a skipped step, whatever the commit says:

1. A style, a growth program, a tier or a tool that *stands in* for the one the step names (a
   `Cel` style that returns the physical style's colour through a ramp; a "grown" tree whose
   limbs are the authored `Form`'s with noise; a "fitted" genome that was set by hand).
2. A gate's number lowered, a shot dropped from a fixed list, a frame count shortened, a device
   row left blank because the device "wasn't to hand" (that is a **Blocked** note, §0.9, not a
   blank), a source replaced by an easier reference after review, or unknown source settings
   presented as Epic or real-time.
3. A number copied from an earlier record in place of a measurement made now, on this commit,
   with the command the record names.
4. A measurement on a cold machine presented as a sustained one, or on a machine with something
   else building.
5. A fit, a growth or a style tuned against the shots it's judged on (the panel's twelve shots
   are judged, never fitted to: a fit uses the lookbook's other shots or its own references).
6. An `estimated` left in place after a gate measured it, or a measured number without its
   command and date.
7. A step marked done by a sentence ("the tests pass", "the numbers are close") rather than the
   pasted output.
8. A `Blocked` note written before every lever the step names, and several the implementer
   found itself, have been tried and recorded.
9. Dropping a step, or reordering without the reason in the commit.
10. A judgement made by the implementer where the plan says a separate agent judges.

### 0.4 Decisions reserved for Ryan

The agent builds each default; Ryan can flip it in review. D10–D26 are plan two's and stand as
built (`games/last-green/design.md`).

| ID | Decision | Default the plan builds (**proposed**) |
|---|---|---|
| D27 | The claim's wording | §0.1 as written, once §H measures it |
| D28 | The showcase | A new program, `examples/vale`: a 1 km² grown valley with a fixed 1,200-frame walk and twelve fixed shots (§B1), built on `land` and the engine's frame, with no game sim. The Last Green keeps its lookbook and adopts each part's capabilities behind switches at the end (D40). The vale supplies consistent Wrela shots for judging progress against public references by subject, without rebuilding their scenes. |
| D29 | The quality references (revised by Ryan, 2026-10-28) | A fixed collection of freely licensed **photographs and footage of the real subjects** (§B3): a temperate broadleaf wood's floor, its canopy against the sun, an old trunk, a woodland stream and fall, a lichened rock face, a meadow's edge, dawn mist, a valley at dusk, a grazing and a walking deer, deadwood and fungi, an overcast interior. CC0, public domain, CC BY or CC BY-SA, from hosts whose terms allow the download (Wikimedia Commons first, its Quality and Featured images preferred). Cite the photographer, source URL, licence, date and place where known; record disclosed processing (HDR merges, stacking, heavy grading) and prefer unprocessed. Unreal renders are not references (none eligible was found); public engine benchmarks are optional context with original conditions. No Unreal installation, Fab acquisition or local Unreal timing row. |
| D30 | Substance and style apart | The frame emits substance buffers (§D1: what's there, and how light reaches it, as separate terms) and `engine::style::Style` is a trait with slots (normal, shading, palette, marks, temporal, lens); `Frame<R: Resolver, S: Style>`. A style is a file under `content/styles/`; adding one changes nothing under `engine/` (G-style). |
| D31 | Two style layers | World style (the genomes' allometry and exaggeration: how things grow) and image style (the `Style`: how they're drawn) are separate, named layers; a `Style` never moves a vertex. |
| D32 | Everything specialized | Every `Quality` switch that gates shader code, and every `Style` slot, is a `const` parameter of the entry points it reaches (spec §18); the program's quality and style are `const` items; the compiler builds one renderer per program. The clearing's 12.65 ms is the cost of not doing this (B16's bisect). |
| D33 | The vale's plants are grown | Every tree, shrub, fern, grass, herb, moss, lichen, litter and fungus in the vale comes from a growth program (§E) run on a species' genome and its site's history (`land::grow::Tree`, `land::bake::tile_fields`); no `Form`, no `SPECIES` row, no hand-placed instance. The land's relief and water stay as `land` already evolves them (D33 doesn't rewrite `landform`). |
| D34 | Nothing unsteerable ships | Every growth program has an inverse: `wrela tool grow --fit <image> --species <s>` (§E6) finds a genome and site that reproduce a silhouette, scored by IoU; a structured goal (`--goal goal.toml`: age, exposure, events) does the same from words. A growth program without a working inverse is a **Blocked** step. |
| D35 | Creatures: author the genome, grow the organism | The authored part of a creature is its body plan (a `content/kit/quadruped.wrela` skeleton and at most twelve shape handles) and its face, eyes and hands; its coat (reaction-diffusion), its proportions (allometric growth from juvenile to adult), its musculature (from bones and load) and its gaits (optimized in simulation) are grown. |
| D36 | Marks live in the field's domain | A style's marks (lines, dabs, strokes, hatching) are anchored in the field's 3D domain and sized in screen space; their coherence is judged by `flicker`, `pop` and `trail` on strips (docs/lookdev.md), never by stills alone. |
| D37 | Devices are tiers | M4 Air: the visual target tier (`Tier::Epic`, every switch on), and the only device this plan measures on (Ryan's revision, 2026-10-28). `Tier::High`, `Medium` and `Low` (proposed for an M1 Air, an Iris Xe laptop and a phone) are built and measured on the M4 for their relative cost; their own devices' rows are a later plan's. Each tier is a `const` set the compiler builds a renderer for (§C7). |
| D38 | The reference review (revised by Ryan, 2026-10-28) | **Agents alone judge** until the plan is finished: eight agent judges in fresh contexts (§B4) supply subjective rubric scores and their tally is the verdict; Ryan is asked for no review, verdict or acceptance before §H's PR, and judges the finished work then if he chooses. Because no person views a reference, only references whose permission covers a local copy shown to an agent judge are eligible (§B3.1); the judges are calibrated on two controls made only from Wrela-owned stills. Source credits remain visible where required; reviews are labelled attributed or blinded, with blinding optional only where permissions allow. Freeze and hash both the twelve Wrela shots and the source mappings at §B5. This is a look-development check, not a scientific study. |
| D39 | Sustained measurement | Every frame-time gate on the M4 Air is measured after ten minutes of the walk (`wrela tool bench --sustained 600`, §C3), with minute 1's and minute 10's GPU medians both recorded. A number from a cold machine is a different number and says so. |
| D40 | The Last Green adopts, it doesn't lead | After Part G, the floor gets the grown species (§E) behind `Stage::grown`, the cel style as `games/last-green`'s `const STYLE` with the physical style's pixels as its G-pixels baseline, and the specialized switches; each behind a switch, each with G-look on the floor's 23 shots, in Part H. The floor's design contracts (`design.md`) gain the rows. |
| D41 | No neural step | Plan two's frontier switches stay off and are not revisited. The learned style this plan first drafted (a net trained on painted frames, giving a pixel's final colour) is **deferred, not built**: `docs/citations.md`'s rule (no generative image model makes anything in wrela) covers it until Ryan says otherwise in writing; §D9 holds the question. |
| D42 | Individuals near, archetypes far | A grown tree within `NEAR` (120 m, **estimated**) of the eye is its own individual (its own grown limbs and crown cards); beyond, each species' trees are drawn as the nearest of its grown archetypes (age class × light class × exposure, §E1.4), whose impostors and volumes are cooked once a kind. The memory gate (512 MiB) decides `NEAR` and the archetype count. |

### 0.5 The gate kinds

Plan one's and plan two's gate kinds are reused by name and are not restated: **G-sim,
G-commands, G-pixels (identical / within n/255), G-draws, G-suite, G-golden, G-pipelines,
G-profile, G-both, G-size, G-bench, G-cold, G-warm, G-hitch, G-device, G-memory, G-stills,
G-spike, G-look, G-precision** (`docs/plan-world-class.md` §0.5, `docs/plan-world-class-2.md`
§0.5). The baselines for every part are `build/base-0.9/<name>`, made at §B0. G-look's four parts
(pairs, the reference error, the artifact measures, the visual judgement) apply to every step that
changes the image on purpose, with the vale's twelve shots as the named set where the step
touches the vale and the floor's 23 where it touches the floor.

**Judging is by agents alone** (D38, Ryan's 2026-10-28 revision). Where plan two's G-look asks
for Ryan's verdict (its part (4)) or lets a measure rise "only with Ryan's acceptance in For
Ryan" (its part (3)), this plan replaces him with the agent judges: part (4)'s blind judge is
an agent in a fresh context, calibrated as before, and its preference with the measures
decides; a measure that rises is allowed only when the commit names the rise as deliberate and
the calibrated agent judge preferred the "after" on every named shot, else the step is not
done. No step waits on a person's judgement. Decisions that aren't judgements (a **Blocked**
note, a reorder, a paid asset, a rule change) still go to Ryan as §0.9 says.

The instrument's pixel error, PSNR, SSIM and coherence measures still compare Wrela with its
own reference render or previous build. They are not computed between unrelated public scenes.
Public-reference comparisons use the visual rubric and the cited source conditions only.
New gate kinds:

- **G-reference:** the subjective reference review (§B4, D38) over the twelve sealed evaluation
  shots, each comparison the vale's still and a freely licensed photograph of its subject.
  Review light, colour, value, depth and artifacts, allowing for different scenes and source
  conditions. Show media at its native aspect ratio, label resolution and compression limits,
  and do not present resizing as recovered detail. A missing or unviewable source is not equal.
  Three rules, all required: (a) **overall**: of every judgement (eight judges × twelve pairs =
  96), the share that prefers the vale or rates the pair equal (both stills within one rubric
  point on every row) is at least 50 %; (b) **per pair**: no pair is lost worse than 6 judges
  to 2; (c) **majority**: at least 6 of 12 pairs are won or equal by the judges' median. A panel
  of six pairs at 5–3 for the vale and six at 2–6 fails (a) at 42 of 96 although it passes (b)
  and (c): the tally's tests hold that case. The eight agent judges' tally is the verdict (D38). The
  panel's source-manifest hash, review mode, order keys, every judge's scores and every
  calibration attempt are in the record. The numerical thresholds are internal quality targets,
  not statistical evidence of engine equivalence. On eight development pairs use the same
  overall and per-pair shares, with at least four pairs won or equal. Run at §B5 (the baseline:
  today's renderer), after Part D (the physical style on the specialized frame), after Part E,
  after Part F, after each Part G step, and at §H.
- **G-sustained:** `wrela tool bench --scene <s> --sustained 600`: the walk paced for ten
  minutes, then 1,200 measured frames; the step's numbers hold on the measured frames, and so
  does **throughput**: the 1,200 frames **begin** within 20.4 s of elapsed time (at least 58.8
  frame starts a second: 60 within 2 %) and at most 12 of their intervals (1 %) are over 25 ms
  (a missed refresh), with **one frame in flight** (§C3): the page records a frame's beginning
  (`began_ms`) and, with `inflight=1`, a frame's start waits for the GPU completion of the
  frame before the previous one, so a sustained start cadence bounds the GPU's throughput from
  below. This is a frame-start proxy: WebGPU exposes no presentation time (the council's R3
  B on F3), the record says so, and the claim's words are "begun", not "presented". A stream of 19
  ms intervals (52.6 fps) passes the p99 and the over-33
  count and fails throughput; the report's tests hold that case. Minute 1's and minute 10's GPU
  medians are both in the record, and the ratio is reported (a machine that throttles shows it;
  the gate is the warm number).
- **G-tier:** the scene built at each tier (`--set TIER=…`) and measured on the M4 with the
  sustained bench: each lower tier's GPU p50 is under the tier above's, the four rows in
  `docs/bench.md` with the tier, the browser and the date (D37; other devices deferred).
- **G-grown:** a test that walks the vale's instances and asserts each plant's provenance is a
  growth record (`Grown { genome, site, seed, program_version }`), none `Authored`; the share of
  authored plants is 0; the growth of the vale's region takes under the step's seconds on the
  M4 Air, measured; the grown structures' hash (§E7) is byte-identical across two runs and between the native
  host and Chrome on the M4.
- **G-steer:** `wrela tool grow --fit <silhouette.png> --species <s>` reaches IoU ≥ 0.85 against
  the silhouette within the step's time, on the step's named test images (held out from any
  fit's references); `--goal` reproduces each named goal's measurable properties (height within
  10 %, crown asymmetry toward the named light, a lightning scar where named).
- **G-style:** `wrela tool styles --check`: for each style under `content/styles/`, the vale
  builds and `wrela pipelines` lists the style's pipelines; `git diff --stat main..HEAD --
  engine/` is empty for the commit that adds a style (the check is a test the part's suite runs:
  it diffs the engine between the commit before the style and the commit with it).
- **G-motion:** the subjective motion review (§F9): the vale's three-second creature strip and
  freely licensed footage of a real deer walking (§B3), judged on the motion rubric with differences in
  species, path, camera and playback rate recorded; the step's preference share holds; the gait
  tests (§F4: slopes to 30°, steps, a stumble recovered) pass.

### 0.6 What this plan leaves out, and why

- **Matching Unreal's breadth.** No editor, no console targets, no MetaHuman-grade human faces,
  no external asset import pipeline for a local reference project. The plan wins on axes a field
  engine
  owns; it doesn't chase the rest.
- **Realistic humans.** Faces and hands are the hardest problem in real-time graphics for every
  engine. D35 keeps them authored handles; the vale's creatures are animals; the climber keeps
  its stylized face. A human-faces plan is its own.
- **Hardware ray tracing, mesh shaders, bindless, 64-bit atomics, subgroup matrices.** Not in
  WebGPU on Chrome stable. Quality comes from structure (the field, the compiler), not from brute
  force; the plan says so in each lighting step.
- **Rain, snow, seasons' change at run time.** The vale is one season, one day; the sky's
  weather and time of day move as they do today. Seasons as growth (a tree's leaves by date) are
  a later plan.
- **Other devices.** The M1 Air, the Iris Xe laptop and a phone are deferred to a later plan
  (D37); the tiers are built for them and measured on the M4 only. Compatibility mode and phones
  as a target stay out.
- **Plan two's frontier switches** (neural impostors, the learned rejector, radiance cascades).
  They stay off with their numbers; D41 allows no neural step in this plan (the learned style is
  deferred, §D9).
- **The growth of the land itself.** `land::landform` already evolves relief by uplift, stream
  power and creep; `land::water` lays the water; `land::soil` makes soil. The plan uses them as
  they are and grows what stands on them. A rewrite of the landforms is out of scope.

### 0.7 The order, and what needs what

```
Part A begins without an Unreal preflight. Public-source discovery and rights checks (§B3)
can begin alongside it; the manifest freezes only after B1 and B2 define the comparisons.

Part A (toolchain 0.10.0)      Part B (the references and baseline)
A1 associated consts ─────┐    B0 pin, baselines
A2 passes to 32 bytes     │    B1 the vale ◄── A3, A4, A6
A3 sampled fields         │    B2 reference manifest and subject mappings ◄── B1
A4 struct update          │    B3 public sources and rights ◄── B2
A5 const cache, report    │    B4 the review tool
A6 --set                  │    B5 baseline panel ◄── B3, B4
A7 integration ◄──────────┘    B6 device rigs
A8 spec, release               B7 plan two's Part C fixed (wrela-others)

Part C, order within: C3 (the sustained bench), C1, C2, C4, C5, C6, C7       ◄── B
Part D, order within: D1, D2, D3, D7a (marks), D4, D5, D6, D7b (coherence), D8, D10, D11   ◄── C
Part E, order within: E0, E2 (the history's light and events), E1, E3, E4, E5, E6, E7, E8 ◄── D
Part F: F1 … F10 ◄── E        Part G: G0, then the ranked items ◄── F10        Part H ◄── G
```

An agent that reorders must keep the `Needs:` lines. The panel runs at B5 before any renderer
change, so every later panel measures a change against a baseline the same panel saw.

### 0.8 Where things live

**The toolchain** (`~/projects/wrela-compiler`, tag `v0.9.0`; line numbers are that tag's, and a
line the implementer finds moved is to be re-anchored in the step's commit, not guessed):

| Concern | Where |
|---|---|
| `const` parameters of entry points | `spec/language.md` §18 ("`const` parameters"); `compiler/sema/src/check/call.rs` (E0602, E0603); `compiler/lower/src/gpu.rs` (`PipelineKey`, the `constants` of a recorded pipeline); the manifest's `"constants"` |
| Constants computed at build, `@fuel` | `spec/language.md` §13.1; `compiler/driver/src/consts.rs` (the wasmtime run, the cache under `build/consts/`) |
| Derivations: `gradient`, `interval`, `filtered`, nesting | `spec/language.md` §14; `compiler/ir/src/derive/{mod,ad,interval}.rs`; `compiler/std/derive.wrela` |
| Tapes, `Surface`, `interpret` | `compiler/std/stage.wrela`; `compiler/std/field.wrela`; §14.4 |
| Storage textures, 3D textures, multi-target passes | `compiler/std/gpu.wrela` (`Texels3d`, `begin_pass2_with_depth` and its siblings; the eight-storage-buffer limit per stage: `docs/toolchain.md`) |
| The store and storage | `compiler/std/store.wrela` (a server's SQLite); `std::io::storage` (a page's) |
| Determinism | `spec/language.md` §12 |

**This repository** (`~/projects/wrela`, `main` at a55a6001; line numbers are that commit's):

| Concern | Where |
|---|---|
| The frame | `engine/frame.wrela:81` (`trait Scene`), `:615` (`Quality`: `cascades`, `neural`, `learned_resolve`, `distance`, `fog`, `clouds_halved`, …), `:1034` (`Frame`), `:2509` (`draw_scene`), `:2858` (`finish`: the resolve, the look, the bloom, the develop) |
| The look today | `engine/paint.wrela:35` (`Look`: dabs, tooth, bloom, lines, sharpen, vignette, `output`), `:354` (`Develop`), `:482` (`lines`), `:502` (`dab_map`), `:624` (`develop`) |
| The lighting | `engine/lit.wrela:25-60` (`Lighting`: the camera, the light, the shadows, the probes and their sights, the clouds, the air, the occlusion, the lights, the mirror, the distance volume, the fog), `sun_at`, `sun_and_sky`, `sky_at`, `through_air`; `engine/light.wrela` (the BRDF) |
| The tags a pixel carries | `engine/view.wrela` (`class_of`, `BARK`, `LEAVES`, `GRASS`, `FLOWER`, `GROUND`, `ROCK`, `WATER`, `CREATURE`) |
| Plants as authored today | `engine/plant.wrela:1-32` (the module's comment), `:33` (`Clump`), `:75` (`Leaves`), `:96` (`Bark`), `:135` (`Plant`), `:540` (`cook_cards`); `games/last-green/trees.wrela` (`Form`, `broadleaf`, `great_tree`, `plants()`, `far_kinds()`) |
| The wood (instances, levels, impostors) | `engine/wood.wrela:1-30`; `engine/impostor.wrela`; `engine/lod.wrela`; `engine/vegetation.wrela` |
| Grass and the floor's flora | `engine/grass.wrela:481` (`Species`), `:498` (`SPECIES`, 33 rows), `:942` (`Flora`), `:967` (`species_of`), `:225` (`place`), `:1507` (`Contact`) |
| The forest's history | `land/grow.wrela:1-28` (the module's comment), `:269` (`Habit`), `:331` (`Cause`), `:343` (`Tree`: `at`, `kind`, `born`, `tall`, `reach`, `lean`, `girth`, `light`, `vigour`, `state`, `died`, `fell`, `fall`, `cause`), `:2406` (`simulate`); `land/bake.wrela:1-30`, `:900` (`tile_fields`: light, litter, thorn, wear, wetness) |
| The land | `land/landform.wrela:1-18` (`evolve`: uplift, stream power, diffusion), `land/soil.wrela` (`SoilKind`), `land/water.wrela`, `land/wild.wrela` (the wildwood a tile at a time), `land/place.wrela` (the map) |
| Creatures | `engine/creature.wrela:106` (`Skeleton`), `:2039` (`trait Creature`), `:2120` (`Parts`); `content/kit/quadruped.wrela` (`body(q, look)`), `content/kit/anatomy.wrela` (muscles, bones, `painted`); `engine/physique.wrela` (`fit_capsules`, `adaptive_physique`); `engine/gait.wrela:32` (`Walk`), `:59` (`Walker`, `SPRINGS`), `:209` (`stand`), `:323` (`step`), `:416` (`energy_of`) |
| The distance volume, the cascades' march | `engine/distance.wrela` (`Distance`, `DistanceGrid`, `distance_at`, `contact_through`); `engine/cascades.wrela:83` (`march`, `HIT`, `CLOSE`) |
| Temporal AA | `engine/temporal.wrela` (`Resolve`, `Found`, `found`, `blended`) |
| Neural | `engine/neural.wrela` (`Model`, `forward`, `eval_gpu`, `unhalves`); `studio/fit/main.wrela` (`wrela tool fit`), `studio/fit/rejector.wrela` |
| The lookdev instrument | `studio/lookbook/main.wrela` (the tool), `quality.wrela` (`measure`, `judged`, the controls), `measures.wrela` (`psnr`, `ssim`, `sharpness`, `flicker`, `pop`, `trail`, `edge_width`); `docs/lookdev.md` |
| The bench | `studio/bench/main.wrela` (`measure`, `--lan`, `--row`), `report.wrela` (`hitches`, `spans`, `stats`), `scenes.wrela` (the scenes and their scripts) |
| Export | `studio/export/main.wrela` (`height`, `path`) |
| The clearing (a standing scene, the vale's model) | `examples/clearing/main.wrela:1-80` (`quality`, `METERING`, the cascades), `stage.wrela` |
| Tool registration | `wrela.toml:100-230` (`[tools.*]`: `package`, `write`, `read`, `browser`, `run`) |
| Records | `docs/bench.md` ("Whether D10 holds", "Other engines on this machine", "Devices", B14's experiments), `games/last-green/design.md` |

### 0.9 The contract: no step is skipped

Plan two's §0.9 holds word for word, and is restated here because it is the part of a plan an
implementer most wants to forget:

- **A step is done only when every line of its "Done when" holds,** each checked by a command, a
  test or a number, never by a sentence; the commit pastes the number or the output beside each
  line. A step with sub-steps has a checklist per sub-step and each sub-step is its own commit.
- **A target has a *now* and a *target*.** The *now* is the number the previous step or
  `docs/bench.md` recorded; after each lever the implementer measures again and writes the
  trajectory in the commit (`888 → 760 → 640 → 505 MiB`). A trajectory that doesn't move after a
  lever says so and names the next lever before trying it.
- **"Too hard" is not an outcome; keep climbing** (Ryan's revision, 2026-10-28). Each step's
  levers are ordered so the first is reachable in a day; the implementer takes them in order and
  records each. When the named levers are spent and the line still doesn't hold, the implementer
  uses its own judgement and errs toward hill climbing: it finds the next lever itself (from the
  measurements, the profile, the hitch table, the panel's sentences), tries it and records the
  trajectory, and keeps going while the number moves. Only when several further levers have
  stopped moving it does the line become a **Blocked** note in the commit (the measurements,
  everything tried, the best next idea); the run does **not** stop: the implementer carries on
  with the steps that don't depend on it, and returns to it when a later step gives it a new
  lever. A Blocked line is never deleted, softened or declared wrong; the part's PR lists every
  one for Ryan.
- **A hypothesis step's experiment is mandatory.** Its code behind its switch, its measurement
  and its record in `docs/bench.md` are the deliverable; only the switch's default depends on the
  result. An experiment that is never run is a skipped step.
- **Reordering is the implementer's call where a dependency or a Blocked line needs it**, the
  reason in the commit; **dropping a step is never allowed.**
- **A step that changes the image satisfies G-look as well as its checklist;** a step that
  changes none holds its pixel gate. Neither is traded for the other.
- **Every estimate is replaced by a measurement** the first time a step measures it, in the code
  and in this plan's copy in the PR.
- **The judge is never the implementer** (D38, G-look (4)). Where a step says a separate agent
  judges, the implementer spawns one in a fresh context with the pair and the rubric alone, and
  records the order key it was not told.

## Part A: toolchain 0.10.0 (`rywible/wrela-compiler`)

What the engine's parts need of the language, the GPU surface and the build, phrased as general
capabilities. Each step follows `docs/release.md` there: a spec change in `spec/language.md`,
tests in the compiler's suite, a `CHANGELOG` line. Nothing here names the engine.

### A1. Associated constants in traits, and `const` arguments from them

Today a trait declares functions only; a `const` argument of an entry point must be a literal, a
negated literal or a `const` item (spec §18, E0603). A generic frame that is specialized per
program needs a type parameter to carry constants.

- `trait T { const K: u32; const ON: bool = true }`: an associated constant, with an optional
  default; `impl T for X { const K: u32 = 4 }`; read as `X::K` or, under a bound `S: T`, as
  `S::K`, which is a constant of the instantiation (monomorphization substitutes it; the fold
  after inlining sees a literal). Allowed types: what a `const` parameter allows (`bool`, `u32`,
  `i32`, `f32`) plus a fieldless enum and a `Text`.
- A `const` argument of an entry point may be `S::K` under a bound (E0603 lifted for this form):
  the recorded pipeline is per instantiation, `blur::<taps = 4>` as today; `wrela pipelines`
  lists it so.
- A `const` item of a generic `impl` block may read the parameter's constants (`const N: u32 =
  S::K * 2`).
- **Tests:** a generic `fn frame<S: T>()` that dispatches `blur.bind(taps: S::K)` builds two
  pipelines for two `S`s and reads back different results; a `bool` constant that is `false`
  for one `S` leaves no branch in that instantiation's WGSL (`wrela pipelines --json` names the
  constants; the WGSL for the `false` instantiation lacks the branch's code: checked by a
  readback that the branch's effect is absent and by the shader's byte count being smaller);
  a missing associated constant without a default is an error naming the trait.

**Done when:** (1) the spec's §3 (traits) and §18 (`const` parameters) say so; (2) the tests
pass; (3) E0603's message names the allowed forms.

### A2. Passes with up to eight colour targets, within the attachment byte budget

`std::gpu` has `begin_pass` to `begin_pass4` and their `_with_depth` forms
(`compiler/std/gpu.wrela:2358-2561`). WebGPU allows eight colour attachments but bounds their
bytes per sample: `maxColorAttachmentBytesPerSample` is 32 by default (an adapter may offer
64), and the compiler already refuses a pass past `MAX_COLOR_ATTACHMENT_BYTES` (32: E0602,
`compiler/lower/src/gpu.rs:1135`, `runtime/abi/src/stream.rs:702`). Eight `Rgba16Float` targets
would be 64 bytes: the first draft of this plan assumed them and was wrong (the council's R1
F1). So: add `begin_pass5` to `begin_pass8` and their `_with_depth` forms, generated as the
existing ones are (the comment at `:2636` names the generator's shape); keep the byte check at
32 and make its message name the bytes each target takes and the total; and add
`attachment_bytes` per render pipeline to `wrela pipelines --json`, so a program's layouts are
visible. No capability negotiation: the engine's layouts fit 32 bytes (§D1), and a layout that
doesn't is a compile error, not a run-time fallback. The attachment cost is the ABI's per-format
render-target cost (`runtime/abi/src/stream.rs:558`: an `Rgba8` target costs 8 attachment bytes
although it stores 4; the council's R2 F1), not the texel's size; §D1's layout is counted that
way, with the formats the ABI has (`Rgba16Float`, `Rg32Uint` at cost 8, `R32Uint` at 4).

- **Tests:** a pass into six `R32Uint` targets (24 attachment bytes) writes six distinct
  values read back; a pass into eight `R32Uint` targets (32) with depth; nine targets, and
  eight `Rgba16Float`s (64 bytes), each fail with E0602 naming the bytes; four `Rgba8`s report
  32, not 16; `wrela pipelines --json` reports the bytes; §D1's exact layout (an `Rgba16Float`,
  two `Rg32Uint`s, two `R32Uint`s) compiles, reports 32 and round-trips its packed channels on
  both hosts.

**Done when:** (1) the forms exist on both hosts; (2) the tests pass; (3) spec §18's table of
passes lists them and the byte limit.

### A3. A texture as a field's data

`docs/toolchain-gaps.md`, "A texture can't be a field's data": `std::field::Surface` needs
`Copy + GpuData`, and a `Texture3d` is neither; a `borrow` field can't be part of an ordinary
struct (E0519). A field whose data lives in a texture (a cooked distance volume, a grown
organism's density) can't be a `Surface`, so a march generic over `Surface` can't take it.

- `std::field::Sampled3d<T>`: a surface whose distance is a trilinear read of a `Texture3d<T>`
  over a grid (origin, voxel, dims), implemented as a GPU-only `Surface` whose texture is bound
  as a resource of the entry point that takes the field (the way a `borrow struct` binds its
  textures: the compiler lifts the texture out of the field into the pipeline's bindings and
  rewrites the field's `distance` to sample it). The field is `Copy` on the CPU as a handle and
  `GpuData` as its grid; the texture travels beside it in the binding, not in the struct.
- **Tests:** a `march` generic over `Surface` over a `Sampled3d` of a cooked sphere hits at the
  sphere's radius within a voxel; the same code over an analytic sphere hits exactly; a
  `Sampled3d` inside a `Union` with an analytic field composes.

**Done when:** (1) the form exists and the gap moves to "Fixed" naming 0.10.0; (2) the tests
pass; (3) spec §20 (fields) documents it.

### A4. The struct update that made malformed IR

`docs/toolchain-gaps.md`, "A struct update of the floor's stage is malformed IR": `Stage {
neural: …, ..stage() }` with a field holding a generic creature body failed the IR verifier
("v1862 is a `Cast<Coat, Body<…>>`, not a `Cast<Coat, Grown, FawnLook>`"). Cut it down to a
program of under 40 lines (a struct with a generic field whose cast differs from another
field's, updated from a function's result), fix the lowering, and keep the small program as a
test.

**Done when:** (1) the small program is in the compiler's tests and passes; (2) the gap moves
to "Fixed"; (3) the floor's `init` can use the update (a note in the engine's PR: Part B's B0
tries it and records).

### A5. The constant cache keyed by everything a value depends on

`docs/toolchain-gaps.md`, "A package's const cache can outlive what it was computed from"
(`compiler/driver/src/const_cache.rs`): a `.wcv` from an earlier build was read in place of the
current sources' value. Make the key a hash of: the constant's compiled WASM (the initializer
and every function it reaches, after inlining), the toolchain's version, the build's options
that affect CPU code, and every `embed`ded file's bytes it reads. Add `wrela build --report
consts`: each constant's name, bytes, fuel used, seconds, and whether it was computed or read
from the cache.

- **Tests:** a constant computed, a dependency's function it calls changed, the package rebuilt:
  the new value; the toolchain's version in the key: a different version's cache directory is
  not read; `--report consts` names a cached read as cached.

**Done when:** (1) the key as above with the tests; (2) the report exists and is documented in
`docs/toolchain.md`; (3) the gap moves to "Fixed".

### A6. A build-time override of a program's `pub const`

A program names its tier and its style as `pub const` items; a device rig builds the same
program at another tier without a second `main`. `wrela build <pkg> --set TIER=Medium --set
STYLE=Cel`: overrides a `pub const` of the program's root module whose type is a primitive, a
fieldless enum or a unit struct that is a `Style`-like marker (a `pub const STYLE: Text` naming
the type is enough if a type can't be a value: choose the form the language already has and
record it). The override is part of the build's identity (the manifest records it; `wrela same`
treats two overrides as two builds). `std::tool::toolchain::build`'s options gain `set`.

- **Tests:** a program with `pub const N: u32 = 4` built with `--set N=8` reads 8 at run time
  and its constants fold with 8; an override of a non-`pub` or non-primitive constant is an
  error naming the rule; the manifest records the override.

**Done when:** (1) the option on the CLI and in `std::tool::toolchain`; (2) the tests pass;
(3) `docs/toolchain.md` documents it.

### A7. Integration against the engine, before the tag

With `WRELA_TOOLCHAIN=~/projects/wrela-compiler` in `~/projects/wrela`: `wrela suite` passes
every member; B0's baselines are identical to their 0.9.0 builds (`wrela same` ×5); Part C's
C1 compiles as a smoke test on a branch (not merged: a `Frame<R, Q: Switches>` with one program,
thrown away) to prove A1 and A6 carry what C needs.

**Done when:** (1) the suite's count pasted; (2) `wrela same` ×5 identical pasted; (3) the
smoke branch's `wrela pipelines` output pasted in the PR.

### A8. Spec, docs and the release

`spec/language.md` (§3 traits, §13 the cache and the report, §18 passes and `const` arguments,
§20 `Sampled3d`), `CHANGELOG`, `docs/release.md`'s steps; the PR `Release 0.10.0: associated
constants, eight-target passes, sampled fields, a build-time const override, and the fixes to
the struct update and the constant cache`.

**Done when:** the release's checklist in `docs/release.md` is satisfied and the tag's workflow
is green; `docs/toolchain-gaps.md` in the engine repository moves A3, A4 and A5's gaps to
"Fixed" at B0.

---

**Done when, Part A:** every step's checklist holds; 0.10.0 is tagged; the engine's suite passes
on it.

## Part B: references and baselines (`rywible/wrela`, and `rywible/wrela-others` for §B7)

Nothing in this part changes how the engine draws. It builds the thing to be judged (the vale),
the public references to judge its look against, the review tool, the device rigs, and the
baseline every later part is measured from. Source selection and rights records are part of
the work. They require no local Unreal installation or reference-scene reconstruction.

### B0. Pin 0.10.0 and save the baselines

- Before the pin moves, build the baselines on 0.9.0: `wrela build --release` of
  `games/capybara`, `examples/herd`, `examples/climber-lab`, `examples/clearing` and
  `games/last-green` into `build/base-0.9/<name>` (the same five plan two's §B0 saved; the
  procedure in `docs/toolchain.md`). These are G-pixels' baselines for every part.
- `wrela.toml`: `toolchain = "0.10.0"`. `wrela suite` on 0.10.0.
- `docs/toolchain-gaps.md`: each gap Part A fixed moved to "Fixed", naming 0.10.0.

**Done when:** (1) the five baselines exist and `wrela same games/capybara --against
build/base-0.9/capybara` prints identical on 0.10.0 (paste); (2) `wrela suite` passes every
member (paste the count); (3) the gaps doc moved.

### B1. The vale (`examples/vale`)

A grown valley, drawn by the engine's frame, with no game sim: a 1 km² square of land evolved by
`land::landform`, water laid by `land::water`, a forest history by `land::grow`, the floor's
species as its first plants (Part E replaces them with grown ones), a 1,200-frame walk and twelve
fixed shots. It is the showcase; §B2 maps its subjects to public references. Its composition
and world design are original, not reconstructions of the source images.

**B1.1 The floor's tree kinds move to content.** `games/last-green/trees.wrela`'s kinds
(`Form`, `broadleaf`, `great_tree`, `kind_plant`, `plants()`, `far_kinds()`, the species'
constants `GREAT`, `OAK_OPEN`, …) are game code the vale can't depend on. Move the species and
their forms to `content/trees/` (a package `trees`, workspace member), keeping every number;
`games/last-green/trees.wrela` keeps what is the game's (the great tree's place in the map, the
wildwood's kinds) and imports the rest.

- **Gate:** G-pixels identical for the floor at 1280×720, frame 240 of the walk, against
  `build/base-0.9/last-green` (the move changes no number).

**B1.2 The land.** `examples/vale/land.wrela`: the square from (−512, −512) to (512, 512) m, a
rock field (`land::landform`'s rock: a soft rock with one hard band crossing the valley at a
third of its length, **proposed**: the band is what makes the fall), an uplift map (a dome
highest at the head, **proposed** 400 m of relief), base level at the mouth; `evolve` at the
bake's grain to a relief with one main stream from the head to the mouth, one fall where the
hard band crosses it (≥ 4 m tall), one mere above the fall; `land::water` lays the water;
`land::soil` the soil. The vale is built at build time as the floor's history is (`land::bake`):
a `const` with `@fuel` (spec §13.1) or a build-time bake file, whichever the floor uses today
(read `land/bake.wrela` and do the same).

- **Tests** (`examples/vale/tests/land`): the stream is one connected path from the head to the
  mouth; the fall's drop is at least 4 m over at most 6 m of its course; the mere's level is
  above the fall's lip; soil kinds include `Alluvium` on the valley floor and `Thin` on the
  crests.

**B1.3 The stage, shared with the floor.** The floor's `games/last-green/stage.wrela` streams
tiles, loads the wood, builds the far layer, chooses casters, supplies solids and fog, and holds
the game's things (the tower, the ring, the blockouts, the climber). Factor what is any
landscape's into `land/stage.wrela` (`land::stage::Landscape`: the tiles round the eye from
`land::stream`, the wood's `load`/`drop`, the far layer, `Scene::solids`, `Scene::fog`,
`casters_in`, `probe_grid`, `trace_of`), generic over what a program adds, so the floor's `Stage`
holds a `Landscape` and its own things, and the vale's `Stage` holds a `Landscape` and nothing
else. The vale's tiles are 128 m, 64 of them, streamed as the floor streams (the memory gate
decides how many are held; the first frame's payload holds the far layer).

- **Gate:** G-pixels identical for the floor (as B1.1); the vale draws its forest from its
  history (a still at the vale's centre shows trees; a test counts loaded instances > 1,000
  after the arrival).

**B1.4 The walk and the twelve shots.** `examples/vale/lookbook.wrela` (an `engine::shot::Shot`
list, chosen as a player chooses one: L, the number, Enter) and `examples/vale/walk.wrela` (a
1,200-frame camera path through at least six of the twelve places, paced as the bench paces).
The implementer places the shots; the plan fixes their **classes**, one each, in this order, so
the set can't drift toward what the renderer does well:

| # | Shot | What it tests |
|---|---|---|
| 1 | the forest floor at noon: the eye at 1.6 m looking along the ground through ferns and litter, trunks beyond | ground cover, the materials of the floor, dappled sun |
| 2 | the canopy against the sun: looking up through leaves, the sun behind them | translucency, the leaves' edges, bloom, the sky |
| 3 | an old tree's trunk at 2 m: bark, a buttress, moss on its shaded side | bark material, curvature, moss as a cause's result |
| 4 | the stream and the fall at 15 m | water, spray, wet rock, foam |
| 5 | the rock face with its strata and lichen at 4 m | rock material, the band, lichen placement |
| 6 | the meadow's edge at golden hour: grass and flowers, long shadows, the wood behind | grass, shadows' softness, colour |
| 7 | dawn fog in the hollow: the sun low through mist | fog, shafts, aerial perspective |
| 8 | dusk from the crest: the vale wide, the mere catching the sky | far detail, impostors, the sky, reflections |
| 9 | the creature grazing in dappled light at 6 m | coat, form, the creature's light |
| 10 | the creature walking toward the eye at 10 m (a three-second strip) | motion (§F9's review reads the strip) |
| 11 | deadwood and fungi at 1.5 m: a fallen log, rot, fungi, litter | the history's consequences, small forms |
| 12 | the wood's interior under overcast: no sun, the probes' and sky's light alone | indirect light, the probes, no shadows to hide behind |

Each shot fixes its time of day and weather (`Shot::time`, `Shot::weather`; the vale's day runs
as the floor's does). Once §B5 has run the panel on them, the list is frozen: its hash (the
shots' eyes, aims, times and weathers, `sha256` over `lookbook.wrela`'s `SHOTS` as JSON) is
recorded in `docs/bench.md` "Public visual references", and every later panel asserts the same
hash. A shot
is replaced only by Ryan, in writing, and the replacement's panels start again.

**The eight development shots.** Fitting and tuning never touch the twelve (§0.3's shortcut 5).
`lookbook.wrela` also holds eight **development** shots, `D1`–`D8`, one each of classes 1, 2,
3, 4, 6, 8, 9 and 12 in that order (`D1` the floor, `D2` the backlit canopy, `D3` a trunk, `D4`
water, `D5` the meadow, `D6` the vale wide, `D7` the creature, `D8` the overcast interior: every
class §G0's map routes a loss through has a development counterpart; the council's R3 B-3), at
*other* places in the vale (at least 80 m from the evaluation shot of the same class), rendered
by the same tools under `--shots dev`. Every fit (§D8's style fit, §G7's tone
matching), every parameter or cadence tuned by eye or by the instrument (§C6's levers, §D7a's
bars' calibration) and every diagnostic panel the implementer runs for itself uses the
development shots only; the twelve are rendered for G-look's record and the panels and nothing
else. B2 and B3 map separate public reference images or clip segments to the eight, so a
development review can run without touching the sealed set or its reference selections. A step's
commit names which set each number
came from; a number from the twelve under a tuning step is a shortcut.

- **Tests:** `examples/vale/tests/shots`: twelve evaluation shots and eight development shots,
  each development shot at least 80 m from its class's evaluation shot; each class's checks (shot
  1's eye within
  0.2 m of 1.6 m above the ground; shot 2's aim within 20° of the sun; shot 9's and 10's
  creature within the frame; shot 12's weather overcast); the walk is 1,200 frames and passes
  within 20 m of at least six shots' eyes.

**B1.5 The scene, the bench and the lookbook.** `studio/bench/scenes.wrela` gains `vale-walk`
(the walk, 1,200 frames after 60 of warm-up at 1080p, as `last-green-walk` is scripted);
`wrela tool lookbook --package examples/vale --stills` and `--quality` run; the vale's `quality`
is the floor's (`Quality {}` with every switch on: `Tier::Epic` once §C7 exists); its
`METERING`, cascades and probe grid are its own, written in `examples/vale/design.md` as
**proposed**.

**B1.6 The design doc.** `examples/vale/design.md`: the land's numbers, the species, the shots'
list with their times, the walk, the quality, every **proposed** value, and a "What is grown"
table that Part E fills (every row "authored" until it isn't: G-grown reads it).

**Done when, B1:** (1) B1.1's and B1.3's G-pixels identical for the floor (paste `wrela same`);
(2) the land tests and the shot tests pass (paste); (3) `wrela tool lookbook --package
examples/vale --stills build/vale-b1` writes twelve 1920×1080 stills (paste the listing);
(4) `wrela tool lookbook --package examples/vale --quality build/vale-b1q` writes `quality.json`
with every measure for every shot (paste one row); (5) `wrela tool bench -- --scene vale-walk`
records the walk (paste the GPU p50/p95/p99, intervals, over-33, cold and warm playable, memory
peak: this is the *now* of every Part C hill); (6) `examples/vale/design.md` exists with its
tables; (7) `wrela suite` passes.
Hill to climb: none yet (B1 measures; C climbs).

### B2. The reference manifest and subject mappings

`references/vale.wrela` (`references::vale::VALE`, a `VisualReference` each, with a test that
every entry has its source, an eligible licence and a hash) is metadata only: the repository
holds only Wrela, so the manifest is a Wrela module, and the reference media stays outside the
repo under `docs/citations.md`'s private-reference rule. **Done ahead of the plan (2026-10-28):**
the twenty stills and the motion clip are selected, downloaded to
`$WRELA_REFERENCE_DIR/vale`, verified against Commons' hashes and recorded; B2's and B3's work is
to check them against B1's finished shots, replace any that no longer fit the shot's class (the
replacement recorded and the module versioned), and freeze them at B5. It records a mapping for each of the twelve
evaluation shots and eight development shots, plus F9's motion clip. Each source entry holds:

- A stable ID, work title, creator and rights holder where identified, original publication
  URL, direct image/page locator or video timestamps, publication date where known, and access
  date. Cite the original creator rather than an unattributed repost or search thumbnail.
- Subject and why it is a useful quality bar; that it is a photograph or footage (not a
  render, a composite or a generated image); the camera, lens, date, place, species and
  processing (HDR merge, focus stack, heavy grading) **only as disclosed** on the file's page.
  Each undisclosed field is `unknown`, not inferred.
- A rights URL or permission record, required credit text and a per-use decision: source-page
  viewing, official embedding, local copying, transforming, sharing with agent services and
  public reproduction. Unknown permissions default to **link-only**; that does not authorize
  fetching the media for a judge. Record platform access restrictions as well as copyright.
- The Wrela shot IDs it informs and whether it belongs to development or evaluation. Use
  distinct source images or clip segments for the two sets. Record dimensions and any allowed
  local transformations; hash permitted local files, and identify link-only media by URL,
  locator and access date (do not claim an immutable byte hash for a remote page).

Freeze the manifest's hash with the shot hash at B5. A removed, changed or unavailable source
is recorded as unavailable, not a win or a reason to choose a weaker target; replace it with
an equivalent subject and quality bar, record why, version the manifest and rerun the affected
baseline and current review before comparing their tallies. No baseline is silently rewritten.

**Done when:** (1) the schema and twenty subject mappings plus the motion entry exist;
(2) the review tool validates missing citations, unknown conditions and rights-dependent modes
at B4; (3) `examples/vale/design.md` documents the mapping and the separation of the sets.

### B3. Public sources, rights and benchmark context

Select references for the visual subjects, not because Wrela already beats them. They are
photographs and footage of the real subjects (D29), chosen where licences are machine-readable
(Wikimedia Commons' file pages, its Quality images and Featured pictures first) and each grant
confirmed on the work's own page. Prefer a temperate European broadleaf wood (beech, oak),
roe or red deer, and natural, unprocessed exposures; record any processing the page states.
Cover all twelve subject classes, including the walking quadruped;
several sources may be needed. The photographs need not have the same camera, layout or
lighting as the vale. Record those differences and use the rubric to judge the
relevant qualities. Do not recreate a photograph's distinctive composition.

**B3.1 Rights before copies.** Add each selected source to `docs/citations.md` with its creator,
URL, intended use and rights status, and complete B2's manifest. Public availability, a credit,
a free sample or a Fab asset listing is not permission to redistribute someone else's render.
Download a still or a video only from a host whose terms allow it (Wikimedia Commons does; a
platform whose terms forbid downloading, YouTube among them, doesn't, whatever the work's own
licence), and extract frames, crop or re-encode only where the licence covers that use
(CC BY and BY-SA do, with the change noted). No stream ripping, access-control bypass,
watermark removal or unauthorized upload to a judging service. Keeping a copy private does
not itself grant permission.

**Where the copies live.** Under `$WRELA_REFERENCE_DIR` (default `~/wrela-references`), outside
the repo, as `vale/<id>.<ext>` with a `vale/CREDITS.md` holding each file's credit, licence
and source URL; the manifest records each file's `sha256`. The review tool reads them from
there and refuses a manifest entry whose file is missing or whose hash differs.

**Eligibility, since only agents judge (D38).** No person views the references, so a reference
counts toward the twenty subject mappings and the motion entry **only if its permission covers
both a local copy outside the repo and showing that copy to an agent judge** (upload to the
judging service the plan uses): a public-domain or CC0 work; a CC BY or CC BY-SA work, its
attribution and licence notice shown with it; a creator's or publisher's written permission
whose text names that use, its text and date recorded in the manifest; or a rights holder's own
published invitation to use the work freely, which reasonably covers private review (Studio
Ghibli's for its official stills, accepted by Ryan on 2026-10-28), its URL, wording and date
recorded and the work traced to the rights holder's own release, not a repost. A link-only source (an
all-rights-reserved video, a portfolio page, a press image whose terms are silent) is **not
eligible** however good it is: it may be cited in `docs/citations.md` as context, never
reviewed. Where no eligible work exists for a subject, the implementer may write to the
creator of a suitable one asking for that use (the request's text in the manifest; the plan
sends nothing on Ryan's behalf without his say-so, so the request is drafted into "For Ryan"
and the subject marked **awaiting permission**), and otherwise chooses an eligible work of the
nearest subject and records the substitution.

Use `docs/citations.md`'s public-reference procedure and its primary-source links:
[YouTube's terms](https://www.youtube.com/static?template=terms),
[CC BY 4.0's legal code](https://creativecommons.org/licenses/by/4.0/legalcode.en), and
[Fab's licence page](https://www.fab.com/eula) (its summary is not the binding agreement).
Check the particular work's actual grant and the intended uses, not just the host's name.
Fab asset rights do not establish rights to a creator's promotional images or video. No assets
are acquired for this plan's reference workflow, and no outside media enters the MIT game or
engine outputs. Evaluation references are for visual judgement only, not texture extraction,
geometry reconstruction, style fitting or model training. D8's own authorized inputs are separate.

**B3.2 What can be published.** Commit citations, the manifest, original written observations,
Wrela's own captures and numeric tallies. Keep any permitted reference copies and any viewer
that contains them outside the repo and release, as the citation rules require. Required
attribution accompanies private copies and displays too. Public PRs and reports link to the
original source; they reproduce third-party media only with a grant covering publication and
its required notices. Do not relabel third-party work as MIT or suggest Epic/creator endorsement.
If attribution must stay visible, use an attributed review; do not strip it for blinding.

**B3.3 Public benchmarks are context.** When a useful public Unreal benchmark is found, cite
its original publisher and exact URL, date, scene/workload, UE version, device/GPU, resolution,
settings, renderer/upscaling and metric/measurement method. Unknown fields remain unknown.
Record a short factual numeric summary in `docs/bench.md` under **"Public Unreal benchmarks
(context only)"**, separately from locally measured rows. Do not copy a publisher's chart or
full table without permission. No speedup ratio, same-device inference or conversion from
another machine's FPS into an M4 estimate. This subsection is optional: if no useful benchmark
is found, say so; it never blocks engine work or a visual review. Wrela's sustained performance,
memory and load gates still require actual local measurements.

**Done when:** (1) every subject mapping and the motion clip has a cited source **eligible for
agent review** (§B3.1: public domain, CC0, CC BY or BY-SA with its notice, recorded written
permission naming that use, or a rights holder's published invitation to free use), or is marked **awaiting permission** with the drafted request in
"For Ryan"; (2) the rights/use decisions and required credits are recorded;
(3) every selected source has a row in `docs/citations.md`; (4) benchmark context, if any, is
clearly separate from local results. Missing rights or inaccessible media means choose another
suitable public source, never install Unreal as a fallback prerequisite or copy first.

### B4. The review tool (`wrela tool compare`)

`studio/compare/` lays out comparisons and tallies subjective judgements. It does not judge.
Its default mode is **attributed**: source credits and links are visible. Optional blinding is
used only for permitted local copies whose required notices can still be honoured; the record
states the mode, and baseline and later reviews use the same mode. No claim of scientific
independence follows from random order or fresh agent contexts.

- `wrela tool compare -- <out> --a <wrela-dir> --references references::vale::VALE
  --shots <list> [--controls <wrela-dir>]`: validates the selected twelve or eight mappings,
  randomizes order, and writes `pairs.json`, a review page and a private `key.json`. Comparisons
  resolve only to permitted local files outside the repo (§B3.1's eligibility); the tool
  refuses a link-only source, and never fetches, caches, screenshots or proxies one. The page
  preserves aspect ratios, credits and known
  source conditions. It makes no guarantee of the same scene or resolution.
- `pairs.json` includes the manifest hash, review mode, IDs, rights-aware display mode and
  credit/source links where required. A public metadata-only report can be written separately;
  private viewers and media are never accidentally bundled into a PR or release. The rights
  check rejects an operation whose permission is unknown or does not allow the requested mode.
- Eight judges in fresh agent contexts receive the rubric, comparisons and necessary credits
  and conditions, not the implementer's preferred outcome: *Score each image 0–3 for light,
  colour, value, depth and artifacts; judge the relevant visual quality while acknowledging
  different scenes and source conditions; prefer one or rate them equal, with a sentence per
  row. This is a subjective reference review.* Use only a permitted way to access the media;
  a link the judge cannot view is **unavailable**, never guessed from its title. Find an
  accessible, appropriately licensed alternative at B3 before freezing. Each writes
  `<out>/judges/<j>.json`. The page has no human verdict form: the agents' tally is the
  verdict (D38).
- Calibration uses two pairs made only from Wrela-owned stills: an original against its 3×3
  blur, and an original against its quarter-resolution version. Drop and replace judges that
  fail a control; keep every attempt. Do not transform someone else's media to make controls.
- `wrela tool compare -- <out> --tally`: reads the key and judgements; reports per-pair
  preferences, rubric means, source conditions, missing judgements and the G-reference result.
  All eight valid judges must have viewed each reference; missing judgements cannot shrink the
  denominator. Write `tally.json` and a Markdown table with citations for `docs/bench.md`.
- **Tests** (`studio/compare/tests.wrela`): synthetic tally math (including six 5–3 and six
  2–6 pairs failing overall at 42/96); calibration rejection; eight-pair thresholds; unknown
  permissions rejecting local-copy/transform/upload modes; a link-only source refused, with no media
  fetch; required credits preserved; unavailable references blocking a complete tally; frozen
  manifest hashes checked. Use synthetic or Wrela-owned media for all tests.

**Done when:** (1) the tests pass; (2) a dry run on two identical Wrela-owned sets tallies
"equal" with synthetic judges; (3) `docs/lookdev.md` documents the modes, subjective rubric,
rights-aware access, calibration and tally, and its judging paragraph says agents alone judge
(D38) where it now says "Ryan's verdict is final"; the hero anchors it shows the judge
(`WRELA_HERO_DIR`) are held to §B3.1's eligibility too (the eight Nintendo screenshots were
removed on 2026-10-28; any other ineligible one is listed in "For Ryan"). No external source media is needed for the dry run.

### B5. The baseline review (the first G-reference record)

The vale as B1 left it (today's renderer, every switch on) against the public references, through
B4's review, with Wrela-owned controls and eight agent judges.

- Record in `docs/bench.md` under a new heading **"Public visual references"**: the shot and
  manifest hashes, source links and review mode, the tally table (per pair: preferences and
  rubric means), the verdict (expected: fails), and the
  **ranked losses**: every rubric row, summed over pairs, by how much the vale lost it (light,
  colour, value, depth, artifacts), and for each pair the judges' sentences on its worst row.
  This is the baseline loss record; Part G chooses work from its development review (§G0).
- The vale's stills for this panel are kept as `build/vale-b5/` and never regenerated (the
  baseline the later panels are compared with).

**Done when:** (1) `tally.json` and the table in `docs/bench.md` (paste the table); (2) the
ranked losses in the record; (3) the shot and manifest hashes recorded; (4) every judge's file and
every calibration attempt kept beside `tally.json`.

### B6. The device rigs (deferred)

The M4 Air is the only device this plan measures on (D37, Ryan's revision, 2026-10-28). Write
the procedure for a later plan's device rows once, in `docs/devices.md`: `wrela tool bench --
--scene vale-walk --lan` serves the four pages; a device opens them in order (the browser
version recorded from the page's `device.json`); the results are put; the row goes in with the
machine's name, or the typed `--row` form for a device that can't put its results. No device
is measured here.

**Done when:** `docs/devices.md` exists with the procedure (paste it).

### B7. Plan two's Part C, fixed (`rywible/wrela-others`)

The council's findings on the JavaScript engines' scenes (rywible/wrela-others#1, its review
section) invalidate their rows. Each is a line of this step's checklist; the three engines are
then re-measured and their rows replaced.

1. Babylon: a right-handed scene before import; instance yaw through the quaternion; the terrain's
   normals computed for a right-handed system (the trees upright, the terrain facing up: a test
   spawns one tree and asserts its tip's height above its base).
2. Babylon: the forest in the water's reflection and refraction lists.
3. PlayCanvas: a 4096² shadow atlas so each of the four cascades is 2048²; verified by the
   allocated sizes.
4. PlayCanvas: the water excluded from the reflection camera by layers; runs with any WebGPU
   validation error are rejected.
5. PlayCanvas: grass placed and swayed in a shader, not on the CPU each frame.
6. Three: the water's normal texture uploaded (`needsUpdate`).
7. All: "first frame" measured at the first *presented* frame after a scene-readiness promise
   resolves; GPU samples collected with frame ids, warm-up ids excluded, pending samples drained
   before the report.
8. All: 1,201 frame boundaries round the 1,200 measured frames.
9. All: the backend asserted (WebGPU, not a WebGL fallback) and the device reported.
10. All: CPU and shader height sampling at texel centres; the water level imported from the
    export (`water.json`, this step), not derived; billboard bases on the ground; one terrain shadow
    policy.

For item 10 only, `studio/export/main.wrela` gains `water <pkg> --out water.json` if the
existing exporter does not supply it: stream polylines, widths/depths, lake outline/level and
fall endpoints, in metres with the coordinate convention recorded. Validate a round trip on
the existing harness scene. This is Wrela data for the JavaScript harness; the removed Unreal
workflow's tree, cover, shot, walk and creature export extensions are not required.

**Done when:** (1) each line has a test or an assertion in the harness, run and pasted; (2) the
three engines re-measured on the M4 Air and their rows replaced in `docs/bench.md` with the new
date (paste); (3) the README's "What differs between the engines" table updated.

---

**Done when, Part B:** every step's checklist holds; the engine PR (`engine/wc3-b`) and the
others' PR are merged; `docs/bench.md` has "Public visual references" with the baseline panel, the
shot
and manifest hashes, source credits and rights/use decisions, the device baseline rows, and
the fixed JavaScript rows. Public Unreal benchmark context is optional and labelled separately.
Hill to climb: pairs won or equal, (B5's number, expected 0–2) → 6 of 12 at §H.

## Part C: specialization and the budget (`rywible/wrela`, branch `engine/wc3-c`)

Part two's bisect (`docs/bench.md`, B16) found the clearing's 2.4 ms rise was not in its
switches but in the paths every shading pass now carries whatever the switches say. This part
makes every switch a compile-time constant, so a program's renderer holds nothing it doesn't use,
and then climbs the four budgets the claim names (the frame, memory, cold and warm starts) on the
vale and on the floor, measured sustained. No step in this part changes a pixel: every step holds
G-pixels identical (or within the number it names) against `build/base-0.9`.

**Order within Part C:** C3 first (the sustained bench, which C1's gate is measured by), then
C1, C2, C4, C5, C6, C7.

### C1. Every switch a constant (D32)

Today `engine::frame::Quality`'s booleans (`distance`, `fog`, `ambient`, `occlusion`,
`reflections`, `contact`, `clouds_halved`, `neural`, `learned_resolve`, `cascades.on`,
`probes.visibility`, `shadows.soft`) are run-time values read inside shaders. With A1 a `const`
argument may be a type parameter's associated constant, so:

- `engine/frame.wrela`: `pub trait Switches { const FOG: bool; const DISTANCE: bool; const
  SIGHT: bool; const AMBIENT: bool; const OCCLUSION: bool; const REFLECTIONS: bool; const
  CONTACT: bool; const PCSS: bool; const HALVED: bool; const CASCADES: bool; const NEURAL: bool;
  const LEARNED: bool; }` and `Frame<R: Resolver, Q: Switches>`. The booleans leave `Quality`
  (its numbers stay: scale, strips, rays, spacing, budgets). Every entry point that reads one
  takes it as a `const bool` parameter and the frame binds `Q::FOG` and the rest where it
  dispatches or draws; a function below an entry point takes the value as an ordinary `bool`
  argument (the fold reaches it after inlining). `requality` can no longer flip a switch; the
  lookdev reference (F12) changes numbers only, which is what it did.
- `engine/frame.wrela`'s `Quality::reference()` and the tests that flipped a switch at run time
  (`engine/tests/fog`, `engine/tests/distance`, `engine/tests/cascades`, `engine/tests/
  probes-visibility`, `engine/tests/render/*`) become two builds where they need two values (a
  test program is generic over `Q` and the test names both instantiations).
- Each program declares its switches: `examples/clearing`'s `Q` has `FOG = false, DISTANCE =
  false` (what a950ec70 set at run time), the floor's and the vale's all true (`Tier::Epic`
  comes in C7).
- **Tests:** `engine/tests/switches` (new): a scene built with `FOG = false` has no fog
  pipeline (`std::tool::toolchain::check`'s pipeline list, which a tool test can read) and its
  shading shaders' bytes are smaller than with `FOG = true` (paste both); G-pixels identical for
  each of the five baselines with each program's switches as they were.
- **Gate:** G-pixels identical on all five; G-pipelines: `wrela pipelines examples/clearing
  --json` lists no `fog`, `distance` or sight pipelines and the floor's count is at most 175
  (B16's); G-profile: the clearing's GPU p50 back to **≤ 10.2 ms** (B5's number; now 12.65):
  this is H1's first half.

**Done when:** (1) the trait and the generic frame compile and every program builds; (2) the
switches test passes with both byte counts pasted; (3) G-pixels identical ×5 (paste); (4) the
clearing's GPU p50 ≤ 10.2 ms sustained (§C3's bench; paste minute 1 and minute 10); (5) the
floor's and the vale's GPU p50 pasted (the *now* for C6).
Hill to climb: the clearing's GPU p50, 12.65 → ≤ 10.2 ms.

### C2. The style slot's plumbing

Part D's `Style` is a second type parameter the same way. This step adds the parameter with one
implementation, so Part D adds styles and touches no frame code:

- `engine/style.wrela` (new): `pub trait Style { const SUBSTANCE: u32; const NORMAL: u32;
  const MARKS: u32; fn shade(...); fn palette(...); fn mark(...); fn lens(...) }` (§D2 fixes
  the signatures; here each has the physical style's body and the constants are 0), and
  `content/styles/physical.wrela` implementing it as today's look. `Frame<R, Q, S: Style>`; the
  shading entry points take `S` as a type argument and call `S::shade` where `lit::sun_and_sky`
  was summed; `finish` calls `S::lens` where `develop` read `paint::Look`.
- **Gate:** G-pixels identical ×5; G-profile: every scene's GPU p50 within 0.1 ms of C1's.

**Done when:** (1) the trait exists with the physical implementation and every program names
`Physical`; (2) G-pixels identical ×5 (paste); (3) G-profile within 0.1 ms (paste).

### C3. The sustained bench (D39)

`studio/bench/main.wrela`: `--sustained <s>`: the play run paces the walk for `s` seconds (the
walk looped) before its 60 warm-up and 1,200 measured frames, and records the GPU median of the
first minute and of the last minute before the measured window (`sustained.json`: `minute_1_ms`,
`minute_last_ms`, `ratio`). The sustained run's page runs with **one frame in flight**
(test mode's `inflight=1` in place of the play run's 2). What that measures, exactly
(`runtime/browser/src/worker.ts:752-815`, the council's R3 B on F3): the worker paces frame
starts by a timer, records `began_ms` before drawing, pushes each frame's
`onSubmittedWorkDone` promise and, with `inflight=1`, waits for the oldest once two are
pending: so frame `i` starts only after frame `i − 2`'s GPU work completed, and a sustained
cadence of starts is a lower bound on the GPU's throughput; presentation itself isn't observable
from a page, and `throughput.json` says `frame_starts`, not presented frames. It records the
elapsed time of the 1,200 starts (`began[last] − began[first]` plus the mean interval), the
starts a second, and the count of intervals over 25 ms. The
report's test feeds a synthetic stream of 19 ms intervals and asserts the throughput gate fails
it (52.6 fps) while the p99 and over-33 gates pass it. The report's table gains a "sustained"
column; `docs/bench.md`'s
rows say `sustained 600 s` where they are. The machine's state is also recorded as the host
reports it (`std::host` has no thermal reading: record the ratio and the room's time of day; a
ratio over 1.15 is reported as "throttling" beside the number).

- **Tests:** `studio/bench/tests.wrela`: a sustained run of `examples/hello-field` for 20 s
  writes `sustained.json` with the two medians and the ratio.

**Done when:** (1) the test passes; (2) `wrela tool bench -- --scene clearing-path --sustained
600` runs and its row says so (paste the row with both medians); (3) `docs/bench.md`'s
"Measuring" section says what sustained means.

### C4. Memory under 512 MiB (D24's floor target, now the vale's too)

The *now*: the floor 888 MiB at its peak, 862 on the walk (B16); the vale's from §B1.5. The
share table (`memory shares:` at playable, which the bench reads) names every lever. Take them
largest first; each is a commit with the share before and after:

1. The wood's held tiles: the vale and the floor hold the tiles `land::stream` names; reduce
   the held ring by one tile where the far layer covers it (the far layer draws what the ring
   drops); measure the share.
2. The impostor atlases: every kind's atlas BC-compressed (`compress.wrela`: BC7 and two BC5,
   3 MiB a kind; where a kind is still held plain, compress it); the far-only kinds at 512².
3. Temporal AA's two histories at the output's size (15.8 MiB each at 1080p): one history, the
   resolve reading and writing it through a copy only where `found` asks (an experiment: measure
   the copy's cost; keep two if the copy costs over 0.1 ms).
4. The probes' sights (17 MB): the 8×8 tiles for layers 0–3 only where visibility is on
   (`Q::SIGHT`), and 4×4 for every layer measured against the probes-visibility tests (the dark
   half under 10 % must still hold; if not, keep 8×8 and say so).
5. The pool: `Pool::room_for` doubles; fit it to the floor's and the vale's counted meshes with
   a quarter's slack.
6. The clipmap's levels and the terrain's texels: the levels past what the far layer draws at
   the eye's height.
7. The froxels and the distance volume: as they are (16.6 and 10.9 MB; they're the lighting).

- **Gate:** G-memory: the floor's and the vale's `gpu_peak` **under 512 MiB** on the walk; the
  share table in `docs/bench.md`; G-pixels within 2/255 (a compressed atlas moves pixels by its
  codec's error; identical where no lever touched the look).

**Done when:** (1) each lever's commit pastes its share before and after; (2) both scenes under
512 MiB (paste `gpu_peak` for each, sustained); (3) G-pixels within 2/255 ×5; (4) the share
table in the record.
Hill to climb: the floor's peak, 888 → < 512 MiB; the vale's, (B1.5's) → < 512.

### C5. Cold under 5 s, warm under 3 s

The *now*: the floor 15.5 s cold (7.6–8.9 in quieter runs), 7.8 s warm; the clearing 9.0 s in
the full bench, 4.5–5.0 in A/B runs; the vale's from B1.5. Plan two's §B2 took the levers it
named; what's left, in order:

1. Fewer pipelines to build: C1's specialization removes the paths a program doesn't use; count
   the floor's pipelines and their WGSL bytes before and after (the bench's `slowest_compiles`).
2. The first frame's payload: the far layer and the first ring's tiles in one fetch, the rest
   after playable; measure `playable_ms` and `loaded_bytes` at playable.
3. The kinds' cooks (`Work::Kind`): the floor cooks seven kinds' cards and atlases at load; cook
   the two kinds the first view needs first, the rest under the arrival cover's budget, and
   measure.
4. The history's bake read: one file for the first ring, decoded on a job (`std::par::spawn`)
   while the GPU cooks.
5. The warm start: what the browser caches (the WASM's code cache, the shaders' cache) and what
   it refetches; `loaded_bytes` warm against cold; a resource fetched warm that the cold run
   fetched too, with the same hash, is a cache-control fault to fix in the release's headers
   (`studio/release`).

- **Gate:** G-cold: the floor's and the vale's median `playable_ms` of three cold runs **under
  5,000 ms**; G-warm: **under 3,000 ms**; G-pixels identical (nothing here moves a pixel).

**Done when:** (1) each lever's commit pastes `playable_ms` before and after (three runs each);
(2) both scenes under the two numbers (paste the three runs each); (3) `docs/bench.md`'s
"Whether D27 holds" rows for cold and warm filled for the M4.
Hill to climb: the floor cold, 15.5 → < 5 s; warm, 7.8 → < 3 s; the vale's from B1.5's.

### C6. The sustained frame

The *now*: the floor's walk 18.14 ms GPU p50, 5 frames over 33, p99 25.94 (B16, cold); the vale's
from B1.5, cold. The gate is sustained (C3). The levers, in order, each a commit with its
numbers:

1. C1's specialization (its gain is measured there; here the floor's and the vale's sustained
   numbers after it are the new *now*).
2. The frame's cadences: the probes' re-trace (`Work::ProbeAgain`), the fog's inject, the
   clouds' march and the grass's placement each run at the cadence the planner allows; measure
   each label's share (`wrela tool bench`'s labels) and halve the cadence of any label over 1 ms
   whose output changes slowly (the clouds, the fog); each cadence is chosen on the eight
   development shots' flicker and pop, then G-look's (3) on the twelve is the check that
   nothing visible changed (chosen on one set, checked on the other).
3. The shadow cascades' strips: the far cascade's redraw spread over more strips where the
   hitch table names it.
4. The resolve and the bloom at the output's size: the bloom's levels as `bloom_levels`
   chooses them; the resolve's `found` rows only under `Q::LEARNED`.
5. The scene's scale: it is 0.667 already (1280×720 up to 1080p); it stays. A lower scale is not
   a lever for the Epic tier (it is for C7's tiers).
6. The hitch table: every frame over 33 ms on the walk named and its heaviest label taken as a
   lever of its own (plan two's §B4 procedure).

- **Gate:** G-sustained on the floor's walk and the vale's walk: GPU p50 **under 12 ms**, paced
  p99 **under 20 ms**, **0** of 1,200 frames over 33 ms, after 600 s; G-look's (3) (flicker,
  pop, trail not up) on the twelve shots and the floor's 23 for any cadence lever; G-pixels
  identical where no cadence lever touched the frame.

**Done when:** (1) each lever's commit pastes the sustained numbers before and after; (2) both
scenes meet the three numbers sustained (paste the rows, minute 1 and minute 10); (3) the hitch
table for both walks empty, or every remaining hitch named with its lever tried (a non-empty
table is a **Blocked** note, §0.9); (4) G-look (3) pasted for cadence levers.
Hill to climb: the floor's walk GPU p50, 18.14 → < 12 ms sustained; over-33, 5 → 0; the vale's
from B1.5's.

### C7. The tiers (D37)

- `engine/frame.wrela`: `pub enum Tier { Epic, High, Medium, Low }` and `Tier::switches()` as
  four `Switches` implementations (`Epic`: all on; `High`: `HALVED` on, `PCSS` off; `Medium`:
  `High` with `SIGHT` off, `CONTACT` off, `REFLECTIONS` off; `Low`: `Medium` with `FOG` off,
  `DISTANCE` off, `AMBIENT` off, **proposed**) and `Tier::quality()` as four `Quality`s (`High`:
  scale 0.667, the near cascade 2048²; `Medium`: scale 0.5, 1024², probes 16 rays; `Low`: scale
  0.5, 1024², no far cascade: **proposed**). A program names its tier as a `const TIER: Tier`
  and `wrela build <pkg> --set TIER=Medium` (A6) overrides it; the bench passes `--set`
  through `std::tool::toolchain::build`'s `set` option.
- **Gate:** G-tier on the M4: each tier's sustained GPU p50 recorded, each lower tier's under
  the tier above's; G-pixels identical at `Epic`. Other devices are deferred (D37).

**Done when:** (1) the four tiers build and `wrela pipelines` counts differ per tier (paste);
(2) the four M4 rows in `docs/bench.md` with their numbers (paste); (3) G-pixels identical at
`Epic` ×5.

---

**Done when, Part C:** every step's checklist holds; `docs/bench.md` has "Whether D27 holds"
started: the M4's frame, memory, cold and warm rows for the vale and the floor, and the device
rows; the PR's "For Ryan" lists every **proposed** tier setting. G-reference is not rerun here
(nothing changed a pixel at `Epic`).

## Part D: substance and style (`rywible/wrela`, branch `engine/wc3-d`)

The frame emits what's there and how light reaches it; a style is a program over that. After
this part the vale draws through four styles and `engine/` doesn't know which. The physical
style is today's image to the pixel; the others are judged by G-look against their own
references and by the panel.

**Order within Part D:** D1, D2, D3, D7a (the marks system, which D4–D6 draw with), D4, D5, D6,
D7b (the four styles' coherence), D8, D10, D11; D9 is deferred (D41). Each step's Done-when
names only what the steps before it in this order built.

**G-look for a new style.** A style has no "before" to prefer: its G-look is (1) its twelve
stills in the PR; (2) and (3) the instrument's measures against the *style's own* reference
(F12 at the style's maximum settings: the measures say convergence and coherence, not taste);
(3)'s bars: flicker at most 1.5× the physical style's on the same shot, pop at most 1.5×, trail
at most the physical style's plus 1 px (**proposed**, calibrated by §D7a); (4) the blind judge
scores the stills on the rubric without a pair, and, where §D8 fitted the style to a reference,
scores the pair (still, reference) for fidelity to the reference's style on one added rubric
row, *style* (3 is "the same hand"). The agent judge's scores decide (D38).

### D1. The substance buffers

`engine/substance.wrela` (new): what a style reads of a pixel *after* the shading pass. Two
things the council corrected in the first draft (R1 F1): the transport terms are **not**
stored (a style's `shade` runs inside the forward shading pass, as the physical lighting does,
and receives the terms as values: storing six of them would be 48 bytes a sample, and WebGPU's
default `maxColorAttachmentBytesPerSample` is 32, which the compiler enforces as E0602); and
the layout is fixed here, to the byte, so no style can exceed it. What is stored is what the
later passes (the marks pass and the lens, D7) read: the form, the cause and the tone. The
part's identity is already in the prepass's `ids` target (`R32Uint`, a different pass), which
the lines read as `paint::lines` does today.

The bytes are **attachment** bytes as the ABI costs them (`runtime/abi/src/stream.rs:558`: an
`Rgba8` costs 8 though it stores 4; the council's R2 F1), so packed integer targets carry the
small channels:

| Target | Format | Attachment bytes | Holds | Mask bit |
|---|---|---|---|---|
| T0 `scene` | `Rgba16Float` | 8 | the lit colour and the tag (exists) | always |
| T1 `form` | `Rg32Uint` | 8 | word 0 `n_form` (Oct32); word 1 `n_coarse` (Oct32) | `FORM` |
| T2 `shape` | `Rg32Uint` | 8 | word 0 `dir` (Oct16: the principal curvature direction) in the low half and `k1` (f16 bits) in the high; word 1 `k2` (f16 bits) in the low half, the high spare | `FORM` |
| T3 `cause` | `R32Uint` | 4 | `history` packed as four bytes: age, light, wetness, stress | `CAUSE` |
| T4 `tone` | `R32Uint` | 4 | the albedo before light as three sRGB bytes and `flat` (1 where D4's interval test flattened it) | `TONE` |
| | | **32** | the budget; the depth target is apart | |

Packing and unpacking are `substance::pack_*` and `unpack_*` (`pack4x8unorm` and `unpack4x8unorm`,
`pack2x16float` and its inverse, as WGSL has them), with a round-trip test per channel.

Where each channel comes from:

- `n_form`, `n_coarse`: each material (`engine/plant.wrela`'s bark and leaves, `rock.wrela`,
  `grass.wrela`, `stone.wrela`, the creatures' coats, `terrain.wrela`'s ground, `water.wrela`)
  gains `at_scale(s: f32)`: its field with the noise octaves finer than `s` dropped, so a normal
  from its gradient is the form's at that scale; `s` is the pixel's footprint in metres times 8
  and 64. The fine normal (`n_fine`) is the shading normal in the pass and isn't stored; a style
  with `NORMAL = 1` lights with `n_form` inside the pass.
- `dir`, `k1`, `k2`: the Hessian of the form-scale field, a `gradient` of a `gradient`
  component (spec §14.2 nests), its eigenvalues and the larger's eigenvector in the tangent
  plane; computed only under `FORM`. Its cost is the expensive one (three nested gradients):
  measured on the vale's walk with `Cel`'s mask; a lever named now: curvature at half the
  scene's size when it is over 1.0 ms.
- `history`: the instance's `land::grow::Tree` record (the wood's `Instance` gains the four as
  `Unorm8x4`) and `land::bake::tile_fields` at the pixel's ground.
- The transport terms: `lit::terms(...)` (new) returns what `sun_and_sky`, `sun_sky_and_gloss`
  and `sky_at` compute, as a `Terms` struct (sun, sky, bounce, spec, sss, ao, fog), in the pass;
  the old functions become one-line wrappers that sum `terms` (G-pixels identical: the same
  arithmetic in the same order; a test asserts it to the ulp on 1,000 random inputs on the
  CPU); the physical style's `shade` is that sum.
- `substance::write::<S>(mut out, colour, s)` writes T0 and only the targets `S::SUBSTANCE`
  names; the pass is `begin_pass5_with_depth` at most (A2 keeps the byte check); a style with
  mask 0 adds no target, and `wrela pipelines --json`'s `attachment_bytes` shows 8 for it, 32
  for the full mask.
- **Tests** (`engine/tests/substance`): a sphere's `n_form` equals its `n_fine` (no detail to
  drop); a bumpy sphere's `n_form` is within 2° of the smooth sphere's; its `k1` and `k2` are
  `1/r` within 5 %; the terms' sum matches the old lighting to the ulp; `history` for a 200-year
  oak reads age ≈ its share of the habit's lifespan; each packed channel round-trips within its
  precision; every style's `attachment_bytes` is at most 32 (a tool test over
  `content/styles/`, which at this step holds `Physical` and the test fixture's full-mask
  style under `engine/tests/substance/`).

**Done when:** (1) the tests pass; (2) G-pixels identical ×5 with the physical style (mask 0);
(3) the per-channel costs on the vale's walk pasted (sustained) with the fixture's full-mask
style, the curvature's lever taken if over 1.0 ms (D4 re-measures with `Cel`); (4) `wrela
pipelines --json`'s `attachment_bytes` for `Physical` (8) and the fixture's style (32) pasted.

### D2. The `Style` trait

`engine/style.wrela`: the slots, each an associated constant or a GPU function, with the
physical bodies as C2 left them:

```wrela
pub trait Style {
    /// Which substance targets the frame writes (D1's mask: FORM, CAUSE, TONE); 0 for a style
    /// that needs none after the pass, as the physical style doesn't.
    const SUBSTANCE: u32
    /// Which normal lights the form: 0 fine, 1 form, 2 coarse.
    const NORMAL: u32
    /// Which marks are drawn (bits: LINES, CREASES, DABS, STROKES, HATCH).
    const MARKS: u32
    /// How hard temporal AA clips where the substance says the tone is flat (0: as the
    /// physical style; 1: the neighbourhood's box shrunk to its centre's tone).
    const FLAT_CLIP: f32
    /// The lit colour of a pixel from its transport terms and substance.
    fn shade(t: Terms, s: Substance, n: vec3) -> vec3
    /// The colour a material's albedo becomes in this style (a palette), before light.
    fn palette(class: u32, history: vec4, albedo: vec3) -> vec3
    /// A mark's darkness at a pixel (0: none), from the marks' field (D7).
    fn mark(m: MarkSample, s: Substance, depth_px: f32) -> f32
    /// The lens: exposure, bloom, grain, vignette, paper, the output curve.
    fn lens(c: vec3, px: vec2, out: vec2, e: Exposure) -> vec3
}
```

- A style is a file `content/styles/<name>.wrela` with `pub struct <Name>` and `impl Style for
  <Name>`, and its parameters as `pub const`s (so `wrela tool fit --style` can write them and
  the lens drags them). `content/styles/README.md` is the template: copy `physical.wrela`, change
  one slot at a time, run `wrela tool styles --check`.
- The frame reads `S::NORMAL` to pick which normal `shade` gets; `S::MARKS` to run the marks
  pass (D7) or not; `S::FLAT_CLIP` in the resolve (`temporal::blended`), a `const` through
  `Q`-style plumbing (C2).
- **Tests:** `engine/tests/style`: a style whose `shade` returns `n * 0.5 + 0.5` draws a
  sphere's normals (readback at 100 pixels within 1/255 of the analytic normal); `MARKS = 0`
  adds no pass (`wrela pipelines`).

**Done when:** (1) the trait as above (signatures may be refined; each change named in the
commit); (2) the tests pass; (3) G-pixels identical ×5 with `Physical`.

### D3. `Physical`

`content/styles/physical.wrela`: today's image. `SUBSTANCE = 0`, `NORMAL = 0`, `MARKS = LINES`
(the characters' lines are part of today's look: `paint::lines`), `FLAT_CLIP = 0`; `shade` sums
the terms; `palette` is the identity; `lens` is `paint::develop`'s curve with `paint::Look`'s
dabs, bloom, sharpen and vignette (the dabs are a mark in D7's terms but stay here until D5
moves them, with G-pixels identical at the move).

- **Gate:** G-pixels identical ×5 against `build/base-0.9`.

**Done when:** (1) identical ×5 (paste); (2) the twelve vale stills regenerated into
`build/vale-d3/` and identical to `build/vale-b5/` (the panel's baseline stills: paste `wrela
same`-style counts from `studio/lookbook`'s pixel diff).

### D4. `Cel`

The style Ryan described: what an artist draws so a thing reads simply and well. Three ideas
from §0.1, each a slot:

- **Light the form, texture the surface.** `NORMAL = 1` (the form normal); `shade`: the sun's
  term through a ramp of `BANDS` (3, **proposed**) steps with a terminator softness
  `TERMINATOR` (0.08 of the cosine, **proposed**); the sky's term flat per class (its mean over
  the hemisphere, not its directional value); bounce as one tint; specular as one placed
  highlight band (`HIGHLIGHT`: a cosine threshold, 0.9, **proposed**) on classes that have one
  (water, eyes, wet rock), none on bark, leaves and grass; `sss` for leaves against the sun as
  one band; fog applied as the physical style applies it.
- **A palette, not an albedo.** `palette(class, history, albedo)`: the material's albedo
  quantized to the class's palette of `PALETTE_N` tones (4, **proposed**), the tone chosen by
  luma, with the hue shifted toward the class's key colour by `HUE_PULL` (0.3, **proposed**);
  `history.y` (light) shifts a leaf's tone warmer, `history.z` (wetness) darker, `history.x`
  (age) greyer on bark. The palette table is `pub const PALETTE: [[vec3; 4]; CLASSES]`, every
  entry **proposed** and listed in "For Ryan".
- **Lines where an artist puts them.** `MARKS = LINES | CREASES`: silhouettes from depth and
  `part` (as `paint::lines`), and creases from `curvature`: a crease where the larger principal
  curvature times the pixel's footprint exceeds `CREASE` (0.35, **proposed**: DeCarlo's
  suggestive contours and Judd's apparent ridges are the references; the engine's curvature is
  exact, theirs estimated), with the line's darkness falling as the crease's extent (its length
  in pixels along the ridge, from the neighbourhood) falls under `CREASE_PX` (12, **proposed**):
  a short crease isn't drawn. Line width by depth: `LINE_PX` at 5 m falling to a third at 60 m.
- **Flatten what the eye can't resolve.** Before `palette`, the material's albedo is replaced by
  its mean over the pixel's footprint where its range over the footprint is under `FLAT` (0.12
  in luma, **proposed**): the `interval` derivation of the material's colour over the footprint
  box (spec §14.1); where the interval is wide the fine albedo is kept. The cost is measured; if
  over 0.5 ms on the vale's walk, the interval is taken at half the scene's size (a lever named
  now).
- `FLAT_CLIP = 0.8` (**proposed**): temporal AA holds flat tones flat.

- **Tests** (`engine/tests/style-cel`): a lit sphere's ramp has exactly `BANDS` distinct tones
  along a row through its centre (readback); a cylinder with 2 mm bark relief lights as a smooth
  cylinder (its terminator's position within 1 px of the analytic one); a cube's edges draw
  creases and its faces none; a 1 cm bump at 60 m draws no crease; a checker of 1 mm squares at
  5 m reads as one flat tone.
- **Gate:** G-look for a new style on the twelve shots; the panel's rubric scores (no pair) with
  the *style* row against §D8's reference where it exists by then (else recorded as "unfitted").

**Done when:** (1) the five tests pass; (2) the stills in the PR; (3) the measures pasted
against the cel reference with (3)'s bars; (4) the judge's scores pasted; (5) the cost of the
interval flattening and the crease pass pasted (sustained) and under 1.0 ms together, or the
half-size lever taken and re-pasted; (6) every **proposed** constant in `content/styles/README.md`'s
table and in "For Ryan".

### D5. `Painterly`

Today's dabs (`paint::dab_map`, anchored to unjittered 3D points) are a mark of D7a's system,
and strokes join them:

- `NORMAL = 1`; `shade`: the terms summed but the sky's directional part halved (a painter's
  simplified ambient); `palette`: `PALETTE_N` 8 with `HUE_PULL` 0.15 (**proposed**).
- `MARKS = DABS | STROKES`: dabs as today; strokes along the form's principal curvature direction
  (from the Hessian's eigenvector), `STROKE_PX` long (24, **proposed**), their density by the
  interval test (D4's `FLAT`: a flat region takes long strokes, a detailed one short dabs), their
  colour the pixel's with a `STROKE_VARY` (0.06, **proposed**) luma offset per stroke from its
  seed; paper tooth as today; lines faint (`line_dark` 0.25).
- `FLAT_CLIP = 0.4`.
- **Tests** (`engine/tests/style-painterly`): on a sphere, strokes' directions follow latitude
  or longitude within 15° (readback of the mark field's direction); on a flat plane the strokes
  are `STROKE_PX` long within 10 %.
- **Gate:** G-look for a new style (the dabs' move into the marks system was D7a's, already
  identical).

**Done when:** (1) the tests pass; (2) the stills, measures and judge's scores pasted.

### D6. `Ink`

No colour: a paper white, hatching and lines.

- `NORMAL = 1`; `shade`: the sun's term as a density in [0, 1], the sky's term adding a floor;
  `palette`: luma only; `lens`: paper white, no bloom, a `GRAIN` (0.02, **proposed**).
- `MARKS = LINES | CREASES | HATCH`: hatching along the principal curvature direction at
  `HATCH_PX` spacing (6, **proposed**) in up to three layers (the second crossed at 60°, the third
  at 120°) as the density rises (Praun et al.'s tonal art maps: each layer appears as a
  threshold is crossed, so a tone is a count of layers, coherent under zoom); silhouettes and
  creases as `Cel`'s, thicker (`LINE_PX` 2.2).
- **Tests** (`engine/tests/style-ink`): a sphere lit from the side has 0 layers at its lit pole,
  3 at its dark limb, with the boundaries where the density crosses the thresholds; the hatching
  is anchored (a dolly of 1 m moves a hatch line with its surface point within 1 px: the
  coherence test D7 formalizes).
- **Gate:** G-look for a new style.

**Done when:** (1) the tests pass; (2) the stills, measures and judge's scores pasted.

### D7. Marks and coherence (D36)

Two commits, in different places of the order (R2 F2): **D7a** before the styles (the system
they draw with) and **D7b** after them (their coherence measured and the bars held).

**D7a. The marks system.** `engine/marks.wrela` (new): a mark is a sample of a field in the
thing's own domain (object space, by the `ids` target's instance; the ground's in world space),
sized in screen space:

- `MarkSample { kind, seed, at: vec3, dir: vec3, size_px, phase }`: at a pixel, the marks that
  cover it come from a lattice in object space whose spacing is the mark's screen size times
  the pixel's footprint (so a lattice level per octave of distance, blended over a half-octave:
  Bénard, Bousseau and Thollot's survey names the two failures, the shower door and the sliding;
  this is the compromise it recommends: anchored in 3D, sized in 2D, faded across levels).
  `dir` is the principal curvature direction (D1's T2) where a style asks for it.
- The dabs' map (`paint::dab_map`) becomes `marks::sample` with `kind = DAB`, and the
  characters' lines (`paint::lines`) `kind = LINE`; `Physical` draws exactly what it drew.
- `S::mark(m, s, depth_px)` gives the mark's darkness; the marks pass draws them into a
  `marks` target at the output's size after the resolve, before the lens (so temporal AA never
  smears a mark: marks are drawn from the resolved substance, not resolved).
- **The coherence instrument** (`studio/lookbook/quality.wrela`): the three measures on a
  dolly strip per style (flicker, pop, trail: they exist) reported per style, and the bars'
  calibration: on the eight development shots' strips with `Physical`, the physical style's
  flicker, pop and trail are the baseline, and the bars are 1.5× for flicker and pop and +1 px
  for trail (**proposed**; this step records the baseline they are relative to).
- **Tests** (`engine/tests/marks`): a mark on a plane keeps its surface point under a 1 m dolly
  (its screen position moves as the point's projection does, within 1 px); its size in pixels is
  constant under the dolly within 10 %; crossing an octave boundary, the two lattices' marks
  blend (no frame has a mark appear at full darkness: the largest frame-to-frame darkness step
  at any pixel is under 0.5).

**Done when, D7a:** (1) the tests pass; (2) `Physical` identical ×5 after the dabs and lines
moved (paste); (3) the physical baseline of the three measures on the eight development strips in
the record, with the bars derived from it.

**D7b. The four styles' coherence** (after D6). Each style's flicker, pop and trail on the eight
development strips and on the twelve, against D7a's bars.

**Done when, D7b:** (1) the coherence table (three measures × four styles × twenty shots) in
`docs/bench.md`; (2) every style holds its bars on both sets (paste), or a **Blocked** note per
style naming the worst strip and the lever tried (its lattice's blend width, its `FLAT_CLIP`).

### D8. A style fitted to a reference (`wrela tool fit --style`)

`studio/fit/style.wrela`: the style's `pub const`s declared fittable (a `Params` list with
ranges in the style file) are searched to match a reference. No painting from Ryan is needed
(his revision, 2026-10-28): the fit's references are freely licensed paintings, chosen for the
style they teach (for `Painterly` and `Cel`, CC0 paintings from The Met's Open Access, apart
from the hero set's 23 the judge views, so a style isn't fitted to its judge's anchors), each
cited in `docs/citations.md` with its licence and kept under `$WRELA_REFERENCE_DIR/styles`:

- `wrela tool fit -- --style cel --package examples/vale --shot D5 --reference <png> --out
  content/styles/cel_fitted.wrela` (the meadow's development shot, never one of the twelve): a
  single reference image the shot should look like (a mode the tool keeps for a painting over a
  lookdev still, should one ever exist); the score is the mean ΔE (CIE76
  on sRGB → Lab) over the still plus `1 − SSIM` on luma; the search is CMA-ES (Hansen) on the
  CPU from a fixed seed, 200 evaluations (each a render of the shot through the style at the
  candidate's constants: the style's constants become a uniform for the fit alone, `FITTING =
  true` as a `const`, so the fit builds once); the result's constants are written as the new
  style file; validation on D1 and D3, which the fit didn't see (ΔE reported, not optimized);
  the twelve are never rendered by the fit.
- `--reference-set <dir>`: **the mode this plan uses**: a set of freely licensed paintings
  (above), whose *statistics* are the targets: the luma histogram's four quantiles, the edge
  density (the share of pixels whose luma step exceeds 0.1), the palette's size (distinct
  colours after quantizing to 32 levels) and the mean chroma; the score is the distance of the
  still's statistics to the set's mean. This learns a hand's tone and economy without tracing a
  picture.
- **Tests** (`studio/fit/tests.wrela`): the fit recovers a style's constants from a still the
  style itself drew with known constants (the identity reference): within 5 % of each constant
  in under 200 evaluations; a run with `--reference-set` on two synthetic sets with different
  histograms moves the style's `PALETTE_N` and `TERMINATOR` the way the sets differ.
- **Gate:** the fit's validation ΔE on the held-out shots is reported; the fitted `Cel` goes
  through G-look for a new style with the *style* rubric row against the reference.

**Done when:** (1) the tests pass; (2) a `--reference-set` fit on D5 for `Cel` and for
`Painterly`, each set (at least twelve CC0 paintings) cited in `docs/citations.md`, its score
trajectory pasted (200 evaluations, the best every 20); (3) the held-out statistics' distance
on D1 and D3 pasted; (4) the fitted styles' G-look pasted.

### D9. The learned style: deferred (D41)

The first draft proposed a small net, trained on a few painted frames and guided by the
substance buffers (StyLit's and Texler et al.'s idea), giving the final colour per pixel. Both
council reviewers read `docs/citations.md`'s rule ("no generative image, sound, music, voice or
video model makes anything in wrela; every image and sound comes from wrela code") as covering a
trained model that supplies a pixel's colour, whatever its size or who painted its training
frames. The plan agrees the rule as written covers it, and a plan isn't where a rule changes.
So: **not built.** The four styles above are code. The question goes to Ryan in Part D's PR
("For Ryan": *is a net trained on your own paintings over wrela's stills, producing colour from
wrela's substance, inside or outside the rule?*); if he writes that it's inside, the step as
first drafted (below) is scheduled in a later plan with its own gate. H4 is withdrawn.

*As first drafted (not built):* a `Model` of 24 inputs (the substance's channels at the pixel
and its eight neighbours' luma), two hidden layers of 48, 3 outputs, evaluated per output pixel
in the lens pass; `wrela tool fit -- --style learned --frames <dir>` training on pairs (a
lookdev still, its painting) under plan two's fixed budget (4,000 steps, batch 16,384, three
seeds); a mechanism check needing no painting (the net trained on `Painterly`'s own output
reproduces it within 2/255 on held-out shots); and a G-spike gate (H4: beats hand-tuned
`Painterly` on flicker and pop at equal preference, else off).

**Done when:** (1) the question is in Part D's PR's "For Ryan" verbatim; (2) `content/styles/`
holds no learned style; (3) `docs/bench.md`'s hypotheses table marks H4 "withdrawn (D41)".

### D10. `wrela tool styles` and the four-styles page (G-style)

- `studio/styles/`: `wrela tool styles -- --check` builds the vale with each style under
  `content/styles/`, lists each build's pipelines, and runs the G-style diff: it reads `git log`
  for the commit that added each style and asserts `git diff --stat <parent>..<commit> --
  engine/` is empty (the check is also a test in `studio/styles/tests.wrela` on the branch's own
  history). `--page <out>`: the twelve shots through the four styles side by side with each
  shot's coherence numbers, as a page.
- **Done when:** (1) `--check` passes for the four styles (paste); (2) the page exists and is
  linked from the PR; (3) `content/styles/README.md` names the slots, the **proposed**
  constants and the fit's commands.

### D11. The second panel

The vale through `Physical` on the specialized frame (nothing changed a pixel since B5, so this
is a control: the panel should tally as B5 did within noise) and through `Cel` (the style Ryan
first asked about) against the public references. The `Cel` panel is a record, not a gate: a
style is a different intent from a scan.

**Done when:** (1) the physical panel's tally pasted and within two preferences per pair of
B5's (a wider swing is a panel fault to find before Part E); (2) the `Cel` panel's tally and
the judges' *style* sentences in the record.

---

**Done when, Part D:** every step's checklist holds; four styles under `content/styles/`;
G-style passes; the coherence table and bars (D7b) in the record; `docs/lookdev.md` gains "Judging a
style"; the PR's "For Ryan" lists every palette and constant.
Hill to climb: styles with zero engine diff, 1 (physical) → 4; the coherence bars held, 0 → 4.

## Part E: the grown world (`rywible/wrela`, branch `engine/wc3-e`)

`land` already decides where each tree stands, of what species, how old, how lit, how it leaned
and how it died (`land::grow::Tree`), and what the ground under it is (`land::soil`,
`land::bake::tile_fields`). What it doesn't decide is the organism's form: a tree is an authored
`Form` (`content/trees`, B1.1), a fern a row of `SPECIES`, moss a noise. This part grows each
organism from a genome and its site's history, with the renderer's representations as the
output (a `Plant` of limbs and clumps; the grass system's blades and fronds; fields for moss,
litter and fungi), so the frame draws them as it does today. The principle every step holds:
**every visible thing has a cause the record can name.**

Two names, so they aren't confused: `land::grow` is the forest's history (what stands where);
`engine::growth` (new) is an organism's development (what shape it has).

**Order within Part E:** E0, E2 (the history's light and events, which E1 reads), E1, E3, E4,
E5, E6, E7, E8.

### E0. Provenance and the growth core

- `engine/growth.wrela` (new): `pub struct Grown: Copy + GpuData { genome: u32 (a hash of the
  genome's values), site: Site, seed: u32, version: u32 }`, `pub struct Site: Copy + GpuData {
  light: f32, wetness: f32, slope: f32, wind: f32, shade_above: f32, age: f32 }`, and `pub enum
  Provenance { Grown(Grown), Authored }`. Every kind the wood draws (`wood::Wood`'s kinds), every
  grass species instance's placement, every moss, litter and fungus field carries a `Provenance`
  (a field on `KindDesc`, on `grass::Species`, on the new fields' structs).
- A deterministic growth RNG: `growth::Rng` seeded by `(genome, site, seed)` and stepped in a
  fixed order; no `std::rng` global, no GPU in any growth (D33: byte-identical on every device).
- `engine/tests/growth-provenance`: a test walks a built program's kinds and species and
  counts `Authored`; the vale's count must reach 0 by E8 (G-grown); the floor's is recorded.

**Done when:** (1) the types exist and every kind and species carries a provenance (the test
compiles and counts); (2) the vale's `Authored` count pasted (the *now* for G-grown).

### E1. Trees grown

**E1.1 The genome.** `content/trees/genomes.wrela`: `pub struct Genome: Copy` with, per species
(oak, beech, birch, pine, hawthorn, blackthorn, alder, the floor's seven), the parameters a
self-organizing tree model needs, each with a source where one exists and **proposed**
otherwise:

| Parameter | Meaning | Source |
|---|---|---|
| `apical` | apical control: how much the leader suppresses laterals (0 decurrent, 1 excurrent) | Palubicki et al. 2009 §4 |
| `branching` | the lateral's angle from its parent (mean, spread) | the species' field guides, **proposed** |
| `divergence` | phyllotaxis: the angle between successive buds (137.5° for most broadleaves, 2-ranked for beech) | Prusinkiewicz and Lindenmayer 1990, ch. 4 |
| `internode` | the shoot's length per year in full light, and its fall with shade | Palubicki 2009; the habit's `rate` |
| `phototropism`, `gravitropism` | how far a shoot bends toward light and against gravity per year | Palubicki 2009 §4.3 |
| `shed` | the light below which a branch is shed, and the years it takes | Palubicki 2009 §4.2 |
| `pipe` | the pipe-model exponent relating a limb's section to the leaves beyond it (2 for a pure pipe model) | Shinozaki et al. 1964 |
| `bark` | the bark's thickness per year of girth, and its cracking rule (E1.4) | Lefebvre and Neyret 2002 |
| `leaf` | the leaf kind, its size in sun and in shade, its density per shoot | **proposed** from the habit |
| `wind` | thigmomorphogenesis: the leader's shortening and the trunk's thickening per unit of exposure | Jaffe 1973 (the effect; the numbers **proposed**) |
| `lifespan`, `tallest` | from the habit | `land::grow::Habit` |

**E1.2 The growth program.** `engine/growth.wrela`: `pub fn grow_tree(g: Genome, site: Site,
events: [Event], seed: u32) -> Plant` (the renderer's `plant::Plant`: limbs as `Tube<4>` curves
with radii, clumps for the crown), a self-organizing model (Palubicki et al. 2009, with space
colonization's bud competition, Runions et al. 2007, for the crown's filling), stepped a year at
a time to the site's `age`:

1. **Light.** The tree's light each year comes from its own history record (E2's
   `light_at(t, age)`, inferred or recorded as it says: the canopy's light at it, recorded as the
   forest grew, so a nurse tree
   that shaded it and rotted away still counts), shaped within the crown by a shadow-propagation
   grid of the tree's *own* crown (a 0.5 m voxel grid, Palubicki's, for self-shading) and by the
   record's `lean` direction (the side the light came from); each bud's light is the record's
   value times the grid's at it. The neighbours' records are not read (a reconstruction from
   the survivors misses the causes that are gone: R1 F4).
2. **Bud fate.** Each bud takes a share of the tree's growth by its light and the apical
   control's weighting (the extended Borchert-Honda model Palubicki uses): a bud with enough
   share makes a shoot of `internode` length times its light, bent by phototropism toward the
   light's gradient and by gravitropism; a shoot's buds are laid at `divergence`.
3. **Shedding.** A branch whose buds' light has been under `shed.light` for `shed.years` is
   dropped; its stub stays as a knot on the parent (a `Tube` of one segment).
4. **Girth.** Each limb's radius from the leaves beyond it by the pipe model; the trunk's lean
   from the history's `lean` as reaction wood over the years the record says.
5. **Events.** The record's `events` (E2: up to four, with their years and strengths) and a
   goal's (E6): a gale breaks the leader above a height drawn from its strength and the
   exposure, and a lateral takes over (the forked crown of a broken tree); browsing removes low
   shoots in its years; a neighbour's fall tears a side of the crown; a lightning scar strips
   bark along a spiral.
6. **Wind.** The site's exposure shortens the leader and thickens the trunk by `wind`'s rates;
   the crown's centroid moves downwind by the exposure.
7. **The crown.** The remaining shoots' leaves become clumps (`Plant::clump`) sized by the
   leaf's sun or shade size at each shoot's light; the cards cook as today (`cook_cards`).

   **The growth graph and its rendering.** The program's own structure is `growth::Tree`: a
   `Vec<Node>` (parent, age, length, direction, radius, light, kind: shoot or knot), unbounded
   but for `BUDS_MAX` a year. The renderer's `plant::Plant` holds `MAX_LIMBS` (48) limbs and
   `MAX_CLUMPS` (128) clumps, and `Plant::tube` drops a limb past that silently
   (`engine/plant.wrela:130, 167`: the council's R1 B-F4). The conversion `into_plant` is
   explicit: the limbs ordered by their pipe-model section, the largest `MAX_LIMBS` kept as
   tubes (a chain of nodes with one child becomes one `Tube<4>` fitted through its points), the
   rest folded into their parents' clumps (their leaves counted into the clump's size); the
   conversion's **error** is measured on like against like (R2 F9): the **woody** silhouette
   of the full graph (every node drawn as a capsule of its radius, an orthographic side view
   from the tool's own rasterizer, no leaves) against the woody silhouette of the rendered
   plant (its tubes alone, the same rasterizer, no clumps), IoU at least **0.97**
   (**estimated**) for every specimen on E1's page, printed in `grown.json`; and the crown's
   volume (the clumps' union) within 5 % of the graph's leaf volume. `MAX_LIMBS` is raised to
   what a veteran needs at that IoU, with the draw cost per instance measured (a limb is a tube
   draw); clumps past `MAX_CLUMPS` are merged by nearest pairs until they fit, their volume
   kept. `Plant::tube` and `Plant::cushion` (`plant.wrela:232`, which also drops silently) assert
   in debug builds instead of dropping.

   Determinism: the CPU alone, `growth::Rng`, limbs in creation order. Cost: the first lever is
   the grid's size (the tree's bounding box plus 2 m); the second, a year's buds capped at
   `BUDS_MAX` (4,096, **proposed**) by their share.

- **Tests** (`engine/tests/growth-trees`, each on the CPU, with the numbers measured from the
  grown `Plant`):
  1. A pine genome in the open grows one leader (one limb reaches 90 % of the height; no lateral
     over 60 %); an oak genome grows a decurrent crown (at least three limbs over 60 %).
  2. A tree grown with a gap to its east (a neighbour to the west, none east) has its crown's
     centroid at least 0.2 × `reach` east of its trunk.
  3. A tree at exposure 0.8 is at least 20 % shorter and 20 % thicker at the base than the same
     genome at exposure 0.
  4. An 80-year oak under a closed canopy (shade_above 0.9 from age 20) has no live limb below
     0.4 of its height; the same in the open has limbs below 0.2.
  5. A gale event at year 40 leaves a fork: two limbs over 70 % of the height from one break.
  6. The same genome, site, events and seed grown twice give byte-identical `Plant`s (a hash);
     a seed change gives a different one.
  7. Cost: a 100-year oak grows in under **50 ms** on the M4 (**estimated**; measured and
     recorded; E7 sets the budget it must meet at load).
  8. The conversion's woody IoU is at least 0.97 and the crown's volume within 5 % for every
     specimen on the page, and no limb or clump is dropped silently (the debug asserts never
     fire).
- **Gate:** the eight tests; a specimen page (E1.5's `wrela tool grow --specimens`: each species
  at ages 10, 40, 100, 200 in the open, at the forest's edge and under canopy, as stills) in the
  PR.

**E1.3 Individuals near, archetypes far (D42).** `engine/wood.wrela`: a kind is now either an
archetype (a species × age class {sapling, young, mature, old, veteran: the habit's lifespan in
fifths} × light class {open, under} = 10 a species, 70 for seven: **proposed**, the memory gate
to decide) grown at build from the median site of its class and cooked as kinds are today
(cards, impostor, volume), or an individual: a tree within `NEAR` (120 m, **estimated**) of the
eye grown from its own record at load (E7), holding its own limbs and cards and no impostor (it
is near). An instance beyond `NEAR` draws its species' nearest archetype by age and light; the
hand-over at `NEAR` is the wood's existing level hand-over (an individual's cards to the
archetype's impostor), judged by G-look's pop on the dolly.

- **Tests:** `engine/tests/growth-wood`: the vale's instances within `NEAR` are individuals
  (their provenance's `seed` is the record's); beyond, archetypes; a dolly through `NEAR` pops
  no more than the kinds' hand-over did at B16 (`still_pop` within 10 % of B16's on shot 8).
- **Gate:** G-memory: the vale under 512 MiB with `NEAR_N` individuals held (the number
  measured and written into `design.md`); G-profile: `kind` and `built things` labels on the
  walk within 0.3 ms of C6's.

**E1.5 The specimen scaffold** (built before E1.2's gate is run; numbered after E1.4 only
because E6 extends it). `studio/grow/` with `wrela tool grow -- --species <s> --age <y> --light
open|edge|under --wind <0..1> --seed <n> --out <dir>` (a specimen: its side-view silhouette,
a lit still through the physical style, and `grown.json`: the genome, the site, the limb count,
the height, the crown's centroid, the conversion's IoU) and `--specimens <dir>` (the page E1's
gate names). E6 adds `--fit`, `--goal` and `--hash` to the same tool. Registered in
`wrela.toml` as `[tools.grow]` with `write = ["ws=.", "out=build/grow"]`.

**E1.4 Bark and moss as causes.** The bark material (`content/trees`'s `Bark`, `plant.wrela`'s
bark field) reads `history` (D1): cracks from the trunk's expansion (the crack density from the
girth's growth rate, their depth from age: Lefebvre and Neyret's fracture model, simplified to
a Worley-edge pattern whose cell size and depth follow the record); moss on the side the light
leaves (where `history.y` is low) and wetness is high, as an `engine/growth.wrela` field `moss_at
(site, p)` (a reaction-diffusion patch texture, E4, placed by cause). Lichen on old bark by age.

- **Tests:** a 200-year oak's bark has deeper cracks than a 40-year one's (the relief's range);
  moss covers the north face more than the south under the vale's sun (a readback of coverage
  on two faces of a test trunk).
- **Gate:** G-look on shots 3 and 11 (deliberate).

**Done when, E1:** (0) E1.2 reads light and events only through E2's accessors (a grep that
`grow_tree` takes no neighbours' records); (1) the genome table in `content/trees/genomes.wrela`
with every source cited in `docs/citations.md`; (2) the eight growth tests pass with the
measured cost and the conversion's IoUs (paste);
(3) the specimen page in the PR; (4) the wood tests pass and the memory and labels gates hold
(paste); (5) the bark tests pass and G-look on shots 3 and 11 pasted; (6) the vale's `Authored`
count falls by its trees (paste).
Hill to climb: the vale's authored kinds, 7 → 0; a 100-year oak's growth, (first measurement) →
under 50 ms.

### E2. Light over a life, and the events, from the history (built before E1)

E1.2 reads each tree's light over its life and the events that befell it. The history's record
holds neither: `Tree.light` is the last years' (`land/grow.wrela:355`), a neighbour that shaded
a sapling and rotted away is gone from the records (`:2524` retains the gone out), and the
land's memory keeps only the latest disturbance at a cell (`:1621`, `:1665`). The council's R1
F4: a reconstruction from the survivors misses real causes. So the history records them as it
runs, compactly, and the bake carries them. Coverage (R2 F3): the floor's habits live to 600
years (the oak) and 1,200 (the ancient oak, `games/last-green/species.wrela:30, 206`), death is
probabilistic so a survivor can outlive its lifespan, and the history starts with trees already
aged (`land/grow.wrela:2356`); the representation covers all three:

- `land::grow::Tree` gains `light_by_age: [u8; 32]` (the crown's light, 0–255, sampled at
  fixed **ages** that rise strictly for every habit (the council's R3 B-1: the blackthorn lives
  50 years): with `span` = 1.5 × the habit's lifespan (the oak's 900, the ancient oak's 1,800,
  the blackthorn's 75) and `early` = `min(75, span / 2)`, samples 0–15 at ages `k × early / 15`
  for k in 0..16 (the years a crown's architecture is decided, dense) and samples 16–31 at
  `early + (k + 1) × (span − early) / 16` for k in 0..16 (the last at `span`); a survivor past
  `span` reads its last sample), `recorded_from: u8` and `recorded_to: u8` (the first and last
  sample indices the history itself recorded: a tree older than the history's start has its
  earlier samples **inferred**, the survey's canopy light at the start, constant, and the growth
  is told so; the samples past `recorded_to` are the **unfinished tail**, never written and
  never read: between the last recorded sample's age and the tree's present age (or the age it
  died), `light_at` interpolates toward `Tree.light`, the present value the history keeps
  (`land/grow.wrela:1445`), and an age past the present is a caller's error: the council's R3
  A-2) and `events:
  [TreeEvent; 4]` (`TreeEvent { year: i16, kind: u8, strength: u8 }`; kinds: gale, fire,
  browse, a neighbour's fall, lightning; the four strongest, kept as the history runs).
  `simulate` fills them each step from what it computes already (`canopy`'s light at the tree
  whenever the tree's age crosses a sample age; the events' effects on it).
- `land::bake`: `TREE_BYTES` 22 → 72 (**estimated**: 32 samples, 2 indices, 16 bytes of events;
  the bake's `VERSION` to 6; the floor's tiles grow by it: measured and recorded; if the floor's
  first-frame payload grows by over 10 %, samples 16–31 are halved to 8 with `span` kept, so the
  covered lifetime doesn't shrink, and the number recorded).
- `land::grow::light_at(t: Tree, age: f32) -> (f32, bool)` reads the series (interpolating
  recorded samples, then toward `Tree.light` at the present age; the bool says inferred;
  `age` past the tree's present or its death is clamped to it and a debug assert names the
  caller), and `events_of(t) -> [TreeEvent; 4]`: what E1.2 consumes.
- **Tests** (`land`'s own `@test`s): two constructed maps with the same final forest but
  different early shading (a nurse tree present in one for the sapling's first 30 years and
  felled before the end in both) give different `light_by_age` for the sapling in samples 0–6;
  a gale in year 40 is in the tree's `events`; each recorded sample equals the canopy's light
  at the step the tree crossed its age, and `light_at(the present age)` equals `Tree.light`;
  an oak whose history stops at age 100 after its light changed at 80 (between samples at 75
  and 126.6) reads ages 90 and 100 as the interpolation toward `Tree.light`, the same before
  and after a bake round trip, and reads no unwritten tail sample; a tree that died between
  two sample ages reads its death's light at and past its death; histories of 400, 600 and
  1,200 years and a survivor at 1.4× its
  lifespan round-trip through the bake and read every age without going out of range; every
  habit's sample ages rise strictly, before and after the payload lever halves samples 16–31
  (the blackthorn's included), and `light_at` interpolates and reports the inferred boundary
  for the blackthorn; a tree present at the history's start has `recorded_from > 0` and
  `light_at` says inferred below it;
  halving samples 16–31 keeps the early shading distinguishable (the first test still passes);
  the floor's tile bytes before and after pasted.

**Done when:** (1) the fields, filled by `simulate`, with the tests; (2) the bake's version and
bytes pasted; (3) the accessors exist and are documented in `land/grow.wrela`'s header.

### E3. Ferns, grass and herbs grown

`engine/grass.wrela`'s `SPECIES` (33 rows of shapes and sizes) becomes the output of growth
programs in `engine/growth.wrela`, one per form, the rows' numbers derived, not set:

- **Ferns** (`grow_frond`): a frond is a rachis with pinnae at phyllotactic spacing, each pinna
  a smaller copy (two levels: bracken's three), unrolled from a fiddlehead by age (circinate
  vernation: the frond's curl falls from 2π at emergence to 0 over its first season); a crown of
  `fronds(age, light)` fronds; shade ferns larger and darker (plasticity by `light`).
- **Grass** (`grow_tussock`): tillers added per season from the base by the light and water
  available (a tussock's count), each a blade whose length follows a growth curve to the
  species' maximum, its droop by its length and the season; the sward's density from the
  tile's `light` and `wetness` (open and wet: dense; under canopy: sparse).
- **Herbs** (`grow_rosette`): leaves at `divergence` round a centre, a stem and a flower by
  season.
- The placement (`grass::place`) reads the cause maps (`tile_fields`: light, wetness, litter,
  soil kind) through `growth::flora_at(site) -> Flora` in place of `species_of`'s noise.
- **Tests** (`engine/tests/growth-flora`): a fern at age 0.1 season has its fronds curled
  (mean curl over π); at 1.0, open (under 0.3); a shade fern's frond is at least 1.3× a sun
  fern's; a tussock in the open has at least twice the tillers of one under canopy; ferns are
  placed where `light` is low and `wetness` mid, grass where `light` is high (counts over a
  test tile with a known cause map); byte-identical twice.
- **Gate:** G-look on shots 1 and 6 (deliberate); G-profile: the grass's labels within 0.3 ms.

**Done when:** (1) the tests pass; (2) no `SPECIES` row holds a shape number that a growth
program doesn't derive (the rows hold only the genome: cited or **proposed**); (3) G-look
pasted; (4) the vale's `Authored` count falls by its flora (paste).

### E4. The floor's small causes: moss, lichen, litter, deadwood, fungi

- **Moss**: a reaction-diffusion patch (Turing 1952; Gray-Scott as the practical form) run once
  per cause class into a 256² tile (`growth::moss_patch(wetness, light) -> Texture`), placed on
  ground, logs and the shaded wet faces of trunks and rocks by cause (E1.4 for trunks).
- **Lichen**: crustose patches on rock faces by exposure and age (a diffusion-limited growth on
  the surface, run once into a patch texture per class), already a material parameter
  (`RockLook`'s lichen): now placed by cause, its coverage from the cause.
- **Litter**: leaf litter density from the canopy above (`tile_fields`' `litter`) and the
  season, as the ground material's litter layer (exists as a texel field in the clipmap:
  `terrain::Cover`; now caused).
- **Deadwood**: the history's fallen trees (`state`, `fell`, `fall`) as logs: a grown tree's
  trunk laid along `fall` at the record's place, its bark lost by rot age (`years since fell /
  habit.rots × tall`), its crown gone; standing dead trees keep their limbs without leaves,
  shed by age.
- **Fungi**: brackets on logs past a rot age, caps in litter where wetness is high, as small
  fields placed by cause (a `growth::fungi_on(log, rot_age) -> Vec<Placed>`).
- **Tests** (`engine/tests/growth-floor`): moss coverage on a test log is higher on its upper,
  wetter half; litter density under a closed canopy is at least 3× the open's; a tree fallen
  20 years ago has no bark on 60 % of its length and at least one bracket; byte-identical.
- **Gate:** G-look on shots 1, 3 and 11 (deliberate).

**Done when:** (1) the tests pass; (2) the five causes' placements counted on the vale and in
`design.md`'s "What is grown" table; (3) G-look pasted.

### E5. The ground from the soil

`terrain`'s ground material reads `land::soil`'s kind and the tile's wetness (D1's `history.z`
at the ground): alluvium as dark loam, thin soil as stony with rock showing, gley as grey and
wet, peat as black; the colour and roughness per kind in `examples/vale/design.md` as
**proposed**. The clearing and the floor keep their ground until H1.

- **Tests:** the vale's valley floor reads alluvium's colour and its crests thin soil's (two
  readbacks).
- **Gate:** G-look on shots 1 and 8 (deliberate).

**Done when:** (1) the test; (2) G-look pasted; (3) the kinds' table in `design.md`.

### E6. Steering (`wrela tool grow`, D34)

`studio/grow/`: the growth programs driven from the command line, and their inverses:

- `wrela tool grow -- --species oak --age 180 --light open --wind 0.6 --seed 3 --out <dir>`:
  grows the specimen and writes a side-view silhouette (orthographic, 1024², the plant's field
  thresholded), a lit still through the physical style, and `grown.json` (the genome, the site,
  the limb count, the height, the crown's centroid).
- `--specimens <dir>`: every species at ages 10, 40, 100, 200 × {open, edge, under} as a page
  (E1's gate).
- `--fit <silhouette.png> --species oak --out <dir>`: finds the site (age, light, wind, slope),
  the seed and a genome delta within each parameter's range (`Genome::RANGES`, ±25 % **proposed**)
  that maximize the IoU of the grown silhouette against the target: CMA-ES (Hansen 2016's
  tutorial's algorithm; a std implementation in `studio/toolkit/cma.wrela`, shared with D8 and
  F5) from a fixed seed, 400 evaluations, the trajectory printed every 20; writes the best's
  `grown.json` and silhouette.
- `--goal <goal.toml> --out <dir>`: a structured goal (§8's example: species, age, exposure,
  light, events with years, a lean) grown directly, with the goal's measurable properties
  checked and printed (height within 10 % where given; the crown's centroid toward the named
  light; a lightning scar present where named).
- **Tests** (`studio/grow/tests.wrela`): the identity fit: three silhouettes grown with hidden
  parameters are fitted to IoU ≥ 0.85 each within 400 evaluations; the goal test: a goal with a
  gale at year 40 grows a fork; a goal's height within 10 %.
- **Gate (G-steer):** the identity fits ≥ 0.85; two public-domain photographs of lone oaks
  (Wikimedia Commons, public domain or CC0, their URLs in `docs/citations.md`'s assets table,
  used as silhouettes to fit against, not as images to reproduce) fitted and their IoU
  **recorded** (not gated: a photo's silhouette holds a camera's perspective and a species'
  variation the model may not; **0.70** is the plan's expectation, **estimated**).

**Done when:** (1) the tests pass (paste the three identity IoUs and evaluation counts);
(2) the two photo fits' IoUs recorded in `docs/bench.md` with their trajectories; (3) the goal
file's format in the tool's header and `docs/toolchain.md`; (4) `--specimens` page in the PR.
Hill to climb: identity IoU, (first) → ≥ 0.85; photo IoU, (first) → recorded.

### E7. Caching and determinism (G-grown)

- **Archetypes at build.** The 70 archetypes grow at build time into a bake file per species
  (`land::bake`'s path: `std::io::Shipped` files the program fetches), or as `const`s with
  `@fuel` where a species' ten fit under the data-section report's 1 MiB (`wrela build --report
  data`); whichever, the growth runs in the build, not at load.
- **Individuals at load.** The trees within `NEAR` grow on a job (`std::par::spawn`) under the
  arrival cover: the first ring's (those within 40 m of the first eye) before playable, the
  rest after; the budget: the first ring's growth under **1,000 ms** on the M4 (**estimated**:
  E1.2's cost sets what `NEAR_N` can be; if a 100-year tree costs 50 ms, 20 trees fit the ring;
  the lever is the growth's cost, then the ring's radius), and G-cold still holds (C5).
- **The page's cache.** A grown individual is stored in the page's storage
  (`std::io::storage`) keyed by the hash of **every** growth input: the genome, the site, the
  seed, `version`, and the tree's history record (its `light_by_age` and `events`, E2), so a
  changed gale or a different early shading changes the key (R1 B-F4); a version bump
  invalidates everything. A test changes one event and asserts a miss and a different result.
- **Determinism.** `wrela tool grow --hash --package examples/vale` prints the hash of every
  grown structure (the archetypes' and the first ring's individuals'); it runs twice natively
  and once in Chrome on the M4 (the page printing the hash), and the three must be
  byte-identical (other devices deferred, D37). A difference is a determinism fault
  in the growth (a float order, a `std` function with platform variance) to find, not a
  tolerance to add.
- **Tests:** the first ring grows under the budget (timed in a test on the M4, the number
  pasted); the cache round-trips a grown `Plant`; the hash is stable across two runs.

**Done when:** (1) the archetypes are built at build (the build's log or `--report data`
pasted); (2) the first ring's growth time pasted and under 1,000 ms; (3) G-cold and G-warm still
hold on the vale (paste three runs each); (4) the three hashes (two native runs, one in Chrome)
pasted and identical; (5) `wrela tool grow --hash` documented.

### E8. The vale regrown, and the third panel

With E1–E7 in, the vale's `Authored` count is 0 (G-grown); every plant's cause is in the record.
Regenerate the twelve stills (`build/vale-e8/`), run G-look on all twelve against `build/vale-
d3/` (a deliberate change: the judge may prefer either; the measures must hold), and run the
panel (G-reference #3) against the public references.

**Done when:** (1) G-grown: the `Authored` count 0 (paste the test); the growth of the vale's
region (archetypes at build plus the first ring at load) timed and pasted; (2) G-look's measures
and the judge's scores on the twelve pasted; (3) the panel's tally in `docs/bench.md` under
"Public visual references" beside B5's and D11's (H2 is judged here: did the grown floor move the rows the baseline
lost?); (4) `examples/vale/design.md`'s "What is grown" table complete.

---

**Done when, Part E:** every step's checklist holds; G-grown passes; G-steer passes; the third
panel is in the record; the PR's "For Ryan" lists every **proposed** genome value and the
archetype count.
Hill to climb: pairs won or equal against the selected public references, (D11's) → (E8's),
recorded; the vale's
authored plants, (E0's count) → 0.

## Part F: grown creatures and motion (`rywible/wrela`, branch `engine/wc3-f`)

A creature's authored part is its body plan and its face; the rest grows (D35). The vale's
creature is a grazer (the floor's `content/grazer` and `content/fawn` are the starting bodies:
a roe-deer-like animal, **proposed**), grown from a genome; its gaits are optimized in
simulation; a stylizer over the pose stream carries the world style into motion. Motion is the
long pole of the whole plan (H6): the bar for its panel is set honestly (40 %), and the steps
are ordered so the creature improves in the shots before the motion panel is run.

### F1. The genome and the body plan

`content/kit/genome.wrela` (new): `pub struct CreatureGenome: Copy { plan: Plan, allometry:
Allometry, coat: CoatGenes, muscle: MuscleGenes, gait: GaitGenes }`:

- `Plan`: the authored part: a `content/kit/quadruped.wrela` skeleton (its chains and legs) and
  at most twelve shape handles (withers height, chest depth, neck length, head length, muzzle
  length, ear size, tail length, leg length share, hoof size, rump height, eye size, brow): the
  numbers the quadruped kit already takes as proportions.
- `Allometry`: per part, an exponent `k` and coefficient `b` in Huxley's `y = b·x^k` (x the
  body mass or the withers height), so the juvenile's and adult's proportions follow one law:
  a fawn's legs are long for its body (k < 1 for the legs' share), its head large (k < 1), its
  neck short (k > 1); the exponents **proposed** from Huxley 1932 and the species' field
  measurements where found, cited.
- `CoatGenes`: reaction-diffusion parameters (feed, kill, the two diffusion rates), the pattern's
  wavelength relative to body size, and the fade of the juvenile pattern with age.
- `MuscleGenes`: the rule relating a muscle's section to the load it carries (F4).
- `GaitGenes`: the ranges the gait optimizer searches (F5).

**Done when:** (1) the struct with the vale's grazer's genome, every number cited or
**proposed**; (2) a test that builds the creature from its genome at age 1.0 and asserts its
withers height and chest depth match the handles.

### F2. Allometric growth

`engine/growth.wrela`: `pub fn grow_creature(g: CreatureGenome, age: f32) -> Body<...>` (the
creature kit's body at that age): each part's size from the allometry at the body mass the age
gives (the species' growth curve, a logistic from birth mass to adult mass over the years to
maturity, **proposed** per species), the skeleton's bones scaled with their parts, the mass
integrated (`physique::integrate`) and checked against the curve.

- **Tests** (`engine/tests/growth-creature`): at age 0.1 the legs' length over the withers is at
  least 1.15× the adult's (long legs); the head's over the body's at least 1.2×; the integrated
  mass at age 1.0 within 10 % of the genome's adult mass; the body at age 0.5 is between the two
  in every handle (monotone); byte-identical twice.
- **Gate:** G-look on shot 9 (the creature's form at its age: deliberate).

**Done when:** (1) the tests; (2) G-look pasted; (3) the growth curve in `design.md`.

### F3. The coat by reaction-diffusion

`growth::coat(g: CoatGenes, body, age, seed) -> CoatMap`: a Gray-Scott reaction-diffusion
(Pearson 1993's parameterization of Turing's 1952 mechanism) run on a 2D map of the body's
surface (the creature's parts have the kit's cylindrical parameterization; where they don't, a
tri-planar map), 2,000 steps (**proposed**), its pattern's wavelength set by scaling the
diffusion rates to the body's size (Murray's result: the same mechanism gives spots on a small
body, stripes on a larger, none on the largest: the spots-to-stripes transition is a test);
the juvenile's spots fade by the genome's `fade` with age; the map is read by the coat material
as its albedo's pattern (the `painted` block of `content/kit/anatomy.wrela` reads it), with the
hair's direction field unchanged.

- **Tests:** the pattern's wavelength on a body twice as large is within 20 % of twice the
  small body's (a Fourier peak of the map); the fawn at age 0.1 has spots (the map's
  connected components over 30, each under 2 % of the map); at age 1.0 under 5 components; the
  pattern is byte-identical twice and differs by seed; no seam at the parts' joins wider than 2
  texels (a continuity test across a join).
- **Gate:** G-look on shot 9 (deliberate).

**Done when:** (1) the tests; (2) G-look pasted; (3) the coat parameters in `design.md`.

### F4. Musculature from load

The walker is kinematic (`engine/gait.wrela:341` advances the root; `:368` plants feet by
construction): it gives poses, not forces (the council's R1 F5). Loads come from the poses by
**inverse dynamics**, specified so two implementations give the same torques (R2 F4):

- `growth::joint_loads(poses: [Pose], body: Body, dt: f32) -> Loads`: each part's mass, centre
  and inertia from `physique::integrate`; each part's linear and angular acceleration from the
  pose stream by central differences at 60 Hz; the body's total required external wrench at
  each frame: force `F = m (a_com − g)`, moment about the centre of mass `M = Σ_i (r_i × m_i a_i)
  + Σ_i (I_i α_i + ω_i × I_i ω_i)`.
- **Contact allocation:** the planted feet (those the walker holds in stance that frame) carry
  `F` and `M` as point forces at their contacts: the 3 n unknowns `f` (n ≤ 4 feet), **in
  units of the body's weight** `m g` (dimensionless, so the solver is the same for a fawn and
  a stag: the council's R3 A-1), minimize `‖A f − b‖² + ε ‖f‖²` subject to each foot's vertical
  component `f_z ≥ 0` and its horizontal components free, where `A f = (Σ f_i, Σ (r_i / h) ×
  f_i)` and `b = (F / (m g), M / (m g h))` (`h` the standing height: the moment rows in units of
  `m g h`, the lever arms in units of `h`); `ε = 1e-6` makes the minimizer unique and within
  `ε ‖f‖² / 2` of the least residual, so a quarter-weight stance's bias is `ε / 4` of the weight
  (2.5e-7, whatever the mass); the solver is a bounded-variable active set over the vertical
  bounds (Stark and Parker's BVLS shape), deterministic, the feet in a fixed order; the forces
  are scaled back to newtons for the torques. The **residual** `‖A f − b‖` is recorded
  per frame. A frame with no planted foot, or a residual over 0.05 (5 % of the weight, or of
  `m g h` for the moment rows), is **infeasible**: counted, excluded from F5's effort, and a
  candidate with over 5 % infeasible frames in a stride is invalid in F5.
- **Torques:** Newton–Euler from each planted foot up its chain to the spine and from the spine
  out to the other chains, **per frame**; `Loads` holds the torque history per joint. F4 takes
  each joint's peak over a stride for the muscles; F5 integrates the history.
- `growth::muscles(g, body, loads)` sizes the anatomy kit's muscles (`content/kit/
  anatomy.wrela`'s `muscle`, `bundle`) by the peaks (a constant stress, Alexander's rule,
  **proposed** 0.3 MPa; a muscle's moment arm from its origin and insertion; its belly's length
  from the bones it spans). Before F5 runs, the standing pose's loads size them; F5 re-sizes
  from the optimized gait. The authored muscles of the fawn become the rule's output; the
  difference is recorded as stills.

- **Tests:** a static known-load fixture: a two-segment leg holding a 10 kg mass at a 0.3 m
  lever gives a knee torque of 29.4 N·m within 5 %; an **accelerating-body fixture**: a rigid
  body of known mass on two feet accelerating at 2 m/s² forward and 1 rad/s² in pitch: the
  allocated contact forces sum to `m (a − g)` within 1 % and their moment to `M` within 1 %;
  an **asymmetric stance fixture**: three feet at known places under a static body: the
  allocation matches the hand-solved minimum-norm solution within 1 %; a **symmetric four-foot
  stance at 10, 50 and 100 kg** allocates a quarter of the weight to each foot with a residual
  under 1e-5 at every mass and is unchanged under any permutation of the feet; a **braking
  fixture** (the body decelerating at 3 m/s²) allocates a
  negative horizontal force whose sum is `m a` within 1 %; a hop (an airborne frame) counts an
  infeasible frame; a heavier genome (adult mass ×1.5) grows thicker thigh
  muscles (the section ×1.3 or more); the muscles' total mass is between 30 and 45 % of the
  body's (the mammalian range); byte-identical.
- **Gate:** G-look on shot 9.

**Done when:** (1) the tests; (2) G-look pasted; (3) the rule and the allocation in
`design.md`.

### F5. Gaits optimized in simulation (a kinematic optimizer)

The spring walker (`Walker`, `SPRINGS`, `step`) is kinematic: `energy_of` is one secondary
spring's stored energy, not locomotion's cost, and a planted foot never slides by construction
(R1 F5). The optimizer is therefore over **what the poses show**, with an objective stated so
the secondary springs can't game it:

- *effort*: the sum over the skeleton's joints of `|τ_j(t) · ω_j(t)|` (F4's per-frame torque
  history times the joint's angular speed) integrated over a 20 s walk, per metre travelled (a
  mechanical-work proxy), over the feasible frames; the secondary chains (head, ears, tail) are
  **frozen at their rest pose** in the pose stream this proxy reads (their masses ride rigidly
  on the body), so their springs can't move the measure in either direction (R2 F4: excluding
  their joints alone would leave their motion in the upstream torques);
- *stability*: the share of stance time the centre of mass's ground projection lies outside the
  support polygon (the planted feet's hull; for a trot, a 4 cm band round the diagonal pair's
  line): a penalty, and a **fall** if it exceeds 25 % of any stride;
- *contact*: a planted foot's gap from the ground (on a slope or a step the body's reach can
  leave a planted foot hanging: the gap is the penalty) and the swing foot's clearance (under
  2 cm over the ground's maximum along its arc is a penalty);
- *speed*: the error against the gait's target speed.

CMA-ES (`studio/toolkit/cma.wrela`, E6's) over the walker's parameters within `GaitGenes`'
ranges (the springs' stiffness and damping, the stride's phase offsets, the swing height, the
lean), one optimization per gait (walk, trot, bound: **proposed** three) per terrain class
(level, 15°, 30°, a 0.3 m step, the vale's own uneven ground), run at build (`const` with
`@fuel`, or a bake file) so the game loads the result.

- **Tests** (`engine/tests/growth-gait`): the gaming check: two candidates that differ only in
  the head's spring stiffness have the same effort within 1e-6 (exact, since the head is
  frozen in the proxy); two torque histories with equal peaks and different integrated power
  give different efforts (a constructed fixture); a fixture's effort matches a hand computation
  within 5 %; a candidate with over 5 % infeasible frames is invalid (a hopping candidate); on the
  30° slope (up
  and down) the optimized walk's stability penalty is 0 and no planted foot hangs over 2 cm for
  60 s of simulation; over the 0.3 m log the swing clears it; after a 0.2 m drop the stability
  penalty is back to 0 within 3 strides; effort per metre at least 10 % under the hand-set
  walk's (the *now*); byte-identical from a seed.
- **Gate:** G-look on shot 10's strip (three frames at a moving edge, as plan two's temporal
  steps were judged); G-profile: the sim's CPU phase (`sim`) within 0.2 ms of C6's.

**Done when:** (1) the tests with the effort numbers pasted (hand-set → optimized); (2) the
optimization's trajectory per gait in `docs/bench.md`; (3) G-look pasted; (4) `design.md`
says the optimizer is kinematic and names its measures.
Hill to climb: effort per metre, (hand-set) → ≥ 10 % under; stance outside the support polygon
on the 30° slope, (hand-set: the share) → 0.

### F6. The pose stylizer (world style in motion, D31)

`engine/motion.wrela` (new): a filter over the pose stream between the gait and the skin, with
the world style's parameters: anticipation (a counter-pose of `ANTICIPATE` degrees for
`ANTICIPATE_S` seconds before a limb's swing: **proposed** 4°, 0.08 s), follow-through (the
secondary chains, head, ears and tail, overshoot by `OVERSHOOT` and settle: the gait's springs
already carry them; the stylizer scales their gain), exaggeration (the stride's height and the
body's bob scaled by `EXAGGERATE`), and holds (`ON_TWOS`: the pose updates every other frame
and holds between, as hand animation does; off for the physical world style). With every
parameter at 0 and `ON_TWOS` off, the pose is the gait's exactly.

- **Tests** (`engine/tests/motion-style`): with the parameters at 0 the pose equals the gait's
  (to the float); with `ON_TWOS` the pose changes only on even frames; anticipation's
  counter-pose precedes each swing by `ANTICIPATE_S` within a frame; the stylized walk still
  passes F5's slide test (the stylizer moves no planted foot).
- **Gate:** G-look on shot 10's strip for the physical world style (no change: identical) and
  the cel world style (deliberate).

**Done when:** (1) the tests; (2) G-look pasted; (3) the parameters in `content/styles/README.md`'s
world-style table.

### F7. Interaction

The creature touches the world: grass bends from its hooves (`grass::Contact` exists for the
climber's cloth and feet: the grazer's hooves and belly become contacts), ferns and low branches
push aside (a `plant::sway` term from the distance to the creature's capsules, the physique's),
water ripples and splashes at a footfall in the stream (the water's ripple source and the spray
particles at the contact), litter stirs (a particle puff at a footfall on litter).

- **Tests** (`engine/tests/interaction`): blades within 0.3 m of a hoof's contact bend at least
  20°; a fern within 0.5 m of the creature's chest leans away by at least 10°; a footfall in
  water emits at least 20 spray particles; none of these costs the `vegetation` or `particles`
  labels over 0.2 ms on the walk.
- **Gate:** G-look on shots 9 and 10; G-profile within 0.2 ms.

**Done when:** (1) the tests; (2) G-look and the labels pasted.

### F8. Eyes and the face (the authored handles)

The grazer's eyes as fields with a cornea (refraction by the index 1.376), an iris and a placed
highlight the style's `shade` gives (the physical style: the sun's specular; the cel style: one
highlight band at `HIGHLIGHT`), and the face's handles (brow, muzzle, ears) as F1's plan names
them, their expressions (ears turned toward a sound, the eyes tracking the player) driven by
the sim's existing senses (`land::senses`).

- **Tests:** the eye's highlight is at the reflected sun's direction within 5° in the physical
  style and a single band in the cel style; the ears turn toward a sound source within 0.5 s.
- **Gate:** G-look on shot 9.

**Done when:** (1) the tests; (2) G-look pasted.

### F9. The motion review (G-motion)

Shot 10's three-second strip (180 frames at 60 fps, the vale's creature walking toward the eye)
and B3's freely licensed footage of a real deer walking, through B4's review. Add to
`docs/lookdev.md` the motion rubric: *weight* (mass reads in motion), *timing* (gait rhythm and
changes), *contact* (feet plant without sliding), *secondary motion* (head, ears and tail
follow the body), *readability* (motion reads at a glance); each 0–3, plus a preference.
Record differences in animal, path, camera, frame rate and animation method where known.
Review at original speed; the source clip need not be three seconds or 60 fps. Agent judges
see each clip as a strip of frames sampled at 10 per second over the same seconds of walking
(the licence permits the extraction; the change is noted with the credit), the vale's strip
sampled the same way. No frame interpolation is used.

- **Gate (G-motion):** judges prefer the vale's motion or rate it equal in at least **40 %**
  of judgements; this is an internal subjective target against the selected clip, not a claim
  of equal animation systems. F5's gait tests pass; the agents' tally is the verdict (D38). Apply
  B4's
  calibration, access and complete-tally rules.

**Done when:** (1) the motion rubric in `docs/lookdev.md`; (2) the cited clip, conditions and
panel tally with every judge's sentences; (3) the verdict (pass or a **Blocked** note with the
rows lost and next lever: F5's optimizer's penalties, F6's parameters, in that order).

### F10. The fourth panel

The twelve shots through `Physical` against the public references (G-reference #4), with the grown
creature in 9 and 10. Recorded beside E8's.

**Done when:** the tally in `docs/bench.md` with H6 judged (did the creature shots move?).

---

**Done when, Part F:** every step's checklist holds; G-motion passes or is **Blocked** with its
record; the fourth panel is in the record; the PR's "For Ryan" lists the genome, the coat and
the stylizer's numbers.
Hill to climb: the motion panel's preference share, (first) → ≥ 40 %.

## Part G: the remaining visual gaps, as the reference review ranks them (`rywible/wrela`, branch
`engine/wc3-g`)

After Parts C–F the panel has run four times. What it says the vale still loses is what this
part builds, in the order the panel ranks it. The part is a menu with a rule, not an open
field: the implementer takes the menu's items in the ranked order and does nothing that isn't
on the menu. Each item is a full step with its own Done-when; an item the panel never ranks as
needed is recorded as "not needed: the panel passed before it" and not built.

### G0. The ranking

Selection reads the **development panel**, not the sealed one (R2 F2: a sealed set that picks
the work isn't sealed): §B4's panel run on the eight development shots against their public
references (eight judges, Wrela-owned controls, as the sealed panel). From its tally: for each
rubric row
(light, colour, value, depth, artifacts), the sum over the eight pairs of (the public reference's
mean score −
the vale's mean score), and for each pair its worst row with the judges' sentences. The rows
in descending order of loss pick the menu's items by the map below; within a row, the pairs
name which item (the map's shot numbers refer to the development shot of that class: 1 → `D1`,
2 → `D2`, 3 → `D3`, 4 → `D4`, 6 → `D5`, 8 → `D6`, 9 → `D7`, 12 → `D8`; a tool test asserts every
row of the map names an existing development shot, and three synthetic tallies, reflection,
backlighting and spray losses, select G2, G3 and G6 without a sealed result). Write the ranking and
the chosen order into
`docs/bench.md` "Public visual references" before the first item's commit, with the map's reasoning
per
item (one sentence: which pairs and sentences picked it).

| Row lost | Pairs and sentences that name | Menu item |
|---|---|---|
| light | "flat", "no bounce", "the shade is dead", "the wood's interior" (12), "under the canopy" (1) | G1 |
| light, artifacts | "reflections", "the mere", "the water" (4, 8) | G2 |
| light, colour | "the leaves glow", "translucent", "backlit" (2) | G3 |
| artifacts | "shadows", "hard", "aliased edge of the shadow" (6, 1) | G4 |
| depth, artifacts | "soft at distance", "the far trees", "blobby" (8) | G5 |
| artifacts, colour | "the fall", "spray", "foam" (4) | G6 |
| colour, value | "washed out", "the range", "too dark", "the sky's colour" (any) | G7 |

The rule: take items in the order of the rows' losses (ties by the number of pairs naming
them); after each item, regenerate the twenty stills and run **both** panels: the development
panel decides (the order is re-ranked from it, and the part stops when it passes G-reference's
three rules on the eight pairs, or when every item the ranking named is built); the sealed panel
on the twelve is run once per item for the record only, never to choose or to stop. An item
named by no row is not built. If every named item is built and the development panel still
fails, the implementer keeps climbing (§0.9): from the development panel's sentences it names
the next item itself, within this plan's scope (the renderer, the materials, the growth; never
the references or the rubric), writes it as a step with tests and a gate like G1–G7, and builds
it, while the development panel's tally keeps improving. When several such items have stopped
moving it, the part closes with a **Blocked** note holding both tallies, everything tried and
the rows still lost, and the run goes on to Part H. The sealed set is thus tallied at most once per item here, as
at B5, D11, E8 and F10, and judged as a gate once, at §H2.

**Done when, G0:** (1) the development panel run and its five rows' losses summed from its
`tally.json` (paste the sums);
(2) the per-pair worst rows and sentences listed; (3) the chosen order of items with one
sentence each on which pairs picked it, in `docs/bench.md` "Public visual references" before G1's
commit;
(4) the items not named marked "not needed unless a later panel names them".

### G1. Bounce light with a surface cache

The probes' bounce (`probes.wrela`'s `bounces`) carries light without the colour of what it
bounced from: a ray that hits a trunk returns a grey. Lumen's surface cache holds each
surface's albedo and direct light where a ray can read it.

- `engine/distance.wrela`'s volume gains a second texture, `albedo: Texture3d<Rgba8>` (the
  voxel's nearest surface's colour, cooked by `cook_distance` from the solids' materials: the
  capsules' bark colour from the kind's `Bark`, the tapes' from their material, the ground's
  from the clipmap's cover) and a third, `lit: Texture3d<Rgba16Float>` (the voxel's direct sun
  on that surface, through the cascades, cooked a slab a frame as the volume is); the probes'
  trace, where a ray hits a solid, returns `albedo × lit` at the hit (the surface cache's
  radiance) instead of the grey; the cascades' march (plan two's §B14.4) is not revisited.
- **Cost:** two textures of the volume's size (the volume is 192×64×192: albedo 9.4 MB, lit
  18.9 MB; **estimated**, measured against C4's memory gate: if over, `lit` at half the voxel
  size); the cook's extra labels on the walk under 0.3 ms (**estimated**).
- **Tests** (`engine/tests/surface-cache`): a probe under a red wall lit by the sun reads a red
  bounce (its `bounces`' hue within 10° of the wall's); the open meadow's bounce unchanged
  within 1/255; the cache's `lit` at a shadowed voxel is 0.
- **Gate:** G-look on shots 1, 3 and 12 (deliberate: colour in the shade); G-memory and
  G-profile within the numbers; the panel (G-reference) after.

**Done when:** (1) the tests; (2) the memory and labels pasted; (3) G-look pasted; (4) the panel's
tally in the record.

### G2. Reflections

Glossy surfaces reflect the last frame's screen and a sky probe (`reflect.wrela`); the mere
reflects nothing that isn't on screen. D15's planar reflection was never built.

- The mere's planar reflection (D15 as plan two proposed it: drawn at half the scene's size
  while the mere is in view, behind `Q::REFLECTIONS` and a `const PLANAR`), and for other
  glossy surfaces a trace through the distance volume reading G1's cache where the screen
  misses (a reflection of a trunk behind the camera).
- **Tests:** a mirror plane under a lit sphere shows the sphere's underside (a readback where
  the screen can't see it); the planar pass's cost on shot 8 under 1.2 ms (**estimated**).
- **Gate:** G-look on shots 4 and 8; G-profile; the panel after.

**Done when:** (1) the tests; (2) the cost pasted; (3) G-look; (4) the panel's tally.

### G3. Leaves against the light

The leaves' translucency (`sss` in D1's terms) is one term per card; backlit leaves in the photographs
glow by thickness and by their veins. A thickness per card from the card's coverage and the
crown's depth (`cook_cards` writes how deep in the crown a card is), the sun's transmission
through the canopy from the probes' density volume (`density_at`) along the sun, and a vein
mask in the leaf atlas (`cook_leaves`), so the `sss` term is `sun × transmission × (1 −
thickness) × vein`.

- **Tests:** a single leaf card against the sun transmits at least 0.3 of the sun's colour; a
  card at the crown's core under 0.05; the vein mask covers 5–15 % of a leaf.
- **Gate:** G-look on shot 2; the panel after.

**Done when:** (1) the tests; (2) G-look; (3) the panel's tally.

### G4. Shadows at Epic

`PCSS = true` in the Epic tier (plan two's §B6c built it; its cost said off): measured sustained
on the vale's walk; if it costs over 0.8 ms (**estimated**), its blocker search at half the
scene's size; the near cascade's side at 4096 for the Epic tier if memory allows (C4: 32 MiB
more); the moving map's filter taps matched.

- **Tests:** a post's shadow is sharper at its foot than at its tip (the penumbra's width
  measured at two heights); the cascade's seam shows no line (a readback across the seam
  within 1/255).
- **Gate:** G-look on shots 1 and 6; G-sustained holds; G-memory holds; the panel after.

**Done when:** (1) the tests; (2) the cost and memory pasted; (3) G-look; (4) the panel's tally.

### G5. Virtual geometry from fields

What Nanite is for meshes, for fields: detail at the screen's pixel wherever the eye looks,
from kilobytes. Plan two's §B11a gave built things a far level from the filtered field; this
goes further for the near: the surface extracted at the pixel's footprint by interval pruning
(Keeter 2020: a tape evaluated over a box tells whether the box can hold the surface; recurse to
the pixel's footprint; render the leaves as the frame renders the near levels today). The scope
is the rocks, the crag and the built things; trees stay cards and impostors.

- **G5.0 A prototype rung first** (as plan two's B11a.0 did): one crag through the pruned
  extraction at four distances against the same crag through today's levels: the stills, the
  pixel difference, the cost, the bytes. The rung decides whether G5.1 proceeds; its numbers go
  in the record either way.
- **G5.1** The extraction as a wood-like system for the rocks and built things: a per-frame
  refine of a block tree round the eye on the GPU (a block's interval over its box; split where
  the surface may be and the block covers over 4 pixels; cache blocks between frames; evict by
  distance), the leaves drawn as today's near meshes are, with the far levels as plan two left
  them.
- **Tests:** a sphere field extracted at the pixel's footprint has its silhouette within 0.5 px
  of the analytic one at every distance; the block tree's refine is under 1.0 ms on the vale's
  walk (**estimated**); bytes held under 32 MiB (**estimated**).
- **Gate:** G-look on shots 5 and 8; G-sustained and G-memory hold; the panel after.

**Done when:** (1) G5.0's record; (2) the tests; (3) the cost and bytes pasted; (4) G-look;
(5) the panel's tally.

### G6. The fall, the spray and the foam

Plan two's §B12 built the falls' spray and foam with Ryan's looks pending (D25). If shot 4's
sentences name them: the spray's particles lit by the sun through the fog (they are: check), the
foam's coverage from the flow's turbulence (a cause: the flow's divergence), the wet rock's
darkening and roughness within the spray's reach (the `wetness` history channel at the rock),
the fall's sheet as a translucent surface with the rock behind it refracted.

- **Tests:** the wet band's width on the fall's flanking rock is within 20 % of the spray's
  reach; the foam's coverage is highest where the flow's divergence is; the sheet refracts (a
  readback behind it differs from the rock's colour by the refraction's offset).
- **Gate:** G-look on shot 4; the panel after.

**Done when:** (1) the tests; (2) G-look; (3) the panel's tally.

### G7. Colour and value

If the colour or value rows are lost with "washed out", "too dark" or "the range": inspect
`paint::Metering` and the physical style's tone curve on the **eight development shots**.
Record Wrela's luma quartiles and clipped share before and after; adjust metering and the
curve's shoulder to improve readable values, judged through the development reference review.
Different public scenes need not share a histogram: do not fit Wrela to their pixels, palette
or per-shot quartiles. The twelve's quartiles are measured and recorded only, never tuned.
Check the sky's colour against its cited sky model's expectation (Hosek-Wilkie or Bruneton),
with the source and conditions recorded.

- **Tests:** synthetic metering fixtures preserve luma ordering and exercise the curve's
  shoulder; the clipped share is under 1 % on every Wrela shot, with quartiles recorded.
- **Gate:** G-look on all twelve (deliberate); the reference review after.

**Done when:** (1) the tests; (2) G-look; (3) the review tally and Wrela's before/after measures.

---

**Done when, Part G:** G0's ranking in the record; every item the ranking named built with its
checklist, in order, each with both panels; the development panel passes G-reference's rules, or
the part's **Blocked** note holds both tallies, the items the implementer added and what
remains.
Hill to climb: pairs won or equal against the selected public references, (F10's) → 6 of 12 with no
pair lost worse
than 6–2.

## Part H: the floor adopts, and the final gates (`rywible/wrela`, branch `engine/wc3-h`)

### H1. The Last Green adopts (D40)

Each capability the vale proved, on the floor, behind a switch, judged on the floor's 23 shots:

- **The grown species.** The floor's seven species regrown from their genomes and its own
  history (`Stage::grown = true`): its archetypes at build, its first ring's individuals at
  load (E7's budget on the floor's cold start: C5's gate holds on the floor too); the great tree
  (`trees::great_tree`) stays authored (it is a character of the lore: a note in `design.md`),
  every other tree grown. G-look on the 23 shots (deliberate).
- **The flora and the floor's causes** (E3–E5) under the same switch.
- **The cel style** as the floor's second style: `games/last-green`'s `pub const STYLE` defaults
  to `Physical`; `wrela build games/last-green --set STYLE=Cel` builds the cel floor; its 23
  stills in the PR with G-look for a new style.
- **The creature growth** (F1–F8) for the fawn and the grazer, the climber's face and hands
  kept as authored handles with the stylized eyes (F8) they get.
- **The tiers**: the floor at `Epic`, `High`, `Medium`, `Low`, measured on the M4.

**Done when:** (1) the floor builds with and without `grown`, with each style, at each tier
(paste `wrela pipelines` counts); (2) G-look on the 23 shots for the grown floor and for the
cel floor pasted; (3) the capybara, the herd and the climber lab identical to `build/base-0.9`
(G-pixels; paste); (4) the floor's sustained, memory, cold and warm numbers at `Epic` on the M4
(paste) and its tier rows; (5) `games/last-green/design.md` gains the switches' rows with their
measured costs.

### H2. The final gates

1. `wrela suite` on 0.10.0 passes, every member (paste the count).
2. **G-reference** (D38): the final panel on the twelve shots and eight agent judges, with
   Wrela-owned controls, the shot and reference-manifest hashes equal to B5's (or B2's
   explicitly versioned replacement baseline), and the same review mode; the tally under
   `docs/bench.md` "Public visual references" as its last row (B5, D11, E8, F10, G's, H2).
3. **G-sustained** on the vale's walk and the floor's walk at `Epic` on the M4: GPU p50 < 12 ms,
   paced p99 < 20 ms, 0 of 1,200 over 33 ms, after 600 s; minute 1 and minute 10 in the record.
4. **G-memory:** both scenes under 512 MiB at their peaks on the walk.
5. **G-cold, G-warm:** both scenes under 5 s cold and 3 s warm (medians of three).
6. **G-tier:** the four tiers' M4 rows, each lower tier cheaper than the one above (other
   devices deferred, D37).
7. **G-grown:** the vale's authored plants 0; the growth's time and the three identical hashes
   (two native runs and one in Chrome on the M4).
8. **G-steer:** the identity fits ≥ 0.85 and the photo fits recorded.
9. **G-style:** `wrela tool styles --check` passes for the four styles; the coherence table and
   bars held.
10. **G-motion:** the motion panel's share ≥ 40 % or its **Blocked** note.
11. **G-size:** `wrela tool release` into `build/release`: the runtime ≤ 1 MB gzipped, the vale's
    and the floor's program and shaders ≤ 6 MB, cold load ≤ 8 MiB as served; no `.br` or `.gz`
    written (D26).
12. **G-pixels:** the capybara, the herd and the climber lab identical to `build/base-0.9`.
13. **G-bench:** `docs/bench.md`'s "Whether D27 holds": a table with a row per gate per scene per
    device, each cell "holds", "misses by …" or "blocked: …", and a sentence per row;
    "Public visual references" with every review and its source citations; optional public
    engine benchmark context in its own section, with original conditions and no inferred speedup.

**Done when:** the thirteen items are in the PR with their numbers; every step's **Done when**
in this plan is satisfied or carries a **Blocked** note listed in the PR with its measurements
and everything tried.

### H3. Records

- `docs/bench.md`: the sections above; the hypotheses H1–H8 (§7) each with its outcome and the
  numbers that decided it.
- `docs/citations.md`: a row per method in §9 whose code landed, any the implementer added,
  and every public visual or benchmark source, with its intended use and rights status.
- `references/vale.wrela`: frozen source locators, subject mappings, known conditions,
  rights/use decisions and required credits; no third-party media in the repo or release.
- `docs/lookdev.md`: "The panel", "Judging a style", the motion rubric, the coherence bars.
- `docs/toolchain-gaps.md`: every gap this plan hit, open or fixed in 0.10.0 or a 0.10.x.
- `examples/vale/design.md`, `games/last-green/design.md`, `content/styles/README.md`,
  `content/trees/genomes.wrela`'s header: every **proposed** number, with its measured cost
  where one was measured.
- `docs/devices.md`: the rigs' procedure and each device's row history.

**Done when, H3:** (1) `wrela suite`'s `allow` member passes with every doc this plan added
listed in `wrela.toml`; (2) a grep of the repository for `estimated` under `engine/`, `land/`,
`content/`, `studio/` and `examples/vale/` finds only values no gate measured, each named in
"For Ryan" (paste the grep); (3) every §9 method whose code landed has its row (paste the
rows' count against the methods' count); (4) each hypothesis's outcome row in `docs/bench.md`
(paste the eight).

### H4. The PR descriptions

Each part's PR follows plan two's shape (its #335): what changed, deviations from this plan with
reasons, the checks with their numbers, the simplify, council and review sections, and "For
Ryan": every **proposed** item built (D27–D42 as built), every hypothesis's outcome, every
Blocked note, and the decisions that need his call. Part H's PR adds the "Whether D27 holds"
table whole and the final panel's table.

---

**Done when, Part H:** H1–H4 hold; the eight PRs (A, B×2, C, D, E, F, G, H) are merged; the
claim's sentence in `docs/bench.md` says whether D27 holds, scene by scene, device by device.

## 7. The hypotheses

Each ships only if its own gate says it pays, and otherwise ships off with the numbers that said
so; each gets its outcome in `docs/bench.md` at §H3.

| ID | Hypothesis | Decided at |
|---|---|---|
| H1 | Specialization alone (C1) recovers at least 2 ms of the clearing's GPU frame and 3 ms of the floor's | C1, C6 |
| H2 | Most of the visual-review gain comes from the grown floor (E3–E5: the clutter), not from lighting | E8's panel against D11's, by rubric row |
| H3 | Mark coherence (D7) is the hardest problem in Part D and the bars will be held by `Cel` and `Ink` before `Painterly` | D7's table |
| H4 | Withdrawn (D41): the learned style is deferred until Ryan rules on `docs/citations.md`'s rule | D9 |
| H5 | Growth is byte-identical across devices on the CPU without fixed-point arithmetic | E7's hashes |
| H6 | Motion, not rendering, decides the creature shots (9, 10) | F10's panel against E8's |
| H7 | Bounce colour (G1) moves the "light" row more than any other single item | G1's panel |
| H8 | The vale needs no more than three of Part G's items to reach G-reference | G's tallies |

## 8. Worked examples

Sketches of the shapes the steps expect, for the implementing agent. None is compiled; names
follow the code they extend, and a sketch that disagrees with the code yields to the code.

**A1, C1: a program's switches, and the frame's use of them.**

```wrela
// engine/frame.wrela
pub trait Switches {
    const FOG: bool
    const DISTANCE: bool
    const SIGHT: bool
    // ...
}
pub struct Frame<R: Resolver, Q: Switches, S: Style> { ... }

impl<R: Resolver, Q: Switches, S: Style> Frame<R, Q, S> {
    fn draw_scene<Sc: Scene>(mut self, scene: mut Sc, sh: Shadows, wind: Wind) {
        // ...
        draw(pass, full.bind(z: 0.0), sky_fogged::<S>.bind(cam, self.light, fog: Q::FOG, ...))
    }
}

// examples/clearing/main.wrela
pub struct ClearingSwitches
impl Switches for ClearingSwitches {
    const FOG: bool = false        // it has no fog
    const DISTANCE: bool = false   // and no solids of its own
    const SIGHT: bool = true
}
```

**D2, D4: a style's shade and palette (the cel style, abridged).**

```wrela
// content/styles/cel.wrela
use engine::style::{Style, Terms, Substance, MarkSample, LINES, CREASES}

pub const BANDS: u32 = 3            // proposed
pub const TERMINATOR: f32 = 0.08    // proposed
pub const PALETTE_N: u32 = 4        // proposed

pub struct Cel
impl Style for Cel {
    const SUBSTANCE: u32 = FORM | CAUSE | TONE          // 32 bytes a sample (D1)
    const NORMAL: u32 = 1
    const MARKS: u32 = LINES | CREASES
    const FLAT_CLIP: f32 = 0.8

    fn shade(t: Terms, s: Substance, n: vec3) -> vec3 {
        let cosine = dot(n, t.sun_dir)
        let band = floor(smoothstep(-TERMINATOR, TERMINATOR, cosine) * f32(BANDS)) / f32(BANDS)
        let sun = t.sun_colour * band * t.sun_visible
        let sky = t.sky_mean                              // flat, not directional
        let bounce = t.bounce_mean
        let spec = if class_has_highlight(s.class) && cosine_half(n, t) > HIGHLIGHT { t.sun_colour
        } else { vec3(0.0) }
        (Self::palette(s.class, s.history, s.albedo) * (sun + sky + bounce) + spec) * t.fog.a +
        t.fog.rgb
    }
    fn palette(class: u32, history: vec4, albedo: vec3) -> vec3 {
        let tones = PALETTE[class]
        let k = u32(clamp(luma(albedo) * f32(PALETTE_N), 0.0, f32(PALETTE_N - 1)))
        pulled(tones[k], KEY[class], HUE_PULL)
    }
    // mark, lens ...
}
```

**D1: a material writing its substance.**

```wrela
// in bark's fragment shader
let s = Substance {
    class: BARK, part: inst.id << 8 | LIMB, depth: pixel.position.z,
    n_fine: oct(n), n_form: oct(bark_at_scale(p, footprint * 8.0).normal), n_coarse:
    oct(limb_axis_normal),
    curvature: curvatures(bark_at_scale, p, footprint * 8.0),
    history: inst.history, motion: m, coverage: 1.0, albedo: bark_colour,
}
let colour = S::shade(lit::terms(lt, p, n, bark_colour), s, n_for::<S>(n, s))   // in the pass
substance::write::<S>(mut out, colour, s)   // T0 always; T1–T3 only as S::SUBSTANCE says
```

**E0, E1: provenance and a genome.**

```wrela
pub struct Grown: Copy + GpuData { pub genome: u32, pub site: Site, pub seed: u32, pub version: u32
}
pub enum Provenance: Copy { Grown(Grown), Authored }

// content/trees/genomes.wrela
pub const OAK = Genome {
    apical: 0.25, branching: (38.0, 9.0), divergence: 137.5, internode: (0.55, 0.6),
    phototropism: 0.18, gravitropism: 0.12, shed: (0.12, 6.0), pipe: 2.0,
    bark: (0.004, 0.6), leaf: Leaf { kind: OAK_LEAF, sun: 0.09, shade: 0.14, density: 3.2 },
    wind: (0.25, 0.3), lifespan: 400.0, tallest: 28.0,
}
```

**E1.2: a year's bud fate (the shape of the loop).**

```wrela
for year in 0..age {
    let light = shadow_grid(mut grid, tree, neighbours_in(year))     // 1. light
    for b in buds_by_share(tree, light, g.apical) {                  // 2. bud fate
        if b.share > SHOOT_MIN {
            let dir = bend(b.dir, light.gradient_at(b.at), g.phototropism, UP, g.gravitropism)
            tree.shoot(b, dir, g.internode.0 * b.light.powf(g.internode.1), g.divergence, mut rng)
        }
    }
    tree.shed(light, g.shed)                                         // 3. shedding
    tree.girth(g.pipe)                                               // 4. the pipe model
    for e in events.in_year(year) { tree.event(e, mut rng) }         // 5. events
    tree.wind(site.wind, g.wind)                                     // 6. wind
}
tree.into_plant(g.leaf, light)                                       // 7. the crown as clumps
```

**E6: a goal file.**

```toml
species = "oak"
age = 180
light = "edge"          # open | edge | under
exposure = 0.6          # 0 sheltered .. 1 a ridge
lean = [0.3, 0.0]       # toward +x
[[events]]
year = 40
kind = "gale"
[[events]]
year = 95
kind = "lightning"
```

**B4: `pairs.json` (attributed mode) and a judge's answer.**

The fields below illustrate the schema, not selected sources; B3 supplies the real entries.
`reference_id` resolves to B2's manifest entry, including its source URL, locator and credit.

```json
{ "mode": "attributed", "reference_manifest_hash": "<frozen hash>",
  "pairs": [ { "k": 7, "wrela": "wrela/7.png", "reference_id": "ue5-forest-01",
               "display_mode": "local-copy", "credit_from_manifest": true } ] }
```
```json
{ "judge": 3,
  "scores": [ { "k": 7, "reference_viewed": true,
                "wrela": { "light": 2, "colour": 3, "value": 2, "depth": 2, "artifacts": 3 },
                "reference": { "light": 3, "colour": 3, "value": 2, "depth": 3, "artifacts": 3 },
                "prefers": "reference",
                "notes": { "light": "Wrela's canopy shade reads flatter; the scenes differ." } } ] }
```

**F5: the gait optimizer's objective.**

```wrela
fn cost(p: GaitParams, body: Body, terrain: TerrainClass) -> f32 {
    var w = stand(terrain, body, at, heading)
    var energy = 0.0
    var slide = 0.0
    for _ in 0..1200 {                                  // 20 s at 60 steps a second
        w = step(terrain, path, body, w, p, DT)
        energy += energy_of(w, body, 0)
        slide += planted_travel(w)
        if w.height < 0.6 * body.standing { return FALL }
    }
    energy / w.distance + SLIDE_W * slide + SPEED_W * abs(w.speed - p.target)
}
```

## 9. Sources to cite when their code lands

Each gets a row in `docs/citations.md` (method, where it's used, licence or publication) when its
step merges. Use them as references, not as code to copy. Plan one's and plan two's rows that
still apply are already in `docs/citations.md`.

| Method | Reference | Used in |
|---|---|---|
| Self-organizing tree models (bud fate by light and apical control, shedding, tropisms) | W. Palubicki, K. Horel, S. Longay, A. Runions, B. Lane, R. Měch, P. Prusinkiewicz, *Self-organizing tree models for image synthesis*, SIGGRAPH 2009 | §E1 |
| Space colonization for crowns | A. Runions, B. Lane, P. Prusinkiewicz, *Modeling Trees with a Space Colonization Algorithm*, Eurographics NPH 2007 | §E1 |
| L-systems, phyllotaxis, ferns and herbs | P. Prusinkiewicz and A. Lindenmayer, *The Algorithmic Beauty of Plants*, Springer 1990 (free from algorithmicbotany.org) | §E1, §E3 |
| The pipe model | K. Shinozaki, K. Yoda, K. Hozumi, T. Kira, *A quantitative analysis of plant form: the pipe model theory*, Japanese Journal of Ecology 1964 | §E1 |
| Bark from the trunk's expansion | S. Lefebvre and F. Neyret, *Synthesizing Bark*, Eurographics Workshop on Rendering 2002 | §E1.4 |
| Thigmomorphogenesis (wind shortens and thickens) | M. J. Jaffe, *Thigmomorphogenesis*, Planta 1973 | §E1 |
| Forest succession at scale (as `land::grow` already does; cited for the archetypes' classes) | M. Makowski, T. Hädrich, J. Scheffczyk, D. L. Michels, S. Pirk, W. Pałubicki, *Synthetic Silviculture: Multi-scale Modeling of Plant Ecosystems*, SIGGRAPH 2019 | §E1.3, §E2 |
| Reaction-diffusion patterns | A. M. Turing, *The Chemical Basis of Morphogenesis*, Phil. Trans. R. Soc. B 1952; J. E. Pearson, *Complex Patterns in a Simple System*, Science 1993; J. D. Murray, *Mathematical Biology II*, ch. 3 (the size-to-pattern result) | §E4, §F3 |
| Allometric growth | J. S. Huxley, *Problems of Relative Growth*, 1932 | §F2 |
| Muscle sizing by stress | R. McN. Alexander, *Principles of Animal Locomotion*, Princeton 2003 (constant muscle stress) | §F4 |
| Muscle-based locomotion optimized | T. Geijtenbeek, M. van de Panne, A. F. van der Stappen, *Flexible Muscle-Based Locomotion for Bipedal Creatures*, SIGGRAPH Asia 2013 (the objective's shape; ours is a spring walker) | §F5 |
| CMA-ES | N. Hansen, *The CMA Evolution Strategy: A Tutorial*, arXiv 1604.00772, 2016 | §D8, §E6, §F5 |
| Lines artists draw | D. DeCarlo, A. Finkelstein, S. Rusinkiewicz, A. Santella, *Suggestive Contours for Conveying Shape*, SIGGRAPH 2003; T. Judd, F. Durand, E. Adelson, *Apparent Ridges for Line Drawing*, SIGGRAPH 2007 | §D4 |
| Hatching that holds under zoom | E. Praun, H. Hoppe, M. Webb, A. Finkelstein, *Real-Time Hatching*, SIGGRAPH 2001 (tonal art maps) | §D6 |
| Temporal coherence of marks | P. Bénard, A. Bousseau, J. Thollot, *State-of-the-Art Report on Temporal Coherence for Stylized Animations*, Computer Graphics Forum 2011 | §D7 |
| Style transfer guided by light paths | J. Fišer, O. Jamriška, M. Lukáč, E. Shechtman, P. Asente, J. Lu, D. Sýkora, *StyLit: Illumination-Guided Example-Based Stylization of 3D Renderings*, SIGGRAPH 2016 | §D1, §D9 |
| Few-shot patch-based video stylization | O. Texler, D. Futschik, M. Kučera, O. Jamriška, Š. Sochorová, M. Chai, S. Tulyakov, D. Sýkora, *Interactive Video Stylization Using Few-Shot Patch-Based Training*, SIGGRAPH 2020 | §D9 |
| Painterly rendering with strokes | A. Hertzmann, *Painterly Rendering with Curved Brush Strokes of Multiple Sizes*, SIGGRAPH 1998 | §D5 |
| Interval pruning of field expressions | M. Keeter, *Massively Parallel Rendering of Complex Closed-Form Implicit Surfaces*, SIGGRAPH 2020 (already cited) | §G5 |
| Lumen's surface cache and software tracing, as context | Epic Games, *Lumen Technical Details* (UE5 documentation) | §G1, §G2 |
| The vale's reference photographs and footage | Each Wikimedia Commons file, its creator, licence, page and hash; selected 2026-10-28, recorded in `docs/citations.md` and `references/vale.wrela` | §B2–B5, §F9 |
| Public Unreal benchmarks, optional context | Each original publisher, exact URL/date, workload, device, settings and measurement method; selected at §B3.3, never substituted for local Wrela measurements | §B3.3 |
| Reference-media permissions | [YouTube terms](https://www.youtube.com/static?template=terms); [CC BY 4.0 legal code](https://creativecommons.org/licenses/by/4.0/legalcode.en); [Fab licence page](https://www.fab.com/eula); actual source-specific permission takes precedence over assumptions from its host | §B3 and `docs/citations.md` |
| Structural similarity, CIE76 | Z. Wang et al. 2004 (already cited); CIE 1976 L*a*b* | §D8 |
| The landforms and the forest history, already in the engine | `land/landform.wrela`'s and `land/grow.wrela`'s own citations (stream power, priority flood, the Borchert-Honda model) | §B1.2, §E2 |

## 10. The council's findings

**Historical review record:** the rounds below reviewed the earlier local-Unreal design.
Ryan's 2026-10-27 revision (§0) replaces that scope with public visual references. Mentions
here of `G-parity`, B3.0, local Unreal captures, Fab entitlements and matched shots describe
past decisions, not current prerequisites; the active instructions are D27/D29/D38, §B2–B5,
§F9 and §H2. The review safeguards for measurement, separate development shots and honest
claims still apply. This edit does not claim a new council review.

A council of two Codex reviewers (`gpt-6.1-sol`, correctness and verification; `gpt-6-astra`,
architecture and risk; both at high reasoning effort, read-only, through `codex exec` on this
checkout) reviewed the plan on 2026-10-27 with a Claude leader (the thread that wrote it; the
council is not single-vendor). The protocol is `~/.claude/skills/council`'s, run through the
CLI. The findings of each round, the decision on each and what changed.

### Round 1

Both reviewers returned six findings; three were raised by both. Every decision-changing claim
was verified against the source before the decision (`compiler/lower/src/gpu.rs:1135`,
`runtime/abi/src/stream.rs:702`, `engine/plant.wrela:130,167`, `land/grow.wrela:2524,1621,1665`,
`engine/gait.wrela:341,368,416`, `docs/citations.md:8`).

| ID | Finding | Decision | What changed |
|---|---|---|---|
| R1-A-F1, R1-B-F1 | A2 and D1 assumed eight `Rgba16Float` colour targets (64 bytes a sample); WebGPU's default `maxColorAttachmentBytesPerSample` is 32 and the compiler refuses more (E0602). | Accepted. | §D1 rewritten: the transport terms are not stored (a style's `shade` runs in the forward pass and takes them as values); a fixed 32-byte layout (T0 scene 8, T1 form 16, T2 cause 4, T3 tone 4), the part's id from the existing `ids` target; §A2 keeps the 32-byte check, names the bytes in its message and reports `attachment_bytes` in `wrela pipelines --json`; a tool test asserts every style's layout is at most 32. |
| R1-A-F2, R1-B-F5 | G-parity's rule (6 of 12 pairs, none worse than 6–2) passed an overall 42–54 loss; D8 fitted shot 6 and G7 tuned on all twelve, against the plan's own ban on fitting judged shots. | Accepted. | §0.5 G-parity has three rules (overall ≥ 50 % of 96 judgements, per pair, majority) and the counterexample is a tally test (§B4); §B1.4 adds six development shots `D1`–`D6`, kept apart, that every fit, tuning and diagnostic panel uses (§B2 and §B3.6 capture eighteen; §C6, §D7, §D8 and §G7 name the set they use). |
| R1-A-F3 | G-sustained's numbers (p50, p99, over-33) don't imply 60 presented frames a second: 19 ms intervals pass at 52.6 fps. | Accepted. | §0.1 and §0.5 G-sustained add throughput: 1,200 frames presented within 20.4 s and at most 12 intervals over 25 ms; the report's tests reject the 19 ms stream. |
| R1-A-F4 | E1.2 and E2 reconstructed a tree's lifetime light from the surviving neighbours' records, but the history retains gone trees out and keeps only the latest disturbance: real causes are lost. | Accepted. | §E2 moved before E1 and rewritten as a history and bake change: `Tree` records `light_by_decade` and up to four `events` as the forest grows; the bake carries them (bytes measured, a halving lever named); E1.2 reads only these; a test grows distinct forms from two histories with the same final forest. |
| R1-A-F5 | The spring walker is kinematic (`energy_of` is one secondary spring's energy; feet are planted by construction), so F5's "energy per metre" and "slide" proved nothing and F4 had no joint loads. | Accepted. | §F4 derives joint loads by inverse dynamics from the pose stream (Newton–Euler over the physique's masses, a known-load fixture test); §F5 is a kinematic optimizer with an effort proxy that excludes the secondary chains (a gaming test), a support-polygon stability measure, contact gaps and speed; its tests restated in those terms. |
| R1-A-F6, R1-B-F6 | D9's learned style trained a net on painted pixels to give the final colour, which `docs/citations.md`'s rule forbids as written. | Accepted. | §D9 is deferred, not built; D41 restated; H4 withdrawn; the question goes to Ryan verbatim in Part D's "For Ryan". |
| R1-B-F2 | The numbered order had later steps as prerequisites of earlier ones (C1's gate needs C3's bench; D5 needs D7's marks; E1's page needs E6's tool while E2 supplies E1's light), and §0.9 forbids reordering. | Accepted. | "Order within" lines for Parts C (C3 first), D (D7 before D4) and E (E2 before E1); E1.5 is the specimen scaffold E6 extends; §0.3's commit rule reads the order line; §0.7's diagram redrawn. |
| R1-B-F3 | B3 made an unverified Mac Unreal setup and Electric Dreams (DirectX 12, 32 GB) a prerequisite of all later work, and assumed Megascans are free on Fab. | Accepted. | §B3.0, a preflight, is the plan's first commit before Part A (the engine installed, one Epic capture, the features the Mac build renders, an asset entitlement table; paid assets are Ryan's call); Electric Dreams is conditional with a Mac-runnable fallback; D29 reworded; §0.7 says so. |
| R1-B-F4 | `plant::Plant` holds 48 limbs and `tube` drops overflow silently, so a grown veteran would lose branches; E7's cache key omitted the events and history E1 consumes. | Accepted. | §E1.2 defines the growth graph apart from the render representation, an explicit conversion whose silhouette IoU (≥ 0.97) is measured per specimen, `MAX_LIMBS` raised with its draw cost measured, and a debug assert in `tube`; §E7's key hashes every growth input including the history record, with a test that one changed event misses. |

### Round 2

Both reviewers read the revised plan. They marked R1-F3, R1-F6 and R1-B-F3 resolved, and
sustained the rest on the leader's own fixes, with four new findings each; the two sets were
the same four. Every claim was verified (`runtime/abi/src/stream.rs:558, 604`, the format enum,
`engine/plant.wrela:232`, `games/last-green/species.wrela:30, 206`, `land/grow.wrela:2356`,
`runtime/browser/src/testmode.ts`'s `inflight`).

| ID | Finding | Decision | What changed |
|---|---|---|---|
| R2-A-1, R2-B-1 | The 32-byte layout counted `Rgba8Unorm` as 4; the ABI costs an attachment by its render-target cost (8), so the layout was 40, and `Rgba32Uint` isn't a format the ABI has. | Accepted. | §D1: five targets, `Rgba16Float` + two `Rg32Uint` + two `R32Uint` = 32 attachment bytes, the small channels packed into integer words with pack/unpack helpers and round-trip tests; §A2 counts attachment cost as the ABI does and tests that four `Rgba8`s report 32 and that D1's exact layout compiles at 32 on both hosts. |
| R2-A-2, R2-B-2 | The order lines put D7 before D4–D6 while D7's gate needed all four styles, and E2 before E1 while E2's gate inspected E1's consumer. | Accepted. | §D7 split into D7a (the marks system, the dabs' and lines' move, the coherence instrument and the physical baseline the bars are relative to: before D4) and D7b (the four styles' table and bars held: after D6); E2's consumer line moved into E1's checklist; D1's early cost measurement uses a test-fixture style with the full mask; the order line says each gate names only what precedes it. |
| R2-A-3, R2-B-3 | Forty decades don't cover the floor's oaks (600 and 1,200 years), survivors outlive their lifespan, and the history starts with aged trees whose earlier decades were never simulated. | Accepted. | §E2: `light_by_age[32]` at fixed ages (5-year steps to 75, then sixteen samples to 1.5× the lifespan), `recorded_from` marking inferred prehistory, events as before; 72 bytes estimated; the payload lever halves the late samples and keeps the span; tests for 400-, 600- and 1,200-year histories, a survivor past its lifespan, an aged start and the early shading's survival of compression. |
| R2-A-4, R2-B-4 | F4's inverse dynamics named no contact allocation or balance, returned peaks where F5 needed histories, and excluding the secondary joints from the effort didn't make the head-stiffness test hold. | Accepted. | §F4 specifies the required wrench, a minimum-norm non-negative least-squares contact allocation (Lawson–Hanson), the residual and an infeasible-frame rule, per-frame Newton–Euler torque histories (peaks for muscles, integrals for effort), and three new fixtures (an accelerating body, an asymmetric stance, a hop); §F5's effort integrates `|τ·ω|` over feasible frames with the secondary chains frozen at rest in the proxy's pose stream, and tests equal-peak different-work histories and the infeasible rule. |
| R2 on F2 (both) | §G0 still chose the work and the stopping point from the sealed shots' panel. | Accepted. | §G0 ranks and stops by a development panel on the six development shots; the sealed panel runs once per item as a record; the sealed set is judged as a gate once, at §H2; §B4's `--shots` takes either list. |
| R2-B on F3 | `began_ms` records a frame's beginning, not its presentation; "presented" needed a definition. | Accepted. | §C3: the sustained run paces with one frame in flight (test mode's `inflight=1`) so a beginning follows the previous frame's hand-over; `throughput.json`; §0.5 says WebGPU exposes no presentation time and names C3's definition. |
| R2 on F9 (both) | The conversion's IoU compared a capsule-only graph with a leafy plant; `Plant::cushion` also drops clumps silently. | Accepted. | §E1.2 measures woody silhouette against woody silhouette (tubes alone, the tool's own rasterizer) plus the crown's volume within 5 %; clumps past the cap are merged with their volume kept; `cushion` asserts too. |
| R2-A on F6 | §0.6 still said D41 allowed one neural step. | Accepted. | The sentence fixed. |

### Round 3

The last round the budget allows, on the Round 2 rewrites only. Both reviewers marked the
layout, the order and the conversion's IoU resolved. Reviewer B sustained three points with new
evidence; Reviewer A (whose first run hung without output, 0.2 s of CPU in thirty minutes, and
was run again, reading the plan after B's fixes had landed) resolved F2 and F3 on those fixes
and raised two more. The leader verified each claim (`games/last-green/species.wrela:128`,
`runtime/browser/src/worker.ts:752-815`, `land/grow.wrela:1445`, the arithmetic of the
regularization's bias). The council was at its round limit, so A's two findings' fixes were
not re-reviewed; each has a test in the plan.

| ID | Finding | Decision | What changed |
|---|---|---|---|
| R3-B-1 | E2's fixed-age schedule isn't monotone for a short-lived habit: the blackthorn lives 50 years, so `span` is 75 and samples 16–31 would run from 80 down to 75. | Accepted. | §E2: `early = min(75, span / 2)`, samples 0–15 at `k × early / 15`, samples 16–31 at `early + (k + 1) × (span − early) / 16`; a test asserts strictly rising ages for every habit before and after the payload lever, and the blackthorn's interpolation and inferred boundary. |
| R3-B-2 | Lawson–Hanson's NNLS constrains every variable and doesn't choose the minimum-norm allocation among residual minimizers; horizontal forces must be free (braking). | Accepted. | §F4: a regularized bounded least squares (`‖A f − b‖² + ε ‖f‖²`, `f_z ≥ 0`, horizontals free, `ε = 1e-6`, BVLS's active set, the feet in a fixed order), the residual's scaling defined (weight, and weight × standing height); fixtures for a symmetric four-foot stance (a quarter each, permutation-invariant) and a braking body. |
| R3-B-3 | §G0's map routed losses through classes 2, 4 and 8, which the six development shots didn't cover. | Accepted. | The development set is eight shots, `D1`–`D8`, classes 1, 2, 3, 4, 6, 8, 9 and 12; twenty captures; the map names each development shot; a tool test asserts every row names one and that synthetic reflection, backlight and spray losses select G2, G3 and G6; every count updated (§B1.4, §B2, §B3.6, §B4, §C6, §D7, §D8, §G0, §G7). |
| R3-B on F3 | With `inflight=1` the worker waits on the GPU completion of the frame before the previous, and paces starts by a timer: a start cadence isn't a presentation cadence. | Accepted. | The claim and G-sustained say "begun", not "presented"; §C3 states the worker's exact mechanics and that the measure is a frame-start proxy bounding GPU throughput from below; `throughput.json` names `frame_starts`. |
| R3-A-1 | F4's regularization `ε ‖f‖²` with `f` in newtons makes the balance error grow with the body's mass: a symmetric static stance reads 5.7 % residual at 50 kg and 19 % at 100 kg, past the 5 % gate, although exact balance exists. | Accepted. | §F4: the forces are solved in units of the body's weight and the lever arms in units of the standing height, so the bias is `ε / 4` of the weight at any mass; the bias bound stated; the symmetric fixture runs at 10, 50 and 100 kg with a residual under 1e-5. |
| R3-A-2 | E2 left the unfinished tail of the series unspecified (a 100-year oak's next sample is at 126.6), so interpolation could read an unwritten slot, and the test "the last recorded sample agrees with `Tree.light`" was wrong, since `Tree.light` updates every step. | Accepted. | §E2: `recorded_to` bounds the recorded samples; between the last one and the present age `light_at` interpolates toward `Tree.light`; past the present or the death it clamps with a debug assert; the tests compare each sample with the canopy's light at its sampling step, `light_at(present)` with `Tree.light`, an oak stopped at 100 with a change at 80 before and after a bake round trip, and a tree that died between samples; 72 bytes. |

**Remaining uncertainty after Ryan's scope revision:** which public sources cover every
subject with rights and access suitable for review (§B3); the 32-byte layout's round trip on
both hosts (§A2); the bake's bytes with 32 light samples (§E2); the growth program's cost
against the load budget (§E1.2 test 7, §E7); and whether a kinematic effort proxy improves what
the motion review sees (§F9). Each has a check in the active plan. Unreal feature support,
sample compatibility and Fab entitlements are no longer prerequisites.
