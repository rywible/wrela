# Representative Wrela Syntax Corpus

**Version 0.1. Planned normative examples for the approved feature, not executed production conformance.** Sources and expected relationships were authored independently of parser output. Detailed fixture spellings and diagnostic ranges are proposed implementation defaults; the eight approved criteria remain authoritative. Unknown types/library calls deliberately receive no semantic guarantees.

33 cases: 12 valid, 14 invalid, 7 semantic-later. 19 are approved-behavior seeds and 14 are proposed examples; this marks behavior provenance, not approval of every illustrative program spelling. CORPUS-STATUS.json lists the exact split. JSON is authoritative for exact UTF-8 bytes, CRLF, byte lengths, hashes and diagnostic ranges. Displayed blocks use normal page line endings for readability.

## Valid

### V01 Particles and visible mutation permission

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
record Particle { position: Vec3, velocity: Vec3, }
fn advance(mut particles: Array<Particle>, dt: f32) -> Result<Unit, StepError> {
    every p in particles {
        p.position = p.position + p.velocity * dt
    }
    return Result::Ok(())
}
fn tick(mut particles: Array<Particle>, dt: f32) -> Result<Unit, StepError> {
    advance(mut particles, dt)?
    return Result::Ok(())
}
```

Expected: Record fields and ordered function parameters survive. Every differs from For; mutation permission at parameter and call survives. Propagation wraps the advance call; multiplication binds before addition. No wave, storage, permission or Result semantics are executed.

### V02 Ordered iteration and ordinary indexing

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn visit(values: Array<f32>) -> Unit {
    for value in values { inspect(value) }
    inspect(values[0])
    return
}
```

Expected: For node differs from Every. Index selects values at literal 0; return has no expression.

### V03 Payload enums and absent versus empty payload syntax

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
enum Bare { Empty, }
enum Parenthesized { Empty(), }
enum Event { Hit(Vec3, f32), Idle, }
```

Expected: Bare.Empty has absent payload syntax; Parenthesized.Empty has present empty payload syntax. Hit has two ordered payload types; all trailing commas remain in CST.

Proposal: Preserving empty payload syntax in normalization is a suggested default under AC-3, not a separately approved criterion.

### V04 Constraints and nested annotation closes

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn twice<T>(value: T) -> T where T: Numeric + Copy {
    return value + value
}
fn keep<T>(values: Array<Array<T>>) -> Array<Array<T>> {
    return values
}
```

Expected: Generic parameter and two ordered constraints survive. Nested annotation closes parse without shift tokens. Types are syntax only; constraints are not resolved.

### V05 Generic tie break and permitted continuation

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn generic_examples(x: T) -> Unit {
    inspect(a<b>(x)); inspect(a<b)
    inspect((f
        <T>(x)))
    inspect((f// comment across a permitted continuation
        <T>(x)))
    run(f
        <T>(x))
    inspect((f<T>
        (x)))
    inspect(f<
        Array<T>,
    >(x))
    return
}
```

Expected: a<b>(x) is Call with type b; a<b is Less. All four continued f calls remain generic calls with type T. Multiline angle interiors are the reviewed direction; call boundary continuation still controls. The first three newline-before-angle shapes are known failing follow-up witnesses, not language exceptions.

### V06 Statement boundaries reset inside lambda blocks

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn boundaries(x: T) -> Unit {
    var n = 1; n = n +
        2
    f
    (x)
    run(fn() {
        f
        (x)
        return
    })
    return
    inspect(x)
}
```

Expected: f newline (x) forms two expression statements at each block level, never a call. The lambda block resets enclosing call continuation. Trailing + continues one addition; bare return ends before inspect. Unreachable-code checking is later.

### V07 Contextual and annotated fn lambdas

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn callbacks(values: Array<f32>) -> Unit {
    map(values, fn(value) { return value * 2 })
    map(values, fn(value: f32) -> f32 { return value + 1 })
    invoke(fn() { return 1 })
    return
}
```

Expected: Three fn lambdas retain explicit Return nodes. First parameter/result annotations are absent; second are explicit; third has zero parameters. Contextual inference is not implemented here.

### V08 Grouped record construction in control heads

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn test_point(p: Point) -> bool {
    if (Point { x: 1, } == p) { return true } else { return false }
}
```

