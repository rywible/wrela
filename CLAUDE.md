wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

docs/language.md has the current language spec. It is about 50k tokens: find a section with `grep -n '^#' docs/language.md` and read only that section.
docs/vision.md has the vision for the project. Its section "The lore" is the flagship's canonical lore: read it before you write anything that touches the story, and add no lore the owner hasn't stated
docs/process.md is the public record of how wrela is made: what AI does and what people do, and every outside work used

In wrela code, prefer the compiler's tools to grep: `tools/wrela context <item> <package>`, `tools/wrela query`, `tools/wrela doc`, `tools/wrela primer <area>` (language.md §22). This wrapper builds the current checkout's CLI inside the shared job queue. When a tool can't answer your question, note it: that gap is a finding for the studio.

Don't add any more docs besides the grammar, the lexical spec and docs/process.md. Don't add CI; the one exception is the flagship's separate leaderboard repository, which verifies replays in GitHub Actions. Work fully locally.

Checks: one command, no tiers or flags.

- Run `tools/check.sh <check-name>` while editing; it selects exactly one test and reuses the same workspace build as the suite. Rust names are `module::test` (or `crate::target::module::test` when ambiguous), Python names `tools::test_module.Class.test_method`, browser names `browser::file.test.ts::full test name`, Wrela names `wrela::<package-path>::<test-name>` (use `compiler/std` for std). Unknown and ambiguous names fail.
- Run `tools/check.sh` before a commit or merge. It runs every retained test, including real GPU shaders and small Chrome agreement scenarios, plus formatting, clippy, doc tests, browser types/dist/size and generated shader budgets. Both forms enter the shared job queue and reject success if sources changed during verification.
- Named infrastructure checks are `rustfmt`, `wrela-fmt`, `clippy`, `doc`, `tools`, `browser`, `browser-types`, `browser-dist`, `browser-size`, and `wgsl`. Successful test results are never reused; compiled artifacts are cached.
- New regressions use small fixtures, explicit resource lifecycle assertions and bounded seeded samples. No test tiers, hidden skips, full-scene bakes, extended camera tours or wall-clock benchmarking are part of the check contract. A passing check does not establish prolonged stability or reference-machine frame rates.
- Keep real shaders for pixel/buffer assertions; use CPU hosts for command/state assertions. Cold Rust builds, software rendering and waiting for another session take longer than a warm check.
- Format through the normal formatters when a formatting check fails. Hooks do not edit sources or run extra checks before this command.
- If admission is not immediate, use the agent's background mechanism and continue editing. To inspect progress, use `python3 tools/jobs.py status`; do not poll with sleeping loops.

Ten local coding sessions share one machine, with these rules:

