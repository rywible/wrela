#![cfg_attr(not(test), allow(dead_code))]

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationMarker {
    pub event: String,
    pub tick: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationEventWindow {
    pub event: String,
    pub start_tick: u32,
    pub end_tick: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationStateDefinition {
    pub name: String,
    pub markers: Vec<AnimationMarker>,
    pub windows: Vec<AnimationEventWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationTransition {
    pub from: String,
    pub to: String,
    pub after_ticks: u32,
}

#[derive(Debug, Clone)]
pub struct AnimationGraphDefinition {
    states: Vec<AnimationStateDefinition>,
    state_lookup: HashMap<String, usize>,
    transitions: Vec<AnimationTransition>,
}

impl AnimationGraphDefinition {
    pub fn new(
        states: Vec<AnimationStateDefinition>,
        transitions: Vec<AnimationTransition>,
    ) -> Result<Self, String> {
        if states.is_empty() {
            return Err("animation graph requires at least one state".to_string());
        }

        let mut state_lookup = HashMap::new();
        for (index, state) in states.iter().enumerate() {
            if state.name.trim().is_empty() {
                return Err("animation graph state name cannot be empty".to_string());
            }
            if state_lookup.insert(state.name.clone(), index).is_some() {
                return Err(format!(
                    "animation graph has duplicate state '{}'",
                    state.name
                ));
            }
            for window in &state.windows {
                if window.start_tick == 0 {
                    return Err(format!(
                        "animation graph window '{}' in '{}' must start at tick >= 1",
                        window.event, state.name
                    ));
                }
                if window.end_tick < window.start_tick {
                    return Err(format!(
                        "animation graph window '{}' in '{}' has end_tick before start_tick",
                        window.event, state.name
                    ));
                }
            }
        }

        for transition in &transitions {
            if transition.after_ticks == 0 {
                return Err(format!(
                    "animation graph transition {} -> {} must have after_ticks >= 1",
                    transition.from, transition.to
                ));
            }
            if !state_lookup.contains_key(&transition.from) {
                return Err(format!(
                    "animation graph transition references unknown source state '{}'",
                    transition.from
                ));
            }
            if !state_lookup.contains_key(&transition.to) {
                return Err(format!(
                    "animation graph transition references unknown target state '{}'",
                    transition.to
                ));
            }
        }

        Ok(Self {
            states,
            state_lookup,
            transitions,
        })
    }

    fn state(&self, name: &str) -> &AnimationStateDefinition {
        let index = self
            .state_lookup
            .get(name)
            .expect("graph state lookup should always resolve for active state");
        &self.states[*index]
    }

    pub fn has_state(&self, name: &str) -> bool {
        self.state_lookup.contains_key(name)
    }

    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    pub fn transition_count(&self) -> usize {
        self.transitions.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationEventKind {
    Marker { event: String },
    WindowOpened { event: String },
    WindowClosed { event: String },
    Transition { from: String, to: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationEvent {
    pub tick: u64,
    pub state: String,
    pub kind: AnimationEventKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnimationAdvanceResult {
    pub events: Vec<AnimationEvent>,
    pub transitions_evaluated: u32,
    pub transitions_taken: u32,
}

#[derive(Debug, Clone)]
pub struct AnimationGraphState {
    graph: AnimationGraphDefinition,
    current_state: String,
    state_tick: u32,
    global_tick: u64,
}

impl AnimationGraphState {
    pub fn new(graph: AnimationGraphDefinition, start_state: &str) -> Result<Self, String> {
        if !graph.state_lookup.contains_key(start_state) {
            return Err(format!(
                "animation graph cannot start in unknown state '{}'",
                start_state
            ));
        }
        Ok(Self {
            graph,
            current_state: start_state.to_string(),
            state_tick: 0,
            global_tick: 0,
        })
    }

    pub fn current_state(&self) -> &str {
        self.current_state.as_str()
    }

    pub fn state_tick(&self) -> u32 {
        self.state_tick
    }

    pub fn global_tick(&self) -> u64 {
        self.global_tick
    }

    pub fn reconcile_to_state(
        &mut self,
        state: &str,
        state_tick: u32,
        global_tick: u64,
    ) -> Result<(), String> {
        if !self.graph.has_state(state) {
            return Err(format!(
                "animation graph cannot reconcile to unknown state '{}'",
                state
            ));
        }
        self.current_state = state.to_string();
        self.state_tick = state_tick;
        self.global_tick = global_tick;
        Ok(())
    }

    pub fn advance_ticks(&mut self, ticks: u32) -> AnimationAdvanceResult {
        let mut result = AnimationAdvanceResult::default();
        for _ in 0..ticks {
            self.global_tick = self.global_tick.saturating_add(1);
            self.state_tick = self.state_tick.saturating_add(1);

            let state = self.graph.state(self.current_state.as_str());
            for marker in &state.markers {
                if marker.tick == self.state_tick {
                    result.events.push(AnimationEvent {
                        tick: self.global_tick,
                        state: self.current_state.clone(),
                        kind: AnimationEventKind::Marker {
                            event: marker.event.clone(),
                        },
                    });
                }
            }
            for window in &state.windows {
                if window.start_tick == self.state_tick {
                    result.events.push(AnimationEvent {
                        tick: self.global_tick,
                        state: self.current_state.clone(),
                        kind: AnimationEventKind::WindowOpened {
                            event: window.event.clone(),
                        },
                    });
                }
                if window.end_tick == self.state_tick {
                    result.events.push(AnimationEvent {
                        tick: self.global_tick,
                        state: self.current_state.clone(),
                        kind: AnimationEventKind::WindowClosed {
                            event: window.event.clone(),
                        },
                    });
                }
            }

            let mut selected_transition: Option<&AnimationTransition> = None;
            for transition in &self.graph.transitions {
                if transition.from == self.current_state {
                    result.transitions_evaluated = result.transitions_evaluated.saturating_add(1);
                    if selected_transition.is_none() && self.state_tick >= transition.after_ticks {
                        selected_transition = Some(transition);
                    }
                }
            }

            if let Some(transition) = selected_transition {
                result.events.push(AnimationEvent {
                    tick: self.global_tick,
                    state: self.current_state.clone(),
                    kind: AnimationEventKind::Transition {
                        from: transition.from.clone(),
                        to: transition.to.clone(),
                    },
                });
                result.transitions_taken = result.transitions_taken.saturating_add(1);
                self.current_state = transition.to.clone();
                self.state_tick = 0;
            }
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Animation blender — state-machine-driven clip crossfading
// ---------------------------------------------------------------------------

use crate::skeletal_animation::{AnimationClip, JointPose};

/// The result of advancing the blender one frame: a blended pose ready for
/// `compute_skinning_matrices`.
#[derive(Debug, Clone)]
pub struct BlendedPose {
    pub joint_poses: Vec<JointPose>,
}

/// Maps integer game states to animation clip indices and crossfades between
/// clips when the game state changes.
#[derive(Debug, Clone)]
pub struct AnimationBlender {
    current_clip: usize,
    target_clip: Option<usize>,
    current_time: f32,
    target_time: f32,
    blend_factor: f32,
    blend_duration_frames: u32,
    blend_elapsed_frames: u32,
    state_to_clip: HashMap<i32, usize>,
    /// The last game state that was set via `transition_to_state`, used to
    /// detect when the state actually changes.
    last_state: Option<i32>,
}

impl AnimationBlender {
    pub fn new() -> Self {
        Self {
            current_clip: 0,
            target_clip: None,
            current_time: 0.0,
            target_time: 0.0,
            blend_factor: 0.0,
            blend_duration_frames: 6,
            blend_elapsed_frames: 0,
            state_to_clip: HashMap::new(),
            last_state: None,
        }
    }

    /// Register a mapping from a game state integer to an animation clip index.
    pub fn set_state_mapping(&mut self, state: i32, clip_index: usize) {
        self.state_to_clip.insert(state, clip_index);
    }

    /// Set the crossfade duration in frames (default: 6 ~= 100ms at 60fps).
    pub fn set_blend_duration_frames(&mut self, frames: u32) {
        self.blend_duration_frames = frames.max(1);
    }

    /// Request a transition to the clip associated with the given game state.
    /// If the state has not changed since the last call, this is a no-op.
    /// If no clip is mapped for this state, this is a no-op.
    pub fn transition_to_state(&mut self, state: i32) {
        if self.last_state == Some(state) {
            return;
        }
        self.last_state = Some(state);

        let clip_index = match self.state_to_clip.get(&state) {
            Some(&idx) => idx,
            None => return,
        };

        if self.target_clip.is_none() && clip_index == self.current_clip {
            // Already playing this clip with no pending transition.
            return;
        }

        if self.target_clip == Some(clip_index) {
            // Already transitioning to this clip.
            return;
        }

        // If we are mid-blend, commit the current blend progress: snap the
        // outgoing clip to the target clip at its current time.
        if self.target_clip.is_some() {
            self.current_clip = self.target_clip.unwrap();
            self.current_time = self.target_time;
        }

        self.target_clip = Some(clip_index);
        self.target_time = 0.0;
        self.blend_factor = 0.0;
        self.blend_elapsed_frames = 0;
    }

    /// Advance time by `dt` seconds, sample the active clip(s), and return
    /// the blended pose. `clips` is the full list of loaded animation clips,
    /// and `joint_count` is the number of joints in the target skeleton.
    pub fn advance(&mut self, dt: f32, clips: &[AnimationClip], joint_count: usize) -> BlendedPose {
        // Advance current clip time (loop).
        if !clips.is_empty() && self.current_clip < clips.len() {
            let dur = clips[self.current_clip].duration_secs.max(0.001);
            self.current_time = (self.current_time + dt) % dur;
        }

        // Advance target clip time if we are blending.
        if let Some(target) = self.target_clip {
            if target < clips.len() {
                let dur = clips[target].duration_secs.max(0.001);
                self.target_time = (self.target_time + dt) % dur;
            }

            self.blend_elapsed_frames += 1;
            self.blend_factor =
                (self.blend_elapsed_frames as f32 / self.blend_duration_frames as f32).min(1.0);

            // If blend is complete, commit the target as the new current clip.
            if self.blend_elapsed_frames >= self.blend_duration_frames {
                self.current_clip = target;
                self.current_time = self.target_time;
                self.target_clip = None;
                self.blend_factor = 0.0;
                self.blend_elapsed_frames = 0;
            }
        }

        // Sample current clip.
        let current_pose = if !clips.is_empty() && self.current_clip < clips.len() {
            crate::skeletal_animation::evaluate_clip(
                &clips[self.current_clip],
                self.current_time,
                joint_count,
            )
        } else {
            vec![JointPose::default(); joint_count]
        };

        // If blending, sample target clip and lerp.
        let final_pose = if let Some(target) = self.target_clip {
            if target < clips.len() {
                let target_pose = crate::skeletal_animation::evaluate_clip(
                    &clips[target],
                    self.target_time,
                    joint_count,
                );
                blend_poses(&current_pose, &target_pose, self.blend_factor)
            } else {
                current_pose
            }
        } else {
            current_pose
        };

        BlendedPose {
            joint_poses: final_pose,
        }
    }

    /// Returns `true` when a crossfade is currently in progress.
    pub fn is_blending(&self) -> bool {
        self.target_clip.is_some()
    }

    /// Returns the current blend factor (0.0 = fully on current clip, 1.0 =
    /// fully on target clip). Returns 0.0 when not blending.
    pub fn blend_progress(&self) -> f32 {
        if self.target_clip.is_some() {
            self.blend_factor
        } else {
            0.0
        }
    }

    /// Returns the clip index currently being played (or the outgoing clip
    /// during a blend).
    pub fn current_clip_index(&self) -> usize {
        self.current_clip
    }

    /// Returns the target clip index during a blend, or `None` when not
    /// blending.
    pub fn target_clip_index(&self) -> Option<usize> {
        self.target_clip
    }
}

/// Linearly interpolate two poses joint-by-joint. Translation and scale are
/// lerped, rotation is slerped.
fn blend_poses(a: &[JointPose], b: &[JointPose], t: f32) -> Vec<JointPose> {
    let len = a.len().max(b.len());
    let mut result = Vec::with_capacity(len);
    for i in 0..len {
        let pa = a.get(i).copied().unwrap_or_default();
        let pb = b.get(i).copied().unwrap_or_default();
        result.push(JointPose {
            translation: lerp_vec3(&pa.translation, &pb.translation, t),
            rotation: slerp_quat(&pa.rotation, &pb.rotation, t),
            scale: lerp_vec3(&pa.scale, &pb.scale, t),
        });
    }
    result
}

fn lerp_vec3(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn slerp_quat(a: &[f32; 4], b: &[f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b_adj = *b;
    if dot < 0.0 {
        b_adj = [-b[0], -b[1], -b[2], -b[3]];
        dot = -dot;
    }
    if dot > 0.9995 {
        // nlerp fallback
        let r = [
            a[0] + (b_adj[0] - a[0]) * t,
            a[1] + (b_adj[1] - a[1]) * t,
            a[2] + (b_adj[2] - a[2]) * t,
            a[3] + (b_adj[3] - a[3]) * t,
        ];
        let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
        if len < 1e-10 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        let inv = 1.0 / len;
        return [r[0] * inv, r[1] * inv, r[2] * inv, r[3] * inv];
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin();
    let wa = ((1.0 - t) * theta).sin() / sin_theta;
    let wb = (t * theta).sin() / sin_theta;
    [
        a[0] * wa + b_adj[0] * wb,
        a[1] * wa + b_adj[1] * wb,
        a[2] * wa + b_adj[2] * wb,
        a[3] * wa + b_adj[3] * wb,
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        AnimationEventKind, AnimationEventWindow, AnimationGraphDefinition, AnimationGraphState,
        AnimationStateDefinition, AnimationTransition,
    };

    fn test_graph() -> AnimationGraphDefinition {
        AnimationGraphDefinition::new(
            vec![
                AnimationStateDefinition {
                    name: "idle".to_string(),
                    markers: vec![],
                    windows: vec![],
                },
                AnimationStateDefinition {
                    name: "run".to_string(),
                    markers: vec![],
                    windows: vec![],
                },
                AnimationStateDefinition {
                    name: "attack".to_string(),
                    markers: vec![],
                    windows: vec![AnimationEventWindow {
                        event: "hit".to_string(),
                        start_tick: 2,
                        end_tick: 4,
                    }],
                },
            ],
            vec![
                AnimationTransition {
                    from: "idle".to_string(),
                    to: "run".to_string(),
                    after_ticks: 2,
                },
                AnimationTransition {
                    from: "run".to_string(),
                    to: "attack".to_string(),
                    after_ticks: 2,
                },
                AnimationTransition {
                    from: "attack".to_string(),
                    to: "idle".to_string(),
                    after_ticks: 5,
                },
            ],
        )
        .expect("graph should be valid")
    }

    #[test]
    fn deterministic_transition_sequence() {
        let graph = test_graph();
        let mut bulk = AnimationGraphState::new(graph.clone(), "idle").expect("state should build");
        let bulk_result = bulk.advance_ticks(9);

        let mut stepped =
            AnimationGraphState::new(graph, "idle").expect("state should build for stepped");
        let mut stepped_events = Vec::new();
        let mut stepped_transitions_evaluated = 0u32;
        for _ in 0..9 {
            let tick_result = stepped.advance_ticks(1);
            stepped_transitions_evaluated =
                stepped_transitions_evaluated.saturating_add(tick_result.transitions_evaluated);
            stepped_events.extend(tick_result.events);
        }

        assert_eq!(bulk.current_state(), stepped.current_state());
        assert_eq!(bulk.state_tick(), stepped.state_tick());
        assert_eq!(bulk.global_tick(), stepped.global_tick());
        assert_eq!(bulk_result.events, stepped_events);
        assert_eq!(
            bulk_result.transitions_evaluated,
            stepped_transitions_evaluated
        );

        let transition_labels: Vec<String> = bulk_result
            .events
            .iter()
            .filter_map(|event| match &event.kind {
                AnimationEventKind::Transition { from, to } => Some(format!("{from}->{to}")),
                _ => None,
            })
            .collect();
        assert_eq!(
            transition_labels,
            vec![
                "idle->run".to_string(),
                "run->attack".to_string(),
                "attack->idle".to_string()
            ]
        );
    }

    #[test]
    fn event_window_alignment() {
        let graph = AnimationGraphDefinition::new(
            vec![
                AnimationStateDefinition {
                    name: "swing".to_string(),
                    markers: vec![],
                    windows: vec![
                        AnimationEventWindow {
                            event: "hit".to_string(),
                            start_tick: 2,
                            end_tick: 4,
                        },
                        AnimationEventWindow {
                            event: "armor".to_string(),
                            start_tick: 4,
                            end_tick: 5,
                        },
                    ],
                },
                AnimationStateDefinition {
                    name: "recover".to_string(),
                    markers: vec![],
                    windows: vec![],
                },
            ],
            vec![AnimationTransition {
                from: "swing".to_string(),
                to: "recover".to_string(),
                after_ticks: 5,
            }],
        )
        .expect("graph should be valid");

        let mut state = AnimationGraphState::new(graph, "swing").expect("state should build");
        let events = state.advance_ticks(5).events;

        let summarized: Vec<(u64, String)> = events
            .iter()
            .map(|event| {
                let label = match &event.kind {
                    AnimationEventKind::WindowOpened { event } => format!("open:{event}"),
                    AnimationEventKind::WindowClosed { event } => format!("close:{event}"),
                    AnimationEventKind::Transition { from, to } => {
                        format!("transition:{from}->{to}")
                    }
                    AnimationEventKind::Marker { event } => format!("marker:{event}"),
                };
                (event.tick, label)
            })
            .collect();

        assert_eq!(
            summarized,
            vec![
                (2, "open:hit".to_string()),
                (4, "close:hit".to_string()),
                (4, "open:armor".to_string()),
                (5, "close:armor".to_string()),
                (5, "transition:swing->recover".to_string()),
            ]
        );
    }

    #[test]
    fn reconcile_to_state_enforces_contract_and_updates_ticks() {
        let graph = test_graph();
        let mut state = AnimationGraphState::new(graph, "idle").expect("state should build");
        state.advance_ticks(3);

        state
            .reconcile_to_state("attack", 4, 99)
            .expect("reconcile to known state should pass");
        assert_eq!(state.current_state(), "attack");
        assert_eq!(state.state_tick(), 4);
        assert_eq!(state.global_tick(), 99);

        let error = state
            .reconcile_to_state("unknown", 0, 100)
            .expect_err("unknown state must fail reconciliation");
        assert!(error.contains("unknown state"));
    }
}

#[cfg(test)]
mod blender_tests {
    use super::*;
    use crate::skeletal_animation::{
        AnimationChannel, AnimationClip, ChannelProperty, Keyframe, KeyframeValue,
    };

    fn make_translation_clip(name: &str, duration: f32, start: [f32; 3], end: [f32; 3]) -> AnimationClip {
        AnimationClip {
            name: name.to_string(),
            duration_secs: duration,
            channels: vec![AnimationChannel {
                joint_index: 0,
                property: ChannelProperty::Translation,
                keyframes: vec![
                    Keyframe { time: 0.0, value: KeyframeValue::Vec3(start) },
                    Keyframe { time: duration, value: KeyframeValue::Vec3(end) },
                ],
            }],
        }
    }

    fn approx_eq_f32(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn approx_eq_vec3(a: &[f32; 3], b: &[f32; 3], eps: f32) -> bool {
        approx_eq_f32(a[0], b[0], eps)
            && approx_eq_f32(a[1], b[1], eps)
            && approx_eq_f32(a[2], b[2], eps)
    }

    #[test]
    fn blender_plays_single_clip_without_transition() {
        let clips = vec![
            make_translation_clip("idle", 1.0, [0.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
        ];
        let mut blender = AnimationBlender::new();
        blender.set_state_mapping(0, 0);
        blender.transition_to_state(0);

        // Advance half the clip duration
        let pose = blender.advance(0.5, &clips, 1);
        assert_eq!(pose.joint_poses.len(), 1);
        // At t=0.5 in a 1s clip going from 0 to 10, translation.x should be ~5.0
        assert!(
            approx_eq_vec3(&pose.joint_poses[0].translation, &[5.0, 0.0, 0.0], 0.1),
            "expected ~[5,0,0] got {:?}",
            pose.joint_poses[0].translation
        );
        assert!(!blender.is_blending());
    }

    #[test]
    fn blender_crossfades_between_two_clips() {
        let clips = vec![
            make_translation_clip("idle", 1.0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            make_translation_clip("walk", 1.0, [10.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
        ];
        let mut blender = AnimationBlender::new();
        blender.set_state_mapping(0, 0); // idle -> clip 0
        blender.set_state_mapping(1, 1); // walk -> clip 1
        blender.set_blend_duration_frames(4);

        // Start in idle
        blender.transition_to_state(0);
        blender.advance(0.0, &clips, 1);

        // Transition to walk
        blender.transition_to_state(1);
        assert!(blender.is_blending());

        // Advance 2 frames (half of the 4-frame blend)
        blender.advance(0.0, &clips, 1);
        let pose = blender.advance(0.0, &clips, 1);

        // At 2/4 = 50% blend, translation.x should be ~5.0 (midpoint of 0 and 10)
        assert!(
            approx_eq_f32(pose.joint_poses[0].translation[0], 5.0, 0.5),
            "expected ~5.0 got {}",
            pose.joint_poses[0].translation[0]
        );
        assert!(blender.is_blending());
        assert!(approx_eq_f32(blender.blend_progress(), 0.5, 0.01));
    }

    #[test]
    fn blender_completes_blend_and_commits_target() {
        let clips = vec![
            make_translation_clip("idle", 1.0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            make_translation_clip("walk", 1.0, [10.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
        ];
        let mut blender = AnimationBlender::new();
        blender.set_state_mapping(0, 0);
        blender.set_state_mapping(1, 1);
        blender.set_blend_duration_frames(3);

        blender.transition_to_state(0);
        blender.advance(0.0, &clips, 1);

        // Start transition to walk
        blender.transition_to_state(1);

        // Advance 3 frames to complete the blend
        blender.advance(0.0, &clips, 1);
        blender.advance(0.0, &clips, 1);
        let pose = blender.advance(0.0, &clips, 1);

        // Blend should be complete — now fully on clip 1
        assert!(!blender.is_blending());
        assert_eq!(blender.current_clip_index(), 1);
        assert!(
            approx_eq_f32(pose.joint_poses[0].translation[0], 10.0, 0.5),
            "expected ~10.0 got {}",
            pose.joint_poses[0].translation[0]
        );
    }

    #[test]
    fn blender_ignores_unmapped_state() {
        let clips = vec![
            make_translation_clip("idle", 1.0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        ];
        let mut blender = AnimationBlender::new();
        blender.set_state_mapping(0, 0);

        blender.transition_to_state(0);
        blender.advance(0.0, &clips, 1);

        // Transition to state 99 which is not mapped
        blender.transition_to_state(99);
        assert!(!blender.is_blending(), "should not blend to unmapped state");
        assert_eq!(blender.current_clip_index(), 0);
    }

    #[test]
    fn blender_same_state_transition_is_noop() {
        let clips = vec![
            make_translation_clip("idle", 1.0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            make_translation_clip("walk", 1.0, [10.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
        ];
        let mut blender = AnimationBlender::new();
        blender.set_state_mapping(0, 0);
        blender.set_state_mapping(1, 1);

        blender.transition_to_state(0);
        blender.advance(0.0, &clips, 1);

        // Transition to the same state again
        blender.transition_to_state(0);
        assert!(!blender.is_blending(), "same state should not trigger blend");
    }

    #[test]
    fn blender_interrupting_blend_snaps_to_target_and_starts_new() {
        let clips = vec![
            make_translation_clip("idle", 1.0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            make_translation_clip("walk", 1.0, [10.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
            make_translation_clip("run", 1.0, [20.0, 0.0, 0.0], [20.0, 0.0, 0.0]),
        ];
        let mut blender = AnimationBlender::new();
        blender.set_state_mapping(0, 0);
        blender.set_state_mapping(1, 1);
        blender.set_state_mapping(2, 2);
        blender.set_blend_duration_frames(6);

        // Start in idle, begin transition to walk
        blender.transition_to_state(0);
        blender.advance(0.0, &clips, 1);
        blender.transition_to_state(1);
        blender.advance(0.0, &clips, 1); // 1 frame into blend

        // Interrupt by transitioning to run
        blender.transition_to_state(2);
        assert!(blender.is_blending());
        assert_eq!(blender.target_clip_index(), Some(2));
        // The current clip should have been snapped to clip 1 (walk)
        assert_eq!(blender.current_clip_index(), 1);
    }

    #[test]
    fn blender_loops_clip_correctly() {
        let clips = vec![
            make_translation_clip("short", 0.5, [0.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
        ];
        let mut blender = AnimationBlender::new();
        blender.set_state_mapping(0, 0);
        blender.transition_to_state(0);

        // Advance past the clip duration — should loop
        let pose1 = blender.advance(0.3, &clips, 1);
        // After 0.3s in a 0.5s clip => t=0.3, translation.x ~ 6.0
        assert!(
            approx_eq_f32(pose1.joint_poses[0].translation[0], 6.0, 0.5),
            "expected ~6.0 got {}",
            pose1.joint_poses[0].translation[0]
        );

        // Advance another 0.3s => total 0.6s, looped to 0.1s, translation.x ~ 2.0
        let pose2 = blender.advance(0.3, &clips, 1);
        assert!(
            approx_eq_f32(pose2.joint_poses[0].translation[0], 2.0, 0.5),
            "expected ~2.0 got {}",
            pose2.joint_poses[0].translation[0]
        );
    }

    #[test]
    fn blender_empty_clips_returns_default_pose() {
        let clips: Vec<AnimationClip> = vec![];
        let mut blender = AnimationBlender::new();
        let pose = blender.advance(0.1, &clips, 3);
        assert_eq!(pose.joint_poses.len(), 3);
        for p in &pose.joint_poses {
            assert_eq!(p.translation, [0.0, 0.0, 0.0]);
            assert_eq!(p.rotation, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(p.scale, [1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn blend_poses_at_endpoints() {
        let a = vec![JointPose {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }];
        let b = vec![JointPose {
            translation: [10.0, 20.0, 30.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [2.0, 2.0, 2.0],
        }];

        let at_zero = super::blend_poses(&a, &b, 0.0);
        assert!(approx_eq_vec3(&at_zero[0].translation, &[0.0, 0.0, 0.0], 1e-5));
        assert!(approx_eq_vec3(&at_zero[0].scale, &[1.0, 1.0, 1.0], 1e-5));

        let at_one = super::blend_poses(&a, &b, 1.0);
        assert!(approx_eq_vec3(&at_one[0].translation, &[10.0, 20.0, 30.0], 1e-5));
        assert!(approx_eq_vec3(&at_one[0].scale, &[2.0, 2.0, 2.0], 1e-5));

        let at_half = super::blend_poses(&a, &b, 0.5);
        assert!(approx_eq_vec3(&at_half[0].translation, &[5.0, 10.0, 15.0], 1e-5));
        assert!(approx_eq_vec3(&at_half[0].scale, &[1.5, 1.5, 1.5], 1e-5));
    }
}
