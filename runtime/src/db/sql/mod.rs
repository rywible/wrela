use crate::db::planner::{self, PlanKind};
use crate::db::types::{BatchOp, DbError, ErrorCode};
use crate::db::{submit_batch, txn_abort, txn_begin, txn_commit, txn_lock_key, txn_prepare};
use std::collections::{HashMap, HashSet};

const ROW_NAMESPACE: &[u8] = b"sql.row";
const INDEX_NAMESPACE: &[u8] = b"sql.index";
const SQL_RETRYABLE_CONFLICT: &str = "SQL_RETRYABLE_CONFLICT";
const SQL_INVALID_MUTATION: &str = "SQL_INVALID_MUTATION";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondaryIndexEntry {
    pub index_name: Vec<u8>,
    pub index_key: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowMutation {
    Put {
        table: Vec<u8>,
        primary_key: Vec<u8>,
        row_value: Vec<u8>,
        secondary_indexes: Vec<SecondaryIndexEntry>,
    },
    Delete {
        table: Vec<u8>,
        primary_key: Vec<u8>,
        secondary_indexes: Vec<SecondaryIndexEntry>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlStatement {
    Insert {
        table: Vec<u8>,
        primary_key: Vec<u8>,
        row_value: Vec<u8>,
        secondary_indexes: Vec<SecondaryIndexEntry>,
    },
    Delete {
        table: Vec<u8>,
        primary_key: Vec<u8>,
        secondary_indexes: Vec<SecondaryIndexEntry>,
    },
    Explain {
        table: Vec<u8>,
        selectivity: u32,
        cardinality_estimate: u64,
        index_available: bool,
        stats_stale: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlTableCatalog {
    pub secondary_indexes: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SqlCatalog {
    tables: HashMap<Vec<u8>, SqlTableCatalog>,
}

impl SqlCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_table(&mut self, table: Vec<u8>, secondary_indexes: Vec<Vec<u8>>) {
        self.tables
            .insert(table, SqlTableCatalog { secondary_indexes });
    }

    fn table(&self, table: &[u8]) -> Option<&SqlTableCatalog> {
        self.tables.get(table)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledSql {
    Mutation(RowMutation),
    Explain(planner::ExplainOutput),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConformanceExpectation {
    MutationPlan {
        lock_count: usize,
        batch_count: usize,
    },
    ExplainPlan {
        kind: PlanKind,
    },
    Rejected {
        token: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceCase {
    pub name: String,
    pub statement: String,
    pub expect: ConformanceExpectation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct DmlPlan {
    pub key_locks: Vec<LockKey>,
    pub batch: Vec<BatchOp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockKey {
    pub namespace: Vec<u8>,
    pub key: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlMutationErrorKind {
    RetryableConflict,
    InvalidMutation,
}

impl SqlMutationErrorKind {
    pub const fn token(self) -> &'static str {
        match self {
            SqlMutationErrorKind::RetryableConflict => SQL_RETRYABLE_CONFLICT,
            SqlMutationErrorKind::InvalidMutation => SQL_INVALID_MUTATION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlMutationError {
    pub kind: SqlMutationErrorKind,
    pub token: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum SqlExecuteResult {
    Committed { version: u64 },
    Rejected(SqlMutationError),
    Failed(DbError),
}

pub fn execute_with_result(handle: i64, mutations: &[RowMutation]) -> SqlExecuteResult {
    match execute(handle, mutations) {
        Ok(version) => SqlExecuteResult::Committed { version },
        Err(err) => match classify_sql_mutation_error(&err) {
            Some(kind) => SqlExecuteResult::Rejected(SqlMutationError {
                kind,
                token: kind.token(),
                message: err.message,
            }),
            None => SqlExecuteResult::Failed(err),
        },
    }
}

pub fn execute(handle: i64, mutations: &[RowMutation]) -> Result<u64, DbError> {
    let plan = plan(mutations)?;
    let txn_id = txn_begin(handle)?;
    let outcome = (|| {
        for lock in &plan.key_locks {
            txn_lock_key(handle, txn_id, lock.namespace.clone(), lock.key.clone())?;
        }
        let version = submit_batch(handle, &plan.batch)?;
        txn_prepare(handle, txn_id)?;
        txn_commit(handle, txn_id)?;
        Ok(version)
    })();
    if outcome.is_err() {
        let _ = txn_abort(handle, txn_id);
    }
    outcome
}

pub fn parse_statement(statement: &str) -> Result<SqlStatement, DbError> {
    let statement = statement.trim();
    if statement.is_empty() {
        return Err(DbError::invalid_argument("sql parse: empty statement"));
    }

    let tokens = tokenize_statement(statement)?;
    let keyword = tokens
        .first()
        .ok_or_else(|| DbError::invalid_argument("sql parse: missing statement keyword"))?;

    if keyword.eq_ignore_ascii_case("INSERT") {
        let table = tokens
            .get(1)
            .ok_or_else(|| DbError::invalid_argument("sql parse: INSERT missing table"))?;
        let primary_key = tokens
            .get(2)
            .ok_or_else(|| DbError::invalid_argument("sql parse: INSERT missing primary key"))?;
        let row_value = tokens
            .get(3)
            .ok_or_else(|| DbError::invalid_argument("sql parse: INSERT missing row value"))?;
        let secondary_indexes = parse_optional_indexes(&tokens[4..])?;
        return Ok(SqlStatement::Insert {
            table: table.as_bytes().to_vec(),
            primary_key: primary_key.as_bytes().to_vec(),
            row_value: row_value.as_bytes().to_vec(),
            secondary_indexes,
        });
    }

    if keyword.eq_ignore_ascii_case("DELETE") {
        let table = tokens
            .get(1)
            .ok_or_else(|| DbError::invalid_argument("sql parse: DELETE missing table"))?;
        let primary_key = tokens
            .get(2)
            .ok_or_else(|| DbError::invalid_argument("sql parse: DELETE missing primary key"))?;
        let secondary_indexes = parse_optional_indexes(&tokens[3..])?;
        return Ok(SqlStatement::Delete {
            table: table.as_bytes().to_vec(),
            primary_key: primary_key.as_bytes().to_vec(),
            secondary_indexes,
        });
    }

    if keyword.eq_ignore_ascii_case("EXPLAIN") {
        let table = tokens
            .get(1)
            .ok_or_else(|| DbError::invalid_argument("sql parse: EXPLAIN missing table"))?;
        let clause_tokens = &tokens[2..];
        if clause_tokens.len() != 8 {
            return Err(DbError::invalid_argument(
                "sql parse: EXPLAIN requires SELECTIVITY/CARDINALITY/INDEX/STALE clauses",
            ));
        }
        if !clause_tokens[0].eq_ignore_ascii_case("SELECTIVITY")
            || !clause_tokens[2].eq_ignore_ascii_case("CARDINALITY")
            || !clause_tokens[4].eq_ignore_ascii_case("INDEX")
            || !clause_tokens[6].eq_ignore_ascii_case("STALE")
        {
            return Err(DbError::invalid_argument(
                "sql parse: EXPLAIN clause order must be SELECTIVITY CARDINALITY INDEX STALE",
            ));
        }

        let selectivity = parse_u32(&clause_tokens[1], "sql parse: invalid SELECTIVITY")?;
        if selectivity > 10_000 {
            return Err(DbError::invalid_argument(
                "sql parse: SELECTIVITY must be in 0..=10000",
            ));
        }
        let cardinality_estimate = parse_u64(&clause_tokens[3], "sql parse: invalid CARDINALITY")?;
        let index_available = parse_index_flag(&clause_tokens[5])?;
        let stats_stale = parse_bool(&clause_tokens[7], "sql parse: invalid STALE flag")?;

        return Ok(SqlStatement::Explain {
            table: table.as_bytes().to_vec(),
            selectivity,
            cardinality_estimate,
            index_available,
            stats_stale,
        });
    }

    Err(DbError::invalid_argument(format!(
        "sql parse: unsupported statement keyword `{keyword}`"
    )))
}

fn tokenize_statement(statement: &str) -> Result<Vec<String>, DbError> {
    let mut tokens = Vec::new();
    let mut chars = statement.chars().peekable();
    while let Some(ch) = chars.peek().copied() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        if ch == '"' || ch == '\'' {
            let quote = ch;
            chars.next();
            let mut token = String::new();
            let mut escaped = false;
            let mut terminated = false;
            for next in chars.by_ref() {
                if escaped {
                    token.push(next);
                    escaped = false;
                    continue;
                }
                if next == '\\' {
                    escaped = true;
                    continue;
                }
                if next == quote {
                    terminated = true;
                    break;
                }
                token.push(next);
            }
            if !terminated {
                return Err(DbError::invalid_argument(
                    "sql parse: unterminated quoted token",
                ));
            }
            tokens.push(token);
            continue;
        }

        let mut token = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_whitespace() {
                break;
            }
            token.push(next);
            chars.next();
        }
        tokens.push(token);
    }
    Ok(tokens)
}

pub fn compile_statement(
    catalog: &SqlCatalog,
    statement: SqlStatement,
) -> Result<CompiledSql, DbError> {
    match statement {
        SqlStatement::Insert {
            table,
            primary_key,
            row_value,
            secondary_indexes,
        } => {
            validate_catalog_indexes(catalog, &table, &secondary_indexes)?;
            Ok(CompiledSql::Mutation(RowMutation::Put {
                table,
                primary_key,
                row_value,
                secondary_indexes,
            }))
        }
        SqlStatement::Delete {
            table,
            primary_key,
            secondary_indexes,
        } => {
            validate_catalog_indexes(catalog, &table, &secondary_indexes)?;
            Ok(CompiledSql::Mutation(RowMutation::Delete {
                table,
                primary_key,
                secondary_indexes,
            }))
        }
        SqlStatement::Explain {
            table,
            selectivity,
            cardinality_estimate,
            index_available,
            stats_stale,
        } => {
            if catalog.table(&table).is_none() {
                return Err(DbError::invalid_argument(format!(
                    "sql catalog: unknown table `{}`",
                    String::from_utf8_lossy(&table)
                )));
            }

            let stats = planner::PlannerStats {
                snapshot: planner::StatsSnapshot {
                    version: 1,
                    histogram_buckets: vec![planner::HistogramBucket {
                        upper_bound: cardinality_estimate.max(1),
                        row_count: cardinality_estimate,
                    }],
                    cardinality_estimate,
                },
                selectivity,
                index_available,
                stats_stale,
            };
            Ok(CompiledSql::Explain(planner::explain(stats)))
        }
    }
}

pub fn run_conformance_suite(
    catalog: &SqlCatalog,
    cases: &[ConformanceCase],
) -> Vec<ConformanceResult> {
    cases
        .iter()
        .map(|case| {
            let evaluated = parse_statement(&case.statement)
                .and_then(|stmt| compile_statement(catalog, stmt))
                .and_then(|compiled| match compiled {
                    CompiledSql::Mutation(mutation) => {
                        plan(&[mutation]).map(ConformanceObserved::MutationPlan)
                    }
                    CompiledSql::Explain(output) => {
                        Ok(ConformanceObserved::ExplainPlan(output.chosen_plan))
                    }
                });
            evaluate_conformance_case(case, evaluated)
        })
        .collect()
}

fn evaluate_conformance_case(
    case: &ConformanceCase,
    evaluated: Result<ConformanceObserved, DbError>,
) -> ConformanceResult {
    let (passed, detail) = match (&case.expect, evaluated) {
        (
            ConformanceExpectation::MutationPlan {
                lock_count,
                batch_count,
            },
            Ok(ConformanceObserved::MutationPlan(plan)),
        ) => {
            let pass = plan.key_locks.len() == *lock_count && plan.batch.len() == *batch_count;
            (
                pass,
                format!(
                    "observed mutation plan locks={} batch={}",
                    plan.key_locks.len(),
                    plan.batch.len()
                ),
            )
        }
        (
            ConformanceExpectation::ExplainPlan { kind },
            Ok(ConformanceObserved::ExplainPlan(observed)),
        ) => {
            let pass = observed == *kind;
            (pass, format!("observed explain plan {:?}", observed))
        }
        (ConformanceExpectation::Rejected { token }, Err(err)) => {
            let observed = classify_sql_mutation_error(&err)
                .map(|kind| kind.token())
                .or_else(|| {
                    if err.code == ErrorCode::InvalidArgument {
                        Some(SQL_INVALID_MUTATION)
                    } else {
                        None
                    }
                });
            let pass = observed == Some(*token);
            (
                pass,
                format!("observed rejection token {}", observed.unwrap_or("NONE")),
            )
        }
        (_, Ok(_)) => (
            false,
            "expected rejection but statement was accepted".to_string(),
        ),
        (_, Err(err)) => (false, format!("unexpected error: {}", err.message)),
    };

    ConformanceResult {
        name: case.name.clone(),
        passed,
        detail,
    }
}

fn classify_sql_mutation_error(err: &DbError) -> Option<SqlMutationErrorKind> {
    match err.code {
        ErrorCode::InvalidArgument if err.message.contains("requires at least one mutation") => {
            Some(SqlMutationErrorKind::InvalidMutation)
        }
        ErrorCode::LimitExceeded
            if err.message.contains("lock held by txn=")
                || err.message.contains("deadlock victim txn=") =>
        {
            Some(SqlMutationErrorKind::RetryableConflict)
        }
        _ => None,
    }
}

fn parse_optional_indexes(tokens: &[String]) -> Result<Vec<SecondaryIndexEntry>, DbError> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    if tokens.len() != 2 || !tokens[0].eq_ignore_ascii_case("INDEX") {
        return Err(DbError::invalid_argument(
            "sql parse: expected optional INDEX clause in form `INDEX idx=key,idx2=key2`",
        ));
    }

    let mut indexes = Vec::new();
    for assignment in tokens[1].split(',') {
        let assignment = assignment.trim();
        if assignment.is_empty() {
            continue;
        }
        let mut kv = assignment.splitn(2, '=');
        let name = kv
            .next()
            .ok_or_else(|| DbError::invalid_argument("sql parse: malformed index assignment"))?;
        let key = kv
            .next()
            .ok_or_else(|| DbError::invalid_argument("sql parse: malformed index assignment"))?;
        if name.is_empty() || key.is_empty() {
            return Err(DbError::invalid_argument(
                "sql parse: index assignment requires `name=key`",
            ));
        }
        indexes.push(SecondaryIndexEntry {
            index_name: name.as_bytes().to_vec(),
            index_key: key.as_bytes().to_vec(),
        });
    }
    Ok(indexes)
}

fn parse_u32(value: &str, message: &str) -> Result<u32, DbError> {
    value
        .parse::<u32>()
        .map_err(|_| DbError::invalid_argument(message))
}

fn parse_u64(value: &str, message: &str) -> Result<u64, DbError> {
    value
        .parse::<u64>()
        .map_err(|_| DbError::invalid_argument(message))
}

fn parse_bool(value: &str, message: &str) -> Result<bool, DbError> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(DbError::invalid_argument(message)),
    }
}

fn parse_index_flag(value: &str) -> Result<bool, DbError> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "true" => Ok(true),
        "off" | "false" => Ok(false),
        _ => Err(DbError::invalid_argument(
            "sql parse: INDEX flag must be one of on/off/true/false",
        )),
    }
}

fn validate_catalog_indexes(
    catalog: &SqlCatalog,
    table: &[u8],
    indexes: &[SecondaryIndexEntry],
) -> Result<(), DbError> {
    let table_catalog = catalog.table(table).ok_or_else(|| {
        DbError::invalid_argument(format!(
            "sql catalog: unknown table `{}`",
            String::from_utf8_lossy(table)
        ))
    })?;
    let known: HashSet<&[u8]> = table_catalog
        .secondary_indexes
        .iter()
        .map(Vec::as_slice)
        .collect();
    for index in indexes {
        if !known.contains(index.index_name.as_slice()) {
            return Err(DbError::invalid_argument(format!(
                "sql catalog: unknown secondary index `{}` for table `{}`",
                String::from_utf8_lossy(&index.index_name),
                String::from_utf8_lossy(table)
            )));
        }
    }
    Ok(())
}

#[derive(Debug)]
enum ConformanceObserved {
    MutationPlan(DmlPlan),
    ExplainPlan(PlanKind),
}

pub fn plan(mutations: &[RowMutation]) -> Result<DmlPlan, DbError> {
    if mutations.is_empty() {
        return Err(DbError::invalid_argument(
            "sql dml requires at least one mutation",
        ));
    }

    let mut key_locks = Vec::new();
    let mut batch = Vec::new();
    for mutation in mutations {
        match mutation {
            RowMutation::Put {
                table,
                primary_key,
                row_value,
                secondary_indexes,
            } => {
                let row_key = row_key(table, primary_key);
                key_locks.push(LockKey {
                    namespace: ROW_NAMESPACE.to_vec(),
                    key: row_key.clone(),
                });
                batch.push(BatchOp::Put {
                    namespace: ROW_NAMESPACE.to_vec(),
                    key: row_key,
                    value: row_value.clone(),
                    expected_version: None,
                });
                for index in secondary_indexes {
                    let index_key =
                        index_key(table, &index.index_name, &index.index_key, primary_key);
                    key_locks.push(LockKey {
                        namespace: INDEX_NAMESPACE.to_vec(),
                        key: index_key.clone(),
                    });
                    batch.push(BatchOp::Put {
                        namespace: INDEX_NAMESPACE.to_vec(),
                        key: index_key,
                        value: primary_key.clone(),
                        expected_version: None,
                    });
                }
            }
            RowMutation::Delete {
                table,
                primary_key,
                secondary_indexes,
            } => {
                let row_key = row_key(table, primary_key);
                key_locks.push(LockKey {
                    namespace: ROW_NAMESPACE.to_vec(),
                    key: row_key.clone(),
                });
                batch.push(BatchOp::Delete {
                    namespace: ROW_NAMESPACE.to_vec(),
                    key: row_key,
                    expected_version: None,
                });
                for index in secondary_indexes {
                    let index_key =
                        index_key(table, &index.index_name, &index.index_key, primary_key);
                    key_locks.push(LockKey {
                        namespace: INDEX_NAMESPACE.to_vec(),
                        key: index_key.clone(),
                    });
                    batch.push(BatchOp::Delete {
                        namespace: INDEX_NAMESPACE.to_vec(),
                        key: index_key,
                        expected_version: None,
                    });
                }
            }
        }
    }

    key_locks.sort_by(|a, b| {
        a.namespace
            .cmp(&b.namespace)
            .then_with(|| a.key.cmp(&b.key))
    });
    key_locks.dedup();
    batch.sort_by(batch_order);

    Ok(DmlPlan { key_locks, batch })
}

pub fn row_namespace() -> &'static [u8] {
    ROW_NAMESPACE
}

pub fn index_namespace() -> &'static [u8] {
    INDEX_NAMESPACE
}

pub fn row_key(table: &[u8], primary_key: &[u8]) -> Vec<u8> {
    encode_parts(&[table, primary_key])
}

pub fn index_key(table: &[u8], index_name: &[u8], index_key: &[u8], primary_key: &[u8]) -> Vec<u8> {
    encode_parts(&[table, index_name, index_key, primary_key])
}

fn batch_order(a: &BatchOp, b: &BatchOp) -> std::cmp::Ordering {
    let a_fields = batch_fields(a);
    let b_fields = batch_fields(b);
    a_fields
        .0
        .cmp(b_fields.0)
        .then_with(|| a_fields.1.cmp(b_fields.1))
        .then_with(|| a_fields.2.cmp(&b_fields.2))
}

fn batch_fields(op: &BatchOp) -> (&[u8], &[u8], u8) {
    match op {
        BatchOp::Delete { namespace, key, .. } => (namespace, key, 0),
        BatchOp::Put { namespace, key, .. } => (namespace, key, 1),
    }
}

fn encode_parts(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(&(part.len() as u16).to_be_bytes());
        out.extend_from_slice(part);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::types::ErrorCode;
    use crate::db::{close_db, open_db, read_point, txn_abort, txn_begin, txn_lock_key};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let base = std::env::temp_dir().join(format!(
            "wrela_db_sql_test_{}_{}_{}",
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

    fn sample_put(primary_key: &[u8]) -> RowMutation {
        RowMutation::Put {
            table: b"users".to_vec(),
            primary_key: primary_key.to_vec(),
            row_value: b"{\"name\":\"ada\"}".to_vec(),
            secondary_indexes: vec![
                SecondaryIndexEntry {
                    index_name: b"by_email".to_vec(),
                    index_key: b"ada@example.com".to_vec(),
                },
                SecondaryIndexEntry {
                    index_name: b"by_handle".to_vec(),
                    index_key: b"ada".to_vec(),
                },
            ],
        }
    }

    fn sample_delete(primary_key: &[u8]) -> RowMutation {
        RowMutation::Delete {
            table: b"users".to_vec(),
            primary_key: primary_key.to_vec(),
            secondary_indexes: vec![
                SecondaryIndexEntry {
                    index_name: b"by_email".to_vec(),
                    index_key: b"ada@example.com".to_vec(),
                },
                SecondaryIndexEntry {
                    index_name: b"by_handle".to_vec(),
                    index_key: b"ada".to_vec(),
                },
            ],
        }
    }

    fn sample_catalog() -> SqlCatalog {
        let mut catalog = SqlCatalog::new();
        catalog.register_table(
            b"users".to_vec(),
            vec![b"by_email".to_vec(), b"by_handle".to_vec()],
        );
        catalog
    }

    #[test]
    fn execute_put_writes_row_and_secondary_indexes() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        execute(handle, &[sample_put(b"u1")]).expect("execute put");

        let row_key = row_key(b"users", b"u1");
        let row = read_point(handle, row_namespace().to_vec(), row_key).expect("read row");
        assert_eq!(row, Some(b"{\"name\":\"ada\"}".to_vec()));

        let email_ix = index_key(b"users", b"by_email", b"ada@example.com", b"u1");
        let email_value =
            read_point(handle, index_namespace().to_vec(), email_ix).expect("read email index");
        assert_eq!(email_value, Some(b"u1".to_vec()));

        let handle_ix = index_key(b"users", b"by_handle", b"ada", b"u1");
        let handle_value =
            read_point(handle, index_namespace().to_vec(), handle_ix).expect("read handle index");
        assert_eq!(handle_value, Some(b"u1".to_vec()));

        assert!(close_db(handle));
    }

    #[test]
    fn execute_delete_removes_row_and_secondary_indexes() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        execute(handle, &[sample_put(b"u2")]).expect("seed put");
        execute(handle, &[sample_delete(b"u2")]).expect("execute delete");

        let row_key = row_key(b"users", b"u2");
        let row = read_point(handle, row_namespace().to_vec(), row_key).expect("read row");
        assert_eq!(row, None);

        let email_ix = index_key(b"users", b"by_email", b"ada@example.com", b"u2");
        let email_value =
            read_point(handle, index_namespace().to_vec(), email_ix).expect("read email index");
        assert_eq!(email_value, None);

        let handle_ix = index_key(b"users", b"by_handle", b"ada", b"u2");
        let handle_value =
            read_point(handle, index_namespace().to_vec(), handle_ix).expect("read handle index");
        assert_eq!(handle_value, None);

        assert!(close_db(handle));
    }

    #[test]
    fn execute_conflict_lock_fails_without_partial_writes() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let row_key = row_key(b"users", b"u3");

        let blocker = txn_begin(handle).expect("begin blocker txn");
        txn_lock_key(handle, blocker, row_namespace().to_vec(), row_key.clone())
            .expect("lock row key");

        let err = execute(handle, &[sample_put(b"u3")]).expect_err("lock conflict");
        assert_eq!(err.code, ErrorCode::LimitExceeded);

        let row = read_point(handle, row_namespace().to_vec(), row_key).expect("read row");
        assert_eq!(row, None);

        let email_ix = index_key(b"users", b"by_email", b"ada@example.com", b"u3");
        let email_value =
            read_point(handle, index_namespace().to_vec(), email_ix).expect("read email index");
        assert_eq!(email_value, None);

        let handle_ix = index_key(b"users", b"by_handle", b"ada", b"u3");
        let handle_value =
            read_point(handle, index_namespace().to_vec(), handle_ix).expect("read handle index");
        assert_eq!(handle_value, None);

        txn_abort(handle, blocker).expect("cleanup blocker");
        assert!(close_db(handle));
    }

    #[test]
    fn execute_with_result_maps_lock_conflict_to_retryable_conflict_token() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");
        let row_key = row_key(b"users", b"u4");

        let blocker = txn_begin(handle).expect("begin blocker txn");
        txn_lock_key(handle, blocker, row_namespace().to_vec(), row_key).expect("lock row key");

        let result = execute_with_result(handle, &[sample_put(b"u4")]);
        match result {
            SqlExecuteResult::Rejected(error) => {
                assert_eq!(error.kind, SqlMutationErrorKind::RetryableConflict);
                assert_eq!(error.token, SQL_RETRYABLE_CONFLICT);
            }
            other => panic!("expected rejected result, got {:?}", other),
        }

        txn_abort(handle, blocker).expect("cleanup blocker");
        assert!(close_db(handle));
    }

    #[test]
    fn execute_with_result_maps_empty_mutation_to_invalid_mutation_token() {
        let dir = temp_dir();
        let handle = open_db(&dir).expect("open db");

        let result = execute_with_result(handle, &[]);
        match result {
            SqlExecuteResult::Rejected(error) => {
                assert_eq!(error.kind, SqlMutationErrorKind::InvalidMutation);
                assert_eq!(error.token, SQL_INVALID_MUTATION);
            }
            other => panic!("expected rejected result, got {:?}", other),
        }

        assert!(close_db(handle));
    }

    #[test]
    fn parse_statement_insert_and_catalog_compile_succeeds() {
        let catalog = sample_catalog();
        let parsed =
            parse_statement("INSERT users u5 rowv INDEX by_email=ada@example.com,by_handle=ada")
                .expect("parse insert");
        let compiled = compile_statement(&catalog, parsed).expect("compile");
        let CompiledSql::Mutation(mutation) = compiled else {
            panic!("expected mutation");
        };
        let planned = plan(&[mutation]).expect("plan");
        assert_eq!(planned.key_locks.len(), 3);
        assert_eq!(planned.batch.len(), 3);
    }

    #[test]
    fn parse_statement_rejects_unknown_keyword() {
        let err = parse_statement("UPSERT users u1 value").expect_err("must reject unsupported");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("unsupported statement"));
    }

    #[test]
    fn parse_statement_supports_quoted_row_values() {
        let parsed = parse_statement(
            "INSERT users u9 \"{\\\"name\\\":\\\"ada lovelace\\\"}\" INDEX by_email=ada@example.com",
        )
        .expect("parse quoted row value");
        let SqlStatement::Insert { row_value, .. } = parsed else {
            panic!("expected insert");
        };
        assert_eq!(row_value, b"{\"name\":\"ada lovelace\"}".to_vec());
    }

    #[test]
    fn parse_statement_rejects_unterminated_quote() {
        let err = parse_statement("INSERT users u9 \"unterminated").expect_err("must reject");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("unterminated quoted token"));
    }

    #[test]
    fn catalog_rejects_unknown_table_or_index() {
        let catalog = sample_catalog();
        let unknown_table =
            parse_statement("INSERT ghosts g1 rowv INDEX by_email=x").expect("parse");
        let err = compile_statement(&catalog, unknown_table).expect_err("unknown table");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("unknown table"));

        let unknown_index =
            parse_statement("INSERT users u1 rowv INDEX by_phone=555").expect("parse");
        let err = compile_statement(&catalog, unknown_index).expect_err("unknown index");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("unknown secondary index"));
    }

    #[test]
    fn explain_statement_compiles_to_planner_output() {
        let catalog = sample_catalog();
        let parsed =
            parse_statement("EXPLAIN users SELECTIVITY 500 CARDINALITY 1000 INDEX on STALE false")
                .expect("parse explain");
        let compiled = compile_statement(&catalog, parsed).expect("compile explain");
        let CompiledSql::Explain(explain) = compiled else {
            panic!("expected explain output");
        };
        assert_eq!(explain.stats_version, 1);
        assert_eq!(explain.stats_stale, false);
    }

    #[test]
    fn conformance_suite_reports_pass_and_fail_deterministically() {
        let catalog = sample_catalog();
        let cases = vec![
            ConformanceCase {
                name: "insert_plan".to_string(),
                statement: "INSERT users u7 rowv INDEX by_email=a,by_handle=b".to_string(),
                expect: ConformanceExpectation::MutationPlan {
                    lock_count: 3,
                    batch_count: 3,
                },
            },
            ConformanceCase {
                name: "explain_plan".to_string(),
                statement: "EXPLAIN users SELECTIVITY 10000 CARDINALITY 1000 INDEX off STALE false"
                    .to_string(),
                expect: ConformanceExpectation::ExplainPlan {
                    kind: PlanKind::FullScan,
                },
            },
            ConformanceCase {
                name: "bad_table".to_string(),
                statement: "INSERT missing u1 rowv".to_string(),
                expect: ConformanceExpectation::Rejected {
                    token: SQL_INVALID_MUTATION,
                },
            },
            ConformanceCase {
                name: "intentional_fail".to_string(),
                statement: "INSERT users u8 rowv".to_string(),
                expect: ConformanceExpectation::MutationPlan {
                    lock_count: 99,
                    batch_count: 99,
                },
            },
        ];
        let results = run_conformance_suite(&catalog, &cases);
        assert_eq!(results.len(), 4);
        assert!(results[0].passed);
        assert!(results[1].passed);
        assert!(results[2].passed);
        assert!(!results[3].passed);
    }
}
