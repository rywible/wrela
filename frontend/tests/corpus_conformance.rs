mod support {
    pub mod cst_audit;
    pub mod digest;
}
use serde::Deserialize;
use support::digest::sha256;
use wrela_frontend::parse;

const CORPUS: &[u8] = include_bytes!("corpus.json");
const STATUS: &[u8] = include_bytes!("CORPUS-STATUS.json");
const IDS: &[&str] = &[
    "V01", "V02", "V03", "V04", "V05", "V06", "V07", "V08", "V09", "V10", "V11", "V12", "I01",
    "I02", "I03", "I04", "I05", "I06", "I07", "I08", "I09", "I10", "I11", "I12", "I13", "I14",
    "S01", "S02", "S03", "S04", "S05", "S06", "S07",
];
#[derive(Clone, Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
#[derive(Clone, Deserialize)]
struct Case {
    id: String,
    group: String,
    source: String,
    syntax_eligible: bool,
    source_byte_length: usize,
    source_sha256: String,
}
fn check_identities(corpus: &Corpus) -> Result<(), String> {
    if corpus.cases.len() != IDS.len() {
        return Err("missing or extra corpus cases".into());
    }
    for (case, expected) in corpus.cases.iter().zip(IDS) {
        if case.id != *expected {
            return Err(format!("case identity/order changed: {}", case.id));
        }
        let expected_group = match expected.as_bytes()[0] {
            b'V' => "valid",
            b'I' => "invalid",
            b'S' => "semantic-later",
            _ => unreachable!(),
        };
        if case.group != expected_group {
            return Err(format!("case group changed: {}", case.id));
        }
        if case.syntax_eligible != (case.group != "invalid") {
            return Err(format!("case classification changed: {}", case.id));
        }
        if case.source.len() != case.source_byte_length
            || sha256(case.source.as_bytes()) != case.source_sha256
        {
            return Err(format!("case source bytes changed: {}", case.id));
        }
    }
    Ok(())
}
#[test]
fn independently_authored_corpus_bytes_and_identities_are_frozen() {
    assert_eq!(
        sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256(CORPUS),
        "73665c11c3465bf321c69a2d22135a5637a5fcedfebcd78557bf1ce7e948fda9"
    );
    assert_eq!(
        sha256(STATUS),
        "579e308be47914cc048106786a853866c9fdff3cd71a01f29f82df681f20e700"
    );
    let corpus: Corpus = serde_json::from_slice(CORPUS).unwrap();
    check_identities(&corpus).unwrap();
}
#[test]
fn case_identity_verification_rejects_missing_duplicate_reordered_and_changed_cases() {
    let original: Corpus = serde_json::from_slice(CORPUS).unwrap();
    let mut missing = original.clone();
    missing.cases.remove(0);
    assert!(check_identities(&missing).is_err());
    let mut duplicate = original.clone();
    duplicate.cases[1] = duplicate.cases[0].clone();
    assert!(check_identities(&duplicate).is_err());
    let mut reordered = original.clone();
    reordered.cases.swap(0, 1);
    assert!(check_identities(&reordered).is_err());
    let mut group = original.clone();
    group.cases[0].group = "invalid".into();
    assert!(check_identities(&group).is_err());
    let mut changed = original.clone();
    changed.cases[0].source.push(' ');
    assert!(check_identities(&changed).is_err());
}
#[test]
fn every_frozen_case_exercises_actual_production_syntax_admission() {
    let corpus: Corpus = serde_json::from_slice(CORPUS).unwrap();
    check_identities(&corpus).unwrap();
    let mut failures = Vec::new();
    for case in corpus.cases {
        let parsed = parse(case.source.as_bytes());
        assert_eq!(
            parsed
                .source
                .pieces
                .iter()
                .flat_map(|p| p.bytes.iter().copied())
                .collect::<Vec<_>>(),
            case.source.as_bytes(),
            "{} source fidelity",
            case.id
        );
        let eligible = parsed.is_syntax_eligible();
        if eligible {
            support::cst_audit::audit(&parsed.cst(), &parsed.source)
                .unwrap_or_else(|error| panic!("{}: {error}", case.id));
        }
        if eligible != case.syntax_eligible {
            failures.push(format!(
                "{} expected {}, got {}, diagnostics {:?}",
                case.id, case.syntax_eligible, eligible, parsed.diagnostics
            ));
        }
        assert_eq!(
            parsed.admit().is_ok(),
            eligible,
            "{} admission disagrees with reported eligibility",
            case.id
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn frozen_recovery_cases_preserve_promised_survivors_and_physical_eof_prefix() {
    use wrela_frontend::{
        diagnostic::DiagnosticCode,
        source::ByteRange,
        syntax::{Declaration, StatementKind},
    };
    let corpus: Corpus = serde_json::from_slice(CORPUS).unwrap();
    for id in ["I04", "I05", "I06", "I07", "I08", "I09", "I10"] {
        let case = corpus.cases.iter().find(|c| c.id == id).unwrap();
        let parsed = parse(case.source.as_bytes());
        let find_function = |name: &str| {
            parsed
                .syntax
                .declarations
                .iter()
                .find_map(|d| match &d.kind {
                    Declaration::Function(f) if parsed.source.token_text(f.name) == Some(name) => {
                        Some(f)
                    }
                    _ => None,
                })
        };
        if id != "I08" && id != "I10" {
            assert!(find_function("good").is_some(), "{id} lost later function");
        }
        if id == "I04" {
            assert!(
                find_function("bad")
                    .unwrap()
                    .body
                    .statements
                    .iter()
                    .any(|s| matches!(s.kind, StatementKind::Return { .. }))
            );
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .any(|d| d.code == DiagnosticCode::Lexical)
            );
        }
        if ["I05", "I06", "I07", "I09"].contains(&id) {
            let name = if id == "I05" { "c" } else { "tail" };
            assert!(
                find_function("bad")
                    .unwrap()
                    .body
                    .statements
                    .iter()
                    .any(|s| match &s.kind {
                        StatementKind::Local { name: token, .. } =>
                            parsed.source.token_text(*token) == Some(name),
                        _ => false,
                    }),
                "{id} lost surviving binding {name}"
            );
        }
        if id == "I05" {
            let ranges: std::collections::BTreeSet<_> = parsed
                .diagnostics
                .iter()
                .map(|d| (d.range.start, d.range.end))
                .collect();
            assert!(ranges.len() >= 2, "independent errors collapsed");
        }
        if id == "I08" {
            let incomplete = parsed
                .syntax
                .declarations
                .iter()
                .find_map(|d| match &d.kind {
                    Declaration::IncompleteFunction(f) => Some(f),
                    _ => None,
                })
                .expect("EOF must retain typed function prefix");
            assert_eq!(
                parsed.source.token_text(incomplete.prefix.kind.name),
                Some("bad")
            );
            assert_eq!(
                incomplete.missing_close,
                ByteRange::empty(case.source.len())
            );
            assert!(incomplete.statements.iter().any(|s| match &s.kind {
                StatementKind::Local { name, .. } => parsed.source.token_text(*name) == Some("s"),
                _ => false,
            }));
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .any(|d| d.range == ByteRange::empty(case.source.len()))
            );
        }
        if id == "I10" {
            assert!(find_function("bad").is_some());
            assert!(find_function("later").is_none());
        }
        assert!(parsed.admit().is_err(), "recovered case {id} admitted");
    }
}
