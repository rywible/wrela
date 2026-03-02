use std::fs;
use std::path::{Path, PathBuf};

const MAX_GIVEN_OCCURRENCES: usize = 0;
const MAX_EXPR_OTHERWISE_OCCURRENCES: usize = 0;

#[test]
fn corpus_files_parse_without_errors() {
    let files = corpus_files();
    let mut failures = Vec::new();

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let (_root, errors) = wrela::parser::parse_with_errors(&source);
        if errors.is_empty() {
            continue;
        }

        let mut sample = Vec::new();
        for err in errors.iter().take(3) {
            sample.push(format!(
                "{} @{}:{}",
                err.message,
                err.span.offset(),
                err.span.len()
            ));
        }
        failures.push(format!(
            "{} -> {} parse error(s): {}",
            path.display(),
            errors.len(),
            sample.join(" | ")
        ));
    }

    assert!(
        failures.is_empty(),
        "parse-invalid corpus files:\n{}",
        failures.join("\n")
    );
}

#[test]
fn corpus_corruption_signatures_absent() {
    let files = corpus_files();
    let literal_signatures = [
        "(hould_w)",
        "retuMap[String, Integer]",
        "StriMap[String, Integer]",
        "objectivenormalized_objective=",
        "rescpu_count=olve_pool_base_size",
        "try_to_write_http_responselistener_handle=_frame_items",
        "errorstatus_code=_descriptionresponse_body=",
    ];

    let mut failures = Vec::new();

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));

        for signature in literal_signatures {
            if source.contains(signature) {
                failures.push(format!(
                    "{} contains corruption signature `{}`",
                    path.display(),
                    signature
                ));
            }
        }

        if contains_spliced_map_type(&source) {
            failures.push(format!(
                "{} contains spliced `Map[String, Integer]` token",
                path.display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "corruption signatures detected:\n{}",
        failures.join("\n")
    );
}

#[test]
fn corpus_legacy_syntax_budget_does_not_regress() {
    let files = corpus_files();
    let mut given_total = 0usize;
    let mut expr_otherwise_total = 0usize;

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        given_total += count_word_occurrences(&source, "given");
        expr_otherwise_total += count_expr_otherwise_occurrences(&source);
    }

    assert!(
        given_total <= MAX_GIVEN_OCCURRENCES,
        "legacy `given` budget regressed: current={}, max={}",
        given_total,
        MAX_GIVEN_OCCURRENCES
    );
    assert!(
        expr_otherwise_total <= MAX_EXPR_OTHERWISE_OCCURRENCES,
        "expression-level `otherwise` budget regressed: current={}, max={}",
        expr_otherwise_total,
        MAX_EXPR_OTHERWISE_OCCURRENCES
    );
}

fn corpus_files() -> Vec<PathBuf> {
    let repo_root = repo_root();
    let mut out = Vec::new();

    for rel in ["language/stdlib", "language/packages", "apps", "benchmarks"] {
        let root = repo_root.join(rel);
        if root.exists() {
            collect_wr_files_recursive(&root, &mut out);
        }
    }

    out.sort();
    out.dedup();
    out
}

fn collect_wr_files_recursive(root: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root)
        .unwrap_or_else(|err| panic!("failed to read directory {}: {err}", root.display()));
    let mut paths = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if matches!(name, ".git" | "target" | "node_modules") {
                continue;
            }
            collect_wr_files_recursive(&path, out);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("wr") {
            out.push(path);
        }
    }
}

fn count_word_occurrences(text: &str, word: &str) -> usize {
    let mut count = 0usize;
    let mut start = 0usize;

    while let Some(rel_idx) = text[start..].find(word) {
        let idx = start + rel_idx;
        let end = idx + word.len();
        if is_word_boundary(text, idx, end) {
            count += 1;
        }
        start = end;
    }

    count
}

fn count_expr_otherwise_occurrences(text: &str) -> usize {
    let mut count = 0usize;
    let mut start = 0usize;

    while let Some(rel_idx) = text[start..].find("otherwise") {
        let idx = start + rel_idx;
        let end = idx + "otherwise".len();
        if is_word_boundary(text, idx, end) {
            let mut cursor = end;
            while let Some(byte) = text.as_bytes().get(cursor) {
                if byte.is_ascii_whitespace() {
                    cursor += 1;
                    continue;
                }
                if *byte != b':' {
                    count += 1;
                }
                break;
            }
        }
        start = end;
    }

    count
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let prev_ok = match text[..start].chars().next_back() {
        Some(ch) => !is_word_char(ch),
        None => true,
    };
    let next_ok = match text[end..].chars().next() {
        Some(ch) => !is_word_char(ch),
        None => true,
    };
    prev_ok && next_ok
}

fn contains_spliced_map_type(text: &str) -> bool {
    let token = "Map[String, Integer]";
    let mut start = 0usize;

    while let Some(rel_idx) = text[start..].find(token) {
        let idx = start + rel_idx;
        let end = idx + token.len();

        let prev = text[..idx].chars().next_back();
        let next = text[end..].chars().next();
        if prev.is_some_and(is_word_char) && next.is_some_and(is_word_char) {
            return true;
        }

        start = end;
    }

    false
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate should be nested under repository root")
        .to_path_buf()
}
