# Wrela VS Code LSP Smoke Test

Purpose: quick manual checks after LSP/extension changes (no automated tests).

## Setup
- Open `examples/basic.wr` in VS Code.
- Ensure the Wrela extension is enabled and the LSP is running.

## Checklist
- Highlighting: keywords vs identifiers look consistent; interpolation punctuation `{}`, `()`, `.` colors match expectations.
- Hover: hovering `its` shows the class instance type.
- Go to Definition: works for class names and method calls.
- References: no inline CodeLens (“References: n”) appears.
- Completions:
  - `whale.` suggests only class members.
  - `toSomething(` shows signature help with parameters, not keyword lists.
  - Typing new parameter definitions does **not** trigger keyword spam; after `:` shows types only.
- Diagnostics:
  - Missing `:` after `if` or `to` shows a specific “expected ':' after ...” message.
  - Missing `)` in a function/method signature shows “expected ')' after ... parameters”.

## Notes
- If anything regresses, capture the exact line and token inspector output.
