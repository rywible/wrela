# Game and world host

The entry point selects trusted game definitions from `games/registry.ts` using a manifest or query parameter. `module-player.ts` supplies rendering, fixed-step driving, inspection, persistence and capture. It contains no sample definition IDs. Each game owns rules, objectives, scene policy and any specialized UI.

The standalone visual showcase lives in `games/showcase`; its study selectors are separate from the generic module host. `@wrela/delivery` owns HTML packaging, while `tools/game-build.ts` discovers game manifests and writes release closures and cooked products.

Use `bun run build` to export, `bun run verify:agent-game` to exercise the exported module host on hardware, and `bun run verify` for the full showcase/Studio workflow. See the [game authoring guide](../../docs/architecture/authoring-games.md).
