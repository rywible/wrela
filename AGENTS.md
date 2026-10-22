# wrela agent instructions

MIT-licensed engine, content, studio and games for browser games with AAA ambition, written in Wrela. Content is fields: functions over space. Ship quality code and evidence-backed claims.

This repository holds only Wrela code. The language, its toolchain (compiler, std, hosts) and the spec live in [rywible/wrela-compiler](https://github.com/rywible/wrela-compiler); this workspace pins one toolchain release.

## Work

- Before starting work, fetch `origin` and pull the current branch with `--ff-only` if it has a remote branch. For a new task, run `git pull --ff-only origin main` on `main` before creating the task branch. Preserve local changes; report any sync blocker before editing.
- Work in each machine's normal local checkout; do not create worktrees. Ryan assigns work across the machines. Keep one active task branch per checkout; never implement on `main`. Use direct commands and ordinary checkout-local build output.
- Own implementation and verification. Routine work has standing permission. Ryan owns goals, architecture, lore and artistic direction; decide implementation details yourself.
- Ship the smallest complete change. Preserve other sessions' work. Delete losing experiments, dead code and unused constants; avoid speculative abstractions and compatibility scaffolding. Preserve `milestone-6` unchanged.
- Add focused regressions for new logic, fix failures at their cause, and run `wrela suite` before committing. While working, run the exact test or the group you are changing. Do not weaken assertions or skip tests for a pass.
- End completed work with a committed, pushed branch and a pull request against `main`. Describe the change, checks and any blockers; leave merging to Ryan through the PR workflow.
- Report frame-time and gate-time effects honestly. Use `wrela profile` for paired relative costs on each Linux hardware GPU, with the same scene, input, resolution, workers and timing mode. Run without concurrent builds or GPU work. Check paired M4 Air baselines periodically; keep each machine's reports separate. Distinguish GPU timings, synchronized CPU frame calls and Chrome frame rates.

## The toolchain

- The root `wrela.toml` pins the toolchain release (`toolchain = "X.Y.Z"`). Any installed `wrela` runs the pinned one, downloading it to `~/.wrela/toolchains` and checking its checksum; `wrela toolchain` says which runs and why. Install a first `wrela` from a [wrela-compiler release](https://github.com/rywible/wrela-compiler/releases).
- A change that needs the toolchain changes `wrela-compiler` first: work there, release it, then move the pin here. To develop both at once, set `WRELA_TOOLCHAIN=<wrela-compiler checkout>` (it builds and runs that checkout) or `WRELA_TOOLCHAIN=<unpacked release>`.
- Where a tool here needs a capability the toolchain lacks, add the capability to the toolchain; never work around it in another language. Every tool here is a Wrela tool program (`wrela tool <name>`, declared under `[tools.<name>]` in `wrela.toml` with exactly the grants it needs).

## Context and layers

- The language spec: `spec/language.md` in wrela-compiler, and on docs.wrela.dev. Read the relevant sections.
- `docs/vision.md`: goals and architecture. Read "The lore" before story work; add no lore Ryan has not stated.
- `docs/process.md`: public record of human/AI work and outside sources.
- Prefer `wrela context <item> <package>`, `wrela query`, `wrela doc` and `wrela primer <area>` for Wrela. Fall back to `rg`; report tool gaps as studio findings.
- `wrela doc --export <dir> <package-dir>...` writes the reference docs: the spec by section and a page per module, in Markdown, with `index.json` (every item and spec section, with its page, anchor and source) and `llms.txt`. Doc comments and each file's opening comment are that reference; keep them true when you change an item.

| Layer | Location and constraint |
|---|---|
| Engine | `engine/` (Wrela); no game names, places or tuning |
| Studio | `studio/` (the lens and the project's tools: release, deploy, lookbook, motion, round, listening, audio, reference, serve), `ui/`, `docsite/` (docs.wrela.dev) |
| Platform | `platform/`: the console's protocols with its games and sessions |
| Content | `content/`: creatures, plants, characters and clouds that games and examples build on; `references/` is their reference data |
| Games | `games/<game>/`, with its cover; the release tool's catalog (`studio/release/catalog.wrela`) lists the released games and `console/` is the console. Games and `examples/` depend on engine and `content/`; only tests and studio fixtures depend on an example or a game |

## Quality and checks

- Engine and studio features must be general enough for any game. Shared helpers live once, in the appropriate layer here, or in std when they make sense outside games (then they belong in wrela-compiler).
- Test rules on small fixtures, bounded seeded samples and explicit resource lifecycle assertions. Judge changes only to the look with the lookbook (`wrela tool lookbook`; `wrela tool motion` for motion).
- Use real shaders on hardware GPUs for pixel/buffer assertions and CPU hosts for command/state assertions. Fail if a hardware adapter is unavailable; never use software rendering. Rendering tests assert correctness, without frame-time or FPS thresholds. Keep performance comparisons in `wrela profile`.
- `wrela suite` checks the whole workspace: every member's format, the allowlist in `wrela.toml` (only Wrela code, manifests, assets with their licences, and these docs) and every member's tests, including GPU frame tests and tool tests with real Chrome. `wrela suite <name>` runs exactly one: `fmt`, `allow`, `wrela::<package-path>::<test-name>`, or a group ending in `::`. Unknown, ambiguous or empty selections fail.
- Passing checks does not prove prolonged stability, whole-scene quality or reference-machine frame rates. Keep extended captures and benchmarks outside checks.

## Records

- `AGENTS.md` is the sole repo instruction file. Add no prose docs beyond `docs/vision.md`, `docs/process.md` and each game's `design.md` (its brief: what Ryan decided, what is proposed and what is open): reference docs come from source through `wrela doc --export`; remove any temporary task notes before committing. Run checks locally. CI only builds and deploys releases from `main` (`.github/workflows/deploy.yml`, with the pinned toolchain); add no checks to it.
- Update `docs/process.md` for new references, outside assets/algorithms, AI tools/models or kinds of AI work: source, rights, what it teaches and how used. Keep style references outside the repo. Never trace/sample/recreate specific reference shots, characters or designs; never create assets with generative image/audio/music/voice/video models.
- Use clear, concrete English, roughly 80% ASD-STE100.