Expected: Record construction is inside grouped control condition. If has distinct then/else blocks; both return explicitly.

### V09 Statement match with block arms

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
enum Event { Hit(Vec3, f32), Idle, }
fn magnitude(event: Event) -> f32 {
    match event {
        Event::Hit(position, power) => { return power },
        Event::Idle => { return 0 },
    }
}
```

Expected: Statement Match owns a scrutinee and two ordered arms. Hit has two ordered pattern bindings; each arm is a block with explicit return. Exhaustiveness and binding meaning are later checks.

### V10 Field and UI library call syntax only

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn body() -> Field {
    return field::smooth_min(field::sphere<f32>(1), field::box_field<f32>(2), 0.2)
}
fn panel() -> Ui {
    return ui::column([
        ui::text("Ready"),
        ui::button("Start", fn() { return Action::Start }),
    ])
}
```

Expected: Qualified library generic calls and nested ordinary calls retain ordered arguments. UI construction is an ordinary array/call/fn-lambda composition. No closed widget vocabulary, accessibility, rendering, field staging or layout syntax is selected.

### V11 Unicode spelling, nested comments, supported escapes and CRLF

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn unicode() -> Unit {
    /* outer /* nested */ remains outer */
    let é = "🚀\n\r\t\"\\"
    let é = "direct Unicode‌‍"
    inspect(é, é) // composed and decomposed stay distinct; join controls here: ‌‍
    return
}
```

Expected: Composed é and decomposed e plus combining acute are distinct identifiers. String contains direct astral Unicode and exactly the five permitted escape forms. Nested comments are one complete comment with nested structure if exposed; no interior text becomes syntax. All line terminators are CRLF in exact source. Approved identifier addendum allows join controls in strings/comments; their bytes stay intact.

### V12 Independent precedence and association witness

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn order(a: f32, b: f32, c: f32, enabled: bool) -> bool {
    return a - b - c * 2 < 10 && enabled || false
}
```

Expected: Expected shape: Or(And(Less(Sub(Sub(a,b),Mul(c,2)),10),enabled),false). Grouping in this independently specified shape expresses structure, not source punctuation.

### I01 No next line generic call at statement level

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad(x: T) -> Unit { f<T>
(x)
return }
```

Expected: Reject; the newline cannot complete a bare generic call.

### I02 Ungrouped record in control head

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad(p: Point) -> Unit { if Point { x: 1 } == p { return } }
```

Expected: Reject direct ungrouped record construction in the control head.

### I03 Raw string is outside the initial slice

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let s = r#"raw"#
return }
```

Expected: Reject raw strings; their old provisional lexer defect is excluded, not fixed.

### I04 lexical-middle

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    @
    return
}
fn good() -> Unit { return }
```

Expected: One lexical primary error at @; retain return and fn good structure. All bytes retained; next-phase gate rejects.

Proposed primary byte ranges: [[23, 24]].

### I05 two-errors

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let a = ;
    let b = ;
    let c = 3
    return
}
fn good() -> Unit { return }
```

Expected: Two independent missing-initializer primary diagnostics at the two semicolons. Retain Local(c) and Function(good); source ownership remains complete.

Proposed primary byte ranges: [[31, 32], [45, 46]].

### I06 nested-element

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    consume([1, , 2]);
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Primary diagnostic at the second comma in otherwise paired brackets. Synchronize within the array list; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[35, 36]].

### I07 crossed-delimiters

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    consume([1, 2)]);
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Report the crossed delimiter at the first ) while [ is open. Quarantine the malformed statement through its explicit semicolon; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[36, 37]].

### I08 EOF-trivia

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let s = "🚀"
    // trailing comment
  
```

