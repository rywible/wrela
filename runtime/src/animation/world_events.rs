use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldEventHeader {
    pub sequence: u64,
    pub tick: u64,
    pub actor_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnimationWorldEventKind {
    FractureStart {
        entity_id: u64,
        integrity_before_bp: u16,
    },
    FractureShard {
        entity_id: u64,
        shard_index: u8,
        shard_seed: u64,
    },
    FractureComplete {
        entity_id: u64,
        shard_count: u8,
    },
    Transfiguration {
        entity_id: u64,
        from_form: String,
        to_form: String,
        contract_version: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationWorldEvent {
    pub header: WorldEventHeader,
    pub kind: AnimationWorldEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransfigurationContract {
    pub version: u16,
    pub allowed_form_edges: Vec<(String, String)>,
}

impl TransfigurationContract {
    pub fn permits(&self, from_form: &str, to_form: &str) -> bool {
        self.allowed_form_edges
            .iter()
            .any(|edge| edge.0 == from_form && edge.1 == to_form)
    }
}

pub fn fracture_event_sequence(
    sequence_start: u64,
    tick: u64,
    actor_id: u64,
    entity_id: u64,
    shard_count: u8,
) -> Vec<AnimationWorldEvent> {
    let mut out = Vec::with_capacity(usize::from(shard_count) + 2);
    out.push(AnimationWorldEvent {
        header: WorldEventHeader {
            sequence: sequence_start,
            tick,
            actor_id,
        },
        kind: AnimationWorldEventKind::FractureStart {
            entity_id,
            integrity_before_bp: 10_000,
        },
    });

    for shard_index in 0..shard_count {
        out.push(AnimationWorldEvent {
            header: WorldEventHeader {
                sequence: sequence_start + 1 + u64::from(shard_index),
                tick,
                actor_id,
            },
            kind: AnimationWorldEventKind::FractureShard {
                entity_id,
                shard_index,
                shard_seed: fracture_shard_seed(entity_id, tick, shard_index),
            },
        });
    }

    out.push(AnimationWorldEvent {
        header: WorldEventHeader {
            sequence: sequence_start + 1 + u64::from(shard_count),
            tick,
            actor_id,
        },
        kind: AnimationWorldEventKind::FractureComplete {
            entity_id,
            shard_count,
        },
    });
    out
}

pub fn transfiguration_contract(
    sequence: u64,
    tick: u64,
    actor_id: u64,
    entity_id: u64,
    from_form: &str,
    to_form: &str,
    contract: &TransfigurationContract,
) -> Result<AnimationWorldEvent, String> {
    if !contract.permits(from_form, to_form) {
        return Err(format!(
            "transfiguration denied by contract v{} for edge {} -> {}",
            contract.version, from_form, to_form
        ));
    }

    Ok(AnimationWorldEvent {
        header: WorldEventHeader {
            sequence,
            tick,
            actor_id,
        },
        kind: AnimationWorldEventKind::Transfiguration {
            entity_id,
            from_form: from_form.to_string(),
            to_form: to_form.to_string(),
            contract_version: contract.version,
        },
    })
}

fn fracture_shard_seed(entity_id: u64, tick: u64, shard_index: u8) -> u64 {
    let mut x = entity_id
        ^ tick.rotate_left(17)
        ^ u64::from(shard_index).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::{
        AnimationWorldEventKind, TransfigurationContract,
        fracture_event_sequence as build_fracture_event_sequence,
        transfiguration_contract as build_transfiguration_contract_event,
    };

    #[test]
    fn fracture_event_sequence() {
        let sequence = build_fracture_event_sequence(900, 128, 7, 42, 3);
        assert_eq!(sequence.len(), 5);

        for (offset, event) in sequence.iter().enumerate() {
            assert_eq!(event.header.sequence, 900 + offset as u64);
            assert_eq!(event.header.tick, 128);
            assert_eq!(event.header.actor_id, 7);
        }

        assert!(matches!(
            sequence.first().expect("first").kind,
            AnimationWorldEventKind::FractureStart { entity_id: 42, .. }
        ));
        assert!(matches!(
            sequence.last().expect("last").kind,
            AnimationWorldEventKind::FractureComplete {
                entity_id: 42,
                shard_count: 3
            }
        ));
    }

    #[test]
    fn transfiguration_contract() {
        let contract = TransfigurationContract {
            version: 2,
            allowed_form_edges: vec![
                ("oak_sapling".to_string(), "oak_ancient".to_string()),
                ("stone_idle".to_string(), "stone_awake".to_string()),
            ],
        };
        let allowed = build_transfiguration_contract_event(
            1200,
            200,
            11,
            99,
            "oak_sapling",
            "oak_ancient",
            &contract,
        )
        .expect("allowed edge should produce event");
        assert!(matches!(
            allowed.kind,
            AnimationWorldEventKind::Transfiguration {
                entity_id: 99,
                contract_version: 2,
                ..
            }
        ));

        let denied = build_transfiguration_contract_event(
            1201,
            200,
            11,
            99,
            "oak_sapling",
            "stone_awake",
            &contract,
        )
        .expect_err("disallowed edge should fail");
        assert!(denied.contains("contract v2"));
    }
}
