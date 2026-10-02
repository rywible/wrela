//! Fuzz smoke testing on stable Rust: random and mutated inputs through the whole check and both
//! renderings, asserting no panics and a few invariants.
//!
//! Input `i` of a run depends only on the seed, `i` and the corpus, so a failure is reproduced by
//! its seed and index. Each input is checked:
//! - without panicking, through `check`, the text rendering and the JSON rendering;
//! - with every span inside its file and the JSON parsing back with one entry per diagnostic;
//! - with each `/* */` rewrite (E0002's fix) leaving no diagnostic inside the comment it writes;
//! - incrementally: a session edited from the previous input gives the same diagnostics as a
//!   fresh one, which exercises the query layer's invalidation.

use std::panic::{AssertUnwindSafe, catch_unwind};

use wrela_driver::Session;

use crate::harness::panic_message;

/// SplitMix64: small, fast and deterministic everywhere. Not for cryptography.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`; `n` must be positive.
    pub fn below(&mut self, n: usize) -> usize {
        // The modulo bias is irrelevant for fuzzing.
        usize::try_from(self.next_u64() % n.max(1) as u64).unwrap_or(0)
    }

    pub fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Fragments that stress the lexer: comment edges, number edges, odd characters.
const FRAGMENTS: &[&str] = &[
    "/*",
    "*/",
    "//",
    "///",
    "/**/",
    "/* /* */",
    "*/ */",
    "\n",
    "\r\n",
    "\r",
    "\u{000B}",
    "\u{2028}",
    "\t",
    " ",
    "$",
    "#",
    "`",
    "\"",
    "'",
    "?",
    "\\",
    "π",
    "é",
    "\u{00A0}",
    "\u{200B}",
    "\u{202E}",
    "\u{FEFF}",
    "\u{1F600}",
    "\u{0}",
    "\u{1B}[31m",
    "0x",
    "0b",
    "1e-",
    "1.0e+9",
    "1..2",
    "..=",
    "t.0.1",
    "**=",
    "<<=",
    "->",
    "=>",
    "::",
    "fn",
    "let",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "@compute(64)",
    "//~ ERROR E0001",
];

