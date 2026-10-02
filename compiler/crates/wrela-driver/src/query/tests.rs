use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{Cycle, Db, Derived, Input, Query, QueryError};

/// `text(k)`: an input string.
struct Text;
impl Query for Text {
    type Key = u32;
    type Value = String;
    const NAME: &'static str = "text";
}
impl Input for Text {}

/// `flag(k)`: an input switch.
struct Flag;
impl Query for Flag {
    type Key = u32;
    type Value = bool;
    const NAME: &'static str = "flag";
}
impl Input for Flag {}

/// `len(k) = text(k).len()`
struct Len;
impl Query for Len {
    type Key = u32;
    type Value = usize;
    const NAME: &'static str = "len";
}
impl Derived for Len {
    fn compute(db: &Db, key: &u32) -> Result<usize, QueryError> {
        Ok(db.input::<Text>(key)?.len())
    }
}

/// `double(k) = 2 * len(k)`
struct Double;
impl Query for Double {
    type Key = u32;
    type Value = usize;
    const NAME: &'static str = "double";
}
impl Derived for Double {
    fn compute(db: &Db, key: &u32) -> Result<usize, QueryError> {
        Ok(2 * db.get::<Len>(key)?)
    }
}

/// `pick(k) = if flag(k) { len(0) } else { len(1) }`: dependencies that change with an input.
struct Pick;
impl Query for Pick {
    type Key = u32;
    type Value = usize;
    const NAME: &'static str = "pick";
}
impl Derived for Pick {
    fn compute(db: &Db, key: &u32) -> Result<usize, QueryError> {
        let source = if db.input::<Flag>(key)? { 0 } else { 1 };
        db.get::<Len>(&source)
    }
}

/// `a(k) = b(k) + 1`
struct A;
impl Query for A {
    type Key = u32;
    type Value = usize;
    const NAME: &'static str = "a";
}
impl Derived for A {
    fn compute(db: &Db, key: &u32) -> Result<usize, QueryError> {
        Ok(db.get::<B>(key)? + 1)
    }
}

/// `b(k) = if flag(k) { a(k) } else { 0 }`: a cycle while the flag is set.
struct B;
impl Query for B {
    type Key = u32;
    type Value = usize;
    const NAME: &'static str = "b";
}
impl Derived for B {
    fn compute(db: &Db, key: &u32) -> Result<usize, QueryError> {
        if db.input::<Flag>(key)? {
            db.get::<A>(key)
        } else {
            Ok(0)
        }
    }
}

/// `recover(k) = a(k), or 99 if that's a cycle`: a query that handles the error itself.
struct Recover;
impl Query for Recover {
    type Key = u32;
    type Value = usize;
    const NAME: &'static str = "recover";
}
impl Derived for Recover {
    fn compute(db: &Db, key: &u32) -> Result<usize, QueryError> {
        match db.get::<A>(key) {
            Err(QueryError::Cycle(_)) => Ok(99),
            other => other,
        }
    }
}

/// `explode(k)` panics while `text(k)` is "boom".
struct Explode;
impl Query for Explode {
    type Key = u32;
    type Value = usize;
    const NAME: &'static str = "explode";
}
impl Derived for Explode {
    fn compute(db: &Db, key: &u32) -> Result<usize, QueryError> {
        let text = db.input::<Text>(key)?;
        assert!(text != "boom", "explode");
        db.get::<Len>(key)
    }
}

fn db() -> Db {
    let mut db = Db::new();
    db.enable_log();
    db
}

#[test]
fn derived_values_are_memoized() {
    let mut db = db();
    db.set::<Text>(&0, "abc".into());
    assert_eq!(db.get::<Double>(&0), Ok(6));
    assert_eq!(db.get::<Double>(&0), Ok(6));
    assert_eq!(db.get::<Len>(&0), Ok(3));
    assert_eq!(db.take_log(), ["execute double(0)", "execute len(0)"]);
}

#[test]
fn changing_an_input_re_runs_its_readers() {
    let mut db = db();
    db.set::<Text>(&0, "abc".into());
    assert_eq!(db.get::<Double>(&0), Ok(6));
    db.take_log();
    assert!(db.set::<Text>(&0, "abcd".into()));
    assert_eq!(db.get::<Double>(&0), Ok(8));
    assert_eq!(db.take_log(), ["execute len(0)", "execute double(0)"]);
}

#[test]
fn early_cutoff_skips_readers_of_an_unchanged_value() {
    let mut db = db();
    db.set::<Text>(&0, "abc".into());
    assert_eq!(db.get::<Double>(&0), Ok(6));
    db.take_log();
    // Same length: `len` re-runs and produces 3 again, so `double` is reused.
    db.set::<Text>(&0, "xyz".into());
    assert_eq!(db.get::<Double>(&0), Ok(6));
    assert_eq!(db.take_log(), ["execute len(0)"]);
}

#[test]
fn setting_an_equal_input_changes_nothing() {
    let mut db = db();
    db.set::<Text>(&0, "abc".into());
    let revision = db.revision();
    assert_eq!(db.get::<Double>(&0), Ok(6));
    db.take_log();
    assert!(!db.set::<Text>(&0, "abc".into()));
    assert_eq!(db.revision(), revision);
    assert_eq!(db.get::<Double>(&0), Ok(6));
    assert!(db.take_log().is_empty());
}

