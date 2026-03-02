use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSegment {
    pub segment_id: u64,
    pub table: String,
    pub column: String,
    pub rows: Vec<Option<Vec<u8>>>,
    pub min_value: Option<Vec<u8>>,
    pub max_value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnStats {
    pub row_count: usize,
    pub null_count: usize,
    pub distinct_count: usize,
    pub min_value: Option<Vec<u8>>,
    pub max_value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnarTable {
    pub name: String,
    pub columns: BTreeMap<String, Vec<ColumnSegment>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnarStore {
    tables: BTreeMap<String, ColumnarTable>,
    next_segment_id: u64,
}

impl Default for ColumnarStore {
    fn default() -> Self {
        Self {
            tables: BTreeMap::new(),
            next_segment_id: 1,
        }
    }
}

impl ColumnarStore {
    pub fn append_segment(
        &mut self,
        table: impl Into<String>,
        column: impl Into<String>,
        rows: Vec<Option<Vec<u8>>>,
    ) -> u64 {
        let table = table.into();
        let column = column.into();
        let segment_id = self.next_segment_id;
        self.next_segment_id = self.next_segment_id.saturating_add(1);
        let (min_value, max_value) = min_max(&rows);
        let segment = ColumnSegment {
            segment_id,
            table: table.clone(),
            column: column.clone(),
            rows,
            min_value,
            max_value,
        };
        let entry = self
            .tables
            .entry(table.clone())
            .or_insert_with(|| ColumnarTable {
                name: table,
                columns: BTreeMap::new(),
            });
        entry.columns.entry(column).or_default().push(segment);
        segment_id
    }

    pub fn table(&self, name: &str) -> Option<&ColumnarTable> {
        self.tables.get(name)
    }

    pub fn scan_column(&self, table: &str, column: &str) -> Vec<Option<Vec<u8>>> {
        self.tables
            .get(table)
            .and_then(|tbl| tbl.columns.get(column))
            .map(|segments| {
                segments
                    .iter()
                    .flat_map(|segment| segment.rows.iter().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn compact_column(&mut self, table: &str, column: &str) -> Option<u64> {
        let table_ref = self.tables.get_mut(table)?;
        let segments = table_ref.columns.get_mut(column)?;
        if segments.len() <= 1 {
            return segments.first().map(|segment| segment.segment_id);
        }

        let mut merged_rows = Vec::new();
        for segment in segments.iter() {
            merged_rows.extend(segment.rows.iter().cloned());
        }

        let new_id = self.next_segment_id;
        self.next_segment_id = self.next_segment_id.saturating_add(1);
        let (min_value, max_value) = min_max(&merged_rows);
        *segments = vec![ColumnSegment {
            segment_id: new_id,
            table: table.to_string(),
            column: column.to_string(),
            rows: merged_rows,
            min_value,
            max_value,
        }];
        Some(new_id)
    }

    pub fn column_stats(&self, table: &str, column: &str) -> Option<ColumnStats> {
        let rows = self.scan_column(table, column);
        if rows.is_empty() {
            return None;
        }
        let null_count = rows.iter().filter(|value| value.is_none()).count();
        let mut distinct = HashMap::new();
        for value in rows.iter().flatten() {
            *distinct.entry(value.clone()).or_insert(0usize) += 1;
        }
        let (min_value, max_value) = min_max(&rows);
        Some(ColumnStats {
            row_count: rows.len(),
            null_count,
            distinct_count: distinct.len(),
            min_value,
            max_value,
        })
    }
}

fn min_max(values: &[Option<Vec<u8>>]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let mut min: Option<Vec<u8>> = None;
    let mut max: Option<Vec<u8>> = None;
    for value in values.iter().flatten() {
        if min.as_ref().is_none_or(|current| value < current) {
            min = Some(value.clone());
        }
        if max.as_ref().is_none_or(|current| value > current) {
            max = Some(value.clone());
        }
    }
    (min, max)
}

#[cfg(test)]
mod tests {
    use super::ColumnarStore;

    #[test]
    fn append_scan_and_compact_are_deterministic() {
        let mut store = ColumnarStore::default();
        let s1 = store.append_segment(
            "orders",
            "status",
            vec![Some(b"new".to_vec()), Some(b"paid".to_vec())],
        );
        let s2 = store.append_segment("orders", "status", vec![Some(b"shipped".to_vec()), None]);
        assert!(s2 > s1);

        let before = store.scan_column("orders", "status");
        assert_eq!(before.len(), 4);
        let compacted = store
            .compact_column("orders", "status")
            .expect("segment id returned");
        assert!(compacted > s2);

        let after = store.scan_column("orders", "status");
        assert_eq!(before, after);
    }

    #[test]
    fn stats_include_distinct_null_and_bounds() {
        let mut store = ColumnarStore::default();
        store.append_segment(
            "orders",
            "region",
            vec![
                Some(b"us-east".to_vec()),
                Some(b"eu-west".to_vec()),
                Some(b"us-east".to_vec()),
                None,
            ],
        );
        let stats = store
            .column_stats("orders", "region")
            .expect("stats available");
        assert_eq!(stats.row_count, 4);
        assert_eq!(stats.null_count, 1);
        assert_eq!(stats.distinct_count, 2);
        assert_eq!(stats.min_value, Some(b"eu-west".to_vec()));
        assert_eq!(stats.max_value, Some(b"us-east".to_vec()));
    }
}
