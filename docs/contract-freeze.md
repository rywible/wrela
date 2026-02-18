# Contract Freeze Surface

The following interfaces are frozen for V2 readiness work.

1. CLI command and option surface in `/Users/ryanwible/projects/wrela/compiler/bin/wrela.rs`
2. Exit code contract: `0/1/2/3/4`
3. Diagnostic JSON shape emitted by `--format=json`
4. Test list/run summary formats
5. Certification report schema (`cert.json`)
6. Thin-core ABI and symbol snapshot in `/Users/ryanwible/projects/wrela/language/spec/thin_core_snapshot.txt`
7. Benchmark manifest schema in `/Users/ryanwible/projects/wrela/benchmarks/*/bench.toml`

Any intentional change to this surface must include:

- explicit rationale,
- updated tests,
- baseline refresh,
- and a note in PR description.
