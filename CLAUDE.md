# wrela: guide for agents

wrela is a new language, compiler, engine and agent-native studio for AAA-ambition games played from a browser link. Content is authored as fields: functions over space. It's MIT-licensed and built in spare time, but run with business-grade discipline: high code quality and honest engineering claims.

## Where things are

| Path               | What                                                                                                                                               |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/vision.md`   | Goal, theses and their evidence, architecture, the renderer, and the **constraints** everything rests on. Read it before any architectural change. |
| `docs/language.md` | The language reference, with feature tiers. The syntax is imagined until the parser exists.                                                        |
| `spec/`            | The grammar: normative and executable (from M1).                                                                                                   |
| `compiler/crates/` | The compiler: `wrela-diag` (spans, codes and their `explain/` texts, JSON), `-syntax`, `-driver` (queries), `-cli` (`wrela`), `-test`.             |
| `compiler/tests/`  | `conformance/` (`//~ ERROR E0001` on the line, `//~^` the line above, `//@ check-pass`) and `golden/` (`WRELA_BLESS=1` rewrites).                  |
| `std/`, `runtime/` | The stdlib and the runtimes (from M1).                                                                                                             |
| `tools/`           | `serve.py` (static server that accepts PUTs into `results/`) and `headless.sh` (runs a page in headless Chrome on the real GPU).                   |

- **Checks, as CI runs them:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, `cargo run -p wrela-test --release --locked --bin fuzz-smoke -- --seconds 60`, `bun run check` and `bun test` in `runtime/browser`.
- **Plans live in GitHub issues on rywible/wrela, not in the repo.**
  - Milestones M1–M6, each with a scope issue.
  - #26: status against the vision (pinned).
  - #31: the vision backlog.
- **History:** the design record (decisions D-001–D-105, sketches, reviews), spikes 01–12 and experiments are in the git tag `design-archive-2026-10`. D-NNN IDs in the docs refer to its `docs/design/decisions.md`.

## Rules

- **The compiler knows nothing about the engine.**
  - No keywords, attributes, lang items or compiler rules phrased in engine terms.
  - The test for any language feature: would it make sense in a wrela program that isn't a game?
  - General sugar is fine.
- **The constraints in `docs/vision.md` are load-bearing.** Only the owner changes one; record the change and its reason in the commit and in vision.md.
- **Keep docs to the minimum.**
  - No new prose docs or plan files in the repo.
  - The "why" of a change goes in its commit message or PR. Plans go in issues.
  - Prefer knowledge in executable form: tests, diagnostics with `wrela explain` texts, the grammar.
  - When something is built, replace the prose describing it with a pointer to its code and tests, in the same change.
- **Label performance numbers as estimates until they're measured.** Label untested claims as hypotheses.
- **Source files use `.wrela`;** docs use ` ```wrela ` fences, which the tests check. ` ```wrela imagined ` marks syntax ahead of the compiler; that count only falls.
- **GPU safety:**
  - Run GPU pages only through `tools/headless.sh`. It holds a lock, so only one GPU user runs at a time.
  - Keep every GPU submission under ~100 ms.
  - Concurrent heavy GPU work once starved WindowServer and reset the owner's desktop.

## How work flows: milestones, slices, PRs

**Milestones**

- Each milestone has a scope issue: outcome, in scope, and out of scope with where each item goes.
- **Nothing leaves a milestone** without landing in a later milestone or the backlog (#31).
- A milestone is broken into slice issues, each with acceptance criteria, only when it's next.

**One PR per slice**

- It says "Closes #N", carries the acceptance checklist, and names the decisions it touches.
- Never a PR per task; never one PR per milestone.
- Split a slice that grows past ~30 reviewable files.

**Branches and worktrees**

- `slice/mN-sNN` is cut from `main`, in its own worktree.
- Task agents merge into the slice branch locally, and the tests pass before a PR exists.

**Opening the PR**

- Open it as a **draft**: CodeRabbit and Greptile skip drafts, and CI still runs.
- Before marking it ready, run a fresh-context code review.
- Both bots review on their own when a PR is marked ready (seen on #33); later pushes need the re-trigger.

**Don't wait on review.** Start the next slice.

- A dependent slice branches from its parent's tip locally.
- After the parent merges, rebase onto `main` and open its PR.
- No GitHub stacked PRs: both bots skip PRs whose base isn't `main`.

**A shepherd agent handles bot findings**

- Fix real bugs, each with a regression test.
- Reject wrong findings, with the reason in a reply.
- Defer the rest to a `bot-followup` issue.
- Push the fixes in one batch, then re-trigger (`@coderabbitai review`, `@greptileai`). After two rounds, label it `needs-owner`.

**Merging**

- Squash, with auto-merge, once the gate passes (`tools/merge-gate.sh <pr>` checks it):
  - CI is green
  - both bots reviewed the head commit (not skipped or rate-limited)
  - all threads are resolved
  - no `needs-owner` label
- Never force-push a PR that's ready for review.

## Layers

| Layer | What                                                      | Written in                                                |
| ----- | --------------------------------------------------------- | --------------------------------------------------------- |
| 0     | Platform hosts: the browser runtime and the native host   | TypeScript, Rust                                          |
| 1     | Language: the compiler (run at build time) and the stdlib | Rust (compiler), wrela (stdlib, with a small unsafe core) |
| 2     | Engine                                                    | wrela                                                     |
| 3     | Studio                                                    | wrela + Rust tooling                                      |
| 4     | Games                                                     | wrela                                                     |
