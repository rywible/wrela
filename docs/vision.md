# wrela: vision

*The goal, the theses and their evidence, the architecture, the renderer, and the constraints everything else rests on. Updated 2026-10-02, after spikes 01–13; the flagship settled 2026-10-05; M4's results 2026-10-06. Decision IDs (D-NNN), sketches and spikes refer to the design record in the git tag `design-archive-2026-10`; spike 13 is in the tag `spike-13-field-math`.*

## Goal

AAA-ambition games that you play by opening a link in a browser.

wrela is an MIT-licensed, spare-time project done for fun. It's run with business-grade discipline for two reasons: to keep code quality high and to stay honest about the engineering. Commercial concerns are out of scope.

## The core claim

**A game is a program, not a pile of data.** Ship the shortest program that generates the world, and cook it on the player's device.

## Theses

They're hypotheses. Each line says where the evidence stands.

| Thesis | Evidence so far |
|---|---|
| **1. Agent-native authoring.** An engine and language designed for agents first lets one human plus a team of agents operate like an AAA studio. | **Weakest.** Agents author recognisable creatures as fields at placeholder quality, and the best grazers at shippable mid-tier. M3 built the lens in wrela and measured it: drags land within 0.5 mm, fits recover changed literals within 0.6 mm, and every action answers the same headless and in Chrome. Three blind authoring rounds, judged by the owner: round 1 (unaided Opus, WGSL); round 2 (2026-10-04: 14 runs of Opus 5.5, Sonnet 5.5 and Fable 5.1, with the lens or with round 1's tools in wrela), where none beat round 1's finals; round 3 (the same day, the same protocol), with proportions as code (specs, solved by `wrela solve --spec`), outlines as code (blueprints), a quadruped base, a fur modifier and round 2's friction fixed. In round 3 the owner picked an agent's final over round 1's for both creatures for the first time (an Opus grazer at shippable mid-tier, an Opus wolf at placeholder), and three of seven grazers reached shippable mid-tier. Wolves stayed weak, and the owner rated the same round 1 wolf one step lower than in round 2, so verdicts move with the company they keep. Over both rounds the lens's effect on the verdict is within the noise (0.83 against 0.67 on blockout 0, placeholder 1, shippable 2, twelve runs each); the model matters more (Opus 1.0, Sonnet 0.75, Fable 0.5 in both rounds), and twice the budget didn't help. Authors took specs up as a tape measure and rejected the solvers every time: without bounds or priors, they met checks by breaking anatomy. A fourth round gave Opus authors public-domain photos (and the wolves an outline cut from one): one reference grazer was picked best in its sheet, but the reference runs averaged below round 3's best beside them, and with a real wolf in view the authors still built tube legs and ball paws. The gap is more construction than knowledge. A fifth round (the wolf alone) added tools to compare by eye and a critic between turns: the authors used their whole budgets and matched the photo's silhouette best of any round, but no better verdicts followed, and a wolf Claude built from a new anatomy kit was rated a blockout. Matching one photo isn't what the owner rewards; clean, simple, coherent forms are. A sixth test asked whether the medium can hold such a form at all: a sculpted wolf (an artist's, CC BY 4.0) replicated in wrela as 27 lofts, each a section's radii along a straight axis, measured from the sculpt (within 2 mm on 97% of its surface), was judged clearly better than every agent's wolf, as clay in the lens (weakly blinded). So the medium can hold a great form; the gap is in authoring. Compressed to harmonics per section and a few sections along each axis, it sheds the sculpt's fur strokes, but a fit that falls short of the surface opens cracks where parts meet. Held up where no other part reaches the surface, it has none, and keeps 98% of the surface within 5 mm in 6,600 numbers (twelve harmonics, a section every 3 cm), 3.4 times fewer than its tables: a leg segment takes 175 to 375 numbers. At 3,519 numbers (eight harmonics, every 4 cm) it's 95%, and it swells up to 2 cm past the sculpt where a section can't follow a narrow form. Toes, folds and the seams between parts don't compress yet. As 18 sweeps (each a form along a curved path through the joints, its sections six named numbers and up to four bumps), about 2,500 numbers, the wolf keeps 77% of the sculpt's surface within 2 mm and 94% within 5 mm. Authoring round 6 asked whether agents build better with such forms: five Opus authors built the grazer from scratch with sweeps, creases and digits, the sweep wolf's source and sheets as a reference, a critic and the whole budget. The owner ranked all three earlier grazers (round 1's, and round 3's and round 4's best) above all five new ones (four placeholder, one blockout), and Claude's blind ranking put the five new ones in the same order. The new forms' own defects showed on the sheets (bands where a sweep bends sharply, flat ends as ridges, a bump that slid between sections), and no author used creases or digits. Forms at the level a sculptor works in, and an example built from them, didn't move quality within 25 minutes. |
| **2. Fields are the substrate.** Content is authored as fields: functions over space. They're compact and need no prebaked assets, so games are megabytes, not gigabytes. | **Holds as the source; not as the per-frame representation.** Evaluating authored fields per pixel per frame fails for anything big on screen. Cooking them on device into meshes, caches and textures works (see the renderer below). |
| **3. The browser is the platform.** WebGPU makes AAA-class rendering viable there. | **Holds on the reference device for a whole scene** with the renderer below. M5's clearing (a terrain clipmap to a far mountain, a forest of cut cards, impostors and density volumes with no popping between them, grass and flowers placed on the GPU, baked probes, cached and moving shadows, a walking creature, temporal AA up from 960×540, and a painted look) takes a median 6.2 ms of GPU work per 1080p frame in Chrome, 8.4 ms at its worst over a 60 s camera path. Chrome only so far; Safari, Firefox and the secondary devices are untested. |
| **4. A compiler that sees the game wins.** A compiler that sees the whole game's semantics can do what engines with opaque assets can't. | **Matches hand-written code at the scale of one scene; the wins are generality.** M4 (#42) built spike 01's herd from wrela source: extraction at 1.04–1.23× the hand-written GPU time from 3 cm to 1 cm cells, the creatures' shadows and shading at 1.03–1.11×, whole frames at 0.95–1.05×, the CPU field, physique and raycasts at 0.87–1.12×, and the same frames (0.05/255). Every ratio was met in the compiler (inlining, if-conversion, values kept in place, a gradient that carries the channels with it), none by hand. The derived interval culls more blocks than the hand-written bound, and the sim gives one hash sequence in every host. Spike 13: the compiler's nested derivations certify a smooth creature's mesh and its topology across an animation, something an opaque asset can't offer; noisy and sharp-edged fields don't certify yet. The claim that fields beat baked assets on raw speed is not supported: per-pixel field shading costs 1.6–2.8× a texture lookup. The case is generality and size. |

**How they interlock:** fields turn art production into programming, which is what agents are good at (1–2). When content is code, the compiler sees the whole game (2–4). Fields trade bytes for compute, and the browser is where compute is tightest (2–3); the compiler and on-device cooking reconcile them.

## Architecture

| Layer | What | Written in |
|---|---|---|
| 0. Platform hosts | Thin boundary to the outside world: the standard browser runtime and a native host | TypeScript, Rust |
| 1. Language | The compiler, run ahead of time on the developer's machine; the stdlib | Rust (compiler), wrela (stdlib, with a small unsafe core) |
| 2. Engine | Rendering, simulation, animation, audio | wrela |
| 3. Studio | Agent-native authoring: tools, queries, headless runs, replays | wrela + Rust tooling |
| 4. Games | The games | wrela |

**The layering rule:** the compiler knows nothing about the engine. The test for any language feature: would it make sense in a wrela program that isn't a game?

### What ships and how it runs

```
wrela build ──► game.wasm     engine + game, CPU
            ──► *.wgsl        GPU entry points
            ──► manifest      pipelines, bind group layouts, GpuData layouts
            ──► data/         compile-time constants
packaging   ──► runtime/      the standard browser runtime, pinned version
```

- **The compiler emits WASM, WGSL, a manifest and data; never JS.** Game code reaches the host through imports declared in the stdlib's unsafe core.
- **One standard runtime,** hand-written TypeScript, ships with every game at the version it was built with, so a runtime update can't break a shipped game. Budget: ≤ 1 MB.
- **Threads:** a render worker owns the GPU and the canvas; a sim worker runs the simulation and hands presentation a double-buffered snapshot; helper workers run parallel work; the audio worklet reads a ring buffer. All share one WASM memory (cross-origin isolated).
- **WASM records commands; the runtime decodes them in bulk** into WebGPU calls. The vocabulary is generic (create a buffer, write a range, dispatch, draw indirect). The native host (wasmtime + wgpu) runs the same command stream for CI, headless agent runs and, later, dedicated servers.
- **Data lives where it's consumed.** Bulk presentation data is generated and used on the GPU and never read back. The CPU sends recipes and changes: field values, snapshots, edits, the camera. Readback is asynchronous and never feeds the sim.
- **Mechanism in the runtime, policy in wrela:** storage is "bytes at a path" over OPFS; save slots and migration are wrela code.

## The renderer

Decided after spikes 01–12 (2026-10-02). **Cook fields on device; rasterize what's big on screen; trace what's small; light with cooked fields.**

- **Fields are the source of truth.** When content spawns or streams in, the engine cooks it on the GPU:
  - meshes, by extraction, with LOD by screen size
  - terrain clipmap tiles
  - impostors and density volumes for far vegetation
  - material inputs: thickness, curvature and occlusion
  - lighting caches: cooked distance fields, and probes baked at load
- **Per frame, rasterize what's big on screen:** terrain, near foliage, creatures (skinned extracted meshes), architecture, cloth and hair (GPU-simulated).
- **Trace fields where it's cheap:** small and distant instances such as herds, crowds and banners (tile binning); far vegetation volumes; eyes.
- **Light with cooked fields:** sun shadows and sky occlusion traced through cooked fields at reduced resolution, and bounce light from probes baked at load. Per-frame field GI doesn't fit the M4. Interiors are unsolved.
- **Never evaluate an authored field per pixel per frame over a large screen area.** Every spike that did failed by 3–36×.
- **Levers:** temporal AA with half-resolution upscaling; simpler, stylized content.
- **Water:** screen-space reflections plus probes; traced reflections are too expensive.
- **Style** is the owner's call. Cel shading with field-drawn outlines is fields' distinctive strength; with rasterized surfaces it needs a short silhouette pass.

**Evidence** (MacBook Air M4, headless Chrome 154, 1080p; final runs one at a time on an otherwise idle GPU, 2026-10-02):

| Spike | What | Result |
|---|---|---|
| 01 | Herd of 40, extracted and skinned, per-pixel field shading | **Pass:** 2.49 ms of creature GPU time with LOD (8 ms budget) |
| 02 | The same herd, pure ray marching | Ties at distance; 2.2× worse close up |
| 03 | A forest, ray marched | **Fail:** vegetation 42–61 ms, lighting 17–20 ms (slices 4 and 3.5 ms) |
| 04 | A world to the horizon | Marched cache fails at 10–26 ms; **a rasterized terrain clipmap passes at 1.3 ms** |
| 05 | Lighting from fields | Per-frame 4.0–5.7 ms (over 3.5); **baked probes + traced sun pass at 2.2–2.6 ms**; the tower interior is wrong either way |
| 06 | 300 moving things and live edits | **Pass:** 1.8 ms; 20 edits/s cost 0.12 ms. A dense town marched as primary visibility fails (9.0 ms against 3) |
| 07 | A furred hero filling the screen | **Fail traced** (89 ms); the rasterized mesh costs 1.6–3.3 ms |
| 08 | Water with traced reflections | **Fail:** 12.8 ms at 30% coverage (1 ms slice); 3.3 ms at quarter-resolution reflections |
| 09 | The lens editor | Click to source ~1 ms, drag solves ~1 ms, exact fits |
| 10 | Materials from field quantities | Eyes pass (+0.4 ms); skin +6.7 ms, layered stone +8.8 ms and leaves +68 ms fail when evaluated per pixel |
| 11 | Hair and cloth | **Raster plus depth-bounded tracing passes** at 1.05 ms (0.39 ms close up); simulation 0.39 ms; pure fields 2.1 ms, failing close up (3.8 ms) |
| 12 | Styles | Cel with field-native outlines is the most attractive; stylized content saves 20–36% |
| 13 | Certified field math (native host: wasmtime and wgpu) | Nested derived bounds certify the smooth creature for meshing (0 open boxes to 3.9 mm; 5 ms per 2M boxes on the GPU) and its topology for every breath (0.4 s); a corner-sampled grid misses a 12 mm part the certificate finds. Bark noise and box edges don't certify; segment tracing with derived bounds costs 3–50× sphere tracing |

**Built:** M4 (#42) compiled spike 01's herd from wrela source. Paced at 60 Hz in Chrome with LOD and the sim running, no frame's GPU time passed 16.7 ms; its creatures cost 1.15× the spike's 3 cm herd with LOD, in the same mesh memory. M5 (#51) drew the Last Green's clearing with the engine's renderer in the look the owner picked (gouache and light for the world, cel for characters): spike 15's frame took 24–29 ms at 1080p; M5's takes 6.2 ms (median) drawn at 960×540 and brought up to 1080p by temporal AA, within 2.1/255 of the same frame drawn at 1080p. Levels of detail blend over a dithered cross-fade rather than switch, so nothing pops (moved into M5 from M8 at the owner's ask).

## Constraints

The load-bearing rules. Changing one needs the owner, and the change and its reason go in the commit and here.

| Constraint | Why |
|---|---|
| **The compiler knows nothing about the engine** (D-050). No keywords, attributes or rules in engine terms; general sugar is fine. | Keeps the language general and the engine replaceable. |
| **The compiler runs ahead of time on desktop**; nothing is compiled in the browser (D-069). | Whole-program monomorphization; a small runtime. |
| **Structure is types; values are data** (D-070). Generics are monomorphized; a seed changes data, never structure. | One pipeline serves every individual of a type. |
| **Value semantics, no GC; parameter modes instead of references; no lifetimes** (D-014, D-064). | Predictable frames; nothing for agents to misuse. |
| **Every transfer is visible:** `take`, `.clone()`, `mut` at the call site (D-064). | Costs and mutations show where they happen. |
| **Determinism:** simulation code is `@deterministic`, with strict CPU floats; GPU results never feed the sim (D-015, D-052, D-074). | Multiplayer, replays and agent testing stay possible. |
| **Multiplayer must never be precluded;** the first target is server-authoritative (D-042). | The flagship may grow into it. |
| **Effects are named and checked per context; staging is guaranteed or rejected** (D-072). | No silent fallbacks on the GPU or audio thread. |
| **The renderer above:** cook fields, rasterize what's big, trace what's small, light with cooked fields. | Spikes 01–12. |
| **Data lives where it's consumed;** the CPU–GPU boundary carries recipes and changes (D-097). | No zero-copy path exists in the browser. |
| **The compiler emits WASM, WGSL and a manifest only; one pinned runtime per game** (D-099, D-100). | Old games keep working. |
| **One origin per game;** saves stay in the game's origin (D-082, D-101). | A bug in one game can't reach another's saves. |
| **Reference device:** MacBook Air M4 in Chrome. **Budgets:** 16.7 ms per frame at 1080p, ≤ 1.5 GB per tab, ≤ 64 pipelines per scene, sim ≤ 4 ms per tick (D-068, D-096). A frame's time is its GPU work, measured with the frames back to back: paced at 60 Hz, the M4's GPU lowers its clock until a frame fills about three quarters of its interval whatever its work, so a paced frame's span measures the clock (M5, #51 AC2). | Every claim is measured against these. |
| **Browsers:** Chrome, Safari and Firefox. The sim computes the same bits in all three and in the native host (owner, 2026-10-05). | The flagship's leaderboards verify replays from any browser on the native host. |
| **Size budgets:** runtime ≤ 1 MB; time-to-play ≤ 6 MB; cold start ≤ 8 MB, playable within 5 s (D-041, D-069). Each of the flagship's floors ≤ 16 MB, streamed when the player reaches it, music excluded: an estimate until the first floor is measured. This replaces D-083's 32 MB total (owner, 2026-10-05). | "Megabytes, not gigabytes," per world. |
| **A field's declared bounds are checked,** not trusted: a bound is a stdlib method (`Lipschitz`), and debug builds and tests check it against the bound the compiler derives (D-077, D-092; revised by the owner's language review of 2026-10-02, which replaced facts as compiler attributes). | Authors' declared bounds were wrong three times in the spikes. |
| **The grammar is the spec;** the hand-written parser conforms by test (D-104). | Agents and tools get a machine-checkable definition. |
| **The source is the truth; every tool is a lens that reads and writes it,** and every tool action is also an API call (D-105). | Agents and humans edit the same thing. |
| **Estimates are labelled until measured; untested claims are hypotheses.** | Honest engineering. |

## The flagship game

**The Reliquary** (working title): an offline, single-player climb of a tower of 100 worlds, played from a link (D-103; settled by the owner on 2026-10-05, with the full constraints in #44).

- **The tower.** When a world ends, something takes the place, frozen at its last moment, and stacks it into the tower. Each floor is a handcrafted world with its own creatures, anchors to free and a Warden that refuses to let its world end. Beating the Warden lets that world's time run again. The tower ends at floor 100 with a final boss. It launches with three floors (the Last Green, a forest; the Titan's Back; the Clockless War) and grows each season.
- **Skill and knowledge only.** No loot and no power levels: one soul blade that evolves by how you fight, one difficulty, and a discovery loop of echoes cut from the frozen moment.
- **The look:** storybook anime under real light. The world is painted in continuous light, with brush dabs anchored on its surfaces and no lines; characters are cel-shaded, with thin lines (picked by the owner from spike 15's three variants, #50; it can evolve). The climber is masked; there's no voice acting. Realistic human faces stay out of scope.
- **Seasons and leaderboards.** Each three-month season is a fresh race from floor 1. Leaderboards come from deterministic replays, verified on the native host against the season's build. The first verified unassisted climb of a season is its champion, who proposes a floor that the owner builds.
- **Nothing collected, nothing sold.** No accounts; saves stay in the game's origin and export as files; hosting is on free tiers.
- **Later:** an MMO in one shared world, once there's money, time and legal advice (#31).

The owner's order of priority still holds:
1. A beautiful, living forest and vegetation.
2. Nature and landscapes.
3. Towers and architecture.
4. Beautiful creatures in those worlds.

## The console (moonshot)

`games.wrela.dev` turns the browser into a game console: it loads like one, shows a game library, and keeps saves locally. Later: sign-in, cloud saves and multiplayer. Each game runs on its own subdomain origin, embedded by the shell, which brokers only what spans games: sign-in, cloud saves, the library, and save export.

## Milestones

Planned in GitHub issues on rywible/wrela, not here. Each milestone has a scope issue listing what's in, what's out, and where the out-of-scope items go.

| Milestone | Outcome |
|---|---|
| **M0: measure first** | Done: spikes 01–12 |
| **M1: hello field** | Done: wrela compiles to WASM and WGSL; a field renders in a browser tab |
| **M2: the language** | Done: the language and stdlib, complete: tiers 0–2, without a compiler in the browser |
| **M3: the first lens** | Done (October 2026): the lens, a studio tool written in wrela, and the agents' command-line tools; thesis 1 tested blind in authoring rounds 2 to 6 |
| **M4: the engine's spine** | Done (October 2026): spike 01's herd simulated and rendered from wrela source within 1.25× of the hand-written numbers; the sim on its own worker, its replays verified with no GPU and on x86-64 |
| **M5: the look** | Built (October 2026): the Last Green's clearing at 60 fps (6.2 ms median of GPU work per 1080p frame in Chrome) with a creature walking across it, in the look the owner picked from spike 15's three; hot reload in both hosts; the great tree and the fawn authored with the lens. The owner's judgments (the look in motion, the round's verdicts) are pending |
| **M6: the duel** | The masked climber against one creature, in all three browsers; the combat-feel gate |
| **M7: the Ashstag** | The first Warden and the soul blade's first evolution; the art-quality gate |
| **M8: the Last Green** | Floor 1 in full, streamed, released free as an offline game |
| **M9: seasons** | Leaderboards from verified replays, seasons and the champion |
| **M10: season 1** | The Titan's Back and the Clockless War; the tower opens with three floors |

Everything else in the vision is in the backlog issue (#31). The pinned issue #26 tracks status against the vision.

## Non-goals (for now)

- Photoreal humans
- A self-hosted compiler
- A compiler in the browser
- Phones
- Marketing, monetization and distribution

## Prior art

| Area | Prior art |
|---|---|
| Field authoring and rendering | Dreams (Media Molecule); Claybook (Aaltonen) |
| Foliage at scale | UE5 Nanite Foliage (voxels in the distance); Decaudin & Neyret, *Rendering Forest Scenes in Real-Time* (2004) |
| Lighting from distance fields | UE5 Lumen; DDGI probes |
| Interval pruning of field expressions | Keeter, *MPR* (2020); Fidget |
| Staging and specialization | Halide; Terra; Zig `comptime` |
| Learned locomotion | MANN (2018); Learned Motion Matching (2020); DeepPhase (2022) |
| Procedural animation | Overgrowth; Rain World |
| Audio | Modal impact synthesis (van den Doel; O'Brien); DDSP |
| Direct manipulation of programs | Sketch-n-Sketch (output-directed programming) |
