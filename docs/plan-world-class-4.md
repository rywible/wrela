# The world-class engine, part four: what the judges look at

*What the engine does next, after part three (rywible/wrela#338 to #357 and Part H's PR, toolchain
0.18.0, 2026-11-01). Plan three built its parts and measured its claim, and the claim didn't hold:
the final panel scored the vale 0 of 96 against its photographs, every pair lost 8 to 0
(`docs/bench.md:2401`). Its Part G closed Blocked without building three items the panels named:
G6, the fall; G2, the water's reflections; G5.1, virtual geometry from fields. Its selection rule
is why: Part G chose from a development panel whose shots have no fall and no mere, so the faults
the sealed panel names second and third never reached its menu (`docs/bench.md:2534`).
This plan opens with those three, then builds what the judges' sentences fault most after them,
one artifact class a part: the solid things near the eye, the leaves as cards, the bark and the
forest floor as magnified tiles, the creature's motion and shading, and the shots' framing. Written
for an implementing agent who has not read the conversation that produced it. Started 2026-11-01
at Ryan's request. §10 is where the council's findings go.*

Ryan owns the goals. A line marked **proposed** is a default the plan chose that Ryan has not
confirmed; the agent builds the default behind a switch where one is cheap, and asks before a
default that can't be switched. A section marked **gate** is a check the branch must pass before
the next step's commit. A number the plan names as a target is an **estimate** until a gate
measures it; an idea the plan calls a **hypothesis** ships only if its own gate says it pays, and
otherwise ships switched off with the numbers that said so.

## 0. Read this first

### 0.1 Where plan three left the claim, and what this plan claims

D27 (plan three §0.1) **doesn't hold** on the one device it names (`docs/bench.md` "Whether D27
holds", `docs/bench.md:3653`):

- **Fidelity:** 0 of 96 judgements prefer the vale or rate a pair equal, at B5 and again at H2.
  Across the plan the light row's losses fell (12.50 → 9.88) and artifacts rose (26.75 → 28.75)
  (`docs/bench.md:2403`). Artifacts are the row every pair loses most.
- **Performance on the M4 Air:** every frame-time gate misses on the vale, and its memory and cold
  start; the floor misses every one (C's numbers, from before Parts D–H, never measured again on
  the M4: `docs/bench.md:3661`). The vale's biggest M4 cost is its vegetation drawing every card twice,
  18.8 ms serial (`docs/bench.md:3629`).
- **What the content costs:** holds. Every vale plant is grown (0 of 26,156 authored), the growth
  hashes the same across hosts, the creature is grown from its genome, four looks render with no
  engine change, and the runtime, program and shaders fit their sizes (`docs/bench.md:5193`,
  `docs/bench.md:5208`).
- **Motion:** G-motion 0 % against 40 % in four rounds (`docs/bench.md:4871`),
  and the walker is kinematic: its level walk's worst stride is 37 % infeasible
  (`docs/bench.md:4736`).

**The claim this plan builds toward, D43 (proposed):** D27's wording stands, and this plan
measures it again whole on the M4 at its end. Between here and there it adds a claim per artifact
class, so progress shows before the twelve pairs move: **for each class this plan builds (the
fall, the water's reflections, solid things near the eye, foliage near the eye, bark and the
floor, the creature), its class panel (§0.5 G-class) moves from losing to equal on the shots where
that class shows, while the M4's sustained GPU p50 for the vale ends no slower than Part A's
baseline of it (A4): each part spends at most its budget (§0.5 G-M4's table), and D3's single pass
for the cards is the saving that pays for them (hypothesis H9).** A class that can't reach equal ends Blocked with its trajectory, as plan three's
did; the claim says which classes did.

Fidelity is still judged against freely licensed photographs of the real subjects by agents
alone (D38, plan three); nothing here changes the references, the rubric or the panels' rules.

### 0.2 The repositories and their pull requests

| Repository | Checkout | What this plan changes there |
|---|---|---|
| `rywible/wrela` | `~/projects/wrela` | Every part. One PR per part, each from a branch named `engine/wc4-<part>` off `main`, each merged before the next part's branch is cut. Part I's PR holds the final gates and the records. |
| `rywible/wrela-compiler` | `~/projects/wrela-compiler` | Only if a step needs a toolchain capability (§0.6); a gap is recorded in `docs/toolchain-gaps.md` first, and a release is cut there, never a local fork. |

`main` is on toolchain 0.18.0 (rywible/wrela#351). No part moves the pin without a step that says so.

### 0.3 How to work

Plan three's §0.3 holds whole (its reading list, one commit per step, the rules of both
repositories, nothing lands that the gates don't cover, estimates labelled, don't invent design,
keep the docs true, use the tools, pixels a first screen, measure idle and warm) and so does its
list of what counts as a shortcut. Three rules change, each because plan three showed why:

1. **One artifact class a part, its rung first.** A part's first step measures its rung (§0.5
   G-rung) before anything is built: today's form and the candidate's against the class's own
   ground truth, at 10, 25, 50 and 100 m, in pixel error, bytes and ms. Plan three's G5.0 found its
   worked numbers wrong only when it rendered the rung (`docs/bench.md:1937`); every class does that first now.
2. **The M4 is in the loop for every performance gate.** A performance gate is measured on the M4
   Air in the part that changes it, not deferred to a final part. The agent can't reach the M4:
   each part's PR lists the commands Ryan runs there (§0.4's D44), the part's PR waits for his
   numbers in its record, and a part whose M4 numbers are missing doesn't merge. Linux numbers
   are kept for trends and named as such.
