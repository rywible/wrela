# wrela agent instructions

MIT-licensed engine, content, studio and games for browser games with AAA ambition, written in Wrela. Content is fields: functions over space. Ship quality code and evidence-backed claims.

This repository holds only Wrela code. The language, its toolchain (compiler, std, hosts) and the spec live in [rywible/wrela-compiler](https://github.com/rywible/wrela-compiler); this workspace pins one toolchain release.

## References

For citing external work, see: ./docs/citations.md
For operating the toolchain, see ./docs/toolchain.md

## Rules

1. One PR for one requested scope of work.
2. Run as few tests as possible to validate a change
3. Cite your sources
4. Store implementation plans and designs outside of the repo.
5. If you find yourself wanting something the toolchain doesn't support, go and build it and cut a new release.

## Layers

Wrela is built vertically: one language, compiler, engine and game, each shaped for the layer above. Each layer serves the one above in its own vocabulary and never reaches up.

| Layer | Where | Knows about | Never knows about |
|---|---|---|---|
| Compiler + std | `rywible/wrela-compiler` | Types, effects and math: gradients, intervals, bounds, how often code runs, how much work it does | Frames, meshes, levels of detail, cameras, any game or scene |
| Engine | `engine/` | Rendering and simulation policy: realization, levels of detail, the frame, upscaling, the cook planner | Any particular game or scene |
| Game | `scenes/`, `mmo/`, `content/` | Content and choices: shapes, looks, which budget each piece gets | Rendering techniques |

1. **Dependencies point down.** A layer uses the ones below it, never the ones above. When the compiler would need an engine idea, phrase the feature in math or timing terms instead ("per load", not "mesh cook").
2. **A compiler or std feature needs a second user.** It must make sense beyond the case that asked for it.
3. **Scene tricks are temporary.** A rendering technique written in a game (a pass drawn at a quarter of the pixels, say) moves into the engine or is deleted within one milestone. A game holds content, not techniques.
4. **The engine never special-cases a game.** When a game needs something, the engine gets a general version and the game uses it.
5. **Performance claims carry measurements:** before and after, on the same machine, in the PR.
6. **One way to do each job.** When a general version lands, the old one is removed in the same change, so there are never two realizers or two systems of levels of detail.
