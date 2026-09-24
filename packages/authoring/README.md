# Agent and human authoring

Owns atomic edits, capabilities, strict input contracts, proposal/review records and publication coordination. Work v2 stores one baseline plus operation alternatives. Browser and filesystem stores adapt the same publication protocol. The interpreter owns document edits; AuthoringSession owns immutable state, revisions and history.

Public imports are the explicit `exports` in package.json. Cross-package relative and private imports are rejected by `bun tools/boundaries.ts`. Browser packages contain no Node or Bun dependencies.

Run `bun run test` for the scoped CPU suite, `bun run check` for types and boundaries, and `bun run verify` for hardware and UI verification.
