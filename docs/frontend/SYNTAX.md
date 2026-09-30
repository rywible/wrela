# Wrela source syntax policy 0.1

This version implements the approved first frontend feature in [FEATURE.md](FEATURE.md). Syntax acceptance is independent of name resolution, type checking, mutation legality, no-shadowing, match exhaustiveness, numeric enforcement and execution. Those phases are not implemented here.

## Source and statement boundaries

Statements end at newline, semicolon, the closing block brace or physical EOF. Semicolons allow same-line statements. A newline continues inside parentheses/brackets or after a trailing operator; commas, annotation colons and `->` continue unfinished syntax. A completed expression cannot silently become a call on the next statement line. A lambda block establishes its own statement context even when enclosed in a call.

Functions and lambdas use `fn`. Named functions explicitly annotate each parameter and the return type. Lambda annotations may be inferred from context. All returns use `return`: a final expression statement has no return meaning, regardless of its semicolon. `let` and `var` remain distinct, as do parameter/call `mut`, `for`/`every`, and postfix propagation `?`.

Blocks and record construction use braces. Fields, arguments and positional enum payloads use commas with optional trailing comma. Record declarations use `record`; payload enums use `enum`. Type constraints use `where T: Constraint + Other`. Types use named/qualified paths and nested angle arguments. Match is a statement with comma-separated `pattern => { statements }` arms. Patterns cover identifiers/paths, positional enum payloads, numbers, booleans and `_`.

Direct record construction at the outer level of an `if`, `for`, `every` or `match` head requires grouping. Ordinary control heads need no parentheses. Value-producing `if`/`match`, guards, record patterns, shifts, bitwise operators, modules/imports and operator-implementation declarations are deferred.

## Expressions and generic calls

From tightest to loosest: postfix call/field/index/`?`; prefix `-`/`!`; `*`/`/`/`%`; `+`/`-`; one comparison (`== != < <= > >=`); `&&`; `||`. Binary layers associate left, prefixes right. Comparison chains require grouping. Assignment is a statement. Grouping, tuples/unit and arrays retain their syntax.

Bare generic calls use a named or qualified-path callee followed by a well-formed nonempty type argument list and call parentheses: `f<T>(x)`. Selection is syntactic and whitespace-independent within permitted continuation boundaries. There is no symbol lookup. Prospective type arguments permit interior newlines. A newline between callee/angle or angle/call is allowed only by existing enclosing parentheses/brackets continuation; at statement level it terminates the statement. Both `(f\n<T>(x))` and `(f<T>\n(x))` follow that same rule. Nested type closers remain distinct `>` tokens; shifts are absent.

## Lexical profile

Identifiers use Unicode 16.0.0 XID_Start/XID_Continue plus `_`, excluding Unicode 16.0.0 Default_Ignorable_Code_Point. Ryan explicitly approved this profile on September 30, 2026 at 18:35 UTC (`Sentinel_b88ecc551e6081918d23dd91c7dfd6e7`). This excludes join controls U+200C/U+200D. A forbidden scalar produces a lexical diagnostic and retains its bytes. The lexer must not silently normalize, strip characters or turn an identifier containing a forbidden character into apparently valid independent identifiers. There is no case folding or confusable checking. Strings/comments retain default-ignorable characters. Keywords admitted by this grammar are reserved.

Whitespace is space/tab and LF/CRLF/CR. Other Unicode whitespace and NUL are lexical errors. Comments are `//` and nested `/* */`; comment newlines participate in statement boundaries. Strings are double quoted, permit direct Unicode and only escapes `\n`, `\r`, `\t`, `\"` and `\\`. Raw, interpolated and implicit multiline strings are deferred. Numbers are unsigned decimal tokens, optionally decimal fraction/exponent; unary minus is separate. Bases, suffixes and separators are deferred. Numeric value restrictions remain semantic work.

## Ownership, coordinates and limits

The source tape owns every input byte exactly once as a token, trivia, invalid-input piece or unparsed remainder. Syntax references tape entries rather than duplicating their ownership. Rendering concatenates owned piece bytes. Token edits reparse the full document and preserve unrelated bytes; incremental parsing is deferred.

Ranges are half-open byte offsets. The document full range is `0..input_length`; significant syntax excludes exterior trivia. EOF diagnostics use physical input length, including trailing trivia. Editor positions are zero-based UTF-16 line/column, treating CRLF as one newline and lone CR/LF as newlines. Invalid UTF-8 remains owned and diagnosed; editor coordinates are unavailable where Unicode interpretation is impossible.

Default safety budgets are 16 MiB source, 256 nested syntax/comment levels, 4096 significant tokens per prospective generic classification, and 64 diagnostics including truncation. Limits are deterministic safety boundaries, not performance promises. Exceeded budgets block admission and preserve all source bytes, including an unparsed remainder where needed.

The generic-classification budget applies while the prefix is ambiguous, including inputs that would become comparisons after longer inspection. For example, `a < (b,c,...)` can begin a generic argument containing a tuple type; the classifier must inspect beyond the closing tuple to establish the comparison. If that inspection exceeds 4096 significant tokens, admission fails with a limit diagnostic rather than falling back to a comparison. Grouping the left operand, `(a) < (b,c,...)`, makes the comparison unambiguous without prospective generic classification. Expression-only content such as `a < (1,b,c,...)` can also resolve the ambiguity before the budget is exhausted. The budget counts candidate terminal checks, including a failed required terminal; constant token-kind peeks that establish non-type content do not consume it. Trivia is excluded.

Recovery is bounded to the fixed tested malformed cases. It retains useful partial declarations/statements through explicit synchronization points. Any diagnostic, error/incomplete syntax or failed structural validation blocks admission to later compiler phases. Arbitrary half-written program recovery is outside this version.

The depth budget also bounds conservative expression complexity (active grouping and operator/postfix chains) and chained else branches before generated parsing. This protects construction, inspection and destruction of recursive syntax even for delimiter-shallow inputs. Commas and statement boundaries reset only their own scope. A limit violation produces a limit diagnostic rather than reinterpreting the source.
