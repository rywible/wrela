use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveMatrix {
    pub transitions: BTreeMap<String, Vec<String>>,
}

pub fn move_matrix() -> MoveMatrix {
    let transitions = BTreeMap::from([
        (
            "idle".to_owned(),
            vec!["walk".to_owned(), "dodge".to_owned()],
        ),
        (
            "walk".to_owned(),
            vec![
                "idle".to_owned(),
                "sprint".to_owned(),
                "light_attack".to_owned(),
            ],
        ),
        (
            "sprint".to_owned(),
            vec![
                "walk".to_owned(),
                "dodge".to_owned(),
                "heavy_attack".to_owned(),
            ],
        ),
        (
            "dodge".to_owned(),
            vec!["idle".to_owned(), "light_attack".to_owned()],
        ),
        (
            "light_attack".to_owned(),
            vec!["light_attack_2".to_owned(), "heavy_attack".to_owned()],
        ),
        (
            "light_attack_2".to_owned(),
            vec!["finisher".to_owned(), "dodge".to_owned()],
        ),
        (
            "heavy_attack".to_owned(),
            vec!["finisher".to_owned(), "idle".to_owned()],
        ),
        ("finisher".to_owned(), vec!["idle".to_owned()]),
    ]);
    MoveMatrix { transitions }
}

pub fn resonance_variants() -> BTreeMap<String, MoveMatrix> {
    let base = move_matrix();

    let mut harmonic = base.clone();
    harmonic
        .transitions
        .entry("sprint".to_owned())
        .and_modify(|edges| edges.push("light_attack_2".to_owned()));

    let mut overdrive = base.clone();
    overdrive
        .transitions
        .entry("dodge".to_owned())
        .and_modify(|edges| edges.push("finisher".to_owned()));

    BTreeMap::from([
        ("base".to_owned(), base),
        ("harmonic".to_owned(), harmonic),
        ("overdrive".to_owned(), overdrive),
    ])
}

pub fn class_signature_vector() -> [u16; 3] {
    [44, 39, 31]
}

#[cfg(test)]
mod tests {
    use super::{move_matrix, resonance_variants};
    use std::collections::BTreeSet;

    #[test]
    fn move_matrix_complete() {
        let matrix = move_matrix();
        let required = [
            "idle",
            "walk",
            "sprint",
            "dodge",
            "light_attack",
            "light_attack_2",
            "heavy_attack",
            "finisher",
        ];

        for key in required {
            assert!(
                matrix.transitions.contains_key(key),
                "move matrix missing required move '{key}'"
            );
            let transitions = matrix
                .transitions
                .get(key)
                .expect("required key should have transitions");
            assert!(
                !transitions.is_empty(),
                "move '{key}' must expose at least one transition"
            );
        }
    }

    #[test]
    fn resonance_variant_consistency() {
        let variants = resonance_variants();
        let base = variants
            .get("base")
            .expect("resonance variants must include base matrix");
        let base_keys = base.transitions.keys().cloned().collect::<BTreeSet<_>>();

        for (variant_name, variant) in variants {
            let variant_keys = variant.transitions.keys().cloned().collect::<BTreeSet<_>>();
            assert_eq!(
                base_keys, variant_keys,
                "variant '{variant_name}' diverged from base move-key contract"
            );
        }
    }
}
