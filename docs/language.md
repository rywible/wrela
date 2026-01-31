# Wrela Language Reference

This document describes the Wrela language **as implemented in this repository**. It focuses on
lexical rules, syntax, typing, and runtime behavior that the current compiler and runtime
actually enforce. When there is a mismatch between intention and implementation, the
implementation wins.

---

## 1) Files, modules, and entrypoints

### Source files
- Wrela source files use the `.wr` extension.
- The module loader also accepts `.sp` files as import targets.

### Module paths and `use`
- Imports are written with `use` and resolved relative to the project root.
- Module paths can be written using `/` or `.` as separators; internally they normalize to `/`.
- `use` is only valid at the top level of a file.

Inline `use` example (syntax only):
```
use Foo, Bar from my/module
```

Block `use` example (syntax only):
```
use:
    Foo,
    Bar
from my.module
```

Rules enforced by the parser and project loader:
- `use` must include at least one name (or `*`) and a module path.
- `use *` cannot be combined with named imports.
- Importing private items from another module is an error.
- Importing from a module that does not exist is an error.

### Entry point
- Top-level executable statements are **not allowed**; only `use`, class definitions, and
  function definitions are legal at the top level.
- The entry module must define `to run()`.
- Only the entry module may define `run`.
- The name `main` is reserved; you must use `run` as the entrypoint.

---

## 2) Lexical structure

### Significant indentation
- Wrela uses **indentation + colons** to delimit blocks.
- Indentation is **spaces only**; a tab character is a lexer error.
- Indentation is only meaningful outside of `()`, `[]`, and `{}`. Inside those, newlines are
  allowed and indentation does not open/close blocks.

### Comments
Wrela has one comment form: **`so:`**
- `so:` starts a comment on the current line and can extend into an indented block.
- Any subsequent line indented *more* than the base line is part of the comment block.

Example:
```
so: This is a comment
    This line is still part of the comment.

A Whale:
    so: This line is a comment inside the class.
    has:
        name: String
```

### Identifiers
- Identifiers may contain letters, digits, `_`, and non-ASCII letters.
- Identifiers are case-sensitive.
- Keywords are reserved and cannot be used as identifiers.

### Keywords
```
A An has can to if else while for in return break continue match otherwise
err crash true false nothing and or not await detach spawn fire optimize use
from public private its it changing
```

### Symbols and operators
```
: ( ) [ ] { } . ... , @ ->
= == != < <= > >= + - * / %
+= -= *= /=
& | ^ ~ << >>
```

---

## 3) Literals

### Numbers
- Decimal integers: `0`, `42`, `1_000`.
- Hex: `0xFF`, `0x10` (underscores allowed).
- Binary: `0b1010`.
- Octal: `0o755`.
- Floats: `1.0`, `.5`, `10.`, `1e3`, `1.5e-2`.

Notes:
- Underscores are permitted in numeric literals and ignored by the lexer.
- Negative numbers are written using the unary `-` operator.
- Malformed numeric literals (e.g., `1e`, `1.2.3`) are lexer errors.

### Booleans and nil
- `true` / `false`
- `nothing` is the nil literal.

### Strings
- Double-quoted strings: `"hello"`.
- Escapes: `\n`, `\r`, `\t`, `\"`, `\\`, `\{`, `\}`.
- String interpolation: `{expr}` inside a string.

Example:
```
"Hello {name}, you have {count} messages"
```

---

## 4) Program structure

### Functions
```
to name(param: Type, ...) -> ReturnType:
    ...
```
- `-> ReturnType` is optional; missing return type implies `Nothing`.
- Parameters are required to have a type annotation.
- Functions default to **private** visibility. `public` or `private` may be added.

### Classes
```
A Whale:
    has:
        name: String
    can swim(distance: Number) -> Bool:
        return true
```
- Class definitions start with `A` or `An`.
- Fields are defined in `has` blocks.
- Methods are defined with `can`.
- Class, field, and method visibility defaults to **private**; `public` / `private` are allowed.

### Fields (`has`)
```
A Whale:
    has:
        name: String
        age: Int
```
- A `has` block may appear multiple times.
- Fields must include a type annotation.

### Methods (`can`)
```
A Whale:
    can swim(distance: Number) -> Bool:
        return true
```
- Methods are functions with an implicit receiver (`it`).

