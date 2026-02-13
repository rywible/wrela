use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct VersionedValue {
    pub version: u64,
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct Memtable {
    rows: BTreeMap<Vec<u8>, Vec<VersionedValue>>,
}

impl Memtable {
    pub fn apply(&mut self, key: Vec<u8>, version: u64, value: Option<Vec<u8>>) {
        let entry = self.rows.entry(key).or_default();
        entry.push(VersionedValue { version, value });
        entry.sort_by_key(|v| v.version);
    }

    pub fn latest_version(&self, key: &[u8]) -> Option<u64> {
        self.rows
            .get(key)
            .and_then(|v| v.iter().max_by_key(|x| x.version).map(|x| x.version))
    }

    pub fn visible(&self, key: &[u8], read_version: u64) -> Option<&[u8]> {
        self.rows.get(key).and_then(|versions| {
            versions
                .iter()
                .filter(|v| v.version <= read_version)
                .max_by_key(|v| v.version)
                .and_then(|v| v.value.as_deref())
        })
    }

    pub fn range_visible(
        &self,
        start: &[u8],
        end: &[u8],
        read_version: u64,
        limit: usize,
    ) -> Vec<(Vec<u8>, Vec<u8>, u64)> {
        self.rows
            .range(start.to_vec()..end.to_vec())
            .filter_map(|(k, versions)| {
                versions
                    .iter()
                    .filter(|v| v.version <= read_version)
                    .max_by_key(|v| v.version)
                    .and_then(|v| {
                        v.value
                            .as_ref()
                            .map(|val| (k.clone(), val.clone(), v.version))
                    })
            })
            .take(limit)
            .collect()
    }
}
