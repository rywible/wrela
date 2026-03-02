use wrela_runtime::db::planner::PlanKind;
use wrela_runtime::db::sql::{
    ConformanceCase, ConformanceExpectation, SqlCatalog, run_conformance_suite,
};

fn catalog() -> SqlCatalog {
    let mut catalog = SqlCatalog::new();
    catalog.register_table(
        b"orders".to_vec(),
        vec![b"idx_tenant".to_vec(), b"idx_created".to_vec()],
    );
    catalog
}

#[test]
fn sql_conformance_suite_is_deterministic_and_reports_expected_outcomes() {
    let cases = vec![
        ConformanceCase {
            name: "insert_with_indexes".to_string(),
            statement: "INSERT orders pk-1 row-1 INDEX idx_tenant=t1,idx_created=2026".to_string(),
            expect: ConformanceExpectation::MutationPlan {
                lock_count: 3,
                batch_count: 3,
            },
        },
        ConformanceCase {
            name: "delete_with_indexes".to_string(),
            statement: "DELETE orders pk-1 INDEX idx_tenant=t1,idx_created=2026".to_string(),
            expect: ConformanceExpectation::MutationPlan {
                lock_count: 3,
                batch_count: 3,
            },
        },
        ConformanceCase {
            name: "explain_prefers_index_seek".to_string(),
            statement: "EXPLAIN orders SELECTIVITY 250 CARDINALITY 1000 INDEX on STALE false"
                .to_string(),
            expect: ConformanceExpectation::ExplainPlan {
                kind: PlanKind::IndexLookup,
            },
        },
        ConformanceCase {
            name: "unknown_table_rejected".to_string(),
            statement: "INSERT missing pk row".to_string(),
            expect: ConformanceExpectation::Rejected {
                token: "SQL_INVALID_MUTATION",
            },
        },
    ];

    let first = run_conformance_suite(&catalog(), &cases);
    let second = run_conformance_suite(&catalog(), &cases);
    assert_eq!(first, second);
    assert!(first.iter().all(|row| row.passed), "results: {first:?}");
}
