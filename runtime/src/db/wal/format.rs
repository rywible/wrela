use bytes::Bytes;
use crc32fast::Hasher as Crc32Hasher;
use std::io;

pub(crate) const MAGIC_V1: [u8; 4] = *b"WAL1";
pub(crate) const MAGIC_V2: [u8; 4] = *b"WAL2";
pub(crate) const MAGIC_V3: [u8; 4] = *b"WAL3";
pub(crate) const MAGIC: [u8; 4] = MAGIC_V3;
pub(crate) const HEADER_BYTES: usize = 4 + 1 + 4 + 4 + 4 + 8 + 4;
const MAX_RECORD_BODY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    Put,
    Delete,
    /// Raft metadata (term, voted_for, commit_index) stored in WAL for atomic durability.
    /// Value format: current_term (u64 BE) | voted_for (u64 BE, u64::MAX = None) | commit_index (u64 BE) | flags (u8).
    RaftMeta,
    Unknown(u8),
}

impl RecordKind {
    pub fn as_u8(self) -> u8 {
        match self {
            RecordKind::Put => 1,
            RecordKind::Delete => 2,
            RecordKind::RaftMeta => 3,
            RecordKind::Unknown(v) => v,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    pub kind: RecordKind,
    pub namespace: Bytes,
    pub key: Bytes,
    pub value: Bytes,
    pub version: u64,
}

#[cfg(test)]
fn checksum(bytes: &[u8]) -> u32 {
    let mut acc = 0x811C9DC5u32;
    for b in bytes {
        acc ^= *b as u32;
        acc = acc.wrapping_mul(0x0100_0193);
    }
    acc
}

fn checksum_slices(slices: &[&[u8]]) -> u32 {
    let mut acc = 0x811C9DC5u32;
    for slice in slices {
        for b in *slice {
            acc ^= *b as u32;
            acc = acc.wrapping_mul(0x0100_0193);
        }
    }
    acc
}

fn checksum_header_and_body_fnv(
    magic: &[u8; 4],
    kind_u8: u8,
    ns_len: u32,
    key_len: u32,
    val_len: u32,
    version: u64,
    body_slices: &[&[u8]],
) -> u32 {
    let mut acc = 0x811C9DC5u32;
    for b in *magic {
        acc ^= b as u32;
        acc = acc.wrapping_mul(0x0100_0193);
    }
    acc ^= kind_u8 as u32;
    acc = acc.wrapping_mul(0x0100_0193);
    for b in ns_len.to_be_bytes() {
        acc ^= b as u32;
        acc = acc.wrapping_mul(0x0100_0193);
    }
    for b in key_len.to_be_bytes() {
        acc ^= b as u32;
        acc = acc.wrapping_mul(0x0100_0193);
    }
    for b in val_len.to_be_bytes() {
        acc ^= b as u32;
        acc = acc.wrapping_mul(0x0100_0193);
    }
    for b in version.to_be_bytes() {
        acc ^= b as u32;
        acc = acc.wrapping_mul(0x0100_0193);
    }
    for slice in body_slices {
        for b in *slice {
            acc ^= *b as u32;
            acc = acc.wrapping_mul(0x0100_0193);
        }
    }
    acc
}

fn checksum_header_and_body_crc32(
    magic: &[u8; 4],
    kind_u8: u8,
    ns_len: u32,
    key_len: u32,
    val_len: u32,
    version: u64,
    body_slices: &[&[u8]],
) -> u32 {
    let mut hasher = Crc32Hasher::new();
    hasher.update(magic);
    hasher.update(&[kind_u8]);
    hasher.update(&ns_len.to_be_bytes());
    hasher.update(&key_len.to_be_bytes());
    hasher.update(&val_len.to_be_bytes());
    hasher.update(&version.to_be_bytes());
    for slice in body_slices {
        hasher.update(slice);
    }
    hasher.finalize()
}

pub(crate) fn has_wal_magic_at(bytes: &[u8], offset: usize) -> bool {
    let end = match offset.checked_add(4) {
        Some(v) => v,
        None => return false,
    };
    if end > bytes.len() {
        return false;
    }
    let candidate = &bytes[offset..end];
    candidate == MAGIC_V1 || candidate == MAGIC_V2 || candidate == MAGIC_V3
}

pub fn encode(record: &Record) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_BYTES + body_len(record));
    encode_to(record, &mut out);
    out
}

fn body_len(record: &Record) -> usize {
    record.namespace.len() + record.key.len() + record.value.len()
}

