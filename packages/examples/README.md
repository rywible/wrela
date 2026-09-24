# Example source libraries

Owns named sample worlds, characters and visual studies. Engines must accept ordinary model documents; production engine packages must not depend on these examples. Tests and applications select them explicitly.

Public imports are the explicit `exports` in package.json. Cross-package relative and private imports are rejected by `bun tools/boundaries.ts`. Browser packages contain no Node or Bun dependencies.

Run `bun run test` for the scoped CPU suite, `bun run check` for types and boundaries, and `bun run verify` for hardware and UI verification.
