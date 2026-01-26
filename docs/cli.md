# CLI Reference (v0)

## Usage

```
wrela <command> [options] <path> [-- args]
```

Commands:
- `init [path]`: create `src/main.wr`
- `update`: update the installed toolchain (downloads latest release)
- `check <path>`: parse/typecheck without codegen
- `build <path>`: compile to a native executable
- `compile <path>`: alias for `build`
- `run <path>`: compile and run
- `dev <path>`: watch and rebuild (polling)

`<path>` may be either the project root (expects `src/main.wr`) or an entry file.

Options:
- `--prefix PATH`: install/update prefix (default: `$PREFIX` or `~/.local/wrela`)
- `-o, --out PATH`: output path for `build`/`run`
- `--emit-mir`: emit MIR before optimization
- `--emit-mir-opt`: emit MIR after optimization
- `--emit-obj=PATH`: emit object file
- `--emit-bin=PATH`: emit executable
- `--poll-ms=N`: poll interval for `dev` (default: 500)
- `--format=json`: emit diagnostics as JSON

## Exit Codes

- 0: success
- 1: usage error (missing input or failed `init`)
- 2: parse/validation error
- 3: type error
- 4: MIR/codegen/link error

## Diagnostics

### Pretty (default)

Human-readable diagnostics emitted to stderr.

### JSON (`--format=json`)

Each diagnostic is emitted as a single JSON object on stdout:

```json
{
  "kind": "error",
  "message": "parse error message",
  "path": "src/main.wr",
  "span": { "offset": 0, "len": 10 }
}
```

Notes:
- `kind` is `error` or `warning`.
- `span.offset` and `span.len` are byte offsets into the source text.

## Examples

```
wrela init .
wrela check src/main.wr
wrela build src/main.wr -o ./wrela.out
wrela run src/main.wr -- arg1 arg2
wrela dev src/main.wr --poll-ms=250
```
