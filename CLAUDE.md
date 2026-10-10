wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

docs/language.md has the current language spec. It is about 50k tokens: find a section with `grep -n '^#' docs/language.md` and read only that section.
docs/vision.md has the vision for the project

In wrela code, prefer the compiler's tools to grep: `wrela context <item> <package>`, `wrela query`, `wrela doc`, `wrela primer <area>` (target/release/wrela; language.md §22). When a tool can't answer your question, note it: that gap is a finding for the studio.

Don't add any more docs besides the grammar and the lexical spec. Don't add CI; the one exception is the flagship's separate leaderboard repository, which verifies replays in GitHub Actions (#46). Work fully locally.

Checks (tools/check.sh; its header says what each tier runs).
- While you work, run only the tests you touch: `tools/check.sh <filter>` takes seconds. To see a test's output, add `--nocapture`. Run a gate or long test only through check.sh: a test run with `cargo test` takes the GPU alone, and every other session's checks wait for it.
- Before each commit, run the gate: `tools/check.sh`, under a minute.
- Before a branch is merged, run `tools/check.sh --long` (about ten minutes). If a long test fails, run that test again (`tools/check.sh --long <name>`), not the tier.
- The `measure:` tests and `tools/check.sh --full` give a milestone's numbers. Run a measurement only when you need its number, alone (`cargo test --release --workspace -- --ignored --exact <name>`).
- A new test that takes more than a second is a long check (`#[ignore = "long: ..."]`), sized with `sized()` so that the long tier stays near ten minutes.
- Run the gate and filtered runs in the foreground. Run anything longer (the long tier, a measurement, a bake) in the background and keep working. If you need its result before you can go on, wait with Monitor, or end your turn: a background command wakes you when it exits.
- Hooks (tools/hooks.py) format what the gate checks before it runs, and refuse polling with `sleep` and a `cargo test` that takes the GPU alone. A refusal says what to do instead.

Full milestones with AC are stored in github. When you're working on a milestone, you can durably keep track of your tasks in a doc in the repo if you need to break it down (name it `M<n>-TASKS.md`: a hook names it at each session's start, and gives it again after a compaction), just delete the file once the milestone is complete.

Go 80% of the way on ASD-STE100 when communicating technical details (and strive for clear english in all communication)

If a diagram is a better way to communicate an idea, use a diagram.
