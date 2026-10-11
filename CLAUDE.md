wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

docs/language.md has the current language spec. It is about 50k tokens: find a section with `grep -n '^#' docs/language.md` and read only that section.
docs/vision.md has the vision for the project. Its section "The lore" is the flagship's canonical lore: read it before you write anything that touches the story, and add no lore the owner hasn't stated
docs/process.md is the public record of how wrela is made: what AI does and what people do, and every outside work used

In wrela code, prefer the compiler's tools to grep: `wrela context <item> <package>`, `wrela query`, `wrela doc`, `wrela primer <area>` (target/release/wrela; language.md §22). When a tool can't answer your question, note it: that gap is a finding for the studio.

Don't add any more docs besides the grammar, the lexical spec and docs/process.md. Don't add CI; the one exception is the flagship's separate leaderboard repository, which verifies replays in GitHub Actions (#46). Work fully locally.

Checks (tools/check.sh; its header says what each tier runs).
- While you work, run only the tests you touch: `tools/check.sh <filter>`. Use `--package=wrela-syntax` (repeat for more crates) to build only selected crates and their dependencies; filters are optional. This focused check does not replace the merge gate. To see a test's output, add `--nocapture`. Run a gate or long test only through check.sh: a test run with `cargo test` takes the GPU alone, and every other session's checks wait for it.
- Before each commit, run the gate: `tools/check.sh`. Cold rendering and compilation can take minutes. Set `WRELA_SOFTWARE_GPU=1` for software Vulkan: hardware timing budgets are reported there, and tests marked `heavy:` run in the long tier. Run a heavy test with `tools/check.sh --long <name>` when your change reaches it.
- On Linux with `strace` and mount privileges, the optional memory of slow tests can reuse passes (tools/memo.py). `tools/check.sh --fresh` runs every test in the selected tier; unavailable tracing is reported and never skips tests. Use CPU hosts for command/state assertions and real shaders for pixel/buffer assertions.
- Before a branch is merged, run `tools/check.sh --long`. If a long test fails, run that test again (`tools/check.sh --long <name>`), not the tier. Linux runs native/browser agreement; Rosetta cross-architecture evidence requires macOS. Hardware timing measurements run with `--full` on the reference machine.
- The `measure:` tests and `tools/check.sh --full` give the numbers behind a claim. Run a measurement only when you need its number, alone (`cargo test --release --workspace -- --ignored --exact <name>`).
- A new test that takes more than a second is a long check (`#[ignore = "long: ..."]`), sized with `sized()` so that the long tier stays near ten minutes.
- Run the gate and filtered runs in the foreground. Run anything longer (the long tier, a measurement, a bake) in the background and keep working. If you need its result before you can go on, wait with Monitor, or end your turn: a background command wakes you when it exits.
- Hooks (tools/hooks.py) format what the gate checks before it runs, and refuse polling with `sleep` and a `cargo test` that takes the GPU alone. A refusal says what to do instead.

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

Work on one trunk, `main`. Branch off the latest main, keep the branch short, and squash-merge it into main yourself as soon as its change passes `tools/check.sh` on the branch, and squash only. Merge main into the branch only when it conflicts with the change, when it brings something the change needs, or to get a speedup of the checks; don't merge unrelated updates or run the gate again for them. Right before the squash-merge, run `tools/premerge.sh`: it builds the branch merged with the latest main (`cargo check` if the branch changed Rust, and `wrela check` on the packages that read what the branch changed), which catches two clean merges that break together. A change that only makes the checks faster merges without the gate: the long tier that runs on main every 6 hours is its safety net. There are no milestone branches (retired 2026-10-11; the branch `milestone-6` stays, unchanged, as M6's archive).

TASKS.md has the open work: a short list of goals, not a log (a hook names it at each session's start, and gives it again after a compaction). Add a goal when the owner sets one; delete it when it's done. The GitHub issues keep each milestone's scope and criteria up to M6, and the backlog (#31).

Go 80% of the way on ASD-STE100 when communicating technical details (and strive for clear english in all communication)

If a diagram is a better way to communicate an idea, use a diagram.

Keep docs/process.md true, in the same change: when you use a reference (an image, a photo, a recording, a score, a model, a dataset), add its row (source, rights, what it teaches, how you used it); the same for a new outside asset or algorithm, a new AI tool or model, or AI doing a new kind of work. A style reference stays out of the repo; only its row goes in. Never trace, sample or recreate a specific shot, character or design from a reference, and never make an asset with a generative image, audio, music, voice or video model.
