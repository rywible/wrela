# Wrela Spec

The authoritative executable spec lives in this project root:

- `/Users/ryanwible/projects/wrela/language/spec/src/main.wr`
- `/Users/ryanwible/projects/wrela/language/spec/tests/spec/language_spec_test.wr`

Design RFCs live under:

- `/Users/ryanwible/projects/wrela/language/spec/rfcs/`
- `/Users/ryanwible/projects/wrela/language/spec/rfcs/0001-field-game-language.md`
- `/Users/ryanwible/projects/wrela/language/spec/rfcs/0002-field-engine-implementation-roadmap.md`

Run spec-lane checks from the project root:

- `cargo run -p wrela -- check /Users/ryanwible/projects/wrela/language/spec`
- `cargo run -p wrela -- test /Users/ryanwible/projects/wrela/language/spec --lane=spec`

Inspect the canonical presentation surface from the sample project:

- `cargo run -p wrela -- frame-contracts /Users/ryanwible/projects/wrela/language/view_basic`
- `cargo run -p wrela -- preview /Users/ryanwible/projects/wrela/language/view_basic --view main_view`
- `cargo run -p wrela -- frame /Users/ryanwible/projects/wrela/language/view_basic --view main_view --attachment depth --attachment-format=ppm`
- `cargo run -p wrela -- presentation-debug /Users/ryanwible/projects/wrela/language/view_basic --view main_view --frames 2 --json`
- `cargo run -p wrela -- preview /Users/ryanwible/projects/wrela/language/view_basic --view main_view --query-backend=wgsl --json-report --json`

Current executable coverage includes:

- literals/strings/interpolation/escapes
- assignment and control flow (`if/else`, `while`, `for`, `match`)
- match extensions (`|` patterns, guards)
- functions, classes, interfaces, enums, generics
- defer ordering
- collection methods/indexing/value-vs-identity assertions
- operators/precedence/ranges
- Result workflows (`error`, `??`, `ignore result`, `capture`)
- `require ... else`
- family query calls (`spatial.distance`, `spatial.normal`, `spatial.nearest`, `surface.sample`, `participants.radiance`, `participants.medium`, `support.summary`) over capture, world, and batch surfaces
- canonical `view` declarations with typed `viewport`, `realtime_quality`, `key_light`, `frame_outputs`, and `temporal_history` helpers
- presentation inspection through `frame-contracts`, `preview`, `frame`, and pass-level `presentation-debug`

## New Question Checklist

Every new query question must preserve the family/contract model. Add it by following this checklist:

1. Add the canonical descriptor with stable id, version, family, question, authored family member name, surface, capture kind, item kind, result kind, domain dependency, backend support, and observability.
2. Add the execution binding that maps the descriptor to planner recipe, executor, kernel, helper name, and any legacy compatibility alias.
3. Add or confirm item/result record shapes in the portable ABI and WGSL layout snapshots.
4. Add domain contract fields only when the family policy needs them; keep per-call data in item records.
5. Add descriptor-driven plan construction and validation paths.
6. Add the CPU oracle first.
7. Add virtual GPU/WGSL support only when they preserve the contract; otherwise mark the backend unsupported in the descriptor.
8. Add observability/cost coverage for the semantic work the question performs.
9. Add parser/HIR, planning/kernel, execution parity, CLI/catalog, and spec coverage.

Notes:

- `?` (Result propagation) is part of the canonical language surface and covered by executable spec assertions.
- Legacy call/control-flow forms (`given`, legacy `otherwise`, and removed keyword forms) are hard parse errors with canonical guidance; they are not auto-migrated by `fmt`/`fix`.
- structural match patterns are parsed/typechecked but are not yet included in executable runtime assertions in this suite.
