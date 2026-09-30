# First Wrela Frontend Feature Specification

**Approved for implementation by Ryan on September 30, 2026 at 18:27:55 UTC:** “That looks right. Launch it” (`Sentinel_2b0c5ab2ed488191b2e68847580a2f8c`). The description and eight criteria below reproduce the parent’s presentation verbatim. Farquad owns final acceptance, merge, post-merge verification and closure. This document/corpus task does not implement the feature.

Build a Rust/LALRPOP frontend that turns ordinary Wrela source into faithful, typed syntax and useful diagnostics. It must preserve the source for inspection and editing, and reject syntax-invalid input before later compiler phases.

Included: functions and fn lambdas, records, payload enums, generic types/calls and constraints, let/var, expressions, mut arguments, if, match, for/every, explicit return and ? propagation.

Excluded: type/name checking, memory enforcement, execution, code generation, field analysis, dedicated UI grammar, module resolution and incremental editor parsing.

Acceptance criteria

AC-1 — Defined language slice
A versioned syntax policy and independently authored corpus specify accepted, rejected and syntactically valid-but-semantically-unchecked programs. Examples define the intended behavior; parser-generated snapshots aren’t the sole authority.

AC-2 — Generated frontend
Pinned LALRPOP generation builds reproducibly. Unexpected conflicts fail verification. Generated code is clearly separated from handwritten grammar actions and frontend logic.

AC-3 — Faithful typed structure
Syntax preserves declarations, types, constraints, child roles and meaningful markers, including mut, let/var, for/every, return and ?. Precedence and association have independent tests. Any normalization is explicit and documented.

AC-4 — Lossless source ownership
Every input byte is represented exactly once in typed tokens, trivia or invalid-input pieces. Valid syntax trees preserve real structure, not just a flat token list. Independent rendering reconstructs the source, and tested token edits preserve unrelated comments and whitespace.

AC-5 — Bounded error recovery
Malformed input retains its complete source representation and useful partial structure. A fixed recovery suite covers lexical errors, missing expressions, multiple errors, broken delimiters and incomplete EOF, specifying which later declarations survive. Any diagnostic or incomplete/error node blocks admission to later phases. Full recovery for arbitrary half-written programs is outside this feature.

AC-6 — Precise positions and safe limits
Diagnostics use valid half-open UTF-8 byte ranges; EOF means the actual end of the file, including trailing trivia. Full-source and significant-syntax ranges are distinct and consistent. Editor coordinates handle Unicode and line endings explicitly. Oversized/deep inputs fail deterministically without losing their source bytes.

AC-7 — Tests that catch structural failures
Tests must reject erased syntax children, invalid node kinds, missing markers, impossible spans, broken containment and missing corpus cases. Include independent malformed, Unicode, trivia and edit cases, with deliberate negative controls proving the checks work.

AC-8 — Maintainable implementation
Use typed domain constructors and clear ownership. Keep parsing, source storage, diagnostics and serialization separate. JSON is an external format, not the internal syntax model. Each abstraction and dependency needs a current purpose. Design review must assess clarity and future changeability as well as correctness.

Proposed final scope choices

- Use statement-form match with block arms and explicit return initially. Defer value-producing if/match, shifts, modules and operator-implementation declarations
- Allow multiline generic type arguments, while preserving our existing continuation rules at the call boundaries
- Pin Unicode 16 identifier tables for this first version, with explicit treatment of invisible characters rather than silently accepting dependency defaults

The presented final scope choices above are part of the reviewed feature boundary. Exact invisible-character treatment still needs an explicit policy; the presentation does not select either permission or exclusion of join controls. Detailed lexical spellings, recovery fixture shapes, normalization choices and resource-budget values in DESIGN-NOTES.md are proposed implementation defaults, not additional acceptance scope.

The grammar spikes demonstrate bounded feasibility, not production acceptance. Their tree-verification, physical EOF, containment, and generic-continuation gaps remain production obligations under these criteria. Implement a clean frontend rather than merge the spike.

The independently authored corpus v0.1 is a representative review/implementation seed, not an already executed production conformance suite. See CORPUS.md and corpus.json for exact examples, intended distinctions and execution status. Evidence and the remaining review checklist are in DESIGN-NOTES.md.

