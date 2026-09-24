# Shared authoring review

Owns constraint evaluation for Studio and CLI. Browser capture resources live in browser.ts; evidence storage is injected. Missing hardware produces unmeasured results, never approval.

Public imports are the explicit `exports` in package.json. Cross-package relative and private imports are rejected by `bun tools/boundaries.ts`. Browser packages contain no Node or Bun dependencies.

Run `bun run test` for the scoped CPU suite, `bun run check` for types and boundaries, and `bun run verify` for hardware and UI verification.
