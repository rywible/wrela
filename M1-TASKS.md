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

- [ ] AC1 end to end in the browser: works by hand (byte-identical frames, same hash); automate:
      golden PNG, headless comparison, CPU probe pixels
- [x] AC2 compiled grazer matches the hand-written one (tests/grazer.rs, GPU)
- [x] AC3 derived interpretations (tests/derive.rs, tests/grazer.rs culling 17.9% vs 19.5%)
- [ ] AC4 checker enforces tier 0: conformance runner + suite; sema GPU signature rules
      (E0601/E0602, varyings); lower_draw buffers
- [ ] AC5 the grammar is the spec: done (oracle, 10^6 differential, GBNF); record numbers
- [ ] AC6 diagnostics bar: >= 50 curated, JSON goldens
- [ ] AC7 strict CPU numerics: hash test Chrome vs wasmtime, relaxed-SIMD test, overflow test
- [ ] AC8 runtime: verify size, golden bytes, version rejection tests exist
- [ ] AC9 foundation: tools/check.sh, reproducible builds, check < 200 ms, build < 2 s
- [ ] AC10 agent baseline
- [ ] AC11 record: language.md, #26, retrospective, delete this file