### The implicit receiver: `it` and `its`
- Inside methods, `it` refers to the current instance.
- `its` is a synonym for `it` and is intended for readability in member access (`its.name`).
- Outside methods, `it` is only allowed in `return` expressions (reserved for future use).

---

## 5) Statements

### Variable declarations and assignment
```
name = expr
changing count = 0
count += 1
```
- `name = expr` declares a **new immutable** local variable.
- `changing` marks a variable as **mutable**.
- `+=`, `-=`, `*=`, `/=` are allowed only on mutable variables.
- Reassigning an immutable variable is an error.

Visibility modifiers (`public` / `private`) are **not allowed on local variables** and
produce a semantic error. `public changing` is also invalid.

### If / else
```
if condition:
    ...
else:
    ...
```

### While
```
while condition:
    ...
```

### For
```
for item in iterable:
    ...
```
- The iterable can be any value supporting the runtime iterator protocol (lists, maps, ranges).

### Match
```
match expr:
    1: return "one"
    2, 3: return "small"
    otherwise:
        return "other"
```
Rules:
- `otherwise` is required.
- Case labels are **expressions**, not patterns.

### Return, break, continue
- `return` is only valid inside functions/methods.
- `break` / `continue` are only valid inside loops.

### Optimize
```
optimize balance:
    ...
```
- Sets a pool optimization objective for `detach` in the current scope.
- Only **one** `optimize` statement is allowed per scope.

---

## 6) Expressions

### Calls and member access
```
foo(1, 2, bar=3)
obj.field
obj.method(arg)
```
Rules:
- Positional arguments must come before named arguments.
- Duplicate named arguments are an error.

### Lists and maps
```
[1, 2, 3]
{ "a": 1, "b": 2 }
```

### String interpolation
```
"{its.name} swam {distance} meters"
```

### Crash
```
crash("unreachable")
```
- `crash(expr)` immediately aborts execution. The expression has type `never`.

---

## 7) Operators and precedence

### Unary operators
- `-x` (numeric negation)
- `not x` (boolean not)
- `~x` (bitwise not)
- `err x` (construct Result error)
- `await x` (await actor call)
- `fire x` (fire-and-forget actor call)
- `detach x * n` / `spawn x * n` (spawn actor/pool)

### Binary operators
- Arithmetic: `+ - * / %`
- Comparisons: `== != < <= > >=`
- Boolean: `and or`
- Bitwise: `& | ^ << >>`
- Range: `...`
- Result handling: `otherwise`

### Precedence (lowest to highest)
1. `otherwise`
2. `or`
3. `and`
4. `|`
5. `^`
6. `&`
7. `==` `!=`
8. `<` `<=` `>` `>=`
9. `...`
10. `<<` `>>`
11. `+` `-`
12. `*` `/` `%`

Function calls and member access bind tighter than all infix operators.

---

## 8) Types

### Type references
Type annotations are identifiers with optional type arguments in brackets:
```
List[String]
Map[String, Int]
Result[Int, Error]
Result[Int]
```

### Built-in types (by name)
- `Int`
- `Float`
- `Number`
- `Bool`
- `String`
- `Nothing` (aka `Nil` in type positions)
- `List[T]`
- `Map[K, V]`
- `Result[Ok, Err]` (or `Result[Ok]`, which defaults `Err` to `Error`)
- `Actor[T]`
- `Pending[T]`

### Type inference (high-level)
- Numeric literals are inferred as `Int` or `Float`.
- `Number` is a supertype used for numeric operations.
- The `...` range operator builds a list of numbers.
- `err x` produces `Result[unknown, type(x)]`.
- `await` on an actor call produces `Result[Ok, Error]`.

### Operator type rules (summary)
- `+` supports numeric addition and string concatenation (`String + String`).
- `- * / %` require numeric operands.
- `< <= > >=` require numeric operands.
- `and` / `or` require `Bool` operands.
- `& | ^ << >>` require `Int` operands.
- `...` requires numeric operands.

---

## 9) Results and error handling

Wrela treats `Result` values as **must-handle**.
`Result[T]` is shorthand for `Result[T, Error]`.

