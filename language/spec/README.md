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

Notes:

- `?` (Result propagation) is part of the canonical language surface and covered by executable spec assertions.
- Legacy call/control-flow forms (`given`, legacy `otherwise`, and removed keyword forms) are hard parse errors with canonical guidance; they are not auto-migrated by `fmt`/`fix`.
- structural match patterns are parsed/typechecked but are not yet included in executable runtime assertions in this suite.
