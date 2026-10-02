# M1 task breakdown (delete when #6 closes)

Working list for milestone 1 (#6). Each AC's check is named; a box is ticked only when its check
passes locally.

## Phases

1. Foundation: workspace, wrela-diag, lexer (spec/lexical.md), AST, parser, formatter, CLI
   skeleton, conformance runner, tools/check.sh.
2. Grammar: spec/grammar.ebnf for all of T0, Earley oracle over the spec'd lexer, grammar-based
   generator (10^6 programs), parse/format round trips, GBNF export + sampler.
3. Semantics: modules and `use`, types, traits (assoc types, defaults), generics + impl Trait,
   type checker, memory checker (modes, moves, projections, exclusivity, closures), effects,
   GPU rules.
4. IR and back ends: monomorphizing lowering, WASM (strict, overflow traps), naga -> WGSL,
   manifest, `wrela build`, reproducible output.
5. ABI and hosts: wrela-abi (stream + manifest + hash, golden bytes, generated TS), native host
   (wasmtime + wgpu, PNG), browser runtime (bootstrap, shim, render worker, decoder, loader).
6. Derived interpretations: gradient (forward mode), interval (sound, GPU widening).
7. AC2 grazer, AC3 corpus + block-grid measurement.
8. Diagnostics suite (>= 50), conformance coverage of every T0 rule.
9. AC10 agent syntax test baseline.
10. AC11 record: language.md, #26, retrospective.

## AC status

- [x] AC1 end to end in the browser (tests/hello_field.rs)
- [x] AC2 compiled grazer matches the hand-written one (tests/grazer.rs, GPU)
- [x] AC3 derived interpretations (tests/derive.rs, tests/grazer.rs culling 17.9% vs 19.5%)
- [x] AC4 checker enforces tier 0 (tests/conformance.rs, 69 cases, 55 rules)
- [x] AC5 the grammar is the spec (wrela-grammar; 10^6 run: 0 disagreements, 0 round-trip failures)
- [x] AC6 diagnostics bar (tests/diagnostics.rs, 61 cases, 24 fixes)
- [x] AC7 strict CPU numerics (hello_field hash, numerics.rs, wrela-wasm relaxed SIMD)
- [x] AC8 runtime (bun checks: 27.9 KB; ABI golden bytes; both hosts reject other versions)
- [x] AC9 foundation (tools/check.sh; reproducible.rs; fuzz.rs; check 40 ms, build 42 ms)
- [ ] AC10 agent baseline
- [ ] AC11 record: language.md, #26, retrospective, delete this file
