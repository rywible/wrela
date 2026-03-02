#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventContract {
    pub event: String,
    pub frame_start: u32,
    pub frame_end: u32,
}

pub fn event_contracts() -> Vec<EventContract> {
    vec![
        EventContract {
            event: "windup".to_owned(),
            frame_start: 3,
            frame_end: 9,
        },
        EventContract {
            event: "release".to_owned(),
            frame_start: 10,
            frame_end: 14,
        },
        EventContract {
            event: "impact".to_owned(),
            frame_start: 15,
            frame_end: 17,
        },
        EventContract {
            event: "recover".to_owned(),
            frame_start: 18,
            frame_end: 27,
        },
    ]
}

pub fn class_signature_vector() -> [u16; 3] {
    [31, 61, 28]
}

#[cfg(test)]
mod tests {
    use super::event_contracts as build_event_contracts;

    #[test]
    fn event_contracts() {
        let contracts = build_event_contracts();
        assert!(
            contracts.iter().any(|contract| contract.event == "windup"),
            "contracts must include windup"
        );
        assert!(
            contracts.iter().any(|contract| contract.event == "impact"),
            "contracts must include impact"
        );
        assert!(
            contracts.iter().any(|contract| contract.event == "recover"),
            "contracts must include recover"
        );

        for contract in &contracts {
            assert!(
                contract.frame_start <= contract.frame_end,
                "event {} has invalid frame bounds",
                contract.event
            );
        }

        for pair in contracts.windows(2) {
            assert!(
                pair[0].frame_start <= pair[1].frame_start,
                "event contracts must remain frame-ordered"
            );
        }
    }
}