pub fn encode_to(record: &Record, out: &mut Vec<u8>) {
    let kind_u8 = record.kind.as_u8();
    let ns_len = record.namespace.len() as u32;
    let key_len = record.key.len() as u32;
    let val_len = record.value.len() as u32;
    let body_checksum = checksum_header_and_body_crc32(
        &MAGIC,
        kind_u8,
        ns_len,
        key_len,
        val_len,
        record.version,
        &[
            record.namespace.as_ref(),
            record.key.as_ref(),
            record.value.as_ref(),
        ],
    );
    let mut header = [0u8; HEADER_BYTES];
    header[0..4].copy_from_slice(&MAGIC);
    header[4] = kind_u8;
    header[5..9].copy_from_slice(&ns_len.to_be_bytes());
    header[9..13].copy_from_slice(&key_len.to_be_bytes());
    header[13..17].copy_from_slice(&val_len.to_be_bytes());
    header[17..25].copy_from_slice(&record.version.to_be_bytes());
    header[25..29].copy_from_slice(&body_checksum.to_be_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&record.namespace);
    out.extend_from_slice(&record.key);
    out.extend_from_slice(&record.value);
}

/// Sentinel namespace for Raft metadata WAL records (skipped during memtable replay).
pub const RAFT_META_NAMESPACE: &[u8] = b"_raft";

/// Raft metadata value layout: current_term (8) | voted_for (8, u64::MAX = None) | commit_index (8) | flags (1).
pub const RAFT_META_VALUE_LEN: usize = 25;

/// Sentinel for "no vote" in persisted Raft metadata so node_id 0 is distinguishable.
const VOTED_FOR_NONE: u64 = u64::MAX;

/// Build a WAL record for Raft metadata. Use namespace `_raft`, empty key.
pub fn record_from_raft_meta(
    current_term: u64,
    voted_for: Option<u64>,
    commit_index: u64,
    needs_membership_flush: bool,
) -> Record {
    let mut value = [0u8; RAFT_META_VALUE_LEN];
    value[0..8].copy_from_slice(&current_term.to_be_bytes());
    value[8..16].copy_from_slice(&voted_for.unwrap_or(VOTED_FOR_NONE).to_be_bytes());
    value[16..24].copy_from_slice(&commit_index.to_be_bytes());
    value[24] = needs_membership_flush as u8;
    Record {
        kind: RecordKind::RaftMeta,
        namespace: Bytes::from_static(RAFT_META_NAMESPACE),
        key: Bytes::new(),
        value: Bytes::copy_from_slice(&value),
        version: 0,
    }
}

/// Decode Raft metadata from a RaftMeta record's value. Returns None if invalid length.
pub fn decode_raft_meta_value(value: &[u8]) -> Option<(u64, Option<u64>, u64, bool)> {
    if value.len() < RAFT_META_VALUE_LEN {
        return None;
    }
    let current_term = u64::from_be_bytes(value[0..8].try_into().ok()?);
    let voted_raw = u64::from_be_bytes(value[8..16].try_into().ok()?);
    let voted_for = if voted_raw == VOTED_FOR_NONE || voted_raw == 0 {
        None
    } else {
        Some(voted_raw)
    };
    let commit_index = u64::from_be_bytes(value[16..24].try_into().ok()?);
    let needs_membership_flush = value[24] != 0;
    Some((
        current_term,
        voted_for,
        commit_index,
        needs_membership_flush,
    ))
}

