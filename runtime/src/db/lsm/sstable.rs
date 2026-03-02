use bytes::Bytes;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SsTableStats {
    pub keys: usize,
    pub bytes: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SsTableBlock {
    pub key_start: Vec<u8>,
    pub key_end: Vec<u8>,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsTableEntry {
    pub key: Bytes,
    pub version: u64,
    pub value: Option<Bytes>,
    pub value_blob_ref: Option<SsTableBlobRef>,
    pub expires_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsTableBlobRef {
    pub blob_id: u64,
    pub len_bytes: u32,
}

impl SsTableEntry {
    pub fn live(
        key: impl Into<Bytes>,
        version: u64,
        value: impl Into<Bytes>,
        expires_at_ms: Option<u64>,
    ) -> Self {
        Self {
            key: key.into(),
            version,
            value: Some(value.into()),
            value_blob_ref: None,
            expires_at_ms,
        }
    }

    pub fn live_blob_ref(
        key: impl Into<Bytes>,
        version: u64,
        blob_id: u64,
        len_bytes: u32,
        expires_at_ms: Option<u64>,
    ) -> Self {
        Self {
            key: key.into(),
            version,
            value: None,
            value_blob_ref: Some(SsTableBlobRef { blob_id, len_bytes }),
            expires_at_ms,
        }
    }

    pub fn tombstone(key: impl Into<Bytes>, version: u64) -> Self {
        Self {
            key: key.into(),
            version,
            value: None,
            value_blob_ref: None,
            expires_at_ms: None,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.value.is_none() && self.value_blob_ref.is_none()
    }

    pub fn is_blob_ref(&self) -> bool {
        self.value_blob_ref.is_some()
    }

    pub fn is_ttl_expired(&self, now_ms: u64) -> bool {
        self.expires_at_ms
            .is_some_and(|expires_at| expires_at <= now_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    InvalidEncoding,
}

const FLAG_TOMBSTONE: u8 = 0b0000_0001;
const FLAG_HAS_TTL: u8 = 0b0000_0010;
const FLAG_VALUE_BLOB_REF: u8 = 0b0000_0100;

fn put_u16(dst: &mut Vec<u8>, value: u16) {
    dst.extend_from_slice(&value.to_be_bytes());
}

fn put_u32(dst: &mut Vec<u8>, value: u32) {
    dst.extend_from_slice(&value.to_be_bytes());
}

fn put_u64(dst: &mut Vec<u8>, value: u64) {
    dst.extend_from_slice(&value.to_be_bytes());
}

fn read_exact<'a>(buf: &'a [u8], cursor: &mut usize, n: usize) -> Result<&'a [u8], DecodeError> {
    let end = cursor.saturating_add(n);
    let out = buf.get(*cursor..end).ok_or(DecodeError::Truncated)?;
    *cursor = end;
    Ok(out)
}

fn read_u16(buf: &[u8], cursor: &mut usize) -> Result<u16, DecodeError> {
    let bytes = read_exact(buf, cursor, 2)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32(buf: &[u8], cursor: &mut usize) -> Result<u32, DecodeError> {
    let bytes = read_exact(buf, cursor, 4)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(buf: &[u8], cursor: &mut usize) -> Result<u64, DecodeError> {
    let bytes = read_exact(buf, cursor, 8)?;
    Ok(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

pub fn encode_block(entries: &[SsTableEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, entries.len() as u32);

    for entry in entries {
        let key_len = u16::try_from(entry.key.len()).unwrap_or(u16::MAX);
        put_u16(&mut out, key_len);
        out.extend_from_slice(&entry.key[..usize::from(key_len)]);
        put_u64(&mut out, entry.version);

        let mut flags = 0u8;
        if entry.is_tombstone() {
            flags |= FLAG_TOMBSTONE;
        }
        if entry.expires_at_ms.is_some() {
            flags |= FLAG_HAS_TTL;
        }
        if entry.value_blob_ref.is_some() {
            flags |= FLAG_VALUE_BLOB_REF;
        }
        out.push(flags);

        if let Some(expires_at) = entry.expires_at_ms {
            put_u64(&mut out, expires_at);
        }

        if let Some(blob_ref) = entry.value_blob_ref {
            put_u64(&mut out, blob_ref.blob_id);
            put_u32(&mut out, blob_ref.len_bytes);
        } else if let Some(value) = &entry.value {
            let value_len = u32::try_from(value.len()).unwrap_or(u32::MAX);
            put_u32(&mut out, value_len);
            out.extend_from_slice(&value[..value_len as usize]);
        }
    }

    out
}

pub fn decode_block(encoded: &[u8]) -> Result<Vec<SsTableEntry>, DecodeError> {
    let mut cursor = 0usize;
    let count = read_u32(encoded, &mut cursor)? as usize;
    let mut out = Vec::with_capacity(count);

    for _ in 0..count {
        let key_len = usize::from(read_u16(encoded, &mut cursor)?);
        let key = Bytes::copy_from_slice(read_exact(encoded, &mut cursor, key_len)?);
        let version = read_u64(encoded, &mut cursor)?;
        let flags = *read_exact(encoded, &mut cursor, 1)?
            .first()
            .ok_or(DecodeError::Truncated)?;

        let expires_at_ms = if flags & FLAG_HAS_TTL != 0 {
            Some(read_u64(encoded, &mut cursor)?)
        } else {
            None
        };

        let has_tombstone = flags & FLAG_TOMBSTONE != 0;
        let has_blob_ref = flags & FLAG_VALUE_BLOB_REF != 0;
        if has_tombstone && has_blob_ref {
            return Err(DecodeError::InvalidEncoding);
        }

        let (value, value_blob_ref) = if has_blob_ref {
            let blob_id = read_u64(encoded, &mut cursor)?;
            let len_bytes = read_u32(encoded, &mut cursor)?;
            (None, Some(SsTableBlobRef { blob_id, len_bytes }))
        } else if has_tombstone {
            (None, None)
        } else {
            let value_len = read_u32(encoded, &mut cursor)? as usize;
            (
                Some(Bytes::copy_from_slice(read_exact(
                    encoded,
                    &mut cursor,
                    value_len,
                )?)),
                None,
            )
        };

        if flags & !(FLAG_TOMBSTONE | FLAG_HAS_TTL | FLAG_VALUE_BLOB_REF) != 0 {
            return Err(DecodeError::InvalidEncoding);
        }

        out.push(SsTableEntry {
            key,
            version,
            value,
            value_blob_ref,
            expires_at_ms,
        });
    }

    if cursor != encoded.len() {
        return Err(DecodeError::InvalidEncoding);
    }

    Ok(out)
}

impl SsTableBlock {
    pub fn from_entries(entries: &[SsTableEntry]) -> Self {
        let mut sorted = entries.to_vec();
        sorted.sort_by(|a, b| a.key.cmp(&b.key));

        let key_start = sorted.first().map(|e| e.key.to_vec()).unwrap_or_default();
        let key_end = sorted.last().map(|e| e.key.to_vec()).unwrap_or_default();
        let payload_bytes = encode_block(&sorted).len();

        Self {
            key_start,
            key_end,
            payload_bytes,
        }
    }
}

impl SsTableStats {
    pub fn from_blocks(blocks: &[SsTableBlock]) -> Self {
        Self {
            keys: blocks.len(),
            bytes: blocks.iter().map(|b| b.payload_bytes).sum(),
        }
    }
}

pub fn estimated_entry_bytes(entry: &SsTableEntry) -> usize {
    let key_bytes = entry.key.len();
    let value_bytes = entry.value.as_ref().map_or(0, Bytes::len);
    let ttl_bytes = usize::from(entry.expires_at_ms.is_some()) * 8;
    let payload_bytes = if entry.is_tombstone() {
        0
    } else if entry.is_blob_ref() {
        8 + 4
    } else {
        4 + value_bytes
    };
    2 + key_bytes + 8 + 1 + ttl_bytes + payload_bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_roundtrip_preserves_entries() {
        let entries = vec![
            SsTableEntry::live(b"a".to_vec(), 1, b"v1".to_vec(), None),
            SsTableEntry::tombstone(b"b".to_vec(), 3),
            SsTableEntry::live(b"c".to_vec(), 4, b"v4".to_vec(), Some(99)),
        ];

        let encoded = encode_block(&entries);
        let decoded = decode_block(&encoded).expect("decode");
        assert_eq!(decoded, entries);
    }

    #[test]
    fn block_roundtrip_preserves_blob_refs() {
        let entries = vec![
            SsTableEntry::live_blob_ref(b"a".to_vec(), 2, 44, 8192, None),
            SsTableEntry::live_blob_ref(b"b".to_vec(), 3, 45, 9000, Some(77)),
        ];
        let encoded = encode_block(&entries);
        let decoded = decode_block(&encoded).expect("decode");
        assert_eq!(decoded, entries);
    }

    #[test]
    fn decode_rejects_tombstone_blob_ref_flag_combo() {
        let entry = SsTableEntry::tombstone(b"a".to_vec(), 7);
        let mut encoded = encode_block(&[entry]);
        let flags_idx = 4 + 2 + 1 + 8;
        encoded[flags_idx] |= FLAG_VALUE_BLOB_REF;
        let decoded = decode_block(&encoded).expect_err("must reject invalid flag combo");
        assert_eq!(decoded, DecodeError::InvalidEncoding);
    }

    #[test]
    fn estimated_entry_bytes_accounts_for_blob_ref_inline_and_tombstones() {
        let tomb = SsTableEntry::tombstone(b"k".to_vec(), 1);
        let inline = SsTableEntry::live(b"k".to_vec(), 2, vec![1; 32], None);
        let blob = SsTableEntry::live_blob_ref(b"k".to_vec(), 3, 99, 128 * 1024, None);

        let tomb_bytes = estimated_entry_bytes(&tomb);
        let inline_bytes = estimated_entry_bytes(&inline);
        let blob_bytes = estimated_entry_bytes(&blob);

        assert!(inline_bytes > tomb_bytes);
        assert!(blob_bytes > tomb_bytes);
        assert!(blob_bytes < inline_bytes);
    }
}
