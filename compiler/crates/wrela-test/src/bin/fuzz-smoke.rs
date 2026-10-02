//! Feeds random and mutated inputs through `check` and both renderings for a while, failing on
//! the first panic or broken invariant. Deterministic for a given seed.
//!
//! ```text
//! cargo run -p wrela-test --release --bin fuzz-smoke -- [--seconds N] [--seed S] [--start I] [--iterations K]
//! ```
//!
//! Without `--seed`, the seed comes from the clock and is printed, so every run explores new
//! inputs and any failure can be replayed with `--seed S --start I --iterations 1`.

use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wrela_test::fuzz;

const USAGE: &str = "usage: fuzz-smoke [--seconds N] [--seed S] [--start I] [--iterations K]  (defaults: 10 s, seed from the clock)";

fn main() -> ExitCode {
    let mut seconds = 10u64;
    let mut seed: Option<u64> = None;
    let mut start = 0u64;
    let mut iterations: Option<u64> = None;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let target = match flag.as_str() {
            "--seconds" => &mut seconds,
            "--start" => &mut start,
            "--seed" => seed.insert(0),
            "--iterations" => iterations.insert(0),
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("fuzz-smoke: unknown argument `{other}`\n{USAGE}");
                return ExitCode::from(2);
            }
        };
        match args.next().and_then(|value| value.parse().ok()) {
            Some(value) => *target = value,
            None => {
                eprintln!("fuzz-smoke: `{flag}` needs a number\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    let seed = seed.unwrap_or_else(|| {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        u64::try_from(now.as_nanos() & u128::from(u64::MAX)).unwrap_or(0)
    });
    let corpus = match wrela_test::fuzz_corpus() {
        Ok(corpus) => corpus,
        Err(problem) => {
            eprintln!("fuzz-smoke: can't build the corpus: {problem}");
            return ExitCode::from(2);
        }
    };
    let budget = Duration::from_secs(seconds);
    println!(
        "fuzz-smoke: seed {seed}, starting at input {start}, for {seconds} s{}, corpus of {} files",
        iterations
            .map(|n| format!(" or {n} inputs"))
            .unwrap_or_default(),
        corpus.len()
    );

    let began = Instant::now();
    let result = fuzz::run(seed, start, &corpus, |count| {
        iterations.is_none_or(|n| count < n) && began.elapsed() < budget
    });
    match result {
        Ok(count) => {
            println!(
                "fuzz-smoke: {count} inputs in {:.1} s, no panics or broken invariants",
                began.elapsed().as_secs_f64()
            );
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!(
                "fuzz-smoke: input {} (seed {}) failed: {}",
                failure.index, failure.seed, failure.problem
            );
            eprintln!("input: {:?}", failure.input);
            eprintln!(
                "replay: cargo run -p wrela-test --release --bin fuzz-smoke -- --seed {} --start {} --iterations 1",
                failure.seed, failure.index
            );
            ExitCode::FAILURE
        }
    }
}
