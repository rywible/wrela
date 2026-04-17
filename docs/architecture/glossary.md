# Temporary Migration Glossary

This file is temporary migration scaffolding for RFC 0009.
It exists only while names and ownership boundaries are being normalized.
Do not grow it into a second architecture guide.

## Terms

- `bounded context`
  A named ownership boundary used by the Phase 50 context map.
  In this repo a context may span multiple modules inside the same crate.

- `public noun`
  A stable type or concept another context is allowed to name directly.
  Everything else should stay internal to the owning context.

- `contract`
  The stable semantic shape, guarantees, and compatibility story a context
  exposes to other contexts.

- `plan`
  The lowered executable recipe derived from a contract.
  Plans own shape and guarantees; executors own backend behavior.

- `execution`
  The CPU, virtual GPU, or WGSL path that runs a plan while preserving the
  contract it claims to implement.

- `substrate`
  Shared runtime, artifact, identity, time, and GPU support that multiple
  execution contexts consume through named seams.

- `tooling and orchestration`
  The CLI, `just`, benchmarks, perf harnesses, and docs that compose workflows
  without owning domain logic.

- `lane`
  A named repo workflow such as `just test`, `just test-all`, `just perf-smoke`,
  or `just ship`.

- `proving surface`
  The tests, benchmarks, or named lanes that prove a context still works after a
  change.

- `anti-corruption seam`
  A small named contract, adapter, or report boundary used when one context
  crosses into another.

## Shrink / Delete Criteria

Shrink or remove this file once all of the following are true:

- the main ownership and naming renames from RFC 0009 have landed
- touched module roots clearly state what they own, what they do not own, their
  primary entrypoints, and their invariants
- the surviving public nouns are stable enough to stand on their own in code and
  module docs

Phase 56 must reduce this glossary to public nouns only or remove it entirely.
