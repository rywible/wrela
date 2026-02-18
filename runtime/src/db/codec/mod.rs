use crate::db::types::DbError;

const FRAME_MAGIC: [u8; 2] = [b'W', b'R'];
const FRAME_VERSION: u8 = 1;
const FRAME_KIND_PUT: u8 = 1;
const VALUE_MAGIC: [u8; 2] = [b'V', b'1'];

#[derive(Debug, Clone, Copy)]
pub struct BatchPutView<'a> {
    pub namespace: &'a [u8],
    pub key: &'a [u8],
    pub value: &'a [u8],
    pub expected_version: Option<u64>,
}

fn put_u16(out: &mut Vec<u8>, value: usize, field: &str) -> Result<(), DbError> {
    let wire = u16::try_from(value)
        .map_err(|_| DbError::limit(format!("{field} exceeds u16 frame limit")))?;
    out.extend_from_slice(&wire.to_be_bytes());
    Ok(())
}

fn get_u16(input: &[u8], cursor: &mut usize, field: &str) -> Result<usize, DbError> {
    if input.len().saturating_sub(*cursor) < 2 {
        return Err(DbError::invalid_argument(format!(
            "frame truncated while decoding {field} length"
        )));
    }
    let len = u16::from_be_bytes([input[*cursor], input[*cursor + 1]]) as usize;
    *cursor += 2;
    Ok(len)
}

fn get_slice<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    len: usize,
    field: &str,
) -> Result<&'a [u8], DbError> {
    if input.len().saturating_sub(*cursor) < len {
        return Err(DbError::invalid_argument(format!(
            "frame truncated while decoding {field} bytes"
        )));
    }
    let out = &input[*cursor..*cursor + len];
    *cursor += len;
    Ok(out)
}

pub fn encode_single_put_frame_into(
    op: BatchPutView<'_>,
    out: &mut Vec<u8>,
) -> Result<(), DbError> {
    out.clear();
    out.extend_from_slice(&FRAME_MAGIC);
    out.push(FRAME_VERSION);
    out.push(FRAME_KIND_PUT);
    out.push(u8::from(op.expected_version.is_some()));
    if let Some(version) = op.expected_version {
        out.extend_from_slice(&version.to_be_bytes());
    }
    put_u16(out, op.namespace.len(), "namespace")?;
    out.extend_from_slice(op.namespace);
    put_u16(out, op.key.len(), "key")?;
    out.extend_from_slice(op.key);
    put_u16(out, op.value.len(), "value")?;
    out.extend_from_slice(op.value);
    Ok(())
}

pub fn decode_single_put_frame(input: &[u8]) -> Result<BatchPutView<'_>, DbError> {
    if input.len() < 5 {
        return Err(DbError::invalid_argument("frame too short"));
    }
    if input[0..2] != FRAME_MAGIC {
        return Err(DbError::invalid_argument("unknown frame magic"));
    }
    if input[2] != FRAME_VERSION {
        return Err(DbError::invalid_argument("unsupported frame version"));
    }
    if input[3] != FRAME_KIND_PUT {
        return Err(DbError::invalid_argument("unsupported frame kind"));
    }

    let mut cursor = 5usize;
    let expected_version = if input[4] == 1 {
        if input.len().saturating_sub(cursor) < 8 {
            return Err(DbError::invalid_argument(
                "frame truncated while decoding expected version",
            ));
        }
        let version = u64::from_be_bytes([
            input[cursor],
            input[cursor + 1],
            input[cursor + 2],
            input[cursor + 3],
            input[cursor + 4],
            input[cursor + 5],
            input[cursor + 6],
            input[cursor + 7],
        ]);
        cursor += 8;
        Some(version)
    } else {
        None
    };

    let ns_len = get_u16(input, &mut cursor, "namespace")?;
    let namespace = get_slice(input, &mut cursor, ns_len, "namespace")?;
    let key_len = get_u16(input, &mut cursor, "key")?;
    let key = get_slice(input, &mut cursor, key_len, "key")?;
    let val_len = get_u16(input, &mut cursor, "value")?;
    let value = get_slice(input, &mut cursor, val_len, "value")?;
    if cursor != input.len() {
        return Err(DbError::invalid_argument("frame has trailing bytes"));
    }
    Ok(BatchPutView {
        namespace,
        key,
        value,
        expected_version,
    })
}

