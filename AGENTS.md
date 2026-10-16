# wrela agent instructions

MIT-licensed language, compiler, engine and agent-native studio for browser games with AAA ambition. Content is fields: functions over space. Ship quality code and evidence-backed claims.

## Work

- Before starting work, fetch `origin` and pull the current branch with `--ff-only` if it has a remote branch. For a new task, run `git pull --ff-only origin main` on `main` before creating the task branch. Preserve local changes; report any sync blocker before editing.
- Work in each machine's normal local checkout; do not create worktrees. Ryan assigns work across the machines. Keep one active task branch per checkout; never implement on `main`. Use direct commands and ordinary checkout-local build output.
- Own implementation and verification. Routine work has standing permission. Ryan owns goals, architecture, lore and artistic direction; decide implementation details yourself.
- Ship the smallest complete change. Preserve other sessions' work. Delete losing experiments, dead code and unused constants; avoid speculative abstractions and compatibility scaffolding. Preserve `milestone-6` unchanged.
- Add focused regressions for new logic, fix failures at their cause, and run `tools/check.sh` before committing. While working, run the exact test or the group you are changing. Do not weaken assertions or skip tests for a pass.
- End completed work with a committed, pushed branch and a pull request against `main`. Describe the change, checks and any blockers; leave merging to Ryan through the PR workflow.
- Report frame-time and gate-time effects honestly. Use `tools/wrela profile` for paired relative costs on each Linux hardware GPU, with the same scene, input, resolution, workers and timing mode. Run without concurrent builds or GPU work. Check paired M4 Air baselines periodically; keep each machine's reports separate. Distinguish GPU timings, synchronized CPU frame calls and Chrome frame rates.

## Context and layers

- `docs/language.md`: current spec. Find headings with `rg -n '^#' docs/language.md`; read relevant sections.
- `docs/vision.md`: goals and architecture. Read "The lore" before story work; add no lore Ryan has not stated.
- `docs/process.md`: public record of human/AI work and outside sources.
- Prefer `tools/wrela context <item> <package>`, `tools/wrela query`, `tools/wrela doc` and `tools/wrela primer <area>` for Wrela. Fall back to `rg`; report tool gaps as studio findings.

| Layer | Location and constraint |
|---|---|
| Hosts | `runtime/native` (Rust: wgpu/wasmtime), `runtime/browser` (TS), `runtime/abi` (shared with compiler); mechanisms here, policy in Wrela |
| Language | `compiler/` (Rust: syntax → sema → IR → lower → WASM/WGSL), `compiler/std` (Wrela); compiler knows no engine |
| Engine | `engine/` (Wrela); no game names, places or tuning |
| Studio | `studio/`, `ui/` (Wrela), `tools/` |
| Games | `games/<game>/` (Wrela), with its cover; `games/console.toml` lists the released games. Games depend on engine and `examples/`; only studio fixtures depend on a game |

## Quality and checks

- Language features must make sense outside games. Shared helpers live once in engine/std, in the appropriate layer.
- Test rules on small fixtures, bounded seeded samples and explicit resource lifecycle assertions. Judge changes only to the look with the lookbook.
- Use real shaders on hardware GPUs for pixel/buffer assertions and CPU hosts for command/state assertions. Fail if a hardware adapter is unavailable; never use software rendering. Rendering tests assert correctness, without frame-time or FPS thresholds. Keep performance comparisons in `tools/wrela profile`.
- `tools/check.sh` runs every retained test and infrastructure check, including real GPU shaders and small Chrome agreement scenarios. `tools/check.sh <check-name>` runs exactly one; a name that ends in `::` runs that group. Unknown, ambiguous or empty selections fail.
- Names: Rust `module::test` or `crate::target::module::test`; Python `tools::test_module.Class.test_method`; browser `browser::file.test.ts::full test name`; Wrela `wrela::<package-path>::<test-name>` (`compiler/std` for std). Groups: a Rust module path such as `renderer::` (in one test program), `wrela::<package-path>::`, `browser::file.test.ts::`.
- Infrastructure names: `rustfmt`, `wrela-fmt`, `clippy`, `doc`, `tools`, `browser`, `browser-types`, `browser-dist`, `browser-size`, `wgsl`. Use normal formatters.
- Passing checks does not prove prolonged stability, whole-scene quality or reference-machine frame rates. Keep extended captures and benchmarks outside checks.

## Records

- `AGENTS.md` is the sole repo instruction file. Add no docs except grammar, lexical spec and `docs/process.md`; remove any temporary task notes before committing. Run checks locally. CI only builds and deploys releases from `main` (`.github/workflows/deploy.yml`); add no checks to it.
- Update `docs/process.md` for new references, outside assets/algorithms, AI tools/models or kinds of AI work: source, rights, what it teaches and how used. Keep style references outside the repo. Never trace/sample/recreate specific reference shots, characters or designs; never create assets with generative image/audio/music/voice/video models.
- Use clear, concrete English, roughly 80% ASD-STE100.