Expected: EOF primary diagnostic is zero-width at physical source byte length. Retain typed source and partial function prefix; do not promise a completed function or suffix.

Proposed primary byte ranges: [[67, 67]].

### I09 newline-string

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let s = "unfinished
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Unterminated string error owns opening quote through before physical newline. Recover at newline; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[31, 42]].

### I10 unclosed-comment

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit { return }
/* outer /* closed inner */ still open
fn later() -> Unit { return }
```

Expected: Retain unclosed comment remainder through EOF; report an unterminated block comment. Earlier function structure may survive; later text belongs to comment/error bytes, with no later-function promise.

### I11 Pipe lambda is excluded

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { map(values, |x| => x)
return }
```

Expected: Reject pipe/arrow lambda spelling.

### I12 Unknown string escape is excluded

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let s = "\u1234"
return }
```

Expected: Reject Unicode escape syntax; direct Unicode spelling is available.

### I13 Excluded join control U200C in identifier

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let bad‌name = 1
return }
```

Expected: Approved Unicode addendum: diagnose U+200C in identifier, preserve all bytes, never strip or normalize.

Proposed primary byte ranges: [[26, 29]].

### I14 Excluded join control U200D in identifier

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let bad‍name = 1
return }
```

Expected: Approved Unicode addendum: diagnose U+200D in identifier, preserve all bytes, never strip or normalize.

Proposed primary byte ranges: [[26, 29]].

### S01 Read-only local assignment is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn readonly() -> Unit { let x = 1
x = 2
return }
```

Expected: Syntax accepts Local(let) plus Assign; permission enforcement rejects this later.

### S02 Mutation target legality is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn literal_target() -> Unit { change(mut 3)
return }
```

Expected: Syntax retains mut on a literal argument; target legality belongs to later checking.

### S03 No shadowing enforcement yet

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn shadow(x: i32) -> i32 { let x = 2
return x }
```

Expected: Syntax accepts distinct declarations; the accepted no-shadowing rule is enforced later.

### S04 Exhaustiveness enforcement later

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
enum Event { Hit(f32), Idle }
fn partial(event: Event) -> Unit { match event { Event::Idle => { return }, } }
```

Expected: Syntax accepts one match arm; missing Hit coverage belongs to later checking.

### S05 Ignored Result enforcement later

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn ignore() -> Unit { might_fail()
return }
```

Expected: Syntax accepts expression statement; ignored-Result rejection needs a resolved Result type later.

### S06 Final expression never creates a return

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn missing_return() -> i32 { 1 }
```

Expected: Body contains ExpressionStatement(Number(1)), not Return. Missing-return/type validity belongs to later checking; semicolon cannot change this meaning.

### S07 Missing mutation permission is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn caller(values: Array<i32>) -> Unit { change(values)
return }
```

Expected: Syntax accepts unmarked argument; callee-specific permission requirement is checked later.

## Invalid

### V01 Particles and visible mutation permission

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
record Particle { position: Vec3, velocity: Vec3, }
fn advance(mut particles: Array<Particle>, dt: f32) -> Result<Unit, StepError> {
    every p in particles {
        p.position = p.position + p.velocity * dt
    }
    return Result::Ok(())
}
fn tick(mut particles: Array<Particle>, dt: f32) -> Result<Unit, StepError> {
    advance(mut particles, dt)?
    return Result::Ok(())
}
```

Expected: Record fields and ordered function parameters survive. Every differs from For; mutation permission at parameter and call survives. Propagation wraps the advance call; multiplication binds before addition. No wave, storage, permission or Result semantics are executed.

### V02 Ordered iteration and ordinary indexing

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn visit(values: Array<f32>) -> Unit {
    for value in values { inspect(value) }
    inspect(values[0])
    return
}
```

Expected: For node differs from Every. Index selects values at literal 0; return has no expression.

