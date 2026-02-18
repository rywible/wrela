use std::io;

const MAGIC: [u8; 4] = *b"WAL1";
const HEADER_BYTES: usize = 4 + 1 + 4 + 4 + 4 + 8 + 4;
const MAX_RECORD_BODY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    Put = 1,
    Delete = 2,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub kind: RecordKind,
    pub namespace: Vec<u8>,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub version: u64,
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut acc = 0x811C9DC5u32;
    for b in bytes {
        acc ^= *b as u32;
        acc = acc.wrapping_mul(0x0100_0193);
    }
    acc
}

pub fn encode(record: &Record) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        HEADER_BYTES + record.namespace.len() + record.key.len() + record.value.len(),
    );
    out.extend_from_slice(&MAGIC);
    out.push(record.kind as u8);
    out.extend_from_slice(&(record.namespace.len() as u32).to_be_bytes());
    out.extend_from_slice(&(record.key.len() as u32).to_be_bytes());
    out.extend_from_slice(&(record.value.len() as u32).to_be_bytes());
    out.extend_from_slice(&record.version.to_be_bytes());
    let body_checksum = checksum(
        &[
            record.namespace.as_slice(),
            record.key.as_slice(),
            record.value.as_slice(),
        ]
        .concat(),
    );
    out.extend_from_slice(&body_checksum.to_be_bytes());
    out.extend_from_slice(&record.namespace);
    out.extend_from_slice(&record.key);
    out.extend_from_slice(&record.value);
    out
}

pub fn decode_at(input: &[u8], offset: usize) -> io::Result<Option<(Record, usize)>> {
    let header_end = offset
        .checked_add(HEADER_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "WAL record offset overflow"))?;
    if header_end > input.len() {
        return Ok(None);
    }
    if input[offset..offset + 4] != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid WAL magic",
        ));
    }
    let kind = match input[offset + 4] {
        1 => RecordKind::Put,
        2 => RecordKind::Delete,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid WAL record kind",
            ));
        }
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
    let namespace = input[start..start + ns_len].to_vec();
    let key = input[start + ns_len..start + ns_len + key_len].to_vec();
    let value = input[start + ns_len + key_len..end].to_vec();
    let checksum_actual =
        checksum(&[namespace.as_slice(), key.as_slice(), value.as_slice()].concat());
    if checksum_actual != checksum_wire {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WAL checksum mismatch",
        ));
    }
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
        bytes.extend_from_slice(&MAGIC);
        bytes.push(RecordKind::Put as u8);
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
        bytes.extend_from_slice(&MAGIC);
        bytes.push(RecordKind::Put as u8);
        bytes.extend_from_slice(&(ns_len as u32).to_be_bytes());
        bytes.extend_from_slice(&(key_len as u32).to_be_bytes());
        bytes.extend_from_slice(&(val_len as u32).to_be_bytes());
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let err = decode_at(&bytes, 0).expect_err("must reject oversized body");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("hard limit"));
    }
}
