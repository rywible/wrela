# wrela: vision

*Last updated 2026-10-01. Decisions referenced as D-NNN live in [decisions.md](decisions.md).*

## Goal

AAA-ambition games that you play by opening a link in a browser.

wrela is an MIT-licensed, spare-time project done for fun. It's run with business-grade discipline for two reasons: to keep code quality high and to stay honest about the engineering. Commercial concerns are explicitly out of scope.

## The core claim

**A game is a program, not a pile of data.** Ship the shortest program that generates the world, and cook it on the player's device.

## Theses

1. **Agent-native authoring.** An engine and language designed for agents first lets one human plus a team of agents operate like an AAA studio.
2. **Fields are the substrate.** Content is authored as fields: functions over space. Fields are compact, need no prebaked assets, and can be cheaply distributed over a CDN. Games should be megabytes, not gigabytes.
3. **The browser is the platform.** It's the broadest compatibility surface there is, and WebGPU makes AAA-class rendering viable there.
4. **A compiler that sees the game wins.** A language and compiler that see the game's semantics, at runtime included, can make optimizations that engines with opaque assets can't.

### How they interlock

- **Fields → agents.** An agent can't usefully edit a 4K texture or a 2M-triangle mesh. It can read, diff and edit a 40-line field expression. Fields turn art production into programming, and programming is what agents are best at.
- **Fields → compiler.** When content is code, the compiler sees the whole game, not just its logic.
- **Fields vs. browser.** Fields trade bytes for compute, and the browser is where compute is tightest.
- **The compiler reconciles them.** Thesis 4 is load-bearing, not a bonus. Without large, measurable compiler wins, fields in the browser probably don't reach AAA frame rates.

## Architecture

| Layer | What | Written in |
|---|---|---|
| 0. Platform hosts | Thin capability boundary to the outside world: a browser host and a native host (D-016) | TypeScript, Rust |
| 1. Language | Ahead-of-time compiler front end; specializing back end that runs in the browser; stdlib (D-007, D-008) | Rust |
| 2. Engine | Rendering, physics, animation, audio, netcode | wrela |
| 3. Studio | Agent-native authoring: tools, queries, headless runs, replays | wrela + Rust tooling |
| 4. Games | The games themselves | wrela |

**The layering rule (D-050):** the compiler knows nothing about the engine. The engine is built only on the language and its stdlib, with no special keywords, attributes or lang items. General sugar is welcome. The test for any language feature: would it make sense in a wrela program that isn't a game?

**What ships:** a game is versioned IR (intermediate representation) plus content. The runtime (platform host, specializing back end and engine) cooks it on the client into WASM, WGSL and cached realizations.

## The console (moonshot)

`games.wrela.dev` turns the browser into a game console (D-018):
- It loads like a console and shows a game library.
- Saves are kept locally in OPFS, the browser's private file storage.
- Sign-in, cloud saves and multiplayer may come later.

The architecture already points this way. The runtime acts as console firmware: downloaded once and cached. Each game is just its IR.

## First game: constraints

- **Required:** creatures and high-quality animation (D-003).
- **Out of scope:** realistic human faces.
- **Art direction:** lean into what fields do best. That means organic forms, smooth blends, deformation, destruction, volumetrics, and worlds that can be reshaped.
- **Multiplayer:** the language must never make it impossible (D-015).

## Milestones (candidates)

1. **Hello field.** Wrela source compiles to WASM and WGSL, and an SDF renders in a browser tab.
2. **Creature.** One creature walks convincingly across uneven field terrain, in a browser tab, at 60fps on a mid-range laptop. This tests field authoring, extraction to triangles, deformation, the animation stack and browser performance all at once.

## Non-goals (for now)

- Photoreal humans
- A self-hosted compiler
- Marketing, monetization and distribution strategy

## Prior art worth knowing

| Area | Prior art |
|---|---|
| Field authoring and rendering | Dreams (Media Molecule); Claybook (Aaltonen) |
| Interval pruning of field expressions | Keeter, *MPR* (2020); Fidget |
| Algorithm/schedule separation | Halide |
| Staging | Terra; Zig `comptime`; MetaOCaml |
| Compiler-known library items | Rust lang items |
| Learned locomotion | Zhang, Starke et al., *Mode-Adaptive Neural Networks for Quadruped Motion Control* (2018); Holden et al., *Learned Motion Matching* (2020); Starke et al., *DeepPhase* (2022) |
| Procedural animation | Overgrowth (Wolfire); Rain World |
| Small neural audio | DDSP (Engel et al. 2020) |
| Modal impact synthesis | van den Doel et al.; O'Brien et al. |
| Field-based text rendering | MSDF fonts (Chlumský) |
