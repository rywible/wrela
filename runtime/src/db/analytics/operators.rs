use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub columns: BTreeMap<String, Vec<Option<Vec<u8>>>>,
}

impl Batch {
    pub fn new(columns: BTreeMap<String, Vec<Option<Vec<u8>>>>) -> Self {
        Self { columns }
    }

    pub fn row_count(&self) -> usize {
        self.columns.values().next().map_or(0, Vec::len)
    }

    pub fn project(&self, selected: &[&str]) -> Batch {
        let mut columns = BTreeMap::new();
        for name in selected {
            if let Some(values) = self.columns.get(*name) {
                columns.insert((*name).to_string(), values.clone());
            }
        }
        Batch { columns }
    }

    pub fn filter_eq(&self, column: &str, expected: &[u8]) -> Batch {
        let Some(values) = self.columns.get(column) else {
            return Batch {
                columns: BTreeMap::new(),
            };
        };

        let mask: Vec<bool> = values
            .iter()
            .map(|value| value.as_ref().is_some_and(|val| val.as_slice() == expected))
            .collect();

        let mut filtered = BTreeMap::new();
        for (name, col) in &self.columns {
            let mut next = Vec::new();
            for (idx, keep) in mask.iter().enumerate() {
                if *keep {
                    next.push(col[idx].clone());
                }
            }
            filtered.insert(name.clone(), next);
        }
        Batch { columns: filtered }
    }

    pub fn aggregate_count_by(&self, column: &str) -> BTreeMap<Vec<u8>, usize> {
        let mut out = BTreeMap::new();
        let Some(values) = self.columns.get(column) else {
            return out;
        };
        for value in values.iter().flatten() {
            *out.entry(value.clone()).or_insert(0) += 1;
        }
        out
    }
}

pub fn hash_join_eq(
    left: &Batch,
    left_key: &str,
    right: &Batch,
    right_key: &str,
    output_columns: &[(&str, &str)],
) -> Batch {
    let mut right_index: BTreeMap<Vec<u8>, Vec<usize>> = BTreeMap::new();
    if let Some(keys) = right.columns.get(right_key) {
        for (idx, key) in keys.iter().enumerate() {
            if let Some(key) = key {
                right_index.entry(key.clone()).or_default().push(idx);
            }
        }
    }

    let mut output: BTreeMap<String, Vec<Option<Vec<u8>>>> = BTreeMap::new();
    for (alias, _) in output_columns {
        output.entry((*alias).to_string()).or_default();
    }

    let Some(left_keys) = left.columns.get(left_key) else {
        return Batch::new(output);
    };

    for (left_idx, left_val) in left_keys.iter().enumerate() {
        let Some(key) = left_val else {
            continue;
        };
        let Some(right_rows) = right_index.get(key) else {
            continue;
        };
        for right_idx in right_rows {
            for (alias, source) in output_columns {
                let (side, col) = source
                    .split_once('.')
                    .expect("output source must use side.column format");
                let value = match side {
                    "left" => left
                        .columns
                        .get(col)
                        .and_then(|vals| vals.get(left_idx).cloned())
                        .unwrap_or(None),
                    "right" => right
                        .columns
                        .get(col)
                        .and_then(|vals| vals.get(*right_idx).cloned())
                        .unwrap_or(None),
                    _ => None,
                };
                output.entry((*alias).to_string()).or_default().push(value);
            }
        }
    }

    Batch::new(output)
}

#[cfg(test)]
mod tests {
    use super::{Batch, hash_join_eq};
    use std::collections::BTreeMap;

    #[test]
    fn vector_filter_project_aggregate_are_deterministic() {
        let mut cols = BTreeMap::new();
        cols.insert(
            "region".to_string(),
            vec![
                Some(b"us".to_vec()),
                Some(b"eu".to_vec()),
                Some(b"us".to_vec()),
            ],
        );
        cols.insert(
            "value".to_string(),
            vec![
                Some(b"1".to_vec()),
                Some(b"2".to_vec()),
                Some(b"3".to_vec()),
            ],
        );
        let batch = Batch::new(cols);
        let filtered = batch.filter_eq("region", b"us");
        let projected = filtered.project(&["region"]);
        let agg = projected.aggregate_count_by("region");
        assert_eq!(projected.row_count(), 2);
        assert_eq!(agg.get(b"us".as_slice()), Some(&2));
    }

    #[test]
    fn hash_join_equijoin_matches_rows() {
        let left = Batch::new(BTreeMap::from([
            (
                "id".to_string(),
                vec![Some(b"1".to_vec()), Some(b"2".to_vec())],
            ),
            (
                "city".to_string(),
                vec![Some(b"austin".to_vec()), Some(b"paris".to_vec())],
            ),
        ]));
        let right = Batch::new(BTreeMap::from([
            (
                "id".to_string(),
                vec![Some(b"2".to_vec()), Some(b"1".to_vec())],
            ),
            (
                "tier".to_string(),
                vec![Some(b"gold".to_vec()), Some(b"silver".to_vec())],
            ),
        ]));

        let joined = hash_join_eq(
            &left,
            "id",
            &right,
            "id",
            &[("city", "left.city"), ("tier", "right.tier")],
        );
        assert_eq!(joined.row_count(), 2);
        assert_eq!(
            joined.columns.get("city").cloned().unwrap_or_default(),
            vec![Some(b"austin".to_vec()), Some(b"paris".to_vec())]
        );
    }
}
