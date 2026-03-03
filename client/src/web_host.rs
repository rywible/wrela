#![cfg_attr(not(test), allow(dead_code))]

use crate::animation_graph::AnimationAdvanceResult;
use crate::animation_graph::{
    AnimationGraphDefinition, AnimationGraphState, AnimationStateDefinition, AnimationTransition,
};
use crate::mesh::MAX_SKINNING_JOINTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkinningPoseUploadBounds {
    pub(crate) max_joints: usize,
}

impl Default for SkinningPoseUploadBounds {
    fn default() -> Self {
        Self {
            max_joints: MAX_SKINNING_JOINTS,
        }
    }
}

pub(crate) fn validate_skinning_pose_upload_bounds(
    pose_joint_count: usize,
    bounds: SkinningPoseUploadBounds,
) -> Result<(), String> {
    if pose_joint_count > bounds.max_joints {
        return Err(format!(
            "pose upload joint count {pose_joint_count} exceeds max {}",
            bounds.max_joints
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimationBudgetGate {
    pub(crate) max_transitions_evaluated: u32,
    pub(crate) max_events_emitted: u32,
    pub(crate) max_pose_upload_joints: usize,
}

impl Default for AnimationBudgetGate {
    fn default() -> Self {
        Self {
            max_transitions_evaluated: 128,
            max_events_emitted: 32,
            max_pose_upload_joints: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimationBudgetSample {
    pub(crate) transitions_evaluated: u32,
    pub(crate) events_emitted: u32,
    pub(crate) pose_upload_joints: usize,
}

impl AnimationBudgetSample {
    pub(crate) fn from_step(result: &AnimationAdvanceResult, pose_upload_joints: usize) -> Self {
        Self {
            transitions_evaluated: result.transitions_evaluated,
            events_emitted: result.events.len() as u32,
            pose_upload_joints,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnimationBudgetGateOutcome {
    pub(crate) within_budget: bool,
    pub(crate) violations: Vec<&'static str>,
}

pub(crate) fn enforce_animation_budget_gate(
    sample: AnimationBudgetSample,
    gate: AnimationBudgetGate,
) -> AnimationBudgetGateOutcome {
    let mut violations = Vec::new();
    if sample.transitions_evaluated > gate.max_transitions_evaluated {
        violations.push("transitions_evaluated");
    }
    if sample.events_emitted > gate.max_events_emitted {
        violations.push("events_emitted");
    }
    if sample.pose_upload_joints > gate.max_pose_upload_joints {
        violations.push("pose_upload_joints");
    }
    AnimationBudgetGateOutcome {
        within_budget: violations.is_empty(),
        violations,
    }
}

pub fn start_client() -> Result<(), &'static str> {
    Err("wrela_client runtime only boots on wasm32 targets")
}

pub(crate) fn consume_animation_graph_contract(
    states: &[String],
    transitions: &[(String, String, u32)],
) -> Result<AnimationGraphState, String> {
    if states.is_empty() {
        return Err("animation contract requires at least one state".to_string());
    }
    let definitions = states
        .iter()
        .map(|state_name| AnimationStateDefinition {
            name: state_name.clone(),
            markers: Vec::new(),
            windows: Vec::new(),
        })
        .collect::<Vec<_>>();
    let transition_defs = transitions
        .iter()
        .map(|(from, to, after_ticks)| AnimationTransition {
            from: from.clone(),
            to: to.clone(),
            after_ticks: (*after_ticks).max(1),
        })
        .collect::<Vec<_>>();
    let graph = AnimationGraphDefinition::new(definitions, transition_defs)?;
    AnimationGraphState::new(graph, states[0].as_str())
}

#[cfg(test)]
mod tests {
    use crate::animation_graph::{
        AnimationEventWindow, AnimationGraphDefinition, AnimationGraphState,
        AnimationStateDefinition, AnimationTransition,
    };

    use super::{
        AnimationBudgetGate, AnimationBudgetSample, SkinningPoseUploadBounds,
        consume_animation_graph_contract, enforce_animation_budget_gate,
        validate_skinning_pose_upload_bounds,
    };
    use crate::mesh::{Vertex3D, VertexFormat, flatten_skinning_palette_for_upload};

    #[test]
    fn skinning_pose_upload_bounds() {
        let bounds = SkinningPoseUploadBounds { max_joints: 96 };
        validate_skinning_pose_upload_bounds(96, bounds)
            .expect("pose upload on exact boundary should pass");
        let error = validate_skinning_pose_upload_bounds(97, bounds)
            .expect_err("pose upload beyond supported joint count must fail");
        assert!(error.contains("97"));
        assert!(error.contains("96"));
    }

    #[test]
    fn animation_budget_gate_enforced() {
        let graph = AnimationGraphDefinition::new(
            vec![
                AnimationStateDefinition {
                    name: "idle".to_string(),
                    markers: vec![],
                    windows: vec![],
                },
                AnimationStateDefinition {
                    name: "attack".to_string(),
                    markers: vec![],
                    windows: vec![AnimationEventWindow {
                        event: "hit".to_string(),
                        start_tick: 1,
                        end_tick: 1,
                    }],
                },
            ],
            vec![AnimationTransition {
                from: "idle".to_string(),
                to: "attack".to_string(),
                after_ticks: 1,
            }],
        )
        .expect("animation graph should be valid");
        let mut state = AnimationGraphState::new(graph, "idle").expect("state should build");
        let step = state.advance_ticks(1);
        let sample = AnimationBudgetSample::from_step(&step, 192);
        let gate = AnimationBudgetGate {
            max_transitions_evaluated: 0,
            max_events_emitted: 0,
            max_pose_upload_joints: 160,
        };

        let outcome = enforce_animation_budget_gate(sample, gate);
        assert!(!outcome.within_budget);
        assert_eq!(
            outcome.violations,
            vec![
                "transitions_evaluated",
                "events_emitted",
                "pose_upload_joints"
            ]
        );
    }

    #[test]
    fn animation_runtime_consumes_contracts() {
        let states = vec![
            "idle".to_string(),
            "attack".to_string(),
            "recover".to_string(),
        ];
        let transitions = vec![
            ("idle".to_string(), "attack".to_string(), 1_u32),
            ("attack".to_string(), "recover".to_string(), 2_u32),
        ];
        let mut runtime = consume_animation_graph_contract(&states, &transitions)
            .expect("animation runtime should load typed graph contracts");
        let first = runtime.advance_ticks(1);
        let second = runtime.advance_ticks(2);
        assert_eq!(first.transitions_taken, 1);
        assert_eq!(runtime.current_state(), "recover");
        assert!(second.transitions_evaluated >= 1);
    }

    #[test]
    fn skinned_render_path_uses_joint_weights() {
        assert_eq!(std::mem::size_of::<Vertex3D>(), 88);
        assert_eq!(Vertex3D::ATTRIBS[3].shader_location, 3);
        assert_eq!(Vertex3D::ATTRIBS[3].format, VertexFormat::Uint16x4);
        assert_eq!(Vertex3D::ATTRIBS[4].shader_location, 4);
        assert_eq!(Vertex3D::ATTRIBS[4].format, VertexFormat::Float32x4);

        let joints = vec![
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.25, 0.0, 0.0, 1.0],
            ],
        ];
        let flattened = flatten_skinning_palette_for_upload(&joints, 4)
            .expect("joint palette should flatten for upload");
        assert_eq!(flattened.len(), 4);
        assert_eq!(flattened[1][3][0], 0.25);
    }
}