3. **No named fault is left off the menu** (§0.5 G-select): the sealed panel never chooses work,
   but a fault it names on a class the development set doesn't show opens a development shot of
   that class before the next part starts.

Read first: this plan whole; plan three's §0.3, §0.5 and §0.9; `docs/bench.md`'s "Public visual
references" (every panel and its sentences), "Whether D27 holds", "G5.0, rendered: the rung
decides", "F9, the motion review", "H1, the floor adopts"; `docs/lookdev.md`; `engine/virtual.wrela`
(the cluster path and the software rasterizer that plan one built and plan two took out of the
game, `docs/bench.md:684`); `docs/toolchain-gaps.md`.

### 0.4 Decisions reserved for Ryan

D27–D42 are plan three's and stand as built. The agent builds each default here; Ryan can flip it
in review. The ones only he can make are also in §11 as questions.

| ID | Decision | Default the plan builds (**proposed**) |
|---|---|---|
| D43 | The claim | §0.1: D27's wording, measured whole at Part I, and a claim per artifact class between. |
| D44 | The M4 in the loop | Each part's PR lists the M4 commands (`wrela tool bench -- --scene vale-walk --sustained 600`, `--starts 3`, the part's own); Ryan runs them at the machine and pastes the report path; the part merges after. |
| D45 | One artifact class a part | Parts B–H each build one class (§0.7), its rung first, its class panel after. |
| D46 | The selection rule | §0.5 G-select: the development set gains a shot for every class the sealed panel faults that it doesn't show (the fall first, then the mere), chosen from the vale's lookbook outside the twelve, with its own public reference under D29's rules. |
| D47 | Virtual geometry near the eye | Everything solid within `NEAR_SOLID` (25 m, **estimated**) is drawn through `engine::virtual` from its field: rocks, crag beds, trunks, logs, root plates, limbs. Past it, today's meshes and impostors. WebGPU has no mesh shaders and no 64-bit atomics (`docs/toolchain-gaps.md:114`), so the visibility buffer is 32-bit, two passes (§D4). |
| D48 | Foliage near the eye | Within `NEAR_LEAF` (15 m, **estimated**) a grown tree's leaves are geometry (each leaf, or each clump's leaves, from its growth), not cards; past it, cards; far, the archetypes' impostors as now. |
| D49 | Bark and the floor as fields | Bark relief from the trunk's growth (Lefebvre and Neyret 2002, already cited) as displacement near and parallax farther; litter and twigs as instanced geometry, not leaf cards on the soil. |
| D50 | A simulated creature | The creature walks a simulated body (rigid links, joints driven by PD controllers) whose controller is optimized with the codebase's CMA-ES (`studio/toolkit/cma.wrela`); the kinematic walker stays as the fallback behind a switch. |
| D51 | Reference footage for gait timings | **Ryan's call, not assumed:** whether gait timings may be measured from freely licensed footage of deer under `docs/citations.md`'s rules (learn, don't copy). Until he says yes, timings come from the cited literature (Hildebrand 1965; Alexander and Jayes 1983) as now. |
| D52 | Framing is content | Shot changes that move an authored camera, a cast's place or a lookbook entry (the vale's twelve are frozen: their hash is the panel's) are Ryan's. The plan builds the checks that find them (§H) and lists each fix for him; it changes no frozen shot. |

### 0.5 The gate kinds

Plan three's gate kinds are reused by name and not restated (G-sim, G-pixels, G-look's four
parts, G-reference, G-sustained, G-tier, G-grown, G-style, G-motion, G-size and the rest: plan
three §0.5). New kinds:

- **G-rung** (an artifact class's ladder, before it's built): for the class's representative
  pieces (named in its part), today's form and each candidate rung measured against the class's
  ground truth at 1080p and 0.82 rad, at the class's distances (10, 25, 50 and 100 m unless its
  part names others: bark at 1, 3 and 10 m, where an old oak's shot puts the eye), in the class's
  own error: **surface error in pixels** for solids, leaves and bark (against the field or the
  growth); **image error** (PSNR and the share of pixels over 8/255 against a ground-truth render
  the part's step 0 makes) for the fall and the reflections, whose fault is light, not surface;
  for motion, the gait's own measures (§G0). Each rung also has its bytes **resident** (every level
  kept, not only those drawn), and its GPU ms to draw and to realize. The table goes in
  `docs/bench.md` before the class's first build step. A candidate is built only where its rung
  shows today's error over the class's threshold (a pixel; 30 dB PSNR for image error,
  **proposed**) at a distance the shots use.
