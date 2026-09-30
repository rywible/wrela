//! A repeatable workload for measuring scanning, generic selection, and parsing together.
use std::{hint::black_box, time::Instant};
use wrela_frontend::{
    lexer::{self, Limits},
    parse,
};

fn measure(mut action: impl FnMut(), iterations: u32) -> (f64, Vec<f64>) {
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start = Instant::now();
        for _ in 0..iterations {
            action();
        }
        samples.push(start.elapsed().as_secs_f64() * 1_000_000. / f64::from(iterations));
    }
    let mut ordered = samples.clone();
    ordered.sort_by(f64::total_cmp);
    (ordered[3], samples)
}

fn main() {
    let iterations: u32 = std::env::args()
        .nth(1)
        .map(|value| value.parse().unwrap())
        .unwrap_or(60);
    assert!(iterations > 0);
    let particles = include_str!("particles.wr").repeat(128);
    let generic_heavy = "fn f<T,U>(x: Array<Map<Key,Box<Value>>>) -> Array<T> where T: Copy + Bound<U> {\n let r = Record { value: make<Array<Map<Key,T>>>(x), compare: a < b }\n return consume<(T,U),[Box<T>]>(r)\n}\n".repeat(128);
    for (label, text) in [("particles", particles), ("generic-heavy", generic_heavy)] {
        let bytes = text.as_bytes();
        let document = parse(bytes);
        assert!(
            document.is_syntax_eligible(),
            "{label}: {:?}",
            document.diagnostics
        );
        assert_eq!(document.source.render(), bytes);
        let (lex_us, lex_samples) = measure(
            || {
                black_box(lexer::lex(black_box(bytes), Limits::default()));
            },
            iterations,
        );
        let (parse_us, parse_samples) = measure(
            || {
                black_box(parse(black_box(bytes)));
            },
            iterations,
        );
        println!(
            "{label}: bytes={} lex_us={lex_us:.1} parse_us={parse_us:.1} lex_samples_us={lex_samples:?} parse_samples_us={parse_samples:?}",
            bytes.len()
        );
    }
}