### V03 Payload enums and absent versus empty payload syntax

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
enum Bare { Empty, }
enum Parenthesized { Empty(), }
enum Event { Hit(Vec3, f32), Idle, }
```

Expected: Bare.Empty has absent payload syntax; Parenthesized.Empty has present empty payload syntax. Hit has two ordered payload types; all trailing commas remain in CST.

Proposal: Preserving empty payload syntax in normalization is a suggested default under AC-3, not a separately approved criterion.

### V04 Constraints and nested annotation closes

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn twice<T>(value: T) -> T where T: Numeric + Copy {
    return value + value
}
fn keep<T>(values: Array<Array<T>>) -> Array<Array<T>> {
    return values
}
```

Expected: Generic parameter and two ordered constraints survive. Nested annotation closes parse without shift tokens. Types are syntax only; constraints are not resolved.

### V05 Generic tie break and permitted continuation

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn generic_examples(x: T) -> Unit {
    inspect(a<b>(x)); inspect(a<b)
    inspect((f
        <T>(x)))
    inspect((f// comment across a permitted continuation
        <T>(x)))
    run(f
        <T>(x))
    inspect((f<T>
        (x)))
    inspect(f<
        Array<T>,
    >(x))
    return
}
```

Expected: a<b>(x) is Call with type b; a<b is Less. All four continued f calls remain generic calls with type T. Multiline angle interiors are the reviewed direction; call boundary continuation still controls. The first three newline-before-angle shapes are known failing follow-up witnesses, not language exceptions.

### V06 Statement boundaries reset inside lambda blocks

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn boundaries(x: T) -> Unit {
    var n = 1; n = n +
        2
    f
    (x)
    run(fn() {
        f
        (x)
        return
    })
    return
    inspect(x)
}
```

Expected: f newline (x) forms two expression statements at each block level, never a call. The lambda block resets enclosing call continuation. Trailing + continues one addition; bare return ends before inspect. Unreachable-code checking is later.

### V07 Contextual and annotated fn lambdas

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn callbacks(values: Array<f32>) -> Unit {
    map(values, fn(value) { return value * 2 })
    map(values, fn(value: f32) -> f32 { return value + 1 })
    invoke(fn() { return 1 })
    return
}
```

Expected: Three fn lambdas retain explicit Return nodes. First parameter/result annotations are absent; second are explicit; third has zero parameters. Contextual inference is not implemented here.

### V08 Grouped record construction in control heads

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn test_point(p: Point) -> bool {
    if (Point { x: 1, } == p) { return true } else { return false }
}
```

Expected: Record construction is inside grouped control condition. If has distinct then/else blocks; both return explicitly.

### V09 Statement match with block arms

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
enum Event { Hit(Vec3, f32), Idle, }
fn magnitude(event: Event) -> f32 {
    match event {
        Event::Hit(position, power) => { return power },
        Event::Idle => { return 0 },
    }
}
```

Expected: Statement Match owns a scrutinee and two ordered arms. Hit has two ordered pattern bindings; each arm is a block with explicit return. Exhaustiveness and binding meaning are later checks.

### V10 Field and UI library call syntax only

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn body() -> Field {
    return field::smooth_min(field::sphere<f32>(1), field::box_field<f32>(2), 0.2)
}
fn panel() -> Ui {
    return ui::column([
        ui::text("Ready"),
        ui::button("Start", fn() { return Action::Start }),
    ])
}
```

Expected: Qualified library generic calls and nested ordinary calls retain ordered arguments. UI construction is an ordinary array/call/fn-lambda composition. No closed widget vocabulary, accessibility, rendering, field staging or layout syntax is selected.

### V11 Unicode spelling, nested comments, supported escapes and CRLF

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn unicode() -> Unit {
    /* outer /* nested */ remains outer */
    let é = "🚀\n\r\t\"\\"
    let é = "direct Unicode‌‍"
    inspect(é, é) // composed and decomposed stay distinct; join controls here: ‌‍
    return
}
```

Expected: Composed é and decomposed e plus combining acute are distinct identifiers. String contains direct astral Unicode and exactly the five permitted escape forms. Nested comments are one complete comment with nested structure if exposed; no interior text becomes syntax. All line terminators are CRLF in exact source. Approved identifier addendum allows join controls in strings/comments; their bytes stay intact.

### V12 Independent precedence and association witness

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn order(a: f32, b: f32, c: f32, enabled: bool) -> bool {
    return a - b - c * 2 < 10 && enabled || false
}
```

