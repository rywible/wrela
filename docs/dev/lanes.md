# Repo Workflow Lanes

Phase 49 makes `just` the repo front door.

Use the surfaces like this:

- `just`: named repo workflows and closure gates
- `cargo`: low-level Rust substrate and escape hatch
- `wrela`: authored-world and product-facing workflows

## Lane Vocabulary

- `fast`: the default repo verification lane
- `full`: the full semantic repo verification lane
- `perf-smoke`: the cheap perf sanity lane
- `perf-closure`: the representative whole-frame closure lane
- `ship`: the local pre-ship gate

## Canonical Recipes

- `just check`
  Runs `cargo check --workspace`.

- `just build`
  Runs `cargo build --workspace`.

- `just test`
  Repo fast lane.
  It composes a small Rust integrity lane:
  `cargo test -p wrela --test one_shot_metrics_harness --test spec_project_integrity --test thin_core_snapshot`
  plus the authored executable spec lane:
  `cargo run -p wrela -- test language/spec --lane=spec`

- `just test-all`
  Repo full lane.
  It composes the full Rust workspace verification lane:
  `cargo test --workspace`
  plus the authored spec project:
  `cargo run -p wrela -- test language/spec`

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

## Boundary Rules

- Use `cargo test` when you are proving Rust units, Rust integration crates, or internal harnesses.
- Use `wrela test` when you are proving authored `.wr` projects and the native Wrela test-runner semantics.
- Use `just` when you want the repo-approved lane name and you do not want to decide which lower-level commands to compose.

That means:

- a Rust-only question such as "does the CLI integration crate still pass?" can use `cargo test -p wrela --test cli`
- an authored-world question such as "does the executable spec project still run?" uses `cargo run -p wrela -- test language/spec --lane=spec`
- a repo question such as "is the fast lane green?" uses `just test`

## Notes

- The `wrela test` surface still uses its native lane vocabulary (`spec`, `integration`, `sim`, `model`, `default`).
  The repo-level `fast` and `full` names live at the `just` layer and may compose multiple lower-level proof surfaces.
- If `just` is not installed yet, you can still use the resolved `cargo` and `wrela` commands above as a temporary escape hatch.
  The repo contract remains `just` first.
