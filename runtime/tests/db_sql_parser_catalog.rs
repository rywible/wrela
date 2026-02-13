use wrela_runtime::db::sql::{
    CompiledSql, SqlCatalog, compile_statement, parse_statement, run_conformance_suite,
};
use wrela_runtime::db::types::ErrorCode;

fn catalog() -> SqlCatalog {
    let mut catalog = SqlCatalog::new();
    catalog.register_table(
        b"users".to_vec(),
        vec![b"by_email".to_vec(), b"by_handle".to_vec()],
    );
    catalog
}

#[test]
fn parser_rejects_invalid_explain_clauses_and_selectivity_bounds() {
    let err = parse_statement("EXPLAIN users CARDINALITY 100 SELECTIVITY 10 INDEX on STALE false")
        .expect_err("invalid clause order must fail");
    assert_eq!(err.code, ErrorCode::InvalidArgument);

    let err =
        parse_statement("EXPLAIN users SELECTIVITY 10001 CARDINALITY 100 INDEX on STALE false")
            .expect_err("selectivity out of range must fail");
    assert_eq!(err.code, ErrorCode::InvalidArgument);
}

#[test]
fn catalog_compile_rejects_unknown_table_and_index() {
    let cat = catalog();

    let stmt = parse_statement("INSERT ghosts g1 rowv INDEX by_email=x").expect("parse");
    let err = compile_statement(&cat, stmt).expect_err("unknown table should fail");
    assert!(err.message.contains("unknown table"));

    let stmt = parse_statement("INSERT users u1 rowv INDEX by_phone=555").expect("parse");
    let err = compile_statement(&cat, stmt).expect_err("unknown index should fail");
    assert!(err.message.contains("unknown secondary index"));
}

#[test]
fn parser_and_compile_are_deterministic_for_equivalent_inputs() {
    let cat = catalog();
    let a = parse_statement("INSERT users u5 rowv INDEX by_email=ada@example.com,by_handle=ada")
        .expect("parse a");
    let b = parse_statement("INSERT users u5 rowv INDEX by_email=ada@example.com,by_handle=ada")
        .expect("parse b");
    assert_eq!(a, b);

    let compiled_a = compile_statement(&cat, a).expect("compile a");
    let compiled_b = compile_statement(&cat, b).expect("compile b");
    assert_eq!(compiled_a, compiled_b);

    match compiled_a {
        CompiledSql::Mutation(_) => {}
        _ => panic!("expected mutation"),
    }

    let res_a = run_conformance_suite(&cat, &[]);
    let res_b = run_conformance_suite(&cat, &[]);
    assert_eq!(res_a, res_b);
}
