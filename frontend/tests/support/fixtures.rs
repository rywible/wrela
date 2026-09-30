//! Independently authored cases: these are not snapshots from the parser.

pub const ACCEPTED: &[(&str, &str)] = &[
    (
        "block-comment-separator",
        "fn f() -> Unit { inspect(1) /* c\n d */ inspect(2)\nreturn }",
    ),
    (
        "permission-propagation",
        "fn step(mut particles: Array<Particle>, dt: Float) -> Unit {\n advance(mut particles, dt)?\n return\n}\n",
    ),
    (
        "generic-before-angle-parentheses",
        "fn run() -> Unit {\n (f\n<T>(x))\n}\n",
    ),
    (
        "generic-before-angle-comment",
        "fn run() -> Unit {\n (f// keep\n<T>(x))\n}\n",
    ),
    (
        "generic-before-angle-argument",
        "fn run() -> Unit {\n run(f\n<T>(x))\n}\n",
    ),
    (
        "generic-after-angle-parentheses",
        "fn run() -> Unit {\n (f<T>\n(x))\n}\n",
    ),
    (
        "lambda-block-resets",
        "fn run() -> Unit {\n run(fn(x) {\n first\n second\n return x\n })\n}\n",
    ),
    (
        "nested-generic-closers",
        "fn run(x: Map<Key, Array<Box<Value>>>) -> Unit {\n consume<Map<Key, Array<Box<Value>>>>(x)\n}\n",
    ),
    (
        "grouped-record-control",
        "fn run() -> Unit {\n if (Point { x: 1 }) { return }\n for p in (Points { x: 1 }) { return }\n every p in (Points { x: 1 }) { return }\n match (Tag { x: 1 }) { _ => { return }, }\n}\n",
    ),
    (
        "record-call-control",
        "fn run() -> Unit {\n if ready(Point { x: 1 }) { return }\n}\n",
    ),
    (
        "enum-payload-distinction",
        "enum Shape { Empty, Explicit(), Pair(Float, Float), }\n",
    ),
    (
        "types-constraints",
        "fn choose<T, U>(mut x: Array<T>, y: U) -> Box<T> where T: Numeric + Copy, U: Copy {\n return box<T>(x[0])\n}\n",
    ),
    (
        "unicode-unchecked-names",
        "fn café(δ: Number) -> Number {\n let café = δ\n return café\n}\n",
    ),
];

pub const REJECTED: &[(&str, &str)] = &[
    (
        "generic-before-angle-statement",
        "fn run() -> Unit {\n f\n<T>(x)\n}\n",
    ),
    (
        "generic-after-angle-statement",
        "fn run() -> Unit {\n f<T>\n(x)\n}\n",
    ),
    ("comparison-chain", "fn run() -> Unit {\n a < b < c\n}\n"),
    (
        "ungrouped-record-if",
        "fn run() -> Unit {\n if Point { x: 1 } { return }\n}\n",
    ),
    (
        "ungrouped-record-for",
        "fn run() -> Unit {\n for p in Points { x: 1 } { return }\n}\n",
    ),
    ("shift-excluded", "fn run() -> Unit {\n a >> b\n}\n"),
    (
        "value-if-excluded",
        "fn run() -> Unit {\n let x = if ready { return }\n}\n",
    ),
    (
        "assignment-expression-excluded",
        "fn run() -> Unit {\n consume(a = b)\n}\n",
    ),
    (
        "invisible-identifier",
        "fn run() -> Unit {\n let a\u{200d}b = 1\n}\n",
    ),
];

pub struct RecoveryCase {
    pub name: &'static str,
    pub source: &'static str,
    pub later_binding: Option<&'static str>,
    pub later_function: Option<&'static str>,
    pub minimum_diagnostics: usize,
    pub physical_eof: bool,
}

pub const RECOVERY: &[RecoveryCase] = &[
    RecoveryCase {
        name: "lexical-middle",
        source: "fn bad() -> Unit {\n @;\n return\n}\nfn good() -> Unit { return }\n",
        later_binding: None,
        later_function: Some("good"),
        minimum_diagnostics: 1,
        physical_eof: false,
    },
    RecoveryCase {
        name: "missing-initializer",
        source: "fn bad() -> Unit {\n let a = ;\n let tail = 3\n}\nfn good() -> Unit { return }\n",
        later_binding: Some("tail"),
        later_function: Some("good"),
        minimum_diagnostics: 1,
        physical_eof: false,
    },
    RecoveryCase {
        name: "two-errors",
        source: "fn bad() -> Unit {\n let a = ;\n var b = ;\n let tail = 3\n}\nfn good() -> Unit { return }\n",
        later_binding: Some("tail"),
        later_function: Some("good"),
        minimum_diagnostics: 2,
        physical_eof: false,
    },
    RecoveryCase {
        name: "nested-element",
        source: "fn bad() -> Unit {\n consume(1, , 3);\n let tail = 3\n}\nfn good() -> Unit { return }\n",
        later_binding: Some("tail"),
        later_function: Some("good"),
        minimum_diagnostics: 1,
        physical_eof: false,
    },
    RecoveryCase {
        name: "crossed-delimiters",
        source: "fn bad() -> Unit {\n consume([1, 2);\n let tail = 3\n}\nfn good() -> Unit { return }\n",
        later_binding: Some("tail"),
        later_function: Some("good"),
        minimum_diagnostics: 1,
        physical_eof: false,
    },
    RecoveryCase {
        name: "newline-string",
        source: "fn bad() -> Unit {\n let broken = \"oops\n let tail = 3\n}\nfn good() -> Unit { return }\n",
        later_binding: Some("tail"),
        later_function: Some("good"),
        minimum_diagnostics: 1,
        physical_eof: false,
    },
    RecoveryCase {
        name: "eof-trivia",
        source: "fn pending() -> Unit {\n let x = 1\n // café 😀\r\n \t",
        later_binding: None,
        later_function: None,
        minimum_diagnostics: 1,
        physical_eof: true,
    },
    RecoveryCase {
        name: "unclosed-comment",
        source: "fn completed() -> Unit { return }\n/* nested /* comment */ remainder 😀\r\n",
        later_binding: None,
        later_function: Some("completed"),
        minimum_diagnostics: 1,
        physical_eof: false,
    },
];
