# Authored source and realization contracts

Owns document schemas, validation, identities, dependency catalogs, and typed-buffer ownership. It has no engine or sample-content dependencies. Use document-repository for indexed catalogs and bounded working sets.

Public imports are the explicit `exports` in package.json. Cross-package relative and private imports are rejected by `bun tools/boundaries.ts`. Browser packages contain no Node or Bun dependencies.

Run `bun run test` for the scoped CPU suite, `bun run check` for types and boundaries, and `bun run verify` for hardware and UI verification.