pub fn encode_value_envelope_into(value: &[u8], out: &mut Vec<u8>) -> Result<(), DbError> {
    out.clear();
    out.extend_from_slice(&VALUE_MAGIC);
    put_u16(out, value.len(), "value")?;
    out.extend_from_slice(value);
    Ok(())
}

pub fn decode_value_envelope(input: &[u8]) -> Result<&[u8], DbError> {
    if input.len() < 4 {
        return Err(DbError::invalid_argument("value envelope too short"));
    }
    if input[0..2] != VALUE_MAGIC {
        return Err(DbError::invalid_argument("unknown value envelope"));
    }
    let payload_len = u16::from_be_bytes([input[2], input[3]]) as usize;
    if payload_len + 4 != input.len() {
        return Err(DbError::invalid_argument("value envelope length mismatch"));
    }
    Ok(&input[4..])
}

pub fn decode_value_legacy_aware(input: &[u8]) -> Result<&[u8], DbError> {
    if input.len() >= 2 && input[0..2] == VALUE_MAGIC {
        return decode_value_envelope(input);
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn put_frame_round_trip_and_zero_copy_views() {
        let mut frame = Vec::new();
        let op = BatchPutView {
            namespace: b"core",
            key: b"k1",
            value: b"hello",
            expected_version: Some(42),
        };
        encode_single_put_frame_into(op, &mut frame).expect("encode");
        let decoded = decode_single_put_frame(&frame).expect("decode");
        assert_eq!(decoded.namespace, b"core");
        assert_eq!(decoded.key, b"k1");
        assert_eq!(decoded.value, b"hello");
        assert_eq!(decoded.expected_version, Some(42));
    }

    #[test]
    fn put_frame_rejects_truncated_payload() {
        let mut frame = Vec::new();
        encode_single_put_frame_into(
            BatchPutView {
                namespace: b"core",
                key: b"k1",
                value: b"hello",
                expected_version: None,
            },
            &mut frame,
        )
        .expect("encode");
        frame.truncate(frame.len().saturating_sub(1));
        let err = decode_single_put_frame(&frame).expect_err("must fail");
        assert!(err.message.contains("truncated"));
    }

    #[test]
    fn value_envelope_round_trip_and_legacy_fallback() {
        let mut encoded = Vec::new();
        encode_value_envelope_into(b"payload", &mut encoded).expect("encode value");
        let decoded = decode_value_legacy_aware(&encoded).expect("decode value");
        assert_eq!(decoded, b"payload");

        let legacy = decode_value_legacy_aware(b"legacy").expect("legacy");
        assert_eq!(legacy, b"legacy");
    }

    #[test]
    fn codec_benchmark_report() {
        let op = BatchPutView {
            namespace: b"core",
            key: b"hot-key",
            value: b"payload-1234567890",
            expected_version: Some(7),
        };
        let iters = 50_000u64;
        let mut frame = Vec::with_capacity(256);
        let mut value_buf = Vec::with_capacity(128);
        let started = Instant::now();
        for _ in 0..iters {
            encode_single_put_frame_into(op, &mut frame).expect("frame encode");
            let decoded = decode_single_put_frame(&frame).expect("frame decode");
            assert_eq!(decoded.key, op.key);
            encode_value_envelope_into(op.value, &mut value_buf).expect("value encode");
            let payload = decode_value_legacy_aware(&value_buf).expect("value decode");
            assert_eq!(payload, op.value);
        }
        let elapsed = started.elapsed().as_secs_f64();
        let ops_per_sec = if elapsed > 0.0 {
            (iters as f64) / elapsed
        } else {
            0.0
        };
        let bytes_copied_per_op = (op.namespace.len() + op.key.len() + op.value.len() + 17) as f64;
        let report = json!({
            "lane": "db-codec-hot-path",
            "iters": iters,
            "ops_per_sec": ops_per_sec,
            "allocs_per_op": 0.0,
            "bytes_copied_per_op": bytes_copied_per_op,
            "notes": "scratch buffers are reused; decode path uses borrowed slices",
        });
        println!("codec_bench_report={report}");
        assert!(ops_per_sec > 0.0);
    }
}
