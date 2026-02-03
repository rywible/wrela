# Wrela VSCode Smoke Test

This is a manual checklist to validate the Wrela TextMate grammar in VSCode.

## Setup

1. Open the repository root in VSCode.
2. Open a new file `scratch.wr` and set language to Wrela.

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

- Tokens are colored consistently for keywords, types, strings, numbers, and comments.
- The `punctuation.separator.wrela` scope renders with the custom color.
- Folding works for blocks in `to main` and `A Foo`.

## Notes

If any item fails, capture a screenshot and the VSCode developer tools console output.
