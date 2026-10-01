# wrela: guide for agents

wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

## Status

Design phase. There's no code yet. The language is being designed sketch by sketch (D-024).

## Read before changing anything

- **`docs/design/decisions.md` is the source of truth.**
  - Every entry has a stable ID (D-NNN).
  - Every entry has a status: Accepted, Proposed, Open or Withdrawn.
- **`docs/design/vision.md`** covers the goal, the four theses and the architecture.
- **`docs/design/sketches/`** holds programs in imagined syntax. The language is derived from them.

## Rules

- **The compiler knows nothing about the engine (D-050).**
  - No keywords, attributes, lang items or compiler rules phrased in engine terms.
  - The test for any language feature: would it make sense in a wrela program that isn't a game?
  - General sugar is fine.
- **Never rewrite a decision.** Add a new entry that supersedes it, then annotate the old entry's status line, for example "Superseded by D-NNN."
- **Proposed is not agreed.** Only the project owner accepts decisions.
- **Label performance numbers as estimates until they're measured.**
- **Sketches use ` ```wrela ` code fences,** and source files use the `.wrela` extension (D-040).

## Layers (D-006, D-016)

| Layer | What | Written in |
|---|---|---|
| 0 | Platform hosts: browser and native | TypeScript, Rust |
| 1 | Language: compiler and stdlib | Rust |
| 2 | Engine | wrela |
| 3 | Studio | wrela + Rust tooling |
| 4 | Games | wrela |
