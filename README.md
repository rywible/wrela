# Wrela

Wrela is a programming language for worlds: authored fields, scenes, materials, media, queries, and deterministic execution surfaces that can move from source to preview to native or GPU-backed runtimes.

The repo now focuses on the compiler, the core runtime, and the language libraries that support world construction. Older database, web-server, authentication, deployment, cluster, and distributed-storage surfaces have been removed.

## What Is Here

- `compiler/`: parser, HIR/MIR lowering, diagnostics, native codegen, previews, tests, and the `wrela` CLI.
- `runtime/`: value/runtime ABI, actor/reactor support, host filesystem/HTTP cassettes, metrics, and native runtime exports.
- `language/stdlib/`: thin-core standard library modules used by authored world programs.
- `language/spec/`: language snapshots, RFCs, and surface integrity references.
- `benchmarks/micro/`: low-level benchmark scenarios.
- `benchmarks/field_engine/`: world/field-engine benchmark scenarios.

## Common Commands

```bash
cargo test --workspace
cargo run -p wrela -- --help
cargo run -p wrela -- build path/to/main.wr
cargo run -p wrela -- preview language/preview
```

The `justfile` mirrors the common development loop:

```bash
just test
just build
just fmt-check
just lint
```

## Benchmark Commands

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
