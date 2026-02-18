use crate::db::analytics::operators::{Batch, hash_join_eq};
use crate::db::analytics::policy::FederatedResidencyMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederatedSource {
    pub source_id: String,
    pub region: String,
    pub shard: Vec<u8>,
    pub batch: Batch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FederatedMergeStrategy {
    UnionAll,
    HashJoinEq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederatedPlan {
    pub plan_id: String,
    pub source_ids: Vec<String>,
    pub strategy: FederatedMergeStrategy,
    pub join_keys: Option<(String, String)>,
    pub output_columns: Vec<(String, String)>,
    pub residency_mode: FederatedResidencyMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FederatedPlanError {
    InvalidSourceCount,
    MissingJoinKeys,
    InvalidOutputColumn,
}

pub fn plan(
    plan_id: impl Into<String>,
    sources: &[FederatedSource],
    strategy: FederatedMergeStrategy,
    join_keys: Option<(String, String)>,
    output_columns: Vec<(String, String)>,
    residency_mode: FederatedResidencyMode,
) -> Result<FederatedPlan, FederatedPlanError> {
    if sources.is_empty() {
        return Err(FederatedPlanError::InvalidSourceCount);
    }
    if strategy == FederatedMergeStrategy::HashJoinEq && sources.len() != 2 {
        return Err(FederatedPlanError::InvalidSourceCount);
    }
    if strategy == FederatedMergeStrategy::HashJoinEq && join_keys.is_none() {
        return Err(FederatedPlanError::MissingJoinKeys);
    }
    if output_columns
        .iter()
        .any(|(_, source)| !source.contains('.'))
    {
        return Err(FederatedPlanError::InvalidOutputColumn);
    }

    Ok(FederatedPlan {
        plan_id: plan_id.into(),
        source_ids: sources.iter().map(|s| s.source_id.clone()).collect(),
        strategy,
        join_keys,
        output_columns,
        residency_mode,
    })
}

pub fn execute(
    plan: &FederatedPlan,
    sources: &[FederatedSource],
) -> Result<Batch, FederatedPlanError> {
    match plan.strategy {
        FederatedMergeStrategy::UnionAll => execute_union_all(sources),
        FederatedMergeStrategy::HashJoinEq => execute_hash_join(plan, sources),
    }
}

fn execute_union_all(sources: &[FederatedSource]) -> Result<Batch, FederatedPlanError> {
    if sources.is_empty() {
        return Err(FederatedPlanError::InvalidSourceCount);
    }
    let mut output = sources[0].batch.clone();
    for source in sources.iter().skip(1) {
        for (column, values) in &source.batch.columns {
            output
                .columns
                .entry(column.clone())
                .or_default()
                .extend(values.iter().cloned());
        }
    }
    Ok(output)
}

fn execute_hash_join(
    plan: &FederatedPlan,
    sources: &[FederatedSource],
) -> Result<Batch, FederatedPlanError> {
    if sources.len() != 2 {
        return Err(FederatedPlanError::InvalidSourceCount);
    }
    let (left_key, right_key) = plan
        .join_keys
        .clone()
        .ok_or(FederatedPlanError::MissingJoinKeys)?;
    let output_columns: Vec<(&str, &str)> = plan
        .output_columns
        .iter()
        .map(|(alias, source)| (alias.as_str(), source.as_str()))
        .collect();
    Ok(hash_join_eq(
        &sources[0].batch,
        &left_key,
        &sources[1].batch,
        &right_key,
        &output_columns,
    ))
}

#[cfg(test)]
mod tests {
    use super::{FederatedMergeStrategy, FederatedSource, execute, plan};
    use crate::db::analytics::operators::Batch;
    use crate::db::analytics::policy::FederatedResidencyMode;
    use std::collections::BTreeMap;

    #[test]
    fn hash_join_plan_and_execute_are_deterministic() {
        let left = FederatedSource {
            source_id: "left".to_string(),
            region: "us".to_string(),
            shard: b"s1".to_vec(),
            batch: Batch::new(BTreeMap::from([
                (
                    "id".to_string(),
                    vec![Some(b"1".to_vec()), Some(b"2".to_vec())],
                ),
                (
                    "city".to_string(),
                    vec![Some(b"austin".to_vec()), Some(b"seattle".to_vec())],
                ),
            ])),
        };
        let right = FederatedSource {
            source_id: "right".to_string(),
            region: "eu".to_string(),
            shard: b"s2".to_vec(),
            batch: Batch::new(BTreeMap::from([
                (
                    "id".to_string(),
                    vec![Some(b"2".to_vec()), Some(b"1".to_vec())],
                ),
                (
                    "tier".to_string(),
                    vec![Some(b"gold".to_vec()), Some(b"silver".to_vec())],
                ),
            ])),
        };
        let plan = plan(
            "plan-1",
            &[left.clone(), right.clone()],
            FederatedMergeStrategy::HashJoinEq,
            Some(("id".to_string(), "id".to_string())),
            vec![
                ("city".to_string(), "left.city".to_string()),
                ("tier".to_string(), "right.tier".to_string()),
            ],
            FederatedResidencyMode::FederatedMergeNoRawExport,
        )
        .expect("plan should build");
        let merged = execute(&plan, &[left, right]).expect("execute should pass");
        assert_eq!(merged.row_count(), 2);
    }
}