#[test]
fn unrelated_inputs_do_not_re_run_anything() {
    let mut db = db();
    db.set::<Text>(&0, "abc".into());
    db.set::<Text>(&1, "z".into());
    assert_eq!(db.get::<Double>(&0), Ok(6));
    db.take_log();
    db.set::<Text>(&1, "zz".into());
    assert_eq!(db.get::<Double>(&0), Ok(6));
    assert!(db.take_log().is_empty());
}

#[test]
fn dependencies_follow_the_last_run() {
    let mut db = db();
    db.set::<Text>(&0, "a".into());
    db.set::<Text>(&1, "bb".into());
    db.set::<Flag>(&7, true);
    assert_eq!(db.get::<Pick>(&7), Ok(1));
    db.set::<Flag>(&7, false);
    assert_eq!(db.get::<Pick>(&7), Ok(2));
    db.take_log();
    // `pick(7)` no longer reads `text(0)`.
    db.set::<Text>(&0, "aaaa".into());
    assert_eq!(db.get::<Pick>(&7), Ok(2));
    assert!(db.take_log().is_empty());
    db.set::<Text>(&1, "bbb".into());
    assert_eq!(db.get::<Pick>(&7), Ok(3));
    assert_eq!(db.take_log(), ["execute len(1)", "execute pick(7)"]);
}

fn cycle(participants: &[&str]) -> Result<usize, QueryError> {
    Err(QueryError::Cycle(Cycle::new(
        participants.iter().map(ToString::to_string).collect(),
    )))
}

#[test]
fn cycles_are_errors_whichever_query_is_asked_first() {
    let mut first_a = db();
    first_a.set::<Flag>(&0, true);
    assert_eq!(first_a.get::<A>(&0), cycle(&["a(0)", "b(0)"]));
    assert_eq!(first_a.get::<B>(&0), cycle(&["a(0)", "b(0)"]));

    let mut first_b = db();
    first_b.set::<Flag>(&0, true);
    assert_eq!(first_b.get::<B>(&0), cycle(&["a(0)", "b(0)"]));
    assert_eq!(first_b.get::<A>(&0), cycle(&["a(0)", "b(0)"]));

    let Err(error) = first_a.get::<A>(&0) else {
        panic!("expected a cycle")
    };
    assert_eq!(error.to_string(), "query cycle: a(0) -> b(0) -> a(0)");
}

#[test]
fn a_cycle_goes_away_when_its_input_changes() {
    let mut db = db();
    db.set::<Flag>(&0, true);
    assert!(db.get::<A>(&0).is_err());
    db.set::<Flag>(&0, false);
    assert_eq!(db.get::<A>(&0), Ok(1));
    assert_eq!(db.get::<B>(&0), Ok(0));
    db.set::<Flag>(&0, true);
    assert!(db.get::<B>(&0).is_err());
    assert!(db.get::<A>(&0).is_err());
}

#[test]
fn a_memoized_cycle_is_revalidated_without_recursing_forever() {
    let mut db = db();
    db.set::<Flag>(&0, true);
    db.set::<Text>(&0, "x".into());
    assert!(db.get::<A>(&0).is_err());
    // A new revision forces re-validation of a dependency graph that contains the cycle.
    db.set::<Text>(&0, "y".into());
    assert_eq!(db.get::<A>(&0), cycle(&["a(0)", "b(0)"]));
    assert_eq!(db.get::<B>(&0), cycle(&["a(0)", "b(0)"]));
}

#[test]
fn a_query_can_recover_from_a_cycle() {
    let mut db = db();
    db.set::<Flag>(&0, true);
    assert_eq!(db.get::<Recover>(&0), Ok(99));
    db.set::<Flag>(&0, false);
    assert_eq!(db.get::<Recover>(&0), Ok(1));
}

#[test]
fn reading_an_unset_input_is_an_error_until_it_is_set() {
    let mut db = db();
    let expected = QueryError::MissingInput {
        query: "text",
        key: "5".into(),
    };
    assert_eq!(db.get::<Len>(&5), Err(expected));
    db.set::<Text>(&5, "hello".into());
    assert_eq!(db.get::<Len>(&5), Ok(5));
}

#[test]
fn a_panicking_query_leaves_the_db_usable() {
    let mut db = db();
    db.set::<Text>(&0, "boom".into());
    let result = catch_unwind(AssertUnwindSafe(|| db.get::<Explode>(&0)));
    assert!(result.is_err());
    db.set::<Text>(&0, "fine".into());
    assert_eq!(db.get::<Explode>(&0), Ok(4));
}

#[test]
fn the_same_operations_give_the_same_results_and_runs() {
    let run = || {
        let mut db = db();
        let mut seen = Vec::new();
        for (i, text) in ["a", "bb", "bb", "c", "dddd"].iter().enumerate() {
            let key = u32::try_from(i % 2).unwrap();
            db.set::<Text>(&key, (*text).to_string());
            db.set::<Flag>(&key, i % 3 == 0);
            seen.push((
                db.get::<Double>(&key),
                db.get::<Pick>(&key),
                db.get::<A>(&key),
            ));
        }
        (seen, db.take_log())
    };
    assert_eq!(run(), run());
}
