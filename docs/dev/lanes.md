# Repo Workflow Lanes

Phase 52 keeps `just` as the repo front door, turns incremental builds on by default, and adds
explicit cleanroom escape hatches.

Use the surfaces like this:

- `just`: named repo workflows and closure gates
- `cargo`: low-level Rust substrate and escape hatch
- `wrela`: authored-world and product-facing workflows

## Lane Vocabulary

- `fast`: the default repo verification lane
- `full`: the full semantic repo verification lane
- `check-clean`: cleanroom workspace typecheck
- `test-clean`: cleanroom workspace Rust test pass
- `perf-smoke`: the cheap perf sanity lane
- `perf-closure`: the representative whole-frame closure lane
- `ship`: the local pre-ship gate

## Canonical Recipes

- `just check`
  Runs `cargo check --workspace`.

- `just build`
  Runs `cargo build --workspace`.

- `just check-clean`
  Runs the cleanroom workspace typecheck:
  `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/check cargo check --workspace`

- `just test`
  Repo fast lane.
  It composes the Rust smoke harness:
  `cargo test -p wrela --test repo_smoke`
  plus the native authored fast lane:
  `cargo run -p wrela -- test language/spec --lane=fast`
  The Rust smoke harness touches parsing/frontend, type checking/lowering, query execution,
  presentation planning, collision execution, CLI smoke, and benchmark/perf manifest loading
  once each without pretending to replace the broader workspace lane.

- `just test-all`
  Repo full lane.
  It composes the full Rust workspace verification lane:
  `cargo test --workspace`
  plus the native authored full lane:
  `cargo run -p wrela -- test language/spec --lane=full`

- `just test-clean`
  Runs the cleanroom workspace Rust test pass:
  `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/test cargo test --workspace`

- `just test-cli`
  Runs `cargo test -p wrela --test cli`.

- `just test-query`
  Runs the focused Rust query-planning and query-contract lane:
  `cargo test -p wrela --test query_contract_registry --test query_program_spine --test phase9_query_plan`

- `just perf-smoke`
  Runs the cheap perf sanity lane:
  `cargo run -p wrela -- perf benchmarks/micro --profile=smoke --runs=1`

- `just perf-closure`
  Runs the representative whole-frame closure lane:
  `cargo run -p wrela -- perf benchmarks/whole_frame --profile=1080p120 --query-backend=wgsl`

- `just ship`
  Runs `fmt-check`, `lint`, `test`, `test-all`, and `perf-smoke` in that order.
  This is the authoritative pre-ship repo lane even if later phases still tighten what it proves.

- `just baseline-devloop`
  Writes the developer-loop report under `.artifacts/devloop/`.
  The Phase 52 report includes:
  warm-vs-cleanroom comparisons, per-context compile bursts, and the CLI split assessment.
  The fast lane still carries an explicit `60000` ms budget and records misses as `missed_budget`
  instead of normalizing them away.

## Boundary Rules

- Use `cargo test` when you are proving Rust units, Rust integration crates, or internal harnesses.
- Use `wrela test` when you are proving authored `.wr` projects and the native Wrela test-runner semantics.
- Use `just` when you want the repo-approved lane name and you do not want to decide which lower-level commands to compose.

That means:

- a Rust-only question such as "does the CLI integration crate still pass?" can use `cargo test -p wrela --test cli`
- an authored-world question such as "does the executable spec project still run?" uses `cargo run -p wrela -- test language/spec --lane=fast`
- a repo question such as "is the fast lane green?" uses `just test`

## Human-Plus-Agent Workflow Contract

- Start by mapping the touched files to the named contexts in
  `../architecture/contexts.md`.
- Use one canonical command per workflow.
  Prefer `just` for repo lanes, `cargo` for Rust-internal escape hatches, and
  `wrela` for authored-world workflows.
- Prefer machine-readable output when it helps automation or verification.
  Examples in this repo include `--json`, `--json-report`, and the devloop
  reports written under `.artifacts/devloop/`.
- Slice work so ownership and proof stay explicit.
  A good task names the touched context, the owned files or module roots, and
  the lane or tests that will prove the change.
- When a dense module root is touched in later phases, its header should explain
  what it owns, what it does not own, its primary entrypoints, and the key
  invariants or "why" notes a contributor needs.
- Closure still requires an independent final review after the proving lane is
  green.

## Notes

- The `wrela test` surface accepts both preset aliases and legacy lanes.
  `fast` means `spec + default`, `full` means all lanes, and the legacy names
  (`spec`, `integration`, `sim`, `model`, `default`) remain valid for narrower targeting.
- If `just` is not installed yet, you can still use the resolved `cargo` and `wrela` commands above as a temporary escape hatch.
  The repo contract remains `just` first.
