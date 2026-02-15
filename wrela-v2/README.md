# Wrela V2

Pure `.wr` self-hosted toolchain workspace.

## Top-level architecture boundaries

- `src/domain/`
- `src/application/`
- `src/infrastructure/`
- `src/application/composition/`
- `tests/contract/`
- `tests/parity/`
- `tools/parity/`

## Current status

- V2 roadmap is tracked in `wrela-v2/ROADMAP.md`.
- No-cheating policy is tracked in `wrela-v2/NO_CHEATING.md`.
- Phase 0 ABI envelope is tracked in `wrela-v2/PHASE0_ABI.md`.
- Bootstrap usage is tracked in `wrela-v2/BOOTSTRAP.md`.
- Rust parity scaffold has been removed from `wrela-v2`.
- Platform abstraction contracts start in `src/domain/platform/contracts.wr`.
- OS adapters start in `src/infrastructure/platform/adapters/`.
- Check pipeline staging starts in `src/application/check_pipeline.wr`.
- CLI bootstrap currently supports `check` and `parity` command paths.
- Parity bootstrap scenarios currently cover:
- `--help` surface
- parse/type exit-code contracts
- `test apps/ledger-lite --list`
- cert schema fixture required fields
