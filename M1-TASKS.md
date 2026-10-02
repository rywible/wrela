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

- [ ] AC1 end to end in the browser
- [ ] AC2 compiled grazer matches the hand-written one
- [ ] AC3 derived interpretations
- [ ] AC4 checker enforces tier 0
- [ ] AC5 the grammar is the spec
- [ ] AC6 diagnostics bar
- [ ] AC7 strict CPU numerics
- [ ] AC8 runtime
- [ ] AC9 foundation
- [ ] AC10 agent baseline
- [ ] AC11 record
