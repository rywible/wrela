use bytes::Bytes;
use wrela_runtime::db::wal::format::{Record, RecordKind, encode};
use wrela_runtime::db::wal::segment::{ReplayMode, WalSegment};

/// Verifies that a WAL segment can survive a torn tail (partial write at
/// end-of-file) and still replay all fully-written records.
#[test]
fn wal_torn_tail_does_not_lose_committed_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wal_path = dir.path().join("wal.log");
    let wal = WalSegment::open(&wal_path).expect("open");

    for i in 0..5u64 {
        wal.append(&Record {
            kind: RecordKind::Put,
            namespace: Bytes::from_static(b"ns"),
            key: Bytes::from(format!("k{i}")),
            value: Bytes::from(format!("v{i}")),
            version: i,
        })
        .expect("append");
    }

    // Simulate torn tail: append a partial record directly to the file.
    let partial = encode(&Record {
        kind: RecordKind::Put,
        namespace: Bytes::from_static(b"ns"),
        key: Bytes::from_static(b"torn"),
        value: Bytes::from_static(b"value"),
        version: 99,
    });
    std::fs::OpenOptions::new()
        .append(true)
        .open(&wal_path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(&partial[..partial.len() / 3])
        })
        .expect("write torn tail");

    let replayed = wal.replay().expect("replay");
    assert_eq!(replayed.len(), 5, "all committed records must survive");
}

/// Verifies that SkipCorruption mode can recover records after mid-file
/// garbage without losing committed data.
#[test]
fn wal_skip_corruption_recovers_after_garbage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wal_path = dir.path().join("wal.log");
    let wal = WalSegment::open(&wal_path).expect("open");

    wal.append(&Record {
        kind: RecordKind::Put,
        namespace: Bytes::from_static(b"ns"),
        key: Bytes::from_static(b"before"),
        value: Bytes::from_static(b"ok"),
        version: 1,
    })
    .expect("append before");

    // Inject garbage.
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .expect("open for garbage");
        f.write_all(b"GARBAGE_BYTES_THAT_ARE_NOT_A_RECORD")
            .expect("write garbage");
        f.sync_data().expect("sync");
    }

    // Append a valid record after the garbage.
    let after_bytes = encode(&Record {
        kind: RecordKind::Put,
        namespace: Bytes::from_static(b"ns"),
        key: Bytes::from_static(b"after"),
        value: Bytes::from_static(b"ok"),
        version: 2,
    });
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .expect("open for after");
        f.write_all(&after_bytes).expect("write after");
        f.sync_data().expect("sync");
    }

    let result = wal
        .replay_with_mode(ReplayMode::SkipCorruption)
        .expect("replay");
    assert_eq!(result.records.len(), 2);
    assert_eq!(&result.records[0].key[..], b"before");
    assert_eq!(&result.records[1].key[..], b"after");
    assert!(
        !result.skipped.is_empty(),
        "must report skipped corruption region"
    );
}