### `err`
```
return err "bad input"
```
Rules:
- `err` may only be used in functions that return `Result`.

### `otherwise`
```
value = risky() otherwise fallback
```
Rules:
- The left-hand side must be a `Result`.
- The right-hand side must be compatible with the `Ok` type.
- If `Ok`, the operator returns the unwrapped value. If `Err`, it returns the fallback.

### Unhandled results
If a `Result` value is produced and **not** handled with `otherwise` (and the enclosing
function does not return `Result`), the compiler reports an error.

---

## 10) Actors, concurrency, and pools

### Creating actors
```
actor = detach Whale() * 1
```
- `detach` and `spawn` are synonyms.
- The `* size` suffix is required.
- `size` can be an integer literal or `n` for auto-sizing.

Rules:
- Pool sizes greater than `1` (or `n`) require a **class constructor** target or a `Pool.of` call.
- If a function (or anything it calls) contains `await`, then every `detach` in that function
  must have an explicit optimization objective (see below) unless the target is a `Pool.of`
  with `objective=...`.

### Await and fire
Actor method calls return a **pending** value and must be handled:
```
pending = actor.swim()
result = await actor.swim()
fire actor.swim()
```
Rules:
- `await` and `fire` are only valid for actor **method calls**.
- `fire` is only valid as a **standalone statement**.
- `await` yields a `Result`, even if the method itself returns a non-`Result` type.
- The default error type used by `await` and `Result[T]` is the named type `Error`.
- You cannot access fields on actor instances (only call methods).

### Async classes and methods
If a class method uses `await`, then:
- The class must be instantiated as an actor (`detach` / `Pool.of`).
- That method must be called on an actor instance.

### Pool objectives
Optimization objectives:
```
latency | throughput | conservation | balance
```

You can set objectives in two places:
1) A scope-wide `optimize` block:
```
optimize balance:
    actor = detach Whale() * 1
```

2) Inline on a `detach` expression:
```
actor = detach Whale() * 1 optimize latency
```

### `Pool.of`
`Pool` is an implicit binding used to create pools:
```
pool = Pool.of(Whale, size=8, objective=throughput, batch=8, backpressure=queue(128))
```

Recognized named arguments:
- `size`: integer literal or `n`.
- `objective`: `latency | throughput | conservation | balance`.
- `batch`: integer literal (batch limit).
- `backpressure`: `drop` or `queue(<int>)`.
- `min`: integer literal (min pool size).
- `max`: integer literal (max pool size).
- `weight`: integer literal.

`Pool.of` is specially validated by the compiler; its argument expressions are not
resolved as normal identifiers.

---

## 11) Built-in bindings

The compiler treats the following names as built-in bindings (functions or implicit values):

Functions:
```
print
parse_int
parse_float
read_file
write_file
bytes_from_string
bytes_to_string
bytes_len
list_push
map_get
map_set
pool_auto_size
pool_size
pool_rr
pool_queue_len
actor_mailbox_len
actor_pause
actor_resume
actor_pause_wait
metrics_get
metrics_dropped_paused_id
metrics_messages_dropped_id
clock_ns
sleep_ms
http_server_serve_get_requests
http_server_serve_post_requests
http_server_serve_requests
http_server_serve_on
http_server_stop
```

Implicit values:
```
nil
Pool
```

---

## 12) Common errors enforced by the compiler

- `it` is only valid inside return expressions (or inside methods).
- `return` is only valid inside functions/methods.
- `break` / `continue` are only valid inside loops.
- `use` is only valid at the top level.
- `match` requires an `otherwise` case.
- `public` / `private` on local variables is invalid.
- `public changing` variables are invalid.
- Positional arguments cannot follow named arguments.
- Duplicate named arguments are invalid.
- `await` / `fire` only apply to actor method calls.
- Actor method calls must be `await`ed or `fire`d.
- `err` can only be used in functions returning `Result`.
- `otherwise` requires a `Result` on the left side.

---

## 13) Quick syntax summary

```
A ClassName:
    has:
        field: Type
    can method(arg: Type) -> Type:
        return its.field

public to function(arg: Type) -> Type:
    changing x = 1
    if x > 0:
        return x
    return err "bad" otherwise 0

match value:
    1, 2: return "small"
    otherwise:
        return "other"
```
