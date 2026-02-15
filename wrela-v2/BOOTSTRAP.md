# V2 Bootstrap Usage (Current)

Current v2 CLI bootstrap commands:

- `init [path]`
- `check <path>`
- `parity`

## Parity Command Environment

`parity` requires:

- `WRELA_V1_BIN`: path to frozen v1 `wrela` binary
- `WRELA_WORKSPACE_ROOT`: workspace root path

It writes a status artifact at:

- `.artifacts/parity/v2-bootstrap-report.txt`
