wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

docs/language.md has the current language spec. It is about 50k tokens: find a section with `grep -n '^#' docs/language.md` and read only that section.
docs/vision.md has the vision for the project. Its section "The lore" is the flagship's canonical lore: read it before you write anything that touches the story, and add no lore the owner hasn't stated
docs/process.md is the public record of how wrela is made: what AI does and what people do, and every outside work used

In wrela code, prefer the compiler's tools to grep: `tools/wrela context <item> <package>`, `tools/wrela query`, `tools/wrela doc`, `tools/wrela primer <area>` (language.md §22). This wrapper builds the current checkout's CLI inside the shared job queue. When a tool can't answer your question, note it: that gap is a finding for the studio.

Don't add any more docs besides the grammar, the lexical spec and docs/process.md. Don't add CI; the one exception is the flagship's separate leaderboard repository, which verifies replays in GitHub Actions (#46). Work fully locally.

Checks (tools/check.sh; its header says what each tier runs).
- For early Rust feedback, use `python3 tools/jobs.py run focused -- cargo check -p <crate>`: type checking avoids release code generation during edits. Follow it with the affected tests and the final gate.
- While you work, run only the tests you touch: `tools/check.sh <filter>`. Use `--package=wrela-syntax` (repeat for more crates) to build only selected crates and their dependencies; filters are optional. This focused check does not replace the merge gate. To see a test's output, add `--nocapture`. check.sh automatically enters the machine's shared job queue.
- Before each commit, run the gate: `tools/check.sh`. Cold rendering and compilation can take minutes. Set `WRELA_SOFTWARE_GPU=1` for software Vulkan: hardware timing budgets are reported there, and tests marked `heavy:` run in the long tier. Run a heavy test with `tools/check.sh --long <name>` when your change reaches it.
- On Linux with `strace` and mount privileges, the optional memory of slow tests can reuse passes (tools/memo.py). `tools/check.sh --fresh` runs every test in the selected tier; unavailable tracing is reported and never skips tests. Use CPU hosts for command/state assertions and real shaders for pixel/buffer assertions.
- For CPU state in a program that also draws, use `wrela trace` or `wrela bisect` with `--no-gpu` (and optionally `--workers N`). This checks submitted commands without executing shaders; use the normal path when the state needs GPU readbacks. Studio field inspection (`parts`, `probe`, `ray`, `measure`, `spec`, literals and synchronous controls) uses this CPU path automatically unless a PNG is requested. Motion capture keeps its render history but saves only the frames requested.
- Before a branch is merged, run `tools/check.sh --long`. If a long test fails, run that test again (`tools/check.sh --long <name>`), not the tier. Linux runs native/browser agreement; Rosetta cross-architecture evidence requires macOS. Hardware timing measurements run with `--full` on the reference machine.
- The `measure:` tests and `tools/check.sh --full` give the numbers behind a claim. Run a measurement only when you need its number, through `python3 tools/jobs.py run measure -- cargo test --release --workspace -- --ignored --exact <name> --test-threads=1`. Full runs reserve the queue and use the machine's available CPU workers. Software renderers give correctness evidence; publish hardware timing claims only from the reference machine.
- A new test that takes more than a second is a long check (`#[ignore = "long: ..."]`), sized with `sized()` so that the long tier stays near ten minutes.
- If admission is not immediate, put the command in the agent's background mechanism and keep editing. Long tiers, measurements and bakes run in the background. If you need the result before you can go on, wait with Monitor, or end your turn: a background command wakes you when it exits.
- Hooks (tools/hooks.py) format what the gate checks before it runs, and refuse polling with `sleep` and raw costly Cargo commands. A refusal says what to do instead.

Ten local coding sessions share one machine, with these rules:
- One costly job runs at a time, across checkouts. Use `python3 tools/jobs.py run build -- <command>` for a build or bake; `run capture` for a render; `run measure` for a timing run. `tools/wrela`, check.sh, premerge.sh, cloud warming, lookbook.py and motion.py enter this queue automatically. Wrap standalone headless.py calls with `run capture`. Reading and editing do not need admission.
- Short checks and captures go first. Waiting work ages into priority. `check.sh --long` releases its reservation between each test and each extra stage; it keeps all the tier's cases and sizes. A running test finishes before another job starts. Measurements reserve the whole queue.
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
- Don't add an engine-wide abstraction (a type, pass or module that other systems must adopt) without the OK of the project's coordinating session or the owner.
- A spike stays on its branch unless it wins.

Work on one trunk, `main`. Branch off the latest main with tools/worktrees.py, keep the branch short, and squash-merge it into main yourself as soon as its change passes `tools/check.sh` on the branch, and squash only. Merge main into the branch only when it conflicts with the change, when it brings something the change needs, or to get a speedup of the checks; don't merge unrelated updates or run the gate again for them. Right before the squash-merge, run `tools/premerge.sh`: it builds the branch merged with the latest main (`cargo check` if the branch changed Rust, and `wrela check` on the packages that read what the branch changed), which catches two clean merges that break together. Changes to check infrastructure also require the gate and focused tests of its failure behavior. There are no milestone branches (retired 2026-10-11; the branch `milestone-6` stays, unchanged, as M6's archive).

TASKS.md has the open work: a short list of goals, not a log (a hook names it at each session's start, and gives it again after a compaction). Add a goal when the owner sets one; delete it when it's done. The GitHub issues keep each milestone's scope and criteria up to M6, and the backlog (#31).

Go 80% of the way on ASD-STE100 when communicating technical details (and strive for clear english in all communication)

If a diagram is a better way to communicate an idea, use a diagram.

Keep docs/process.md true, in the same change: when you use a reference (an image, a photo, a recording, a score, a model, a dataset), add its row (source, rights, what it teaches, how you used it); the same for a new outside asset or algorithm, a new AI tool or model, or AI doing a new kind of work. A style reference stays out of the repo; only its row goes in. Never trace, sample or recreate a specific shot, character or design from a reference, and never make an asset with a generative image, audio, music, voice or video model.
