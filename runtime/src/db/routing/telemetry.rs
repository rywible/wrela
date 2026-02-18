use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadSample {
    pub shard: Vec<u8>,
    pub region: String,
    pub latency_ms: u64,
    pub reads: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionTelemetry {
    pub region: String,
    pub ewma_latency_ms: u64,
    pub recent_reads: u64,
    pub sample_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct TelemetryBucket {
    ewma_latency_ms: u64,
    recent_reads: u64,
    sample_count: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ReadTelemetryStore {
    alpha_bps: u32,
    by_shard_region: BTreeMap<(Vec<u8>, String), TelemetryBucket>,
}

impl ReadTelemetryStore {
    pub fn new(alpha_bps: u32) -> Self {
        Self {
            alpha_bps: alpha_bps.clamp(1, 10_000),
            by_shard_region: BTreeMap::new(),
        }
    }

    pub fn record(&mut self, sample: ReadSample) {
        let key = (sample.shard, sample.region);
        let bucket = self.by_shard_region.entry(key).or_default();
        if bucket.sample_count == 0 {
            bucket.ewma_latency_ms = sample.latency_ms;
        } else {
            bucket.ewma_latency_ms = ewma_step(
                bucket.ewma_latency_ms,
                sample.latency_ms,
                self.alpha_bps as u64,
            );
        }
        bucket.recent_reads = bucket.recent_reads.saturating_add(sample.reads.max(1));
        bucket.sample_count = bucket.sample_count.saturating_add(1);
    }

    pub fn snapshot_for_shard(&self, shard: &[u8]) -> Vec<RegionTelemetry> {
        self.by_shard_region
            .iter()
            .filter(|((s, _), _)| s.as_slice() == shard)
            .map(|((_, region), bucket)| RegionTelemetry {
                region: region.clone(),
                ewma_latency_ms: bucket.ewma_latency_ms,
                recent_reads: bucket.recent_reads,
                sample_count: bucket.sample_count,
            })
            .collect()
    }
}

fn ewma_step(prev: u64, observed: u64, alpha_bps: u64) -> u64 {
    let keep_bps = 10_000_u64.saturating_sub(alpha_bps);
    let num = prev
        .saturating_mul(keep_bps)
        .saturating_add(observed.saturating_mul(alpha_bps));
    num / 10_000
}

#[cfg(test)]
mod tests {
    use super::{ReadSample, ReadTelemetryStore};

    #[test]
    fn telemetry_store_updates_ewma_and_keeps_deterministic_ordering() {
        let mut store = ReadTelemetryStore::new(3_000);
        store.record(ReadSample {
            shard: b"orders".to_vec(),
            region: "eu".to_string(),
            latency_ms: 20,
            reads: 10,
        });
        store.record(ReadSample {
            shard: b"orders".to_vec(),
            region: "us".to_string(),
            latency_ms: 8,
            reads: 15,
        });
        store.record(ReadSample {
            shard: b"orders".to_vec(),
            region: "eu".to_string(),
            latency_ms: 10,
            reads: 12,
        });

        let snapshot = store.snapshot_for_shard(b"orders");
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].region, "eu");
        assert_eq!(snapshot[1].region, "us");
        assert!(snapshot[0].ewma_latency_ms <= 20);
    }
}
