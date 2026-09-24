# WebGPU rendering

Owns GPU realization and frame submission. ViewportTargets owns attachment lifetime; frame-passes owns shadow/display attachment policy and timing slots. Resource caches, atmosphere, indirect lighting and water have separate lifecycles. GPU changes require hardware verification.

Optional material specializations compile on demand through `PipelineVariants`, with the general pipeline available while compilation runs. Device replacement discards pending results. Compact-relief vertex layouts are created only when a matching mesh is drawn. This keeps unrelated combinations out of the first-frame startup path. Set `WRELA_COLD_SHADERS=1` on browser verification commands to disable Chrome's shader disk cache; the environment manifest records this setting.

Public imports are the explicit `exports` in package.json. Cross-package relative and private imports are rejected by `bun tools/boundaries.ts`. Browser packages contain no Node or Bun dependencies.

Run `bun run test` for the scoped CPU suite, `bun run check` for types and boundaries, and `bun run verify` for hardware and UI verification.
