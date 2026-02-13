use crate::db::types::BatchOp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaftCommand {
    Put {
        namespace: Vec<u8>,
        key: Vec<u8>,
        value: Vec<u8>,
        expected_version: Option<u64>,
    },
    Delete {
        namespace: Vec<u8>,
        key: Vec<u8>,
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
                namespace: b"core".to_vec(),
                key: b"a".to_vec(),
                value: b"1".to_vec(),
                expected_version: None,
            },
            BatchOp::Delete {
                namespace: b"core".to_vec(),
                key: b"b".to_vec(),
                expected_version: Some(7),
            },
            BatchOp::Put {
                namespace: b"tenant2".to_vec(),
                key: b"c".to_vec(),
                value: b"3".to_vec(),
                expected_version: Some(2),
            },
        ];

        let frame = build_append_frame(&batch);
        assert_eq!(frame.command_count, 3);
        assert_eq!(
            frame.commands[0],
            RaftCommand::Put {
                namespace: b"core".to_vec(),
                key: b"a".to_vec(),
                value: b"1".to_vec(),
                expected_version: None
            }
        );
        assert_eq!(
            frame.commands[1],
            RaftCommand::Delete {
                namespace: b"core".to_vec(),
                key: b"b".to_vec(),
                expected_version: Some(7)
            }
        );
        assert_eq!(
            frame.commands[2],
            RaftCommand::Put {
                namespace: b"tenant2".to_vec(),
                key: b"c".to_vec(),
                value: b"3".to_vec(),
                expected_version: Some(2)
            }
        );
    }

    #[test]
    fn append_frame_is_deterministic_for_same_batch() {
        let batch = vec![
            BatchOp::Put {
                namespace: b"core".to_vec(),
                key: b"k".to_vec(),
                value: b"v".to_vec(),
                expected_version: Some(1),
            },
            BatchOp::Delete {
                namespace: b"core".to_vec(),
                key: b"x".to_vec(),
                expected_version: None,
            },
        ];
        let a = build_append_frame(&batch);
        let b = build_append_frame(&batch);
        assert_eq!(a, b);
    }
}
