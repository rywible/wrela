# Wrela

Wrela is a programming language for worlds: authored fields, scenes, materials, media, queries, and deterministic execution surfaces that can move from source to preview to native or GPU-backed runtimes.

The repo now focuses on the compiler, the core runtime, and the language libraries that support world construction. Older database, web-server, authentication, deployment, cluster, and distributed-storage surfaces have been removed.

## What Is Here

- `compiler/`: parser, HIR/MIR lowering, diagnostics, native codegen, previews, tests, and the `wrela` CLI.
- `runtime/`: value/runtime ABI, actor/reactor support, host filesystem/HTTP cassettes, metrics, and native runtime exports.
- `language/stdlib/`: thin-core standard library modules used by authored world programs.
- `language/spec/`: language snapshots, RFCs, and surface integrity references.
- `language/view_basic/`: minimal canonical `view` sample with typed viewport, quality, lighting, outputs, and history helpers.
- `benchmarks/micro/`: low-level benchmark scenarios.
- `benchmarks/field_engine/`: world/field-engine benchmark scenarios.

## Recommended Repo Workflow

The repo front door is `just`:

```bash
just check
just test
just test-all
just test-cli
just test-query
just perf-smoke
just perf-closure
just ship
just baseline-devloop
```

See [docs/dev/lanes.md](docs/dev/lanes.md) for the command-surface contract and
[docs/dev/devloop_playbook.md](docs/dev/devloop_playbook.md) for the Phase 49 developer-loop
measurement protocol and baseline-report workflow.

## Command Surface Boundary

- `just` is the repo workflow surface. Use it for named lanes such as `test`, `test-all`,
  `perf-smoke`, and `ship`.
- `cargo` is the low-level Rust substrate and escape hatch. Use it when you are working
  directly on Rust crates, build internals, or ad-hoc troubleshooting.
- `wrela` is the authored-world and product-facing surface. Use it for authored project
  workflows such as `preview`, `frame`, `perf`, and `test`.

Repo lanes are allowed to compose both Rust and authored-world proof surfaces. In particular,
`just test` runs a small Rust integrity lane plus `wrela test` over `language/spec`, while
`just test-all` keeps the full Rust workspace verification lane and the authored spec project
in the same front-door workflow.

## Wrela CLI Examples

When you need the product-facing surface directly from source, use the `wrela` CLI via
`cargo run -p wrela -- ...`:

```bash
cargo run -p wrela -- --help
cargo run -p wrela -- query-contracts
cargo run -p wrela -- build path/to/main.wr
cargo run -p wrela -- frame-contracts language/view_basic
cargo run -p wrela -- preview language/view_basic --view main_view
cargo run -p wrela -- frame language/view_basic --view main_view --attachment depth --attachment-format=ppm
cargo run -p wrela -- presentation-debug language/view_basic --view main_view --frames 2 --json
cargo run -p wrela -- preview language/view_basic --view main_view --json-report --json
```

Presentation is now authored through canonical `view` declarations with typed helpers such as
`viewport(...)`, `realtime_quality(...)`, `key_light(...)`, `frame_outputs(...)`, and
`temporal_history(...)`. The CLI exposes those compiled views directly through `preview`,
`frame`, `frame-contracts`, `presentation-plan`, and `presentation-debug` rather than through
legacy authored `render` declarations.

`presentation-debug` is the pass-level inspection entrypoint: it runs a named view for one or
more frames, reports frame-cost/quality state, and can export the same color, depth, and normal
attachments that `preview` and `frame` expose through narrower host flows.

## Benchmark Commands

For repo workflows, prefer the named `just` lanes:

```bash
just perf-smoke
just perf-closure
```

When you need the underlying `wrela perf` invocations directly, use:

```bash
cargo run -p wrela -- perf benchmarks/micro --profile=standard --runs=5
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=cpu
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=wgsl
```

Paired comparisons are available through `perfcmp`:

```bash
cargo run -p wrela -- perfcmp benchmarks/field_engine \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
```

## Direction

Wrela should stay opinionated around authoring worlds. New runtime features should justify how they support fields, scene queries, previews, deterministic execution, simulation, or world asset pipelines. Platform integrations should remain thin host boundaries rather than becoming product-specific subsystems.

New query questions should start from the family/contract checklist in `language/spec/README.md`, not from legacy flat builtin names.
