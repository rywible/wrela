wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

docs/language.md has the current language spec
docs/vision.md has the vision for the project

Don't add any more docs besides the grammar and the lexical spec. Don't add CI; the one exception is the flagship's separate leaderboard repository, which verifies replays in GitHub Actions (#46). Work fully locally.

Checks (tools/check.sh; its header says what each tier runs). Never wait on a check with sleep or polling: run it in the background, keep working, and read its result when it ends.
- While you work, run only the tests you touch: `tools/check.sh <filter>` takes seconds.
- Before each commit, run the gate: `tools/check.sh`, under a minute.
- Before a branch is merged, run `tools/check.sh --long` (about ten minutes) in the background. If a long test fails, run that test again (`tools/check.sh --long <name>`), not the tier.
- The `measure:` tests and `tools/check.sh --full` give a milestone's numbers. Run a measurement only when you need its number, alone, in the background.
- A new test that takes more than a second is a long check (`#[ignore = "long: ..."]`), sized with `sized()` so that the long tier stays near ten minutes.

Full milestones with AC are stored in github. When you're working on a milestone, you can durably keep track of your tasks in a doc in the repo if you need to break it down, just delete the file once the milestone is complete.

Go 80% of the way on ASD-STE100 when communicating technical details (and strive for clear english in all communication)

If a diagram is a better way to communicate an idea, use a diagram.