- **G-class** (a small panel on the class): the shots where the class shows (development shots,
  and the class's own development shot under G-select), each against its public reference, eight
  agent judges in fresh contexts with calibration controls, B5's brief, the class's rubric row
  named in the brief. Its tally is read for that class's sentences. The gate, per pair: the class's
  named faults (the part lists its words) in no judge's notes, and **at least four of the eight
  judges prefer the still or rate the pair equal** (the pair's median judgement equal or better).
  This is stronger than G-reference's per-pair safeguard (no pair lost worse than 6 to 2,
  `studio/compare/tally.wrela`), and is its own rule with its own tally test: two judges not
  against fails, four passes. A G-class panel never uses the sealed twelve.
- **G-select** (the selection rule's test): a synthetic tally whose only fault is on a class no
  development shot shows must open that class's development shot and put its item on the menu; a
  tool test in `studio/compare` holds it, and the plan's menu (§A1) is regenerated from both panels'
  sentences before each part.
- **G-M4** (D44): a performance gate's numbers from the M4, pasted from Ryan's run with its report
  path; a Linux number in its place is a skipped step (plan three's shortcut 3). **The rule, every
  part:** the part's switch defaults **on** only if the vale's sustained (600 s) GPU p50 with it on
  is within the A4 baseline plus the sum of the budgets of the parts merged so far (D3's measured
  saving subtracted), and its paced p99 no worse than the baseline's plus the same; else it ships
  **off** with its numbers, as a hypothesis does, and the part's class claim says so. The budgets
  (GPU ms, **proposed**):

  | Part | Budget | Why |
  |---|---|---|
  | B, the fall | 0.5 | a sheet, its foam and spray, on screen in a few shots |
  | C, reflections | 1.0 | a half-size planar view where still water shows |
  | D, near solids | 0.0 | the visibility buffer must pay for itself (D3, H9: a third of the 18.8 ms saved) |
  | E, foliage | 1.5 | instanced blades within `NEAR_LEAF` |
  | F, bark and floor | 1.0 | relief evaluated per pixel near |
  | G, the creature | 1.0 GPU (fur) and 0.3 CPU (the body) | in its two shots |

  At Part I the claim (D43) reads the vale's p50 against the baseline: no slower.
- **G-motion-sim** (§G): the simulated gaits' tests: a walk, a trot and a bound on level ground,
  15° and 30° slopes, a 0.3 m step, uneven ground, and a shove recovered. Feasibility is the
  simulated body's, not the kinematic walker's (whose measure counts every frame without a
  planted foot infeasible, `engine/loads.wrela`, and so would forbid a bound's flight): a stance
  frame is infeasible where the ground reaction it needs lies outside its contacts' friction cones
  (μ 0.8, **proposed**) or its support; a flight frame is valid where the body's centre of mass
  falls ballistically (its acceleration g within 2 %). Each gait's worst stride under 5 %
  infeasible, no fall, no penetration over 1 cm. The tests' own fixtures: a ballistic hop passes;
  a body hovering, and one whose stance needs a pull from the ground, fail.

### 0.6 What this plan leaves out, and why

- **The references, the rubric and the panels' rules.** They are D29 and D38 as built.
- **A neural step.** D41 stands.
- **New toolchain capabilities unless a step proves the need.** WebGPU's limits (no mesh shaders, no
  64-bit atomics, no subgroup operations guaranteed) are designed round, not waited on.
- **Other devices.** D37 stands: the M4 alone.
- **The floor's grown wood** past what §E and §F give it: plan three's H1 left it Blocked
  (before on 18 of 23 shots: `docs/bench.md:4970`). Its one lever tried, the forest kinds
  grown in the open, moved it to 13 after and 9 before (`docs/bench.md:5045`)
  but changes the archetypes the vale draws, so it waits on §11's question 4.

### 0.7 The order, and what needs what

```
Part A: the selection rule, the rung tool, the class panel, the M4 loop
  ├── Part B: the fall (G6)                 ◄── A
  ├── Part C: the water's reflections (G2)  ◄── A
  ├── Part D: solid things near the eye (G5.1, re-scoped)  ◄── A
  │     └── Part E: foliage near the eye    ◄── D (the visibility buffer)
  │     └── Part F: bark and the forest floor as fields  ◄── D
  ├── Part G: the creature: a simulated gait, secondary motion, its shading  ◄── A
  ├── Part H: the shots' framing            ◄── A (its checks), Ryan (its fixes)
  └── Part I: the final gates and the records  ◄── B–H
```

B, C and G don't depend on each other and may be built in any order after A; D before E and F,
which share its visibility buffer. Within each part, its rung (step 0) first.

### 0.8 Where things live

- `studio/compare/menu.wrela`: the menu and its map (A1).
- `studio/rung` (new, A2): `wrela tool rung -- --class <class> --piece <name>`: the G-rung table.
- `studio/compare` (A3): `--class <class>` runs a G-class panel.
- `engine/water.wrela`, `engine/falls.wrela` (new, B): the fall.
- `engine/reflect.wrela` (C): the reflections.
- `engine/virtual.wrela`, `engine/visibility.wrela` (new, D): the near solids.
- `engine/vegetation.wrela`, `engine/plant.wrela`, `content/trees` (E, F): leaves and bark.
- `engine/body.wrela` (new, G), `content/kit` (G): the simulated creature.
- `studio/framing` (new, H): the framing checks.
- `docs/bench.md`, `docs/lookdev.md`, `docs/citations.md`, `docs/toolchain-gaps.md`: as plan three.

### 0.9 The contract: no step is skipped

Plan three's §0.9 holds word for word (a step is done only when every line of its "Done when"
holds, each by a number; a target has a now and a target; "too hard" is not an outcome; a
hypothesis's experiment is mandatory; reordering is the implementer's call with its reason,
dropping never; a step that changes the image satisfies G-look; every estimate replaced by a
measurement; the judge is never the implementer). Three lines are added because plan three broke
them:

- **A Blocked note needs its levers tried, not named.** Plan three's H1 closed the floor's grown
  wood with its levers named and untried, and its council said so. A Blocked line lists each lever
  *tried* with its trajectory; a lever named but untried is a step still open.
- **A part merges with its G-look measures (2) and (3) run**, even where the judge's preference
  decides: plan three's H1 skipped them for the grown floor first and ran them after its council.
- **Nothing is called complete that its gates didn't pass.** A plan's last PR says which claims
  hold, which don't, and which steps it closed Blocked; "complete" means every step is merged with
  its gates passed or its Blocked lines tried.

### 0.10 What needs Ryan at the M4

Each part's PR lists these as its "On the M4" section (D44); the part merges after his numbers are
in its record.

| Part | At the M4 (Chrome, 1080p, Epic, nothing else running) | About |
|---|---|---|
| A | the baseline: `wrela tool bench -- --scene vale-walk --scene last-green-walk --sustained 600`, then `--starts 3`; the four tiers at 120 s | 60 min |
| B | the vale's walk with the fall in view, 600 s | 15 min |
| C | the walk with `PLANAR` on and off, 600 s each | 25 min |
| D | the walk with `VISIBILITY` on and off, 600 s each (H9); the solids' rung's draw ms (`wrela tool rung -- --class solid --m4`) | 35 min |
| E | the walk, 600 s; the leaves' rung's ms | 20 min |
| F | the walk, 600 s; the bark's rung's ms | 20 min |
| G | the vale's walk and the creature shots' clips (`wrela tool lookbook -- … --clips 7 8`), the body's CPU ms | 20 min |
| I | everything in A again, and `wrela tool grow -- --hash --package examples/vale` in Chrome (G-grown's third device) | 75 min |

## Part A: the selection rule, the rung tool, the class panel, the M4 loop (`engine/wc4-a`)

### A1. The selection rule (D46)

Plan three's §G0 read the development panel alone (its R2 F2: a sealed set that chooses the work
isn't sealed). Its eight development shots (D1–D8) show no fall and no still water, so the faults
the sealed panel names on those classes never reached the menu: at H2 the menu's ranking from the
sealed tally names G2 6.88 and G6 4.62 lost, second and third (`docs/bench.md:2524`,
`docs/bench.md:2525`), and Part G built neither.

- The development set gains **D9, the fall**, **D10, the mere** and **D11, the stream** (D4's shot
  shows none of its stream: `docs/bench.md:2349`): lookbook shots of
  the vale outside the twelve (never any sealed shot's camera), each with a public reference chosen under
  D29's rules (Wikimedia Commons first), credited in `references/vale.wrela` and
  `docs/citations.md`, their hashes added to a new development manifest (the sealed manifest's hash
  is unchanged: `b78df49e…`).
- `studio/compare/menu.wrela`'s map gains a class column; a class the sealed tally faults that no
  development shot shows is reported by `--tally` as "unshown" and fails the menu's check.
- **Coverage:** a class's development shot must show its class: the class's pixels (from the
  frame's ids: the water's, the solids' kinds, the creature's) at least 5 % of the frame
  (**proposed**) and unoccluded at its centre. `studio/compare` refuses to register a shot to a
  class it doesn't cover, and every existing mapping is checked (D4 against the stream fails today).
- **Tests:** G-select's synthetic tally; the coverage check (a shot without its subject fails);
  the twelve's shots and manifest hashes unchanged; the development manifest's hash recorded.

**Done when:** (1) D9, D10 and D11 render (stills pasted as paths), cover their classes, and have
references with credits;
(2) the G-select test passes; (3) the development panel run on D1–D11 (eight judges), its tally in
`docs/bench.md`, and the menu regenerated from it with each part's class named.

### A2. The rung tool

`studio/rung` (new): `wrela tool rung -- --class solid --piece crag-bed` (and `log`, `trunk`,
`root-plate`, `fall`, `leaf-clump`, `bark`, `litter`) renders a piece at today's form and at each
candidate rung at 10, 25, 50 and 100 m and writes the G-rung table. It generalizes plan three's
`engine/tests/crag-rung`, whose numbers are its first fixture: today's 45 cm cells stray **35.90
px at 10 m** and 3.59 at 100 m, and the 8 mm footprint is **102,230 vertices and 7,359,720 bytes**
for a 1.2 × 0.8 × 1.0 m bed (`docs/bench.md:1950`,
`docs/bench.md:1951`).

**Done when:** (1) the tool reproduces the crag rung's table within 2 % on Linux; (2) its tool test
(a sphere's field at a known cell) passes; (3) `docs/toolchain.md` names it.

### A3. The class panel

`wrela tool studio/compare -- <out> --class <class> --set development` builds a G-class panel from
the development shots the menu maps to that class. The brief names the class's row and asks for
its faults by name. Its tally reports the class's sentences (by keyword map, as the menu does).

**Done when:** (1) the panel's tool test (a synthetic tally whose class faults vanish passes it, one
whose faults remain fails it); (2) a baseline G-class panel run for each class this plan builds,
before its part starts, recorded in `docs/bench.md`.

### A4. The M4 loop (D44)

A part's PR carries a section "**On the M4**": the exact commands, in order, that Ryan runs at the
machine; the report each writes; what the part's gates read from it. `wrela tool bench`'s report is
the hand-off (`--row` already merges another machine's rows). Part A's own: the vale's and the
floor's walks at Epic, 600 s sustained, and `--starts 3`, **as plan three leaves them**: the
baseline every later part's G-M4 is measured against (the M4's last numbers are C's, from before
Parts D–H).

**Done when:** (1) Ryan's baseline run pasted (the report path and the four scenes' rows); (2) the
"On the M4" section's template in `docs/bench.md`.

## Part B: the fall (G6) (`engine/wc4-b`)

**What the judges say.** On every panel the fall loses its pair 8 to 0, and at H2 its eight judges
call it "a flat cyan strip", "a flat sliver", "a thin teal ribbon", "a flat cyan sheet", "a thin
flat card" (`docs/bench.md:2495`, `docs/bench.md:2496`, `docs/bench.md:2497`, `docs/bench.md:2498`, `docs/bench.md:2501`). Its
walls are "untextured plaster" (`docs/bench.md:2497`).

### B0. The fall's rung

The piece: the vale's fall (its lip, its sheet, its plunge pool). Ground truth: a reference
simulation of the sheet (a 2D height-field sheet over the lip's profile, a particle spray off its
foot), run offline in the tool. Rungs: today's card; a ribbon mesh following the simulated sheet;
the ribbon with foam and spray. Measured as G-rung.

### B1. A flowing sheet

The sheet is a strip of quads following the lip's profile and a ballistic fall (each column's
water leaves the lip at the stream's speed and falls under gravity), its thickness thinning with
the fall, its surface scrolled along its length (flow-mapped, Vlachos 2010, already cited), its
edges breaking into streaks where it thins.

### B2. Foam and spray

White water at the lip and the foot (from the sheet's speed and curvature), and a spray of
particles at the plunge (Ihmsen et al. 2012: spray, foam and bubbles classified from a fluid's
particles, used here on the sheet's particles), drawn through `engine::particles`' veils, lit by the
probes, misting the froxels (the fog's existing falls' mist).

### B3. Wet rock

The fall's walls and the rocks it strikes wetted by distance from the sheet and its spray (darker,
glossier: `engine::rock`'s roughness lowered and its albedo darkened by a wetness channel, the
substance's cause target already carries wetness).

**Done when, Part B:** (1) G-rung table pasted; (2) the G-class panel on D9 (the fall) and the
shots where water falls: the faults "flat", "strip", "card", "ribbon" gone from the judges' notes
and the pair at equal or better; (3) G-look on the vale's twelve (the fall's shot 10, and every
shot whose pixels move); (4) G-M4 by §0.5's rule (its budget 0.5 ms): the switch's default from
it.

## Part C: the water's reflections (G2) (`engine/wc4-c`)

**What the judges say.** The menu's map names G2 from the sealed tally's dusk-from-the-crest and
fall pairs (`docs/bench.md:2524`); the development panel faults the stream's sheet
as "a translucent plane" (13 notes in 5 pairs: `docs/bench.md:2350`), though D4 frames
no stream (A1's D11 does).

### C0. The water's rung

Pieces: the mere at the crest's distance, the stream at 10 m. Ground truth: the step's first
deliverable, a reflection traced to convergence in the tool (offline, many rays a pixel, against
the scene's distance volume and the clipmap's ground; no such reference exists today). Rungs:
today's screen-space march (`engine::reflect`, McGuire and Mara, cited); planar reflection for the
mere (a second view of the scene below the plane, D15 says it was never built); SSR with the
probes' fallback.

### C1. Planar reflection for still water

The mere and the pools: a reflected camera, the scene drawn into a half-size target with the
water's plane as its clip, read by the water's shading with its normal's distortion. Built behind
`Switches::PLANAR`, its cost measured on the M4.

### C2. The stream's surface

The stream's sheet reads as "a translucent plane": its depth's colour (absorption by depth), its
refraction of the bed, its normals from the flow, and the reflections C1 or SSR give it.

**Done when, Part C:** (1) G-rung pasted; (2) the G-class panel on D10 (the mere) and D11 (the
stream): "translucent plane", "flat" and the reflections' faults gone, the pairs at equal or
better; (3) G-look on the twelve; (4) G-M4 by §0.5's rule (its budget 1.0 ms): `PLANAR`'s default
from it.

## Part D: solid things near the eye (G5.1, re-scoped) (`engine/wc4-d`)

**What the judges say.** The logs are the most-faulted thing in the vale: 44 notes in 6 of 8
development pairs, 19 of them logs through each other or floating (`docs/bench.md:2347`), and at H2 "an implausible radial bundle of capsule-shaped roots", "stacked
intersecting logs", "plain cylinder logs" (`docs/bench.md:2413`, `docs/bench.md:2491`, `docs/bench.md:2492`); trunks and logs faceted (10 notes);
a limb "kinked at a right angle" (`docs/bench.md:2516`); "the camera clipping into
the trunk" (§H).

### D0. The solids' rung

Pieces: a crag bed (plan three's fixture), a log, a trunk's foot with its root plate, a limb's
fork. Ground truth: each piece's field (its SDF, as `realize` extracts it). Rungs: today's
(the beds' 45 cm cells; the trunks' and logs' tubes of 14 sides); `engine::virtual`'s cut at 1
pixel. The table, G-rung's columns, plus each rung's draw ms on the M4 (D44).

### D1. The near solids into the cluster path

Everything solid within `NEAR_SOLID` is realized as `engine::virtual`'s clusters from its field:
crag beds and rocks (`stone`), trunks, logs and root plates (`plant`'s tubes as fields: a tube's
SDF, its bark's relief from §F added as displacement where §F lands), limbs at their forks (one
field per fork: no kink where two tubes meet). The cut picks each cluster's level by its error on
the screen (`Cut::pixels`, 1 **proposed**); far, today's meshes and impostors.

**Residency.** The cut decides what's drawn, not what's kept: `engine::virtual` cooks every level
of a surface and keeps them (`Clusters::bytes` counts each level's geometry and its scratch), so
D1's rung counts resident bytes separately from drawn triangles. The near set's levels are cooked
on demand as a piece enters `NEAR_SOLID` (finest level last, a slice a frame under the planner's
`Work::Realize`), and evicted past it with hysteresis (`NEAR_SOLID` + 5 m, **proposed**); the
step's gate is the walk's GPU peak, every level and transient counted, within G-memory on the M4.

### D2. The visibility buffer, 32-bit, two passes

WebGPU has neither mesh shaders nor 64-bit atomics (`docs/toolchain-gaps.md:114`).
The visibility buffer (Burns and Hunt 2013) is a `R32Uint` target. `engine::virtual` already packs
its id in 32 bits (`id_of`: a level's 4 bits, a cluster set's 2, a software flag, then the
triangle's index in its level's list; `MOST_SETS` 4), but a set is one surface with its own
buffers, and its resolve runs a fullscreen pass a level. Two trunks of one kind would collide.
**D2's prerequisite, its own commit:** one global table of every near piece's picked clusters
(each frame's picks gathered into one list, a triangle's index into it the id's low bits, a
piece's transform and material by its cluster's entry), so an id names one triangle of one piece,
and one resolve pass reads it, within seven storage buffers a kernel
(`docs/toolchain-gaps.md:135`). Its test: two meshes of one kind sharing
local triangle indices resolve to their own pieces. The buffer is drawn by the hardware rasterizer
with its depth test for clusters over `Cut::small` pixels, and by the software rasterizer's two passes (an `atomicMax` of the depth's
bits, then an `atomicMin` of the id at the winning depth: `docs/toolchain-gaps.md:119`)
for the small ones, merged through the depth test (`soft_merge`, already built). Shading reads the
id once a pixel: one shading pass for every near solid.

### D3. The vegetation drawn once (hypothesis H9)

The vale's vegetation draws every card twice (a prepass and a shading pass), 18.8 ms serial on
the M4 at C6 (`docs/bench.md:3629`). With the visibility buffer the
cards write their ids in one pass and shade once, as the solids do. **Hypothesis H9:** the vale's
vegetation passes drop by at least a third on the M4. Built behind `Switches::VISIBILITY`; the
experiment runs whatever its result.

### D4. The fallen wood's form and contact

The logs laid in the history (plan three's G8 levers, untried: `docs/bench.md:2351`): the bake lays each fallen trunk against the ground and
the trunks standing at the year it falls, so every reader (the drawing, the floor's collision, the
probes' occluders) sees one laid log; the root plate a torn disc of soil and roots from its tree's
root field, not capsules in a star.

**Done when, Part D:** (1) G-rung pasted with the M4's ms and the resident bytes; (2) the global
table's test; (3) the G-class panel on the development
shots where logs, trunks and rock show (D1, D3, D6): the
faults "floating", "through", "cylinder", "capsule", "faceted", "kinked" gone from the notes and
the pairs at equal or better; (4) G-look on the twelve and the floor's 23; (5) G-M4 by §0.5's
rule (its budget 0: the visibility buffer pays for itself), `VISIBILITY` on and off, H9's outcome;
(6) G-memory: the walk's GPU peak, every level resident and every transient counted, within the
vale's 512 MiB on the M4.

## Part E: foliage near the eye (`engine/wc4-e`)

**What the judges say.** After the logs, the leaves as cards: "confetti-like leaf cards", "leaf
cards hovering over bare ground", "sprite-like foliage clumps", "cutout foliage", "blotchy leaf
clusters" (`docs/bench.md:2423`, `docs/bench.md:2424`,
`docs/bench.md:2422`, `docs/bench.md:1355`); 14 notes in 5 development pairs
(`docs/bench.md:2350`).

### E0. The leaves' rung

Pieces: a beech's crown clump at 5, 10, 25 m; a sapling. Ground truth: the growth's own leaves
(each leaf's blade from the species' leaf shape, at its node). Rungs: today's cards; leaves as
instanced blades within `NEAR_LEAF`; clumps as small meshes from the growth.

### E1. Leaves as geometry near the eye

Within `NEAR_LEAF` a grown tree's leaves are instanced blades (the species' blade, bent by its
vein's curl, two-sided with transmission by thickness: plan three's G3), placed at the growth's
nodes, written into the visibility buffer (D2). Past it, the cards as now; far, the archetypes'
impostors.

### E2. The litter as geometry

The forest floor's "leaf cards on the soil" become the same blades lying on the ground (curled,
weathered by the cause target's age), in the count the ground's litter field gives.

**Done when, Part E:** (1) G-rung pasted; (2) the G-class panel on D1 (the forest floor), D2 (the
canopy against the sun) and D3: "confetti", "cards", "cutout", "sprite" gone, the pairs at equal or
better; (3) G-look on the twelve and the floor's 23; (4) G-M4 by §0.5's rule (its budget 1.5 ms):
the blades' default from it.

## Part F: bark and the forest floor as fields (`engine/wc4-f`)

**What the judges say.** Near bark is magnified and tiled: 26 notes of magnified or stretched
textures and 25 of bark tiling in the development pairs (`docs/bench.md:2348`, `docs/bench.md:2349`); at H2 an old oak's bark is "ghosted double layers of bark relief"
(`docs/bench.md:2509`), "extruded slabs" (`docs/bench.md:2506`) and "horizontal tiling seams" (`docs/bench.md:2507`).

### F0. Bark's rung

Pieces: an old oak's trunk at 1, 3, 10 m (an old oak's shot puts the eye at about a metre).
Ground truth: the bark's relief grown from the trunk's expansion (Lefebvre and Neyret 2002, cited:
a bark's fractures from the girth's growth, layer by layer). Rungs: today's magnified tile
(plan three's G9); the grown relief as a field evaluated per pixel (parallax occlusion,
Tatarchuk 2006); as displacement on D1's near clusters.

### F1. The growth's records (a prerequisite)

The growth keeps one radius a node and compacts its graph, removing dead nodes
(`engine/growth.wrela`: `GrowthNode`, its compaction, `die`), so neither a trunk's girth by year
nor the limbs it shed survive to be read. This step records them in the growth's output: each
trunk node's radius at a few ages (8 samples, **proposed**) and each shed branch's tube before its
removal, both in the grown archetypes' bytes and E7's cache key (its `VERSION` raised), their
bytes measured against the memory gate. Its tests: a trunk's earlier radii recovered and
monotone; a branch shed before the compaction recovered with its tube; the hashes the same on
two runs.

### F2. Bark grown from the trunk's expansion

Each trunk's bark relief grown from its own girth's history (F1's radii by age):
fractures open where the circumference grew past the bark's strain, plates between them, a
species' parameters (an oak's deep furrows, a beech's smooth skin, a birch's peeling bands) from
its genome. Evaluated as a field: on D1's clusters as displacement within `NEAR_SOLID`, as
parallax past it, as today's tile far.

### F3. The floor's litter and twigs as geometry

Twigs and small branches shed by the growth (F1's shed record) lie as instanced tubes on the
floor; E2's leaves lie with them; the ground's smeared texture (`docs/bench.md:1135`) is the
ground's own field, not a magnified tile.

**Done when, Part F:** (1) G-rung pasted; (2) the G-class panel on D1 and D3: "tiling",
"stretched", "smeared", "ghosted", "slab" gone, the pairs at equal or better; (3) G-look on the
twelve and the floor's 23; (4) F1's tests and its bytes; (5) G-M4 by §0.5's rule
(its budget 1.0 ms).

## Part G: the creature: a simulated gait, its secondary motion, its shading (`engine/wc4-g`)

**What the judges say.** G-motion 0 % against 40 % in four rounds, every row lost
(`docs/bench.md:4871`); the walker is kinematic and its level walk's worst
stride is 37 % infeasible (`docs/bench.md:4736`); the grazer reads "a smooth low-detail
model" (`docs/bench.md:2478`), the floor's grown fawn "a flat
saturated orange cutout" in the hand's (`docs/bench.md:5063` is
the grown roe's win over it).

### G0. The motion's rung

Pieces: the grazer's walk, trot and bound on level ground and a 15° slope. Ground truth: the
literature's gait timings (Hildebrand 1965, Alexander and Jayes 1983, cited) until D51 says
whether licensed footage may be measured. Rungs: today's kinematic walker; the simulated body with
PD control.

### G1. A simulated body with PD controllers (D50)

`engine/body.wrela` (new): the grazer's skeleton as rigid links (masses from `kit::genome`'s
segments), joints driven by PD controllers toward a gait's target angles, contacts with the
ground's height field, integrated at 240 Hz on the CPU (deterministic: G-sim's hash, as plan three's
E7 held the growth's). The controllers' parameters (each gait's targets, gains and timings)
optimized with CMA-ES (`studio/toolkit/cma.wrela`, Hansen 2016, cited) against the walker's own
measures (effort, stability, the infeasible share) plus a fall's penalty, as plan three's F5 did
for the kinematic walker (Coros et al. 2011 for quadrupeds' PD control; Yin et al. 2007,
SIMBICON).

### G2. Secondary motion

Ears, tail and the breath's rise as springs on the simulated body (`engine::gait`'s `Spring`,
already built), driven by its accelerations; the ears also by F8's attention.

### G3. The creature's shading and fur

The hide's fur as shells over the skin near the eye (Lengyel et al. 2001) with an anisotropic
highlight (Kajiya and Kay 1989); its coat's map (plan three's reaction-diffusion) as the fur's
colour; its countershading; its eyes as F8's.

**Done when, Part G:** (1) G-rung pasted; (2) G-motion-sim's tests pass (the worst stride under 5
% infeasible on every gait); (3) G-motion's panel (plan three's §F9, its rubric and footage under
D51's answer) at or above 40 %, or Blocked with its trajectory; (4) G-look on shots 8 and 7
(grazing at the edge, down the trail); (5) G-M4 by §0.5's rule (its budget 1.0 ms GPU for the fur,
0.3 ms CPU for the body).

## Part H: the shots' framing (`engine/wc4-h`)

**What the judges say.** Some faults are the camera's, not the renderer's: at H2 "the render's
camera sits almost inside the terrain", "a camera almost in the ground", "the camera clipping
into the trunk" (`docs/bench.md:2495`, `docs/bench.md:2509`); the vale's grazer "tiny" at 6 m and 10 m (plan three's F10); the floor's fawn shots hide a
roe-sized deer behind a log and in the distance (`docs/bench.md:5063`, plan three H1).

### H1. The framing checks

`studio/framing` (new): for each lookbook shot, the eye's clearance from the ground and from every
solid (the distance volume's), the subject's share of the frame and its occlusion (the interest
check's `subject seen`), and the near plane's intersections. A shot failing a check is listed
with its numbers.

### H2. The fixes, for Ryan

Each failing shot's fix (move the eye, raise it, move a cast) is written as a proposal with its
before/after stills, **not applied** to a frozen shot (D52): the vale's twelve are the panel's
(their hash is B5's), and the floor's shots are its lookbook's. Ryan applies or declines each.

**Done when, Part H:** (1) the checks' tool test; (2) the failing shots listed with numbers;
(3) the proposals in the PR's "For Ryan" with their stills.

## Part I: the final gates and the records (`engine/wc4-i`)

1. `wrela suite` passes, every member (the count pasted).
2. **G-reference** on the twelve (D38, B5's hashes and mode), its tally the last row of "Public
   visual references".
3. **G-class** for each class built, again at the end.
4. **G-M4** (D44): the vale's and the floor's walks at Epic, 600 s sustained, the four tiers, the
   starts; G-memory, G-cold and G-warm.
5. **G-grown, G-style, G-size, G-pixels** as plan three's H2.
6. **G-bench:** "Whether D27 holds" again (D43), a row per gate per scene per device, and the
   class claims: which classes reached equal on their panels and which ended Blocked.
7. **Records:** the hypotheses H9–H13 with their outcomes; a row in `docs/citations.md` for every
   §9 method whose code landed; `docs/toolchain-gaps.md`.

**Done when, Part I:** the seven items in the PR with their numbers; every step's Done-when
satisfied or its Blocked line tried, listed for Ryan; the claim's sentence says which claims hold.

## 7. The hypotheses

| ID | Hypothesis | Decided at |
|---|---|---|
| H9 | One visibility pass for the cards drops the vale's vegetation passes by at least a third on the M4 | D3 |
| H10 | Geometry near the eye (D, E, F) moves the development panel's artifacts row by more than plan three's items did together (16.00 at G0's panel → 14.25 at G8 and G9's, its best) | D, E, F's class panels |
| H11 | The fall and the water (B, C) move pairs 10 and 6 to equal on their class panels | B, C |
| H12 | A simulated gait lifts G-motion off 0 % | G |
| H13 | The framing fixes Ryan accepts move their pairs' artifacts row by at least 1 point | H |

## 8. Worked example: a rung

Plan three's crag rung (`engine/tests/crag-rung`, `docs/bench.md:1950`):
today's cell strays 35.9 px at 10 m and 3.6 px at 100 m, and its stills differ from the footprint's
on two thirds of their pixels at every distance; the footprint at 10 m is 102,230 vertices and 7.36
MB for one 1.3 m piece. A rung like it decides a class's build: here, that today's crag can't be
judged at any distance the shots use, and that the footprint at 10 m is too large to keep for every
bed (a crag of a few hundred beds would be gigabytes). The cut (D1) decides what's drawn; the near
set's residency (D1's cook on demand and eviction) decides what's kept, and the rung measures
both, since a cut alone frees no bytes.

## 9. Sources to cite when their code lands

| Method | Reference | Used in |
|---|---|---|
| The visibility buffer | C. A. Burns and W. A. Hunt, *The Visibility Buffer: A Cache-Friendly Approach to Deferred Shading*, JCGT 2(2), 2013 | §D2 |
| Virtualized geometry, as context | B. Karis, R. Stubbe, G. Wihlidal, *A Deep Dive into Nanite Virtualized Geometry*, SIGGRAPH 2021 Advances | §D1 |
| Cluster culling in compute | G. Wihlidal, *Optimizing the Graphics Pipeline with Compute*, GDC 2016 | §D1 |
| Bark from the trunk's expansion | S. Lefebvre and F. Neyret, *Synthesizing Bark*, EGWR 2002 (already cited) | §F1 |
| Parallax occlusion | N. Tatarchuk, *Dynamic Parallax Occlusion Mapping with Approximate Soft Shadows*, I3D 2006 | §F1 |
| Spray, foam and bubbles | M. Ihmsen, N. Akinci, G. Akinci, M. Teschner, *Unified Spray, Foam and Air Bubbles for Particle-Based Fluids*, The Visual Computer 28, 2012 | §B2 |
| Flow-mapped water | A. Vlachos, *Water Flow in Portal 2*, SIGGRAPH 2010 (already cited) | §B1 |
| PD-controlled locomotion | K. Yin, K. Loken, M. van de Panne, *SIMBICON: Simple Biped Locomotion Control*, SIGGRAPH 2007; S. Coros, A. Karpathy, B. Jones, L. Reveret, M. van de Panne, *Locomotion Skills for Simulated Quadrupeds*, SIGGRAPH 2011 | §G1 |
| CMA-ES | N. Hansen, *The CMA Evolution Strategy: A Tutorial*, 2016 (already cited) | §G1 |
| Fur | J. Lengyel, E. Praun, A. Finkelstein, H. Hoppe, *Real-Time Fur over Arbitrary Surfaces*, I3D 2001; J. T. Kajiya and T. L. Kay, *Rendering Fur with Three Dimensional Textures*, SIGGRAPH 1989 | §G3 |
| Gait timings | M. Hildebrand 1965; R. McN. Alexander and A. S. Jayes 1983 (already cited); footage only under D51 | §G0 |

## 10. The council's findings

(Filled by the council's rounds.)

## 11. For Ryan: the questions only you can answer

1. **D51:** may the creature's gait timings be measured from freely licensed footage of deer
   (Wikimedia Commons, CC BY or CC0), frame by frame, under `docs/citations.md`'s "learn, don't
   copy"? If not, the literature's timings stand.
2. **D52:** the framing fixes change authored shots. Will you review each proposal (§H2) and apply
   the ones you accept, knowing that changing any of the vale's twelve breaks the panel's frozen
   hash and starts a new baseline?
3. **D44:** can you run the M4 commands at the end of each part (about 40 minutes a part: two
   sustained walks and the starts), so a part's performance gates are measured where they're
   defined?
4. **The floor's grown wood:** the forest kinds grown in the open moved the floor's G-look from 4
   after / 18 before to 13 after / 9 before, but the archetypes are the vale's too. Do you want the
   floor to have its own archetypes (about 20 s more build and 500 KB more data, **estimated**), or
   the change tried on the vale's panels for both?
5. **The floor's fawn:** recompose its two shots that hide a roe, or a larger deer (a fallow deer's
   genome) for the floor?
6. **D43:** is a per-class claim (each class's panel at equal) a claim you want, or only D27 whole?
7. **NEAR_SOLID (25 m) and NEAR_LEAF (15 m)** are estimates the rungs will measure; do you want
   a memory ceiling other than D27's 512 MiB for them?
