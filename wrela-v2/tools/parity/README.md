# parity tools

Parity tooling for v2 lives here as pure `.wr` code.

The old Rust scaffold has been removed to keep `wrela-v2` purity intact.

Current bootstrap modules:

- `runner.wr` for host command execution wrappers.
- `contract_suite.wr` for frozen contract scenario orchestration.
- `src/application/parity_command.wr` delegates command execution to `ContractSuite`.

Current bootstrap scenario coverage:

- `--help` surface exits cleanly.
- parse error exits with code `2`.
- type error exits with code `3`.
- `test apps/ledger-lite --list` exits cleanly and emits output.
- cert schema fixture includes required field names.
