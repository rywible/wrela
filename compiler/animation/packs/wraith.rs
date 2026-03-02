use crate::animation::synth::deterministic_seed;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSignature {
    pub class_name: &'static str,
    pub values: [u16; 3],
}

pub fn class_signature() -> ClassSignature {
    ClassSignature {
        class_name: "wraith",
        values: [63, 57, 14],
    }
}

pub fn class_signature_fingerprint(signature: &ClassSignature) -> u64 {
    let v0 = signature.values[0].to_string();
    let v1 = signature.values[1].to_string();
    let v2 = signature.values[2].to_string();
    let labels = [signature.class_name, v0.as_str(), v1.as_str(), v2.as_str()];
    deterministic_seed("animation.pack.signature", &labels)
}

#[cfg(test)]
mod tests {
    use super::{class_signature, class_signature_fingerprint};
    use crate::animation::packs::{ancient, order, traveller};

    #[test]
    fn class_signature_distinct() {
        let wraith = class_signature();
        let wraith_fp = class_signature_fingerprint(&wraith);

        let traveller_fp = class_signature_fingerprint(&super::ClassSignature {
            class_name: "traveller",
            values: traveller::class_signature_vector(),
        });
        let order_fp = class_signature_fingerprint(&super::ClassSignature {
            class_name: "order",
            values: order::class_signature_vector(),
        });
        let ancient_fp = class_signature_fingerprint(&super::ClassSignature {
            class_name: "ancient",
            values: ancient::class_signature_vector(),
        });

        assert_ne!(
            wraith_fp, traveller_fp,
            "wraith must be signature-distinct from traveller"
        );
        assert_ne!(
            wraith_fp, order_fp,
            "wraith must be signature-distinct from order"
        );
        assert_ne!(
            wraith_fp, ancient_fp,
            "wraith must be signature-distinct from ancient"
        );
    }
}
