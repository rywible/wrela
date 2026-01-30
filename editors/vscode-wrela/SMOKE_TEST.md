# Wrela VSCode Smoke Test

This is a manual checklist to validate the Wrela LSP and extension end-to-end.

## Setup

1. Open the repository root in VSCode.
2. Ensure the language server can start:
   - Build: `cargo build -p wrela-lsp`
   - Or configure `wrela.languageServer.command`.
3. Open a new file `scratch.wr` and set language to Wrela.

## Test File

Paste this into `scratch.wr`:

```
use b, a, a from core

to main():
    foo = Foo()
    foo.
    x = 1
    y = x + x

A Foo:
    has:
        value: Int
    can bar(x: Int) -> Int:
        return x
```

## Checklist

- Diagnostics: unresolved names should show warnings.
- Hover: hover on `Foo` and `bar` shows signature/details.
- Completion: after `foo.` shows `value` and `bar`.
- Rename: rename `x` updates all occurrences.
- References: "Find All References" on `bar` returns its uses.
- Code lens: "References: N" appears above definitions.
- Formatting: run "Format Document" and verify trailing whitespace is removed.
- Organize imports: use the "Organize Imports" action and verify sorting.
- Folding: fold the `A Foo` class block and `to main` block.
- Inlay hints: parameter hints on `bar(...)` and type hints on `x`.
- Type definition:
  - Run `Wrela: Go To Type Definition` on `foo`.
  - Run `Wrela: Peek Type Definition` on `foo`.

## Notes

If any item fails, capture the console logs from the Wrela output channel.
