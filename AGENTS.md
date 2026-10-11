# wrela agent instructions

MIT-licensed language, compiler, engine and agent-native studio for browser games with AAA ambition. Content is fields: functions over space. Ship quality code and evidence-backed claims.

## Dev loop

- Own implementation, verification and local squash merge. Routine work has standing permission; do not stop at a plan or request routine approval. Ryan owns goals, architecture, lore and artistic direction; decide implementation details yourself.
- Ship the smallest complete change. Avoid unrelated cleanup, speculative abstractions and compatibility scaffolding. Preserve other sessions' work.

1. Inspect the checkout and latest local `main`. Run `python3 tools/worktrees.py create <name>` (or `--ref <ref>` for specified work); use the returned checkout and `codex/<name>`. Never implement directly on `main`.
2. Edit, add a focused regression for new logic, run `tools/check.sh <check-name>`, and fix failures at their cause. Batch independent reads; background queued jobs and continue independent work.
3. Review the diff; delete temporary task notes and losing experiments. Run `tools/check.sh` before committing; freeze sources during verification. Later source changes require another full pass before commit/merge.
4. Confirm `main` is clean and still at the commit incorporated in the verified branch. If it moved, merge current `main` into the branch and rerun `tools/check.sh`. Squash-merge into `main` yourself as soon as it passes. Keep branches short; no milestone branches. Preserve `milestone-6` unchanged.
5. After merging, return to the `main` checkout. Preserve needed outputs outside the session, discard only its disposable caches, then run `python3 tools/worktrees.py remove <name>` to delete both worktree and local branch. Cleanup is part of completion; report any blocker, never force removal.

- Long work: keep decisions, remaining tasks and checks in checkout-local `TASKS.md`; delete before final verification.
- Each PR/completion report gives changes, checks, commit/blockers, and frame-time/gate-time impact (explicitly say when neither changes). For frame costs, measure on the M4 with `tools/wrela profile examples/last-green --json`, compare against a baseline from the same machine and mode, and judge each system against its share of 16.7 ms. Distinguish GPU timings, CPU frame calls, and Chrome frame rates.

## Context and layers

- `docs/language.md`: current spec (~50k tokens). Find headings with `rg -n '^#' docs/language.md`; read only relevant sections.
- `docs/vision.md`: goals/architecture. Read "The lore" before story work; add no lore Ryan has not stated.
- `docs/process.md`: public record of human/AI work and outside sources.
- For Wrela, prefer `tools/wrela context <item> <package>`, `tools/wrela query`, `tools/wrela doc`, `tools/wrela primer <area>` (spec §22); the wrapper queues this checkout's CLI build. Fall back to `rg` when needed; report tool gaps as studio findings.

| Layer | Location and constraint |
|---|---|
| Hosts | `runtime/native` (Rust: wgpu/wasmtime), `runtime/browser` (TS), `runtime/abi` (shared with compiler); mechanisms here, policy in Wrela |
| Language | `compiler/` (Rust: syntax → sema → IR → lower → WASM/WGSL, driver/CLI), `compiler/std` (Wrela); compiler knows no engine |
| Engine | `engine/` (Wrela); no game names, places or tuning |
| Studio | `studio/`, `ui/` (Wrela), `tools/` (scripts/gate) |
| Games | `examples/<game>/` (Wrela); depend on engine, never the reverse |

## Quality

- Language features must make sense outside games. Shared Wrela helpers live once in engine/std, in the appropriate layer.
- Test new logic on small fixtures: placement/layout/camera/bake/sim rules directly, bounded seeded samples, explicit resource lifecycle assertions. Judge changes only to the look with the lookbook.
- Real shaders for pixel/buffer assertions; CPU hosts for command/state assertions.
- Delete losing experiments, dead code and unused constants; no fallback flags or mode numbers. Merge spikes only if they win.
- Never weaken assertions, skip tests or bypass admission for a pass. Check infrastructure changes also need focused failure regressions.

## Checks

One command; no tiers or flags. Both forms queue automatically and reject passes if sources change. Cache compiled artifacts, never test passes.

- `tools/check.sh <check-name>`: exactly one test or infrastructure check; unknown/ambiguous names fail.
- `tools/check.sh`: every retained test, real GPU shaders, small Chrome agreement scenarios, formatting, clippy, doc tests, browser types/dist/size, generated shader budgets.

| Test | Exact name |
|---|---|
| Rust | `module::test` or `crate::target::module::test` when ambiguous |
| Python | `tools::test_module.Class.test_method` |
| Browser | `browser::file.test.ts::full test name` |
| Wrela | `wrela::<package-path>::<test-name>`; std: `compiler/std` |

Infrastructure: `rustfmt`, `wrela-fmt`, `clippy`, `doc`, `tools`, `browser`, `browser-types`, `browser-dist`, `browser-size`, `wgsl`. Use normal formatters; hooks never edit sources or run extra checks.

No hidden skips, full-scene bakes, extended camera tours or wall-clock benchmarks in checks. Passing does not prove prolonged stability, whole-scene quality or reference-machine frame rates. Allow for cold builds, software rendering and queue waits.

## Shared machine

Ten sessions; one costly job across checkouts. Reading/editing need no admission.

- Other builds/bakes: `python3 tools/jobs.py run build -- <command>`; renders: `run capture`; timings: `run measure`. `tools/wrela`, `check.sh`, cloud warming, `lookbook.py`, `motion.py` queue themselves. Wrap standalone `headless.py` with `run capture`; never bypass admission with raw Cargo.
- Background queue waits; inspect with `python3 tools/jobs.py status`, never sleep-poll. Checks retain admission through completion; measurements reserve the queue. Interrupted jobs fail. Cancel only your jobs with `python3 tools/jobs.py cancel <id>`.
- Keep defaults: ≤4 Cargo jobs within CPU quota, one Rust test executable, two test threads, two compiler/runtime workers at lower priority. Explicit concurrency tests retain requested counts. At most two small GPU holders; large renders/measurements take the GPU alone. Let older running jobs finish before adopting new tools.
- Share `~/.cache/wrela/resources`; only isolated tests may set absolute `WRELA_RESOURCE_DIR`. Never bypass memory/disk admission.
- Share Cargo/native/constant caches in the common Git directory's `wrela/cache/`; test scratch stays checkout-local, with private constants when counting builds. No copied build output/`node_modules`, duplicate targets or custom cache paths except deliberate isolated runs. Install browser dependencies only where needed; Bun shares downloads.
- `python3 tools/worktrees.py status`: space/caches (12 GiB admission reserve, 20 GiB cache ceiling). `prune --build-cache`: explicit cold reset only. Preserve captures/unknown output. `remove <name>` deletes merged worktrees and local branches, including squash merges; refuses dirty/unmerged work, pending jobs, switched branches and ignored output needing preservation. Never force removal.

## Records and communication

- `AGENTS.md` is the sole repo instruction file. No new docs except grammar, lexical spec, `docs/process.md` and temporary task notes above. Work locally; no CI here (the separate flagship leaderboard repo may verify replays in GitHub Actions).
- Update `docs/process.md` in the same change for new references, outside assets/algorithms, AI tools/models or kinds of AI work: source, rights, what it teaches, how used. Style references stay outside the repo. Never trace/sample/recreate specific reference shots, characters or designs; never create assets with generative image/audio/music/voice/video models.
- Clear English, roughly 80% ASD-STE100. Terse, concrete updates; diagrams when clearer.
