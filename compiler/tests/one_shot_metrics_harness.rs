use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OneShotCorpusV2 {
    schema_version: u32,
    suite_id: String,
    cases: Vec<OneShotCase>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OneShotCase {
    id: String,
    workspace_dir: String,
    command: String,
    target: String,
    #[serde(default = "default_max_loops")]
    max_loops: u32,
    attempts: Vec<OneShotAttempt>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OneShotAttempt {
    id: String,
    #[serde(default = "default_visible_to_agent")]
    visible_to_agent: bool,
    #[serde(default)]
    machine_applicable: bool,
    #[serde(default)]
    writes: Vec<OneShotWrite>,
    #[serde(default)]
    deletes: Vec<String>,
    #[serde(default)]
    noop: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OneShotWrite {
    path: String,
    content: String,
}

const EXPECTED_SCHEMA_VERSION: u32 = 2;
const EXPECTED_CORPUS_FINGERPRINT: &str = "c2b32fc394e2fbd2";
const MIN_CASES: usize = 24;
const MIN_MULTI_LOOP_CASES: usize = 8;
const MIN_HIDDEN_ATTEMPTS: usize = 4;
const MIN_MACHINE_APPLICABLE_ATTEMPTS: usize = 8;

fn default_max_loops() -> u32 {
    3
}

fn default_visible_to_agent() -> bool {
    true
}

fn load_corpus_v2() -> OneShotCorpusV2 {
    let raw = include_str!("fixtures/agent_one_shot/corpus_v2.json");
    serde_json::from_str(raw).expect("valid one-shot corpus v2 fixture")
}

fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn corpus_fingerprint(corpus: &OneShotCorpusV2) -> String {
    let canonical = serde_json::to_vec(corpus).expect("serialize canonical corpus");
    fnv1a64_hex(&canonical)
}

fn is_safe_relative_path(path: &str, allow_dot: bool) -> bool {
    if path.trim().is_empty() {
        return false;
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return false;
    }
    let mut saw_normal = false;
    for component in candidate.components() {
        match component {
            std::path::Component::Normal(_) => saw_normal = true,
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return false,
        }
    }
    allow_dot || saw_normal
}

fn assert_v2_corpus_integrity(corpus: &OneShotCorpusV2) {
    assert_eq!(
        corpus.schema_version, EXPECTED_SCHEMA_VERSION,
        "schema_version must be {}",
        EXPECTED_SCHEMA_VERSION
    );
    assert!(
        !corpus.suite_id.trim().is_empty(),
        "suite_id must be non-empty"
    );
    assert!(
        corpus.cases.len() >= MIN_CASES,
        "expected at least {} cases, got {}",
        MIN_CASES,
        corpus.cases.len()
    );

    let mut case_ids = HashSet::new();
    let mut parse_cases = 0usize;
    let mut check_cases = 0usize;
    let mut test_cases = 0usize;
    let mut check_command_cases = 0usize;
    let mut test_command_cases = 0usize;
    let mut multi_loop_cases = 0usize;
    let mut hidden_attempts = 0usize;
    let mut machine_applicable_attempts = 0usize;

    for case in &corpus.cases {
        assert!(!case.id.trim().is_empty(), "case id must be non-empty");
        assert!(
            case_ids.insert(case.id.clone()),
            "duplicate case id '{}'",
            case.id
        );
        assert!(
            is_safe_relative_path(&case.workspace_dir, true),
            "unsafe workspace_dir '{}'",
            case.workspace_dir
        );
        assert!(
            is_safe_relative_path(&case.target, true),
            "unsafe target '{}'",
            case.target
        );
        assert!(
            case.max_loops >= 1,
            "case '{}' must set max_loops >= 1",
            case.id
        );
        assert!(
            !case.attempts.is_empty(),
            "case '{}' must include attempts",
            case.id
        );

        if case.id.starts_with("parse_") {
            parse_cases += 1;
        } else if case.id.starts_with("check_") {
            check_cases += 1;
        } else if case.id.starts_with("test_") {
            test_cases += 1;
        }
        match case.command.as_str() {
            "check" => check_command_cases += 1,
            "test" => test_command_cases += 1,
            other => panic!("unsupported command '{}' in case '{}'", other, case.id),
        }

        if case.attempts.len() >= 2 {
            multi_loop_cases += 1;
        }

        let mut attempt_ids = HashSet::new();
        for attempt in &case.attempts {
            assert!(
                !attempt.id.trim().is_empty(),
                "case '{}' has attempt with empty id",
                case.id
            );
            assert!(
                attempt_ids.insert(attempt.id.clone()),
                "case '{}' has duplicate attempt id '{}'",
                case.id,
                attempt.id
            );
            assert!(
                attempt.noop || !attempt.writes.is_empty() || !attempt.deletes.is_empty(),
                "case '{}' attempt '{}' must define writes/deletes or noop=true",
                case.id,
                attempt.id
            );

            if !attempt.visible_to_agent {
                hidden_attempts += 1;
            }
            if attempt.machine_applicable {
                machine_applicable_attempts += 1;
            }

            for write in &attempt.writes {
                assert!(
                    is_safe_relative_path(&write.path, false),
                    "case '{}' attempt '{}' has unsafe write path '{}'",
                    case.id,
                    attempt.id,
                    write.path
                );
            }
            for delete in &attempt.deletes {
                assert!(
                    is_safe_relative_path(delete, false),
                    "case '{}' attempt '{}' has unsafe delete path '{}'",
                    case.id,
                    attempt.id,
                    delete
                );
            }
        }
    }

    assert_eq!(parse_cases, 8, "expected exactly 8 parse_* cases");
    assert_eq!(check_cases, 8, "expected exactly 8 check_* cases");
    assert_eq!(test_cases, 8, "expected exactly 8 test_* cases");
    assert_eq!(
        check_command_cases, 16,
        "expected exactly 16 check command cases"
    );
    assert_eq!(
        test_command_cases, 8,
        "expected exactly 8 test command cases"
    );
    assert!(
        multi_loop_cases >= MIN_MULTI_LOOP_CASES,
        "expected at least {} cases requiring 2+ loops, got {}",
        MIN_MULTI_LOOP_CASES,
        multi_loop_cases
    );
    assert!(
        hidden_attempts >= MIN_HIDDEN_ATTEMPTS,
        "expected at least {} hidden attempts, got {}",
        MIN_HIDDEN_ATTEMPTS,
        hidden_attempts
    );
    assert!(
        machine_applicable_attempts >= MIN_MACHINE_APPLICABLE_ATTEMPTS,
        "expected at least {} machine_applicable attempts, got {}",
        MIN_MACHINE_APPLICABLE_ATTEMPTS,
        machine_applicable_attempts
    );
}

#[test]
fn corpus_v2_integrity_and_mix_requirements_hold() {
    let corpus = load_corpus_v2();
    assert_v2_corpus_integrity(&corpus);
}

#[test]
fn corpus_v2_fingerprint_is_stable() {
    let corpus = load_corpus_v2();
    assert_v2_corpus_integrity(&corpus);
    assert_eq!(corpus_fingerprint(&corpus), EXPECTED_CORPUS_FINGERPRINT);
}

#[test]
#[should_panic(expected = "duplicate case id")]
fn corpus_v2_rejects_duplicate_case_ids() {
    let mut corpus = load_corpus_v2();
    corpus.cases[1].id = corpus.cases[0].id.clone();
    assert_v2_corpus_integrity(&corpus);
}

#[test]
#[should_panic(expected = "unsafe write path")]
fn corpus_v2_rejects_path_traversal() {
    let mut corpus = load_corpus_v2();
    corpus.cases[0].attempts[0].writes[0].path = "../escape.wr".to_string();
    assert_v2_corpus_integrity(&corpus);
}

#[test]
#[should_panic(expected = "must define writes/deletes or noop=true")]
fn corpus_v2_rejects_empty_attempt_payload() {
    let mut corpus = load_corpus_v2();
    corpus.cases[0].attempts[0].writes.clear();
    corpus.cases[0].attempts[0].deletes.clear();
    corpus.cases[0].attempts[0].noop = false;
    assert_v2_corpus_integrity(&corpus);
}
