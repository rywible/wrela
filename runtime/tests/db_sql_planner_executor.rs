use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use wrela_runtime::db::DbConfig;
use wrela_runtime::db::config::ReplicationConfig;
use wrela_runtime::db::planner::PlanKind;
use wrela_runtime::db::sql::{
    CompiledSql, ConformanceCase, ConformanceExpectation, RowMutation, SqlCatalog,
    compile_statement, execute, parse_statement, row_key, row_namespace, run_conformance_suite,
};
use wrela_runtime::db::{close_db, open_db_with_config, read_point};

fn open_sql_planner_db(path: &std::path::Path) -> i64 {
    let config = DbConfig::for_testing().with_replication(ReplicationConfig {
        factor: 3,
        write_quorum: 2,
        ..DbConfig::for_testing().replication
    });
    open_db_with_config(path, &config).expect("open db")
}

fn temp_dir() -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let base = std::env::temp_dir().join(format!(
        "wrela_db_sql_planner_exec_{}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&base).expect("create temp dir");
    base
}

fn catalog() -> SqlCatalog {
    let mut catalog = SqlCatalog::new();
    catalog.register_table(
        b"orders".to_vec(),
        vec![b"idx_tenant".to_vec(), b"idx_created".to_vec()],
    );
    catalog
}

#[test]
fn explain_planner_path_selection_is_stable() {
    let cat = catalog();

    let index_stmt =
        parse_statement("EXPLAIN orders SELECTIVITY 250 CARDINALITY 1000 INDEX on STALE false")
            .expect("parse explain");
    let compiled = compile_statement(&cat, index_stmt).expect("compile explain");
    let CompiledSql::Explain(index_plan) = compiled else {
        panic!("expected explain output");
    };
    assert_eq!(index_plan.chosen_plan, PlanKind::IndexLookup);

    let full_scan_stmt =
        parse_statement("EXPLAIN orders SELECTIVITY 10000 CARDINALITY 1000 INDEX off STALE false")
            .expect("parse explain");
    let compiled = compile_statement(&cat, full_scan_stmt).expect("compile explain");
    let CompiledSql::Explain(full_scan_plan) = compiled else {
        panic!("expected explain output");
    };
    assert_eq!(full_scan_plan.chosen_plan, PlanKind::FullScan);
}

#[test]
fn executor_commits_insert_mutation_and_row_is_readable() {
    let dir = temp_dir();
    let handle = open_sql_planner_db(&dir);

    let mutation = RowMutation::Put {
        table: b"orders".to_vec(),
        primary_key: b"pk-1".to_vec(),
        row_value: b"payload-1".to_vec(),
        secondary_indexes: vec![],
    };
    execute(handle, &[mutation]).expect("execute mutation");

    let key = row_key(b"orders", b"pk-1");
    let value = read_point(handle, row_namespace().to_vec(), key)
        .expect("read")
        .expect("row exists");
    assert_eq!(value, b"payload-1".to_vec());

    assert!(close_db(handle));
}

#[test]
fn conformance_suite_for_planner_executor_paths_passes() {
    let cat = catalog();
    let cases = vec![
        ConformanceCase {
            name: "insert_plan".to_string(),
            statement: "INSERT orders pk-1 row-1 INDEX idx_tenant=t1,idx_created=2026".to_string(),
            expect: ConformanceExpectation::MutationPlan {
                lock_count: 3,
                batch_count: 3,
            },
        },
        ConformanceCase {
            name: "index_lookup_plan".to_string(),
            statement: "EXPLAIN orders SELECTIVITY 250 CARDINALITY 1000 INDEX on STALE false"
                .to_string(),
            expect: ConformanceExpectation::ExplainPlan {
                kind: PlanKind::IndexLookup,
            },
        },
        ConformanceCase {
            name: "full_scan_plan".to_string(),
            statement: "EXPLAIN orders SELECTIVITY 10000 CARDINALITY 1000 INDEX off STALE false"
                .to_string(),
            expect: ConformanceExpectation::ExplainPlan {
                kind: PlanKind::FullScan,
            },
        },
    ];

    let results = run_conformance_suite(&cat, &cases);
    assert!(results.iter().all(|row| row.passed), "results: {results:?}");
}
