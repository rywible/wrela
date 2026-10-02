# wrela: guide for agents

wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

## Status

Design phase, with first measurements. There's no compiler yet.
- Spike 01 measured hand-written output for the grazer on the primary reference device, a MacBook Air M4 (D-089, D-096). It passes D-067's kill criteria.
- `docs/design/language.md` describes the whole language as it stands, so compiler work may start (D-088).
- Spikes and experiments are throwaway code that answers a question. They aren't the start of the engine or compiler.

## Read before changing anything

- **`docs/design/decisions.md` is the source of truth.**
  - Every entry has a stable ID (D-NNN).
  - Every entry has a status: Accepted, Proposed, Open or Withdrawn.
- **`docs/design/language.md`** describes the whole language as it stands, with feature tiers. Keep it current when a decision changes the language.
- **`docs/design/memory-model.md`** is the one place the memory rules live: parameter modes, projections, exclusivity, regions, snapshots.
- **`docs/design/platform.md`** is the one place the browser runtime is described: what ships, threads, the CPU–GPU boundary, console play.
- **`docs/design/vision.md`** covers the goal, the four theses and the architecture.
- **`docs/design/sketches/`** holds programs in imagined syntax. The language is derived from them.
- **`docs/design/reviews/`** holds independent audits, plus the responses that map each finding to a decision.
- **`spikes/`** and **`experiments/`** hold measurements and experiments. Each has a README with its method, results and caveats.

## Rules

- **The compiler knows nothing about the engine (D-050).**
  - No keywords, attributes, lang items or compiler rules phrased in engine terms.
  - The test for any language feature: would it make sense in a wrela program that isn't a game?
  - General sugar is fine.
- **Never rewrite a decision.** Add a new entry that supersedes it, then annotate the old entry's status line, for example "Superseded by D-NNN."
- **Proposed is not agreed.** Only the project owner accepts decisions. The owner may delegate a choice; delegated decisions say "Accepted (delegated)". Statuses are defined in the header of `decisions.md`.
- **Accepted is not validated.** Check the evidence table near the end of `decisions.md` before building on a load-bearing decision.
- **Label performance numbers as estimates until they're measured.** Label claims that haven't been tested as hypotheses.
- **Sketches use ` ```wrela ` code fences,** and source files use the `.wrela` extension (D-040).

## Layers (D-006, D-016)

| Layer | What | Written in |
|---|---|---|
| 0 | Platform hosts: browser and native | TypeScript, Rust |
| 1 | Language: compiler (run at build time) and stdlib | Rust (compiler), wrela (stdlib, with a small unsafe core) |
| 2 | Engine | wrela |
| 3 | Studio | wrela + Rust tooling |
| 4 | Games | wrela |
