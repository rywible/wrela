# Realization compiler

Owns semantic-to-runtime products, stable product identities, cooking and workers. The compile entry point owns compilation; the root index is only a public facade. Cooking follows entry and dynamic roots. Browser worker access is exported as /client.

Public imports are the explicit `exports` in package.json. Cross-package relative and private imports are rejected by `bun tools/boundaries.ts`. Browser packages contain no Node or Bun dependencies.

Run `bun run test` for the scoped CPU suite, `bun run check` for types and boundaries, and `bun run verify` for hardware and UI verification.