pub fn decode_at(input: &[u8], offset: usize) -> io::Result<Option<(Record, usize)>> {
    let header_end = offset
        .checked_add(HEADER_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "WAL record offset overflow"))?;
    if header_end > input.len() {
        return Ok(None);
    }
    let magic_slice = &input[offset..offset + 4];
    let magic = if magic_slice == MAGIC_V1 {
        MAGIC_V1
    } else if magic_slice == MAGIC_V2 {
        MAGIC_V2
    } else if magic_slice == MAGIC_V3 {
        MAGIC_V3
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid WAL magic",
        ));
    };
    let is_v1 = magic == MAGIC_V1;
    let is_v3 = magic == MAGIC_V3;
    let kind = match input[offset + 4] {
        1 => RecordKind::Put,
        2 => RecordKind::Delete,
        3 => RecordKind::RaftMeta,
        other => RecordKind::Unknown(other),
    };
    let ns_len = u32::from_be_bytes([
        input[offset + 5],
        input[offset + 6],
        input[offset + 7],
        input[offset + 8],
    ]) as usize;
    let key_len = u32::from_be_bytes([
        input[offset + 9],
        input[offset + 10],
        input[offset + 11],
        input[offset + 12],
    ]) as usize;
    let val_len = u32::from_be_bytes([
        input[offset + 13],
        input[offset + 14],
        input[offset + 15],
        input[offset + 16],
    ]) as usize;
    let version = u64::from_be_bytes([
        input[offset + 17],
        input[offset + 18],
        input[offset + 19],
        input[offset + 20],
        input[offset + 21],
        input[offset + 22],
        input[offset + 23],
        input[offset + 24],
    ]);
    let checksum_wire = u32::from_be_bytes([
        input[offset + 25],
        input[offset + 26],
        input[offset + 27],
        input[offset + 28],
    ]);
    let body_len = ns_len
        .checked_add(key_len)
        .and_then(|len| len.checked_add(val_len))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "WAL record length overflow"))?;
    if body_len > MAX_RECORD_BODY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WAL record body exceeds hard limit",
        ));
    }

    let start = header_end;
    let end = start
        .checked_add(body_len)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "WAL record offset overflow"))?;
    if end > input.len() {
        return Ok(None);
    }
    let namespace_slice = &input[start..start + ns_len];
    let key_slice = &input[start + ns_len..start + ns_len + key_len];
    let value_slice = &input[start + ns_len + key_len..end];
    let checksum_actual = if is_v1 {
        checksum_slices(&[namespace_slice, key_slice, value_slice])
    } else if is_v3 {
        checksum_header_and_body_crc32(
            &magic,
            input[offset + 4],
            ns_len as u32,
            key_len as u32,
            val_len as u32,
            version,
            &[namespace_slice, key_slice, value_slice],
        )
    } else {
        checksum_header_and_body_fnv(
            &magic,
            input[offset + 4],
            ns_len as u32,
            key_len as u32,
            val_len as u32,
            version,
            &[namespace_slice, key_slice, value_slice],
        )
    };
    if checksum_actual != checksum_wire {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WAL checksum mismatch",
        ));
    }
    let namespace = Bytes::copy_from_slice(namespace_slice);
    let key = Bytes::copy_from_slice(key_slice);
    let value = Bytes::copy_from_slice(value_slice);
    Ok(Some((
        Record {
            kind,
            namespace,
            key,
            value,
            version,
        },
        end,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_rejects_huge_record_lengths() {
        let mut bytes = Vec::with_capacity(HEADER_BYTES);
        bytes.extend_from_slice(&MAGIC_V2);
        bytes.push(RecordKind::Put.as_u8());
        bytes.extend_from_slice(&(u32::MAX).to_be_bytes());
        bytes.extend_from_slice(&(u32::MAX).to_be_bytes());
        bytes.extend_from_slice(&(u32::MAX).to_be_bytes());
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let err = decode_at(&bytes, 0).expect_err("must reject huge lengths");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("overflow") || err.to_string().contains("hard limit"));
    }

    #[test]
    fn decode_rejects_body_larger_than_hard_limit() {
        let ns_len = 1024usize;
        let key_len = 1024usize;
        let val_len = MAX_RECORD_BODY_BYTES + 1 - ns_len - key_len;

        let mut bytes = Vec::with_capacity(HEADER_BYTES);
        bytes.extend_from_slice(&MAGIC_V2);
        bytes.push(RecordKind::Put.as_u8());
        bytes.extend_from_slice(&(ns_len as u32).to_be_bytes());
        bytes.extend_from_slice(&(key_len as u32).to_be_bytes());
        bytes.extend_from_slice(&(val_len as u32).to_be_bytes());
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let err = decode_at(&bytes, 0).expect_err("must reject oversized body");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("hard limit"));
    }

    #[test]
    fn decode_unknown_record_kind_succeeds() {
        let ns = Bytes::from_static(b"core");
        let key = Bytes::from_static(b"k1");
        let val = Bytes::from_static(b"v1");
        let bytes = encode(&Record {
            kind: RecordKind::Unknown(42),
            namespace: ns.clone(),
            key: key.clone(),
            value: val.clone(),
            version: 1,
        });

        let (record, next) = decode_at(&bytes, 0)
            .expect("decode must succeed")
            .expect("must return a record");
        assert_eq!(record.kind, RecordKind::Unknown(42));
        assert_eq!(record.namespace, ns);
        assert_eq!(record.key, key);
        assert_eq!(record.value, val);
        assert_eq!(next, bytes.len());
    }

    #[test]
    fn replay_skips_unknown_record_kinds() {
        // Encode a Put, then an Unknown, then a Delete. The caller should be
        // able to filter out Unknown records while processing the rest.
        let put = encode(&Record {
            kind: RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k1"),
            value: Bytes::from_static(b"v1"),
            version: 1,
        });
        let del = encode(&Record {
            kind: RecordKind::Delete,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k2"),
            value: Bytes::new(),
            version: 2,
        });

        // Build an unknown-kind record (kind=99) with valid checksum.
        let unknown = encode(&Record {
            kind: RecordKind::Unknown(99),
            namespace: Bytes::from_static(b"test"),
            key: Bytes::from_static(b"uk"),
            value: Bytes::from_static(b"uv"),
            version: 5,
        });

        let mut stream = Vec::new();
        stream.extend_from_slice(&put);
        stream.extend_from_slice(&unknown);
        stream.extend_from_slice(&del);

        // Decode all three records sequentially.
        let mut offset = 0;
        let mut records = Vec::new();
        while let Ok(Some((record, next))) = decode_at(&stream, offset) {
            records.push(record);
            offset = next;
        }
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].kind, RecordKind::Put);
        assert_eq!(records[1].kind, RecordKind::Unknown(99));
        assert_eq!(records[2].kind, RecordKind::Delete);
    }

    #[test]
    fn decode_supports_legacy_v1_records() {
        let ns = b"legacy";
        let key = b"k";
        let val = b"v";
        let body = [ns.as_slice(), key.as_slice(), val.as_slice()].concat();
        let cksum = checksum(&body);
        let mut bytes = Vec::with_capacity(HEADER_BYTES + body.len());
        bytes.extend_from_slice(&MAGIC_V1);
        bytes.push(RecordKind::Put.as_u8());
        bytes.extend_from_slice(&(ns.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&(key.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&(val.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&7u64.to_be_bytes());
        bytes.extend_from_slice(&cksum.to_be_bytes());
        bytes.extend_from_slice(&body);

        let (record, _) = decode_at(&bytes, 0)
            .expect("decode must succeed")
            .expect("record must exist");
        assert_eq!(record.kind, RecordKind::Put);
        assert_eq!(record.version, 7);
        assert_eq!(record.namespace, Bytes::from_static(b"legacy"));
    }

    #[test]
    fn encode_decode_roundtrip_many_sizes() {
        for ns_len in [0, 1, 4, 128, 1024] {
            for key_len in [0, 1, 64, 512] {
                for val_len in [0, 1, 256, 4096] {
                    let ns = Bytes::from(vec![b'n'; ns_len]);
                    let key = Bytes::from(vec![b'k'; key_len]);
                    let val = Bytes::from(vec![b'v'; val_len]);
                    let record = Record {
                        kind: RecordKind::Put,
                        namespace: ns.clone(),
                        key: key.clone(),
                        value: val.clone(),
                        version: ns_len as u64 + key_len as u64 + val_len as u64,
                    };
                    let encoded = encode(&record);
                    let (decoded, next) = decode_at(&encoded, 0)
                        .unwrap_or_else(|e| {
                            panic!("decode failed for ns={ns_len} key={key_len} val={val_len}: {e}")
                        })
                        .expect("record must exist");
                    assert_eq!(decoded.namespace, ns);
                    assert_eq!(decoded.key, key);
                    assert_eq!(decoded.value, val);
                    assert_eq!(decoded.version, record.version);
                    assert_eq!(next, encoded.len());
                }
            }
        }
    }

    #[test]
    fn single_bit_flip_in_header_is_detected() {
        let record = Record {
            kind: RecordKind::Put,
            namespace: Bytes::from_static(b"ns"),
            key: Bytes::from_static(b"key"),
            value: Bytes::from_static(b"value"),
            version: 42,
        };
        let encoded = encode(&record);
        for bit_pos in 0..(HEADER_BYTES * 8) {
            let byte_idx = bit_pos / 8;
            let bit_idx = bit_pos % 8;
            if byte_idx < 4 {
                continue;
            }
            let mut corrupted = encoded.clone();
            corrupted[byte_idx] ^= 1 << bit_idx;
            match decode_at(&corrupted, 0) {
                Err(_) => {}
                Ok(Some((rec, _))) => {
                    assert_ne!(
                        rec.version, record.version,
                        "corrupted bit {bit_pos} should not silently decode to same record"
                    );
                }
                Ok(None) => {}
            }
        }
    }

    #[test]
    fn multi_record_stream_roundtrip() {
        let mut stream = Vec::new();
        let records: Vec<Record> = (0..50)
            .map(|i| Record {
                kind: if i % 3 == 0 {
                    RecordKind::Delete
                } else {
                    RecordKind::Put
                },
                namespace: Bytes::from(format!("ns{i}")),
                key: Bytes::from(format!("key{i}")),
                value: Bytes::from(vec![i as u8; (i * 7) as usize % 500]),
                version: i,
            })
            .collect();

        for rec in &records {
            stream.extend_from_slice(&encode(rec));
        }

        let mut offset = 0;
        let mut decoded = Vec::new();
        while offset < stream.len() {
            match decode_at(&stream, offset) {
                Ok(Some((rec, next))) => {
                    decoded.push(rec);
                    offset = next;
                }
                Ok(None) => break,
                Err(e) => panic!("decode error at offset {offset}: {e}"),
            }
        }
        assert_eq!(decoded.len(), records.len());
        for (orig, dec) in records.iter().zip(decoded.iter()) {
            assert_eq!(orig.kind, dec.kind);
            assert_eq!(orig.namespace, dec.namespace);
            assert_eq!(orig.key, dec.key);
            assert_eq!(orig.value, dec.value);
            assert_eq!(orig.version, dec.version);
        }
    }
}
