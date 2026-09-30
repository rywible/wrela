# Independent frontend conformance corpus

The authoritative seed is [frontend/tests/corpus.json](../../frontend/tests/corpus.json), independently authored before production parsing. It contains exact source strings/UTF-8 lengths/hashes, stable case IDs, acceptance classifications and intended structural relationships. The 33 cases are 12 valid, 14 invalid and 7 syntactically valid programs whose semantic requirements are deliberately unchecked.

The frozen seed SHA-256 is `73665c11c3465bf321c69a2d22135a5637a5fcedfebcd78557bf1ce7e948fda9`; [CORPUS-STATUS.json](../../frontend/tests/CORPUS-STATUS.json) has SHA-256 `579e308be47914cc048106786a853866c9fdff3cd71a01f29f82df681f20e700`. Its historical execution fields record the authoring stage. Production tests now exercise every admission outcome and independently audit all 19 eligible recursive trees. The status metadata distinguishes 19 approved-behavior seeds from 14 proposed spelling/recovery examples; implementation adopts the coherent spellings documented in [SYNTAX.md](SYNTAX.md), without treating them as additional acceptance criteria.

| Cases | Intended distinctions |
| --- | --- |
| V01–V04 | Records, functions, mut parameters/calls, for/every, propagation, enum payload forms, generic constraints/nested type closers. |
| V05–V08 | Syntactic generic-call tie-break, permitted newline continuation, lambda block boundaries, explicit return, contextual/annotated lambdas and grouped record control heads. |
| V09–V12 | Statement match, ordinary qualified library calls, Unicode spelling/comments/escapes/CRLF, and independently specified precedence/association. |
| I01–I03 | Forbidden next-line generic calls, ungrouped record control heads and deferred raw strings. |
| I04–I10 | Lexical middle errors, multiple missing expressions, nested/crossed delimiters, incomplete EOF with trivia, newline-terminated bad strings and unclosed comments; named survivors are asserted. |
| I11–I14 | Deferred pipe lambdas, unsupported string escapes and forbidden identifier join controls with preserved bytes. |
| S01–S07 | Read-only assignment, mutation legality, no-shadowing, exhaustiveness, ignored Result, missing explicit return and missing call mutation permission remain semantic work. |

The [particle example](../../frontend/examples/particles.wr) is V01's exact source and can be inspected with the CLI. Corpus examples define intended behavior; parser-produced snapshots never replace their authority. Deleting, duplicating, reordering, changing groups or changing source bytes causes integrity checks to fail.

Additional independent fixtures in `frontend/tests/support/fixtures.rs` and the typed/source/limits/structural contract tests cover compositions, recovery, UTF-8/UTF-16, edits, safe limits and deliberate corruption controls. Admission outcomes and structural fidelity are separate assertions. Extending implementation coverage does not mutate the original seed or redefine the approved feature criteria.
