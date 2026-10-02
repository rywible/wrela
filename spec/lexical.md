# wrela lexical structure, tier 0

Normative (D-104). `spec/grammar.ebnf` is written over the tokens defined here. Diagnostics and
tests cite rules by number (L1, L2, …); a number is never reused or changed, and new rules are
appended. Codes named here exist. Where a rule says *error* without a code, the compiler reports
one from the lexical range E0001–E0099; a *tier error* comes from E0900–E0999.

## Source text

- **L1.** A source file is UTF-8 text with the extension `.wrela`. A file that isn't valid UTF-8 is
  rejected before lexing.
- **L2.** A byte-order mark (U+FEFF) at the very start of a file is ignored; byte offsets still
  count it. Anywhere else outside a comment or string literal it's E0001 (L3).
- **L3.** Outside comments and string literals, a file holds only ASCII letters, digits, `_`, the
  punctuation of L15, space, tab and line breaks. Any other character is E0001, reported once per
  run of such characters. A name or number with non-ASCII letters or digits in it stays one token
  (with one E0001), so later phases see the name the author meant.
- **L4.** A line break is `\n` or `\r\n`. A `\r` without a `\n` after it is E0001, even in a
  comment, and is read as a line break; so is each other character Unicode counts as a line break:
  vertical tab (U+000B), form feed (U+000C), next line (U+0085), line separator (U+2028) and
  paragraph separator (U+2029). Some editors show them as line breaks and others don't, so the
  compiler reads them the way a reader most likely does. The fix replaces each with `\n`.
- **L5.** Space and tab separate tokens and are otherwise ignored. Any other whitespace character
  that isn't a line break (L4), such as a non-breaking space, is E0001 by L3.

## Comments

- **L6.** `//` starts a comment that runs to the end of the line, not including the line break.
  Any line break of L4 ends it, so a comment holds any character except those.
- **L7.** A comment that starts with exactly three slashes (`///`, not `////`) is a doc comment. It
  documents the item, field or variant that follows it.
- **L8.** Comments, doc comments included, are trivia: the grammar never sees them, and they are
  never the previous or next token in L17.
- **L9.** `/*` is E0002: wrela has no block comments. The lexer skips to the matching `*/`
  (nesting, as in Rust) or to the end of the file, and continues. Its fix rewrites the comment as
  `//` (or `///` for `/** */` on a line of its own) only when the comment holds no line break of
  L4 and nothing but a `//` comment follows it on its line, so the rewrite never turns text into
  code.

## Identifiers and keywords

- **L10.** An identifier is `[A-Za-z_][A-Za-z0-9_]*`, taking the longest run. `_` alone is not an
  identifier: it's the wildcard token `_`.
- **L11.** These identifiers are keywords, reserved everywhere:

  ```
  as borrow break const continue else enum extern false fn for if impl in let loop match mut
  pub return self Self struct take trait true type unsafe use var where while
  ```

  Type names (`f32`, `u32`, `vec3`, `bool`, …) and attribute names (`vertex` in `@vertex`) are
  ordinary identifiers.

## Number literals