Expected: Expected shape: Or(And(Less(Sub(Sub(a,b),Mul(c,2)),10),enabled),false). Grouping in this independently specified shape expresses structure, not source punctuation.

### I01 No next line generic call at statement level

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad(x: T) -> Unit { f<T>
(x)
return }
```

Expected: Reject; the newline cannot complete a bare generic call.

### I02 Ungrouped record in control head

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad(p: Point) -> Unit { if Point { x: 1 } == p { return } }
```

Expected: Reject direct ungrouped record construction in the control head.

### I03 Raw string is outside the initial slice

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let s = r#"raw"#
return }
```

Expected: Reject raw strings; their old provisional lexer defect is excluded, not fixed.

### I04 lexical-middle

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    @
    return
}
fn good() -> Unit { return }
```

Expected: One lexical primary error at @; retain return and fn good structure. All bytes retained; next-phase gate rejects.

Proposed primary byte ranges: [[23, 24]].

### I05 two-errors

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let a = ;
    let b = ;
    let c = 3
    return
}
fn good() -> Unit { return }
```

Expected: Two independent missing-initializer primary diagnostics at the two semicolons. Retain Local(c) and Function(good); source ownership remains complete.

Proposed primary byte ranges: [[31, 32], [45, 46]].

### I06 nested-element

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    consume([1, , 2]);
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Primary diagnostic at the second comma in otherwise paired brackets. Synchronize within the array list; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[35, 36]].

### I07 crossed-delimiters

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    consume([1, 2)]);
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Report the crossed delimiter at the first ) while [ is open. Quarantine the malformed statement through its explicit semicolon; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[36, 37]].

### I08 EOF-trivia

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let s = "🚀"
    // trailing comment
  
```

Expected: EOF primary diagnostic is zero-width at physical source byte length. Retain typed source and partial function prefix; do not promise a completed function or suffix.

Proposed primary byte ranges: [[67, 67]].

### I09 newline-string

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let s = "unfinished
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Unterminated string error owns opening quote through before physical newline. Recover at newline; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[31, 42]].

### I10 unclosed-comment

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit { return }
/* outer /* closed inner */ still open
fn later() -> Unit { return }
```

Expected: Retain unclosed comment remainder through EOF; report an unterminated block comment. Earlier function structure may survive; later text belongs to comment/error bytes, with no later-function promise.

### I11 Pipe lambda is excluded

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { map(values, |x| => x)
return }
```

Expected: Reject pipe/arrow lambda spelling.

### I12 Unknown string escape is excluded

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let s = "\u1234"
return }
```

Expected: Reject Unicode escape syntax; direct Unicode spelling is available.

### I13 Excluded join control U200C in identifier

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let bad‌name = 1
return }
```

Expected: Approved Unicode addendum: diagnose U+200C in identifier, preserve all bytes, never strip or normalize.

Proposed primary byte ranges: [[26, 29]].

### I14 Excluded join control U200D in identifier

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let bad‍name = 1
return }
```

Expected: Approved Unicode addendum: diagnose U+200D in identifier, preserve all bytes, never strip or normalize.

Proposed primary byte ranges: [[26, 29]].

### S01 Read-only local assignment is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn readonly() -> Unit { let x = 1
x = 2
return }
```

Expected: Syntax accepts Local(let) plus Assign; permission enforcement rejects this later.

### S02 Mutation target legality is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn literal_target() -> Unit { change(mut 3)
return }
```

Expected: Syntax retains mut on a literal argument; target legality belongs to later checking.

### S03 No shadowing enforcement yet

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn shadow(x: i32) -> i32 { let x = 2
return x }
```