/// Generates input `index` for `seed`.
pub fn input(seed: u64, index: u64, corpus: &[String]) -> String {
    let mut rng = Rng::new(seed ^ index.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    rng.next_u64();
    match rng.below(4) {
        0 => random_bytes(&mut rng),
        1 => random_text(&mut rng),
        _ if corpus.is_empty() => random_text(&mut rng),
        _ => mutate(&mut rng, corpus),
    }
}

/// Arbitrary bytes, made valid UTF-8 the way the CLI can't: the CLI rejects invalid files, so the
/// driver only ever sees `str`.
fn random_bytes(rng: &mut Rng) -> String {
    let len = rng.below(512);
    let bytes: Vec<u8> = (0..len).map(|_| (rng.next_u64() & 0xFF) as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Text that is mostly ASCII and mostly token-shaped.
fn random_text(rng: &mut Rng) -> String {
    const ASCII: &[u8] = b"abcxyz_ABZ0123456789 \n\t()[]{}<>=+-*/%^&|!.,;:@$#`'\"?~\\";
    let len = rng.below(256);
    let mut text = String::new();
    while text.len() < len {
        if rng.chance(6) {
            text.push_str(rng.pick(FRAGMENTS));
        } else {
            text.push(char::from(*rng.pick(ASCII)));
        }
    }
    text
}

/// A corpus file with a few random edits.
fn mutate(rng: &mut Rng, corpus: &[String]) -> String {
    let mut chars: Vec<char> = rng.pick(corpus).chars().collect();
    for _ in 0..=rng.below(8) {
        let at = rng.below(chars.len() + 1);
        match rng.below(5) {
            0 if !chars.is_empty() => {
                let end = (at + rng.below(16)).min(chars.len());
                chars.drain(at.min(end)..end);
            }
            1 => {
                let fragment = rng.pick(FRAGMENTS);
                chars.splice(at..at, fragment.chars());
            }
            2 if !chars.is_empty() => {
                let i = rng.below(chars.len());
                chars[i] = char::from_u32(u32::try_from(rng.below(0x11_0000)).unwrap_or(0))
                    .unwrap_or('\u{FFFD}');
            }
            3 => {
                let other: Vec<char> = rng.pick(corpus).chars().collect();
                let from = rng.below(other.len() + 1);
                let to = (from + rng.below(64)).min(other.len());
                chars.splice(at..at, other[from..to].iter().copied());
            }
            _ => {
                let end = (at + rng.below(32)).min(chars.len());
                let copy: Vec<char> = chars[at..end].to_vec();
                chars.splice(at..at, copy);
            }
        }
    }
    chars.into_iter().collect()
}

/// Checks one input (after `previous`, for the incremental comparison). `Err` describes a panic
/// or a broken invariant.
pub fn check_input(input: &str, previous: &str) -> Result<(), String> {
    match catch_unwind(AssertUnwindSafe(|| invariants(input, previous))) {
        Ok(result) => result,
        Err(payload) => Err(format!("panicked: {}", panic_message(payload.as_ref()))),
    }
}

fn invariants(input: &str, previous: &str) -> Result<(), String> {
    let mut fresh = Session::new();
    let file = fresh
        .add_file("fuzz.wrela", input)
        .map_err(|e| e.to_string())?;
    let diagnostics = fresh.check(file).map_err(|e| format!("query error: {e}"))?;

    let len = input.len();
    for d in diagnostics.iter() {
        let spans = std::iter::once(d.primary.span)
            .chain(d.secondary.iter().map(|l| l.span))
            .chain(d.help.iter().flat_map(|h| h.edits.iter().map(|e| e.span)));
        for span in spans {
            let in_bounds =
                span.file() == file && span.start() <= span.end() && span.end() as usize <= len;
            let on_chars = input.is_char_boundary(span.start() as usize)
                && input.is_char_boundary(span.end() as usize);
            if !in_bounds || !on_chars {
                return Err(format!(
                    "{}: span {span:?} is outside the input or splits a char",
                    d.code
                ));
            }
        }
    }

    let _ = wrela_diag::render_all(&diagnostics, fresh.sources());
    let json = wrela_diag::to_json(&diagnostics, fresh.sources());
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("the JSON doesn't parse: {e}"))?;
    let count = value["diagnostics"].as_array().map_or(usize::MAX, Vec::len);
    if value["version"] != wrela_diag::JSON_VERSION || count != diagnostics.len() {
        return Err(format!("the JSON has the wrong version or count:\n{json}"));
    }

    // A `/* */` rewrite is the whole fix: the comment it writes has no diagnostic inside it.
    let block_comments = diagnostics
        .iter()
        .filter(|d| d.code == wrela_diag::codes::BLOCK_COMMENT);
    for edit in block_comments.flat_map(|d| d.help.iter().flat_map(|h| &h.edits)) {
        let mut text = input.to_string();
        text.replace_range(edit.span.range(), &edit.replacement);
        let mut fixed = Session::new();
        let file = fixed
            .add_file("fuzz.wrela", text)
            .map_err(|e| e.to_string())?;
        let after = fixed.check(file).map_err(|e| format!("query error: {e}"))?;
        let (start, end) = (
            edit.span.start(),
            edit.span.start() + edit.replacement.len() as u32,
        );
        if let Some(d) = after
            .iter()
            .find(|d| d.primary.span.start() < end && d.primary.span.end() > start)
        {
            return Err(format!(
                "the E0002 rewrite {:?} leaves {} inside it: {}",
                edit.replacement, d.code, d.message
            ));
        }
    }

    let mut edited = Session::new();
    let file = edited
        .add_file("fuzz.wrela", previous)
        .map_err(|e| e.to_string())?;
    edited
        .check(file)
        .map_err(|e| format!("query error: {e}"))?;
    edited.set_text(file, input).map_err(|e| e.to_string())?;
    let incremental = edited
        .check(file)
        .map_err(|e| format!("query error: {e}"))?;
    if incremental != diagnostics {
        return Err("checking after an edit differs from checking fresh".to_string());
    }
    Ok(())
}

/// A failed input, with what's needed to reproduce it.
#[derive(Debug)]
pub struct Failure {
    pub seed: u64,
    pub index: u64,
    pub input: String,
    pub problem: String,
}

/// Runs inputs `start..` until `keep_going(count)` says stop. Returns how many ran.
pub fn run(
    seed: u64,
    start: u64,
    corpus: &[String],
    mut keep_going: impl FnMut(u64) -> bool,
) -> Result<u64, Box<Failure>> {
    // The input before `start`, so a replay from `start` sees the same edit as the original run.
    let mut previous = start
        .checked_sub(1)
        .map(|i| input(seed, i, corpus))
        .unwrap_or_default();
    let mut count = 0;
    while keep_going(count) {
        let index = start + count;
        let input = input(seed, index, corpus);
        if let Err(problem) = check_input(&input, &previous) {
            return Err(Box::new(Failure {
                seed,
                index,
                input,
                problem,
            }));
        }
        previous = input;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_are_deterministic_per_seed_and_index() {
        let corpus = vec!["fn f() {}\n".to_string(), "/* x */".to_string()];
        for index in 0..50 {
            assert_eq!(input(7, index, &corpus), input(7, index, &corpus));
        }
        let a: Vec<String> = (0..20).map(|i| input(1, i, &corpus)).collect();
        let b: Vec<String> = (0..20).map(|i| input(2, i, &corpus)).collect();
        assert_ne!(a, b);
    }

    /// The quick variant for `cargo test`; CI also runs the `fuzz-smoke` binary for a minute.
    #[test]
    fn smoke() {
        let corpus = crate::fuzz_corpus().unwrap();
        assert!(
            corpus.len() > 10,
            "the corpus should hold the tests, explanations and docs"
        );
        let ran = run(0x5EED, 0, &corpus, |count| count < 2000);
        match ran {
            Ok(count) => assert_eq!(count, 2000),
            Err(f) => panic!(
                "seed {} input {}: {}\n{:?}",
                f.seed, f.index, f.problem, f.input
            ),
        }
    }
}