- **L12.** A number token starts with a digit and takes the longest run of letters, digits and `_`.
  A decimal number (one that doesn't start with `0` then `x`, `X`, `b`, `B`, `o` or `O`) also takes
  one `.` that is followed by a digit, and a `+` or `-` directly after an `e` or `E` when a digit
  directly follows the sign. Two exceptions keep a `.` out: a `.` followed by
  another `.` (`1..5` is `1`, `..`, `5`), and a number directly after a `.` token, with no space
  between, takes no `.` and no exponent sign (`t.0.1` is `t`, `.`, `0`, `.`, `1`).
- **L13.** The token's text is then classified. An `_` may appear anywhere among a form's digits,
  including right after its prefix or its exponent's `e`, and is ignored; but each run of digits
  below needs at least one real digit.

  | Form | Syntax, ignoring `_` | Token |
  |---|---|---|
  | decimal integer | `[0-9]+` | INT |
  | hexadecimal integer | `0x[0-9A-Fa-f]+` | INT |
  | binary integer | `0b[01]+` | INT |
  | octal integer | `0o[0-7]+` | INT |
  | float | `[0-9]+ "." [0-9]+ exponent?`, or `[0-9]+ exponent` | FLOAT |
  | exponent | `[eE] [+-]? [0-9]+` | |

  A radix prefix is read first. Text that starts with `0x`, `0b` or `0o` is that radix's integer:
  its digits are the longest run of the radix's digits and `_` after the prefix. It's malformed
  if that run has no digit (`0x`, `0b_`, `0xg`), if a decimal digit out of the radix's range comes
  right after it (`0b102`, `0o9`), or if the prefix is uppercase (`0X1F`, `0B1`, `0O7`; the help
  writes `0x1F`); otherwise the rest of the text is its suffix, read as below. Text that doesn't
  start with a prefix is decimal: its longest decimal-integer or float prefix, then a suffix.
  A number with an empty suffix is just the INT or FLOAT; otherwise, by its suffix:

  - **A type suffix** (`i8 u8 i16 u16 i32 u32 i64 u64 f32 f64`, as in `1.0f32` or `7_u32`) is an
    error. The help: drop the suffix (a literal takes its type from context), or convert with
    `f32(1.0)`.
  - **A suffix starting with `e` or `E`** is a malformed exponent (`1e`, `2.5e+`), an error. No
    unit is named `e` (D-025), so `1e5` is always a float.
  - **Any other suffix that is an identifier** makes the token a SUFFIXED number: a unit suffix,
    as in `15cm`. Units are tier 1, so it's a tier error.
  - **Anything else** is a malformed number, an error.

  A float has digits on both sides of its `.`: `1.` is `1` then `.`, and `.5` is `.` then `5`.
  There are no negative literals: `-1` is unary minus applied to `1`. Whether a literal's value
  fits its type is checked with types, not here.

## String literals (tier 1)

- **L14.** A string literal is `"` … `"` on one line, with the escapes `\\ \" \n \r \t \0` and
  `\u{…}` (1–6 hex digits naming a Unicode scalar value). It lexes as a STRING token, and using one
  is a tier error. An unterminated string, a line break (any of L4) inside one, or an unknown
  escape is an error. There are no character literals: `'` is E0001.

## Operators and punctuation

- **L15.** Operators and punctuation, matched longest first:

  ```
  **=  <<=  >>=  ..=
  ::  ->  =>  ==  !=  <=  >=  &&  ||  +=  -=  *=  /=  %=  ^=  &=  |=  <<  >>  **  ..
  (  )  [  ]  {  }  ,  ;  :  .  @  =  +  -  *  /  %  ^  &  |  !  <  >  ?
  ```

  `**` is exponentiation and `^` is XOR (D-076). `?` is tier 1: it lexes, and using it is a tier
  error. `#`, `$`, `~` and `` ` `` are E0001.
- **L16.** Where the grammar expects a `>` that closes a generic argument list, a `>>`, `>=` or
  `>>=` token is split: the parser takes its first `>`, and the rest stays a token
  (`Option<Option<T>>`).

## Newlines

- **L17.** A line break produces a NEWLINE token only when all three hold:
  1. the innermost open bracket (L19), if there is one, is `{`, not `(` or `[`;
  2. the previous token can end a statement: an identifier, INT, FLOAT, STRING, SUFFIXED, `true`,
     `false`, `self`, `Self`, `return`, `break`, `continue`, `)`, `]`, `}`, `>`, `>>` or `?`;
  3. the next token is not `.` (the single dot; `..` and `..=` don't count).

  "Previous" and "next" skip comments and line breaks (L8). Otherwise the line break is
  whitespace.
- **L18.** Line breaks with no token between them (blank lines, comment-only lines) produce at most
  one NEWLINE. There is no NEWLINE before the first token. The token stream ends with EOF, and a
  NEWLINE before EOF follows L17 (EOF is not `.`).
- **L19.** `(`, `[` and `{` open brackets. `)`, `]` and `}` close the innermost open bracket of
  their kind, and with it any brackets opened after it. A closing bracket with no open bracket of
  its kind changes nothing (the parser reports mismatched brackets).
- **L20.** Consequences of L17, for readers (not extra rules):
  - Only a leading `.` continues a line that could have ended. A line that starts with `(`, `[`,
    `-` or `|` begins a new statement: there's no JavaScript-style continuation (D-038, D-079).
  - A binary operator that continues an expression trails its line: `a +`, then `b` on the next
    line. `>` and `>>` can't, because they may close a generic argument list: split a comparison
    inside parentheses instead.
  - `else` goes on the line of the `}` before it, and a block's `{` on the line of its head.
  - A generic argument list isn't a bracket: break a long one after a `,`.
  - `;` separates statements on one line; a line break after `;` is whitespace.

## Tokens the grammar sees

- **L21.** `spec/grammar.ebnf` uses the token classes IDENT (L10), INT and FLOAT (L13), SUFFIXED
  (L13), STRING (L14), NEWLINE (L17, L18) and EOF (L18). Keywords (L11), `_` (L10) and operators
  and punctuation (L15) appear in the grammar as quoted text, such as `"fn"` and `"+="`.