Expected: Syntax accepts distinct declarations; the accepted no-shadowing rule is enforced later.

### S04 Exhaustiveness enforcement later

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
enum Event { Hit(f32), Idle }
fn partial(event: Event) -> Unit { match event { Event::Idle => { return }, } }
```

Expected: Syntax accepts one match arm; missing Hit coverage belongs to later checking.

### S05 Ignored Result enforcement later

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn ignore() -> Unit { might_fail()
return }
```

Expected: Syntax accepts expression statement; ignored-Result rejection needs a resolved Result type later.

### S06 Final expression never creates a return

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn missing_return() -> i32 { 1 }
```

Expected: Body contains ExpressionStatement(Number(1)), not Return. Missing-return/type validity belongs to later checking; semicolon cannot change this meaning.

### S07 Missing mutation permission is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn caller(values: Array<i32>) -> Unit { change(values)
return }
```

Expected: Syntax accepts unmarked argument; callee-specific permission requirement is checked later.

## Semantic-later

### V01 Particles and visible mutation permission

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
record Particle { position: Vec3, velocity: Vec3, }
fn advance(mut particles: Array<Particle>, dt: f32) -> Result<Unit, StepError> {
    every p in particles {
        p.position = p.position + p.velocity * dt
    }
    return Result::Ok(())
}
fn tick(mut particles: Array<Particle>, dt: f32) -> Result<Unit, StepError> {
    advance(mut particles, dt)?
    return Result::Ok(())
}
```

Expected: Record fields and ordered function parameters survive. Every differs from For; mutation permission at parameter and call survives. Propagation wraps the advance call; multiplication binds before addition. No wave, storage, permission or Result semantics are executed.

### V02 Ordered iteration and ordinary indexing

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn visit(values: Array<f32>) -> Unit {
    for value in values { inspect(value) }
    inspect(values[0])
    return
}
```

Expected: For node differs from Every. Index selects values at literal 0; return has no expression.

### V03 Payload enums and absent versus empty payload syntax

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
enum Bare { Empty, }
enum Parenthesized { Empty(), }
enum Event { Hit(Vec3, f32), Idle, }
```

Expected: Bare.Empty has absent payload syntax; Parenthesized.Empty has present empty payload syntax. Hit has two ordered payload types; all trailing commas remain in CST.

Proposal: Preserving empty payload syntax in normalization is a suggested default under AC-3, not a separately approved criterion.

### V04 Constraints and nested annotation closes

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn twice<T>(value: T) -> T where T: Numeric + Copy {
    return value + value
}
fn keep<T>(values: Array<Array<T>>) -> Array<Array<T>> {
    return values
}
```

Expected: Generic parameter and two ordered constraints survive. Nested annotation closes parse without shift tokens. Types are syntax only; constraints are not resolved.

### V05 Generic tie break and permitted continuation

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn generic_examples(x: T) -> Unit {
    inspect(a<b>(x)); inspect(a<b)
    inspect((f
        <T>(x)))
    inspect((f// comment across a permitted continuation
        <T>(x)))
    run(f
        <T>(x))
    inspect((f<T>
        (x)))
    inspect(f<
        Array<T>,
    >(x))
    return
}
```

Expected: a<b>(x) is Call with type b; a<b is Less. All four continued f calls remain generic calls with type T. Multiline angle interiors are the reviewed direction; call boundary continuation still controls. The first three newline-before-angle shapes are known failing follow-up witnesses, not language exceptions.

### V06 Statement boundaries reset inside lambda blocks

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn boundaries(x: T) -> Unit {
    var n = 1; n = n +
        2
    f
    (x)
    run(fn() {
        f
        (x)
        return
    })
    return
    inspect(x)
}
```

Expected: f newline (x) forms two expression statements at each block level, never a call. The lambda block resets enclosing call continuation. Trailing + continues one addition; bare return ends before inspect. Unreachable-code checking is later.

