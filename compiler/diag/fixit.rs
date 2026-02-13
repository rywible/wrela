use crate::diag::DiagFix;
use std::collections::HashSet;

pub fn normalize_and_filter_fixes(fixes: Vec<DiagFix>) -> Vec<DiagFix> {
    let mut fixes = fixes
        .into_iter()
        .filter(|fix| match fix.safety_tier.as_str() {
            "safe" => fix.confidence >= 0.95,
            _ => true,
        })
        .collect::<Vec<_>>();

    fixes.sort_by_key(|fix| {
        (
            fix.span.path.clone(),
            fix.span.offset,
            fix.span.len,
            fix.replacement.clone(),
        )
    });

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for fix in fixes {
        let key = format!(
            "{}:{}:{}:{}",
            fix.span.path, fix.span.offset, fix.span.len, fix.replacement
        );
        if !seen.insert(key) {
            continue;
        }

        let overlaps_conflict = out.iter().any(|existing: &DiagFix| {
            if existing.span.path != fix.span.path {
                return false;
            }
            let a0 = existing.span.offset;
            let a1 = existing.span.offset.saturating_add(existing.span.len);
            let b0 = fix.span.offset;
            let b1 = fix.span.offset.saturating_add(fix.span.len);
            let overlaps = a0 < b1 && b0 < a1;
            overlaps && existing.replacement != fix.replacement
        });
        if overlaps_conflict {
            continue;
        }
        out.push(fix);
    }
    out
}
