wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

docs/language.md has the current language spec
docs/vision.md has the vision for the project. Its section "The lore" is the flagship's canonical lore: read it before you write anything that touches the story, and add no lore the owner hasn't stated
docs/process.md is the public record of how wrela is made: what AI does and what people do, and every outside work used

Don't add any more docs besides the grammar, the lexical spec and docs/process.md. Don't add CI; the one exception is the flagship's separate leaderboard repository, which verifies replays in GitHub Actions (#46). Work fully locally.

Checks (tools/check.sh; its header says what each tier runs). Never wait on a check with sleep or polling: run it in the background, keep working, and read its result when it ends.
- While you work, run only the tests you touch: `tools/check.sh <filter>` takes seconds.
- Before each commit, run the gate: `tools/check.sh`, under a minute.
- Before a branch is merged, run `tools/check.sh --long` (about ten minutes) in the background. If a long test fails, run that test again (`tools/check.sh --long <name>`), not the tier.
- The `measure:` tests and `tools/check.sh --full` give a milestone's numbers. Run a measurement only when you need its number, alone, in the background.
- A new test that takes more than a second is a long check (`#[ignore = "long: ..."]`), sized with `sized()` so that the long tier stays near ten minutes.

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

Full milestones with AC are stored in github. When you're working on a milestone, you can durably keep track of your tasks in a doc in the repo if you need to break it down, just delete the file once the milestone is complete.

Go 80% of the way on ASD-STE100 when communicating technical details (and strive for clear english in all communication)

If a diagram is a better way to communicate an idea, use a diagram.

Keep docs/process.md true, in the same change: when you use a reference (an image, a photo, a recording, a score, a model, a dataset), add its row (source, rights, what it teaches, how you used it); the same for a new outside asset or algorithm, a new AI tool or model, or AI doing a new kind of work. A style reference stays out of the repo; only its row goes in. Never trace, sample or recreate a specific shot, character or design from a reference, and never make an asset with a generative image, audio, music, voice or video model.