### V07 Contextual and annotated fn lambdas

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn callbacks(values: Array<f32>) -> Unit {
    map(values, fn(value) { return value * 2 })
    map(values, fn(value: f32) -> f32 { return value + 1 })
    invoke(fn() { return 1 })
    return
}
```

Expected: Three fn lambdas retain explicit Return nodes. First parameter/result annotations are absent; second are explicit; third has zero parameters. Contextual inference is not implemented here.

### V08 Grouped record construction in control heads

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn test_point(p: Point) -> bool {
    if (Point { x: 1, } == p) { return true } else { return false }
}
```

Expected: Record construction is inside grouped control condition. If has distinct then/else blocks; both return explicitly.

### V09 Statement match with block arms

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
enum Event { Hit(Vec3, f32), Idle, }
fn magnitude(event: Event) -> f32 {
    match event {
        Event::Hit(position, power) => { return power },
        Event::Idle => { return 0 },
    }
}
```

Expected: Statement Match owns a scrutinee and two ordered arms. Hit has two ordered pattern bindings; each arm is a block with explicit return. Exhaustiveness and binding meaning are later checks.

### V10 Field and UI library call syntax only

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn body() -> Field {
    return field::smooth_min(field::sphere<f32>(1), field::box_field<f32>(2), 0.2)
}
fn panel() -> Ui {
    return ui::column([
        ui::text("Ready"),
        ui::button("Start", fn() { return Action::Start }),
    ])
}
```

Expected: Qualified library generic calls and nested ordinary calls retain ordered arguments. UI construction is an ordinary array/call/fn-lambda composition. No closed widget vocabulary, accessibility, rendering, field staging or layout syntax is selected.

### V11 Unicode spelling, nested comments, supported escapes and CRLF

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn unicode() -> Unit {
    /* outer /* nested */ remains outer */
    let é = "🚀\n\r\t\"\\"
    let é = "direct Unicode‌‍"
    inspect(é, é) // composed and decomposed stay distinct; join controls here: ‌‍
    return
}
```

Expected: Composed é and decomposed e plus combining acute are distinct identifiers. String contains direct astral Unicode and exactly the five permitted escape forms. Nested comments are one complete comment with nested structure if exposed; no interior text becomes syntax. All line terminators are CRLF in exact source. Approved identifier addendum allows join controls in strings/comments; their bytes stay intact.

### V12 Independent precedence and association witness

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn order(a: f32, b: f32, c: f32, enabled: bool) -> bool {
    return a - b - c * 2 < 10 && enabled || false
}
```

Expected: Expected shape: Or(And(Less(Sub(Sub(a,b),Mul(c,2)),10),enabled),false). Grouping in this independently specified shape expresses structure, not source punctuation.

### I01 No next line generic call at statement level

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad(x: T) -> Unit { f<T>
(x)
return }
```

Expected: Reject; the newline cannot complete a bare generic call.

### I02 Ungrouped record in control head

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad(p: Point) -> Unit { if Point { x: 1 } == p { return } }
```

Expected: Reject direct ungrouped record construction in the control head.

### I03 Raw string is outside the initial slice

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let s = r#"raw"#
return }
```

Expected: Reject raw strings; their old provisional lexer defect is excluded, not fixed.

### I04 lexical-middle

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    @
    return
}
fn good() -> Unit { return }
```

Expected: One lexical primary error at @; retain return and fn good structure. All bytes retained; next-phase gate rejects.

Proposed primary byte ranges: [[23, 24]].

### I05 two-errors

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let a = ;
    let b = ;
    let c = 3
    return
}
fn good() -> Unit { return }
```

Expected: Two independent missing-initializer primary diagnostics at the two semicolons. Retain Local(c) and Function(good); source ownership remains complete.

Proposed primary byte ranges: [[31, 32], [45, 46]].

### I06 nested-element

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    consume([1, , 2]);
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Primary diagnostic at the second comma in otherwise paired brackets. Synchronize within the array list; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[35, 36]].

### I07 crossed-delimiters

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    consume([1, 2)]);
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Report the crossed delimiter at the first ) while [ is open. Quarantine the malformed statement through its explicit semicolon; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[36, 37]].

### I08 EOF-trivia

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let s = "🚀"
    // trailing comment
  
```

