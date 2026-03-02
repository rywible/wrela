use bytes::Bytes;

use crate::db::types::BatchOp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaftCommand {
    Put {
        namespace: Bytes,
        key: Bytes,
        value: Bytes,
        expected_version: Option<u64>,
    },
    Delete {
        namespace: Bytes,
        key: Bytes,
        expected_version: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaftAppendFrame {
    pub command_count: usize,
    pub commands: Vec<RaftCommand>,
}

pub fn build_append_frame(batch: &[BatchOp]) -> RaftAppendFrame {
    let commands = batch
        .iter()
        .map(|op| match op {
            BatchOp::Put {
                namespace,
                key,
                value,
                expected_version,
            } => RaftCommand::Put {
                namespace: namespace.clone(),
                key: key.clone(),
                value: value.clone(),
                expected_version: *expected_version,
            },
            BatchOp::Delete {
                namespace,
                key,
                expected_version,
            } => RaftCommand::Delete {
                namespace: namespace.clone(),
                key: key.clone(),
                expected_version: *expected_version,
            },
        })
        .collect::<Vec<_>>();

    RaftAppendFrame {
        command_count: commands.len(),
        commands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_frame_preserves_batch_order() {
        let batch = vec![
            BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"a"),
                value: Bytes::from_static(b"1"),
                expected_version: None,
            },
            BatchOp::Delete {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"b"),
                expected_version: Some(7),
            },
            BatchOp::Put {
                namespace: Bytes::from_static(b"tenant2"),
                key: Bytes::from_static(b"c"),
                value: Bytes::from_static(b"3"),
                expected_version: Some(2),
            },
        ];

        let frame = build_append_frame(&batch);
        assert_eq!(frame.command_count, 3);
        assert_eq!(
            frame.commands[0],
            RaftCommand::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"a"),
                value: Bytes::from_static(b"1"),
                expected_version: None
            }
        );
        assert_eq!(
            frame.commands[1],
            RaftCommand::Delete {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"b"),
                expected_version: Some(7)
            }
        );
        assert_eq!(
            frame.commands[2],
            RaftCommand::Put {
                namespace: Bytes::from_static(b"tenant2"),
                key: Bytes::from_static(b"c"),
                value: Bytes::from_static(b"3"),
                expected_version: Some(2)
            }
        );
    }

    #[test]
    fn append_frame_is_deterministic_for_same_batch() {
        let batch = vec![
            BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k"),
                value: Bytes::from_static(b"v"),
                expected_version: Some(1),
            },
            BatchOp::Delete {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"x"),
                expected_version: None,
            },
        ];
        let a = build_append_frame(&batch);
        let b = build_append_frame(&batch);
        assert_eq!(a, b);
    }
}
