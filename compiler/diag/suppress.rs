use crate::diag::{DiagRecord, DiagStage};
use std::collections::HashMap;

pub fn suppress_cascades(mut records: Vec<DiagRecord>) -> Vec<DiagRecord> {
    records.sort_by_key(|record| {
        let primary = record.labels.first();
        let path = primary.map(|l| l.span.path.clone()).unwrap_or_default();
        let offset = primary.map(|l| l.span.offset).unwrap_or(0);
        (path, offset)
    });

    let mut blocked_regions: HashMap<(String, usize), String> = HashMap::new();
    let mut out = Vec::new();

    for mut record in records {
        let Some(primary) = record.labels.first() else {
            out.push(record);
            continue;
        };
        let region = (primary.span.path.clone(), primary.span.offset / 64);
        let is_downstream = matches!(record.stage, DiagStage::Semantic | DiagStage::Type);

        if let Some(blocker) = blocked_regions.get(&region)
            && is_downstream
        {
            record.blocked_by = Some(blocker.clone());
            continue;
        }

        if matches!(record.stage, DiagStage::Parse | DiagStage::Validate) {
            blocked_regions.insert(region, record.diag_id.clone());
        }

        out.push(record);
    }
    out
}