Expected: EOF primary diagnostic is zero-width at physical source byte length. Retain typed source and partial function prefix; do not promise a completed function or suffix.

Proposed primary byte ranges: [[67, 67]].

### I09 newline-string

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit {
    let s = "unfinished
    let tail = 3
    return
}
fn good() -> Unit { return }
```

Expected: Unterminated string error owns opening quote through before physical newline. Recover at newline; retain Local(tail) and Function(good).

Proposed primary byte ranges: [[31, 42]].

### I10 unclosed-comment

Status: proposed_example. Concrete spellings, shape or recovery contract need implementation-policy adoption; this case is not an extra acceptance requirement.

```text
fn bad() -> Unit { return }
/* outer /* closed inner */ still open
fn later() -> Unit { return }
```

Expected: Retain unclosed comment remainder through EOF; report an unterminated block comment. Earlier function structure may survive; later text belongs to comment/error bytes, with no later-function promise.

### I11 Pipe lambda is excluded

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { map(values, |x| => x)
return }
```

Expected: Reject pipe/arrow lambda spelling.

### I12 Unknown string escape is excluded

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let s = "\u1234"
return }
```

Expected: Reject Unicode escape syntax; direct Unicode spelling is available.

### I13 Excluded join control U200C in identifier

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let bad‌name = 1
return }
```

Expected: Approved Unicode addendum: diagnose U+200C in identifier, preserve all bytes, never strip or normalize.

Proposed primary byte ranges: [[26, 29]].

### I14 Excluded join control U200D in identifier

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn bad() -> Unit { let bad‍name = 1
return }
```

Expected: Approved Unicode addendum: diagnose U+200D in identifier, preserve all bytes, never strip or normalize.

Proposed primary byte ranges: [[26, 29]].

### S01 Read-only local assignment is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn readonly() -> Unit { let x = 1
x = 2
return }
```

Expected: Syntax accepts Local(let) plus Assign; permission enforcement rejects this later.

### S02 Mutation target legality is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn literal_target() -> Unit { change(mut 3)
return }
```

Expected: Syntax retains mut on a literal argument; target legality belongs to later checking.

### S03 No shadowing enforcement yet

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn shadow(x: i32) -> i32 { let x = 2
return x }
```

Expected: Syntax accepts distinct declarations; the accepted no-shadowing rule is enforced later.

### S04 Exhaustiveness enforcement later

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
enum Event { Hit(f32), Idle }
fn partial(event: Event) -> Unit { match event { Event::Idle => { return }, } }
```

Expected: Syntax accepts one match arm; missing Hit coverage belongs to later checking.

### S05 Ignored Result enforcement later

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn ignore() -> Unit { might_fail()
return }
```

Expected: Syntax accepts expression statement; ignored-Result rejection needs a resolved Result type later.

### S06 Final expression never creates a return

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn missing_return() -> i32 { 1 }
```

Expected: Body contains ExpressionStatement(Number(1)), not Return. Missing-return/type validity belongs to later checking; semicolon cannot change this meaning.

### S07 Missing mutation permission is semantic

Status: approved_behavior_seed. Expected behavior follows accepted language direction and approved feature boundary; full source spellings remain representative and not individually user-reviewed.

```text
fn caller(values: Array<i32>) -> Unit { change(values)
return }
```

Expected: Syntax accepts unmarked argument; callee-specific permission requirement is checked later.

## Execution status

The new corpus has not been fed to either spike as a conformance authority. Separately, the unchanged original audit replay reproduced 222 responses, and the focused follow-up audit reproduced 31 expected outcomes plus three known continuation failures. Those executed research results do not claim this corpus passes. V05 contains their future-required newline-before-angle shapes; V04/V11 also cover known unimplemented nested-annotation/nested-comment work. Full arbitrary malformed trees, UI layout syntax and runtime/library semantics are excluded.