- One costly job runs at a time, across checkouts. Use `python3 tools/jobs.py run build -- <command>` for a build or bake; `run capture` for a render; `run measure` for a timing run. `tools/wrela`, check.sh, cloud warming, lookbook.py and motion.py enter this queue automatically. Wrap standalone headless.py calls with `run capture`. Reading and editing do not need admission.
- Short checks and captures go first. Waiting work ages into priority. Each check holds one reservation; a running check finishes before another job starts. Measurements reserve the whole queue.
- Ordinary jobs default to at most four Cargo compile jobs (bounded by the CPU quota), one Rust test executable, two test threads and two default runtime/compiler workers, at a lower CPU priority. Do not override these defaults to run extra compilation or tests. Explicit test worker counts still test the requested concurrency. Across native hosts and Chrome, at most two small GPU holders run; large renders and measurements take the GPU alone. These are cooperative limits, not OS quotas on arbitrary commands or unrelated apps. Older running checkouts must finish before their sessions adopt the new tools.
- Queue and GPU permits live in `~/.cache/wrela/resources`, independent of TMPDIR. Tests may isolate them with an absolute `WRELA_RESOURCE_DIR`; sessions must share the default. `tools/jobs.py status` shows live jobs; `cancel <id>` cancels one and stops its owned process tree. Timeouts, sustained critical memory pressure and less than 4 GiB disk free stop an owned job with failure. Warning pressure admits focused checks, builds and gates with one compiler/CPU/test worker; renders, long work and warming wait. Critical pressure pauses all admissions. Review job results; an interrupted job is not a pass. Verification rejects success if its source inputs changed during the run.
- Create a session with `python3 tools/worktrees.py create <name>` from main (or `--ref <ref>` to continue a specified commit). It creates `codex/<name>` under the sibling `.wrela-sessions/` directory and copies no build output or node_modules. At most ten managed session trees are allowed. Do not duplicate an existing target directory.
- Admitted jobs share Cargo, native-code and constant caches under the common Git directory's `wrela/cache/`; tests still use checkout-local scratch and private constants when they count builds. Explicit custom cache paths are for deliberate isolated runs. Install browser dependencies only where needed; Bun's download cache already spans worktrees. Separate checkouts and scratch isolate sources and test artifacts; sharing a target is safe here because whole jobs serialize and rebuild the selected checkout's binaries.
- `python3 tools/worktrees.py status` reports cache size, existing targets and free space. Admission retains 12 GiB free, and shared caches have a 20 GiB soft ceiling. Maintenance removes only abandoned owned scratch after six hours, content-keyed cache files older than seven days and disposable incremental state when needed. `prune --build-cache` explicitly clears the shared Cargo cache under admission; it costs a cold rebuild. Existing targets and captures are preserved. `remove <name>` refuses dirty trees, unmerged commits, queued jobs and ignored outputs that need preservation; it retains the branch.

Architecture: docs/vision.md's "Architecture" gives the layers and the constraints. Where each layer lives:

```
0 hosts     runtime/native (Rust: wgpu + wasmtime), runtime/browser (TS), runtime/abi (shared with the compiler)
1 language  compiler/ (Rust: syntax → sema → ir → lower → wasm, wgsl; driver; cli = `wrela`), compiler/std (wrela)
2 engine    engine/ (wrela): no game's names, places or tuning
3 studio    studio/, ui/ (wrela), tools/ (scripts, the gate)
4 games     examples/<game>/ (wrela): depends on engine/, never the other way
```

Code health, with many agents merging at once:

- Each PR says what it changes in a frame's time (`wrela estimate examples/last-green`: each system against its slice of the 16.7 ms frame, #55 AC8) and in the gate's time. If the change can't touch either, say so in one line.
- Delete an experiment that lost. Don't keep it behind a flag, a mode number or an unused constant.
- New logic ships with a focused test: a placement, a layout, a camera rule, a bake or a sim rule, tested on a small case, not only by a long run of the whole floor. A change only to the look is judged by the lookbook instead.
- A helper that two places need lives once, in engine/ or std, not as a copy.
- A spike stays on its branch unless it wins.

Work on one trunk, `main`. Branch off the latest main with tools/worktrees.py, keep the branch short, and squash-merge it into main yourself as soon as its change passes `tools/check.sh` on the branch, and squash only. If main changes after verification, merge it into the branch and run the same `tools/check.sh` on the combined sources before merging. This catches changes that pass separately but break together. Changes to check infrastructure also require the gate and focused tests of its failure behavior. There are no milestone branches (retired 2026-10-11; the branch `milestone-6` stays, unchanged, as M6's archive).

If you're doing a long running job, store tasks durably in an md file for reference across turns, but delete it before you merge in your change to main.

Go 80% of the way on ASD-STE100 when communicating technical details (and strive for clear english in all communication)

If a diagram is a better way to communicate an idea, use a diagram.

Keep docs/process.md true, in the same change: when you use a reference (an image, a photo, a recording, a score, a model, a dataset), add its row (source, rights, what it teaches, how you used it); the same for a new outside asset or algorithm, a new AI tool or model, or AI doing a new kind of work. A style reference stays out of the repo; only its row goes in. Never trace, sample or recreate a specific shot, character or design from a reference, and never make an asset with a generative image, audio, music, voice or video model.
