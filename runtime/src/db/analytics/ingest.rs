use crate::db::analytics::columnar::ColumnarStore;
use crate::db::cdc::{CdcEmitter, CdcOpKind};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestCheckpoint {
    pub stream: String,
    pub commit_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestStats {
    pub applied_events: usize,
    pub skipped_events: usize,
    pub next_commit_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestPipeline {
    checkpoints: BTreeMap<String, u64>,
}

impl Default for IngestPipeline {
    fn default() -> Self {
        Self {
            checkpoints: BTreeMap::new(),
        }
    }
}

impl IngestPipeline {
    pub fn checkpoint(&self, stream: &str) -> Option<u64> {
        self.checkpoints.get(stream).copied()
    }

    pub fn restore_checkpoint(&mut self, checkpoint: IngestCheckpoint) {
        let entry = self.checkpoints.entry(checkpoint.stream).or_insert(0);
        if checkpoint.commit_seq > *entry {
            *entry = checkpoint.commit_seq;
        }
    }

    pub fn ingest_stream(
        &mut self,
        stream: &str,
        emitter: &CdcEmitter,
        store: &mut ColumnarStore,
        table: &str,
        value_column: &str,
        batch_limit: usize,
    ) -> IngestStats {
        let current = self.checkpoints.get(stream).copied().unwrap_or(0);
        let events = emitter.events_since(current, batch_limit);
        let mut applied_rows = Vec::new();
        let mut skipped = 0usize;
        let mut next_seq = current;

        for event in events {
            if event.commit_seq <= current {
                skipped = skipped.saturating_add(1);
                continue;
            }
            next_seq = next_seq.max(event.commit_seq);
            match event.kind {
                CdcOpKind::Put => applied_rows.push(event.value),
                CdcOpKind::Delete => applied_rows.push(None),
            }
        }

        if !applied_rows.is_empty() {
            store.append_segment(
                table,
                value_column,
                applied_rows
                    .into_iter()
                    .map(|opt| opt.map(|b| b.to_vec()))
                    .collect(),
            );
        }
        self.checkpoints.insert(stream.to_string(), next_seq);

        IngestStats {
            applied_events: next_seq.saturating_sub(current) as usize,
            skipped_events: skipped,
            next_commit_seq: next_seq,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IngestCheckpoint, IngestPipeline};
    use crate::db::analytics::columnar::ColumnarStore;
    use crate::db::cdc::CdcEmitter;
    use bytes::Bytes;

    #[test]
    fn ingestion_is_checkpointed_and_idempotent() {
        let mut emitter = CdcEmitter::default();
        emitter.emit_put(
            Bytes::from_static(b"s1"),
            b"k1".to_vec().into(),
            b"v1".to_vec().into(),
            1,
        );
        emitter.emit_put(
            Bytes::from_static(b"s1"),
            b"k2".to_vec().into(),
            b"v2".to_vec().into(),
            2,
        );

        let mut pipeline = IngestPipeline::default();
        let mut store = ColumnarStore::default();

        let first = pipeline.ingest_stream("orders", &emitter, &mut store, "orders", "value", 100);
        assert_eq!(first.applied_events, 2);
        assert_eq!(pipeline.checkpoint("orders"), Some(2));

        let second = pipeline.ingest_stream("orders", &emitter, &mut store, "orders", "value", 100);
        assert_eq!(second.applied_events, 0);
        assert_eq!(store.scan_column("orders", "value").len(), 2);
    }

    #[test]
    fn checkpoint_restore_is_monotonic() {
        let mut pipeline = IngestPipeline::default();
        pipeline.restore_checkpoint(IngestCheckpoint {
            stream: "orders".to_string(),
            commit_seq: 50,
        });
        pipeline.restore_checkpoint(IngestCheckpoint {
            stream: "orders".to_string(),
            commit_seq: 12,
        });
        assert_eq!(pipeline.checkpoint("orders"), Some(50));
    }
}
