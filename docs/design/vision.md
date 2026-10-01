# wrela: vision

*Last updated 2026-10-01, after the [design audit](reviews/2026-10-01-audit.md) and spike 01. Decisions referenced as D-NNN live in [decisions.md](decisions.md).*

## Goal

AAA-ambition games that you play by opening a link in a browser.

wrela is an MIT-licensed, spare-time project done for fun. It's run with business-grade discipline for two reasons: to keep code quality high and to stay honest about the engineering. Commercial concerns are explicitly out of scope.

## The core claim

**A game is a program, not a pile of data.** Ship the shortest program that generates the world, and cook it on the player's device.

## Theses

**These are hypotheses, not findings.** D-067 sets out how each will be tested, and what would count as failing. The first measurements, on the primary reference device (D-096) and in one browser, bear on theses 1–4; see milestone 0.

1. **Agent-native authoring.** An engine and language designed for agents first lets one human plus a team of agents operate like an AAA studio.
2. **Fields are the substrate.** Content is authored as fields: functions over space. Fields are compact, need no prebaked assets, and can be cheaply distributed over a CDN. Games should be megabytes, not gigabytes.
3. **The browser is the platform.** It's the broadest compatibility surface there is. *Hypothesis:* WebGPU makes AAA-class rendering viable there.
4. **A compiler that sees the game wins.** A language and compiler that see the whole game's semantics can make optimizations that engines with opaque assets can't.
   - *Clarified 2026-10-01 (D-069):* the original wording said "at runtime included." The owner meant the game's *runtime semantics*, not a compiler that runs in the browser. The compiler was always meant to be an ahead-of-time Rust program on desktop. Claude had misread this and briefly designed a browser tier (D-008), which is now superseded. A browser compiler would come back only if a game needs structure chosen at runtime that enums and the interpreter can't serve, such as user-generated creatures or in-game sculpting.

### How they interlock

- **Fields → agents.** An agent can't usefully edit a 4K texture or a 2M-triangle mesh. It can read, diff and edit a 40-line field expression. Fields turn art production into programming, and programming is what agents are best at.
- **Fields → compiler.** When content is code, the compiler sees the whole game, not just its logic.
- **Fields vs. browser.** Fields trade bytes for compute, and the browser is where compute is tightest.
- **The compiler reconciles them.** Thesis 4 is load-bearing, not a bonus. Without large, measurable compiler wins, fields in the browser probably don't reach AAA frame rates.

## Architecture

| Layer | What | Written in |
|---|---|---|
| 0. Platform hosts | Thin capability boundary to the outside world: a browser host and a native host (D-016) | TypeScript, Rust |
| 1. Language | Compiler and specializer, run at build time (D-007, D-069); stdlib (D-081) | Rust (compiler), wrela (stdlib) |
| 2. Engine | Rendering, physics, animation, audio, netcode | wrela |
| 3. Studio | Agent-native authoring: tools, queries, headless runs, replays | wrela + Rust tooling |
| 4. Games | The games themselves | wrela |

**The layering rule (D-050):** the compiler knows nothing about the engine. The engine is built only on the language and its stdlib, with no special keywords, attributes or lang items. General sugar is welcome. The test for any language feature: would it make sense in a wrela program that isn't a game?

**What ships (D-069):** a game ships as WASM, WGSL and data, with the engine compiled in. Generators *run* on the client, cooking fields into meshes and other cached realizations. Nothing is *compiled* there.

## The console (moonshot)

`games.wrela.dev` turns the browser into a game console (D-018):
- It loads like a console and shows a game library.
- Saves are kept locally in OPFS, the browser's private file storage.
- Sign-in, cloud saves and multiplayer may come later.

The architecture already points this way:
- **The "firmware" is small:** the platform host and the console shell (D-069).
- **Each game is served from its own origin,** and the shell embeds it (D-082). A bug in one game can't reach another game's saves.

## First game: constraints

- **Required:** creatures and high-quality animation (D-003).
- **Out of scope:** realistic human faces.
- **Art direction:** lean into what fields do best. That means organic forms, smooth blends, deformation, destruction, volumetrics, and worlds that can be reshaped.
- **Multiplayer:** the language must never make it impossible (D-015).

## Milestones (candidates)

0. **Measure first (D-067).**
   - Hand-write the WGSL and WASM the compiler would emit for the grazer, and measure it on the reference devices (D-068) against written kill criteria.
     - *Done on the primary device, 2026-10-01* ([spike 01](../../spikes/01-grazer/), D-089, D-096): **passes.** Creatures take 2.49 ms of GPU time for a 40-grazer herd at 1080p with mesh LOD, and 6.49 ms without, against an 8 ms budget. **Still owed:** Chrome stable, Safari and Firefox on this device, and the secondary devices.
   - Separately, test whether agents can author good-looking creatures as field code.
     - *Done once, 2026-10-01* ([experiments/agent-authoring](../../experiments/agent-authoring/), D-095): two unaided agents reached placeholder quality in about 20 minutes each. A re-run with better authoring tools and human art direction is next.
1. **Hello field.** Wrela source compiles to WASM and WGSL, and an SDF renders in a browser tab.
2. **Creature.** One creature walks convincingly across uneven field terrain, in a browser tab, at 60fps on the primary reference device, a MacBook Air M4 (D-096). This tests field authoring, extraction to triangles, deformation, the animation stack and browser performance all at once.

## Non-goals (for now)

- Photoreal humans
- A self-hosted compiler
- A compiler in the browser, until a sketch needs one (D-069)
- Marketing, monetization and distribution strategy

## Prior art worth knowing

| Area | Prior art |
|---|---|
| Field authoring and rendering | Dreams (Media Molecule); Claybook (Aaltonen) |
| Interval pruning of field expressions | Keeter, *MPR* (2020); Fidget |
| Algorithm/schedule separation | Halide |
| Staging | Terra; Zig `comptime`; MetaOCaml. wrela settled on monomorphization: structure is types (D-070). |
| Compiler-known library items (D-081) | Rust lang items |
| Learned locomotion | Zhang, Starke et al., *Mode-Adaptive Neural Networks for Quadruped Motion Control* (2018); Holden et al., *Learned Motion Matching* (2020); Starke et al., *DeepPhase* (2022) |
| Procedural animation | Overgrowth (Wolfire); Rain World |
| Small neural audio | DDSP (Engel et al. 2020) |
| Modal impact synthesis | van den Doel et al.; O'Brien et al. |
| Field-based text rendering | MSDF fonts (Chlumský) |
