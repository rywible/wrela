# Wrela V2 Scaffold

This directory is intentionally scaffold-only.

## Top-level architecture boundaries

- `src/domain/`
- `src/application/`
- `src/infrastructure/`
- `src/application/composition/`
- `tests/contract/`
- `tests/parity/`

## Current status

- Architecture root is language-structure-only (no crate at root).
- Compile-ready parity placeholder crate lives at `tools/parity-scaffold/`.
- Contract and parity interfaces are intentionally minimal.
- No compiler/runtime subsystem migration has started yet.
