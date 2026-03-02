use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakableJointV1 {
    pub parent: u64,
    pub child: u64,
    pub break_threshold_milli: i64,
}

pub fn evaluate_breaks(
    joints: &[BreakableJointV1],
    impulses_milli: &[(u64, u64, i64)],
) -> Vec<(u64, u64)> {
    let impulse_map = impulses_milli
        .iter()
        .map(|(a, b, impulse)| ((*a, *b), *impulse))
        .collect::<BTreeMap<_, _>>();

    let mut broken = Vec::new();
    for joint in joints {
        let impulse = impulse_map
            .get(&(joint.parent, joint.child))
            .or_else(|| impulse_map.get(&(joint.child, joint.parent)))
            .copied()
            .unwrap_or(0);
        if impulse >= joint.break_threshold_milli {
            broken.push((joint.parent, joint.child));
        }
    }
    broken
}

pub fn chain_reaction_breaks(
    joints: &[BreakableJointV1],
    initial: &[(u64, u64)],
    max_depth: usize,
) -> Vec<(u64, u64)> {
    let mut adjacency = BTreeMap::<u64, Vec<u64>>::new();
    for joint in joints {
        adjacency.entry(joint.parent).or_default().push(joint.child);
        adjacency.entry(joint.child).or_default().push(joint.parent);
    }

    let mut queue = VecDeque::<(u64, usize)>::new();
    for &(a, b) in initial {
        queue.push_back((a, 0));
        queue.push_back((b, 0));
    }

    let mut broken_set = BTreeSet::<(u64, u64)>::new();
    while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for neighbor in adjacency.get(&node).into_iter().flatten() {
            let edge = if node < *neighbor {
                (node, *neighbor)
            } else {
                (*neighbor, node)
            };
            if broken_set.insert(edge) {
                queue.push_back((*neighbor, depth + 1));
            }
        }
    }

    broken_set.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{BreakableJointV1, chain_reaction_breaks, evaluate_breaks};

    #[test]
    fn articulated_chain_reaction_is_bounded() {
        let joints = vec![
            BreakableJointV1 {
                parent: 1,
                child: 2,
                break_threshold_milli: 100,
            },
            BreakableJointV1 {
                parent: 2,
                child: 3,
                break_threshold_milli: 100,
            },
            BreakableJointV1 {
                parent: 3,
                child: 4,
                break_threshold_milli: 100,
            },
        ];
        let initial = evaluate_breaks(&joints, &[(1, 2, 250)]);
        assert_eq!(initial, vec![(1, 2)]);
        let cascaded = chain_reaction_breaks(&joints, &initial, 2);
        assert!(cascaded.contains(&(1, 2)));
        assert!(cascaded.contains(&(2, 3)));
    }
}
