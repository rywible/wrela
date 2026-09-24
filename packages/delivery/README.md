# Portable game delivery

Owns standalone HTML packaging of reachable semantic source, cooked products, workers and optional game manifests. Manifests select trusted registered modules; imported source does not execute scripts.

Public imports are the explicit `exports` in package.json. Cross-package relative and private imports are rejected by `bun tools/boundaries.ts`. Browser packages contain no Node or Bun dependencies.

Run `bun run test` for the scoped CPU suite, `bun run check` for types and boundaries, and `bun run verify` for hardware and UI verification.
