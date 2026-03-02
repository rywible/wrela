use std::array;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportLane {
    Control,
    Raft,
    Snapshot,
    Bulk,
}

impl TransportLane {
    const COUNT: usize = 4;
    const ALL: [TransportLane; Self::COUNT] = [
        TransportLane::Control,
        TransportLane::Raft,
        TransportLane::Snapshot,
        TransportLane::Bulk,
    ];

    fn as_index(self) -> usize {
        match self {
            Self::Control => 0,
            Self::Raft => 1,
            Self::Snapshot => 2,
            Self::Bulk => 3,
        }
    }
}

const LANE_SCHEDULE: [TransportLane; 7] = [
    TransportLane::Control,
    TransportLane::Control,
    TransportLane::Raft,
    TransportLane::Control,
    TransportLane::Raft,
    TransportLane::Snapshot,
    TransportLane::Bulk,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportFlowConfig {
    pub window_bytes: usize,
    pub max_frame_bytes: usize,
    pub max_queued_per_lane: usize,
}

impl TransportFlowConfig {
    pub fn new(window_bytes: usize, max_frame_bytes: usize, max_queued_per_lane: usize) -> Self {
        Self {
            window_bytes: window_bytes.max(1),
            max_frame_bytes: max_frame_bytes.max(1),
            max_queued_per_lane: max_queued_per_lane.max(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueError {
    EmptyFrame,
    FrameTooLarge {
        bytes: usize,
        max_frame_bytes: usize,
    },
    LaneSaturated {
        lane: TransportLane,
        max_queued: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckError {
    UnknownFrame { frame_id: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledFrame<T> {
    pub frame_id: u64,
    pub lane: TransportLane,
    pub bytes: usize,
    pub payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransportLaneState {
    pub pending_frames: usize,
    pub pending_bytes: usize,
    pub in_flight_frames: usize,
    pub in_flight_bytes: usize,
    pub dispatch_weight: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportSchedulerState {
    pub available_credits: usize,
    pub in_flight_bytes: usize,
    pub total_pending_frames: usize,
    pub total_pending_bytes: usize,
    pub schedule_cursor: usize,
    pub lanes: [TransportLaneState; TransportLane::COUNT],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportChaosConfig {
    pub seed: u64,
    pub drop_percent: u8,
    pub duplicate_percent: u8,
    pub delay_percent: u8,
}

impl TransportChaosConfig {
    pub fn new(seed: u64, drop_percent: u8, duplicate_percent: u8, delay_percent: u8) -> Self {
        Self {
            seed,
            drop_percent: drop_percent.min(100),
            duplicate_percent: duplicate_percent.min(100),
            delay_percent: delay_percent.min(100),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportChaosAction {
    Pass,
    Drop,
    Duplicate,
    Delay,
}

pub fn classify_chaos_action(
    config: TransportChaosConfig,
    frame_id: u64,
    lane: TransportLane,
) -> TransportChaosAction {
    let roll = chaos_roll(config.seed, frame_id, lane);
    let mut threshold = config.drop_percent as u16;
    if roll < threshold {
        return TransportChaosAction::Drop;
    }
    threshold = threshold.saturating_add(config.duplicate_percent as u16);
    if roll < threshold {
        return TransportChaosAction::Duplicate;
    }
    threshold = threshold.saturating_add(config.delay_percent as u16);
    if roll < threshold {
        return TransportChaosAction::Delay;
    }
    TransportChaosAction::Pass
}

fn chaos_roll(seed: u64, frame_id: u64, lane: TransportLane) -> u16 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    frame_id.hash(&mut hasher);
    lane.hash(&mut hasher);
    (hasher.finish() % 100) as u16
}

#[derive(Debug)]
pub struct TransportScheduler<T> {
    config: TransportFlowConfig,
    lanes: [VecDeque<QueuedFrame<T>>; TransportLane::COUNT],
    in_flight_bytes: usize,
    in_flight_frames: HashMap<u64, InFlightFrame>,
    next_frame_id: u64,
    cursor: usize,
}

impl<T> TransportScheduler<T> {
    pub fn new(config: TransportFlowConfig) -> Self {
        Self {
            config,
            lanes: array::from_fn(|_| VecDeque::new()),
            in_flight_bytes: 0,
            in_flight_frames: HashMap::new(),
            next_frame_id: 1,
            cursor: 0,
        }
    }

    pub fn enqueue(
        &mut self,
        lane: TransportLane,
        payload: T,
        bytes: usize,
    ) -> Result<u64, EnqueueError> {
        if bytes == 0 {
            return Err(EnqueueError::EmptyFrame);
        }
        if bytes > self.config.max_frame_bytes {
            return Err(EnqueueError::FrameTooLarge {
                bytes,
                max_frame_bytes: self.config.max_frame_bytes,
            });
        }

        let lane_queue = &mut self.lanes[lane.as_index()];
        if lane_queue.len() >= self.config.max_queued_per_lane {
            return Err(EnqueueError::LaneSaturated {
                lane,
                max_queued: self.config.max_queued_per_lane,
            });
        }

        let frame_id = self.next_frame_id;
        self.next_frame_id = self.next_frame_id.wrapping_add(1);
        lane_queue.push_back(QueuedFrame {
            frame_id,
            payload,
            bytes,
        });
        Ok(frame_id)
    }

    pub fn try_dispatch(&mut self) -> Option<ScheduledFrame<T>> {
        let available = self.available_credits();
        if available == 0 {
            return None;
        }

        let mut selected: Option<(usize, TransportLane)> = None;
        for offset in 0..LANE_SCHEDULE.len() {
            let schedule_index = (self.cursor + offset) % LANE_SCHEDULE.len();
            let lane = LANE_SCHEDULE[schedule_index];
            let maybe_frame = self.lanes[lane.as_index()].front();
            if let Some(frame) = maybe_frame
                && frame.bytes <= available
            {
                selected = Some((schedule_index, lane));
                break;
            }
        }

        let (schedule_index, lane) = selected?;
        self.cursor = (schedule_index + 1) % LANE_SCHEDULE.len();
        let frame = self.lanes[lane.as_index()]
            .pop_front()
            .expect("selected lane must have a frame");
        self.in_flight_bytes += frame.bytes;
        self.in_flight_frames.insert(
            frame.frame_id,
            InFlightFrame {
                lane,
                bytes: frame.bytes,
            },
        );

        Some(ScheduledFrame {
            frame_id: frame.frame_id,
            lane,
            bytes: frame.bytes,
            payload: frame.payload,
        })
    }

    pub fn ack(&mut self, frame_id: u64) -> Result<usize, AckError> {
        let bytes = self
            .in_flight_frames
            .remove(&frame_id)
            .ok_or(AckError::UnknownFrame { frame_id })?
            .bytes;
        self.in_flight_bytes = self.in_flight_bytes.saturating_sub(bytes);
        Ok(bytes)
    }

    pub fn lane_dispatch_weight(lane: TransportLane) -> usize {
        LANE_SCHEDULE
            .iter()
            .filter(|scheduled| **scheduled == lane)
            .count()
    }

    pub fn available_credits(&self) -> usize {
        self.config
            .window_bytes
            .saturating_sub(self.in_flight_bytes)
    }

    pub fn in_flight_bytes(&self) -> usize {
        self.in_flight_bytes
    }

    pub fn total_pending(&self) -> usize {
        self.lanes.iter().map(VecDeque::len).sum()
    }

    pub fn pending_in_lane(&self, lane: TransportLane) -> usize {
        self.lanes[lane.as_index()].len()
    }

    pub fn pending_bytes_in_lane(&self, lane: TransportLane) -> usize {
        self.lanes[lane.as_index()]
            .iter()
            .map(|frame| frame.bytes)
            .sum()
    }

    pub fn in_flight_in_lane(&self, lane: TransportLane) -> usize {
        self.in_flight_frames
            .values()
            .filter(|frame| frame.lane == lane)
            .count()
    }

    pub fn in_flight_bytes_in_lane(&self, lane: TransportLane) -> usize {
        self.in_flight_frames
            .values()
            .filter(|frame| frame.lane == lane)
            .map(|frame| frame.bytes)
            .sum()
    }

    pub fn state(&self) -> TransportSchedulerState {
        let mut lanes = [TransportLaneState::default(); TransportLane::COUNT];
        for lane in TransportLane::ALL {
            let lane_idx = lane.as_index();
            lanes[lane_idx] = TransportLaneState {
                pending_frames: self.pending_in_lane(lane),
                pending_bytes: self.pending_bytes_in_lane(lane),
                in_flight_frames: self.in_flight_in_lane(lane),
                in_flight_bytes: self.in_flight_bytes_in_lane(lane),
                dispatch_weight: Self::lane_dispatch_weight(lane),
            };
        }
        TransportSchedulerState {
            available_credits: self.available_credits(),
            in_flight_bytes: self.in_flight_bytes(),
            total_pending_frames: self.total_pending(),
            total_pending_bytes: lanes.iter().map(|lane| lane.pending_bytes).sum(),
            schedule_cursor: self.cursor,
            lanes,
        }
    }

    pub fn lane_state(&self, lane: TransportLane) -> TransportLaneState {
        self.state().lanes[lane.as_index()]
    }
}

#[derive(Debug)]
struct QueuedFrame<T> {
    frame_id: u64,
    payload: T,
    bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InFlightFrame {
    lane: TransportLane,
    bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_scheduler(window_bytes: usize) -> TransportScheduler<&'static str> {
        TransportScheduler::new(TransportFlowConfig::new(window_bytes, 64, 16))
    }

    #[test]
    fn dispatch_follows_deterministic_weighted_lane_order() {
        let mut scheduler = test_scheduler(128);
        scheduler
            .enqueue(TransportLane::Control, "c1", 4)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Control, "c2", 4)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Control, "c3", 4)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Raft, "r1", 4)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Raft, "r2", 4)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Snapshot, "s1", 4)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Bulk, "b1", 4)
            .expect("enqueue");

        let mut out = Vec::new();
        while let Some(frame) = scheduler.try_dispatch() {
            out.push((frame.lane, frame.payload));
            scheduler.ack(frame.frame_id).expect("ack");
        }

        assert_eq!(
            out,
            vec![
                (TransportLane::Control, "c1"),
                (TransportLane::Control, "c2"),
                (TransportLane::Raft, "r1"),
                (TransportLane::Control, "c3"),
                (TransportLane::Raft, "r2"),
                (TransportLane::Snapshot, "s1"),
                (TransportLane::Bulk, "b1"),
            ]
        );
    }

    #[test]
    fn flow_control_blocks_until_ack_restores_credits() {
        let mut scheduler = test_scheduler(10);
        scheduler
            .enqueue(TransportLane::Control, "c1", 7)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Control, "c2", 6)
            .expect("enqueue");

        let first = scheduler
            .try_dispatch()
            .expect("first frame should dispatch");
        assert_eq!(first.payload, "c1");
        assert_eq!(scheduler.in_flight_bytes(), 7);
        assert_eq!(scheduler.available_credits(), 3);
        assert!(scheduler.try_dispatch().is_none(), "insufficient credits");

        let returned = scheduler.ack(first.frame_id).expect("ack must succeed");
        assert_eq!(returned, 7);
        assert_eq!(scheduler.in_flight_bytes(), 0);
        assert_eq!(scheduler.available_credits(), 10);

        let second = scheduler
            .try_dispatch()
            .expect("second frame should dispatch after ack");
        assert_eq!(second.payload, "c2");
        scheduler.ack(second.frame_id).expect("ack");
    }

    #[test]
    fn dispatch_skips_lanes_with_head_of_line_that_exceeds_available_credits() {
        let mut scheduler = test_scheduler(10);
        scheduler
            .enqueue(TransportLane::Control, "c1", 8)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Raft, "r1", 3)
            .expect("enqueue");
        scheduler
            .enqueue(TransportLane::Snapshot, "s1", 2)
            .expect("enqueue");

        let first = scheduler.try_dispatch().expect("first dispatch");
        assert_eq!(first.payload, "c1");
        assert_eq!(scheduler.available_credits(), 2);

        let second = scheduler
            .try_dispatch()
            .expect("should dispatch snapshot frame while raft does not fit");
        assert_eq!(second.payload, "s1");
        assert_eq!(second.lane, TransportLane::Snapshot);
    }

    #[test]
    fn enqueue_validates_frame_size_and_lane_capacity() {
        let mut scheduler = TransportScheduler::new(TransportFlowConfig::new(128, 16, 1));

        let err = scheduler
            .enqueue(TransportLane::Control, "bad", 0)
            .expect_err("zero-sized frame must fail");
        assert_eq!(err, EnqueueError::EmptyFrame);

        let err = scheduler
            .enqueue(TransportLane::Control, "bad", 17)
            .expect_err("oversized frame must fail");
        assert_eq!(
            err,
            EnqueueError::FrameTooLarge {
                bytes: 17,
                max_frame_bytes: 16,
            }
        );

        scheduler
            .enqueue(TransportLane::Control, "ok", 8)
            .expect("first frame should fit");
        let err = scheduler
            .enqueue(TransportLane::Control, "full", 8)
            .expect_err("lane queue cap should be enforced");
        assert_eq!(
            err,
            EnqueueError::LaneSaturated {
                lane: TransportLane::Control,
                max_queued: 1,
            }
        );
    }

    #[test]
    fn ack_rejects_unknown_frame_and_does_not_change_credits() {
        let mut scheduler = test_scheduler(10);
        scheduler
            .enqueue(TransportLane::Control, "c1", 5)
            .expect("enqueue");
        let frame = scheduler.try_dispatch().expect("dispatch");
        assert_eq!(scheduler.available_credits(), 5);

        let err = scheduler
            .ack(frame.frame_id + 1)
            .expect_err("unknown frame");
        assert_eq!(
            err,
            AckError::UnknownFrame {
                frame_id: frame.frame_id + 1,
            }
        );
        assert_eq!(scheduler.available_credits(), 5);
        scheduler.ack(frame.frame_id).expect("ack known frame");
        assert_eq!(scheduler.available_credits(), 10);
    }

    #[test]
    fn chaos_classification_is_deterministic_for_same_seed() {
        let config = TransportChaosConfig::new(17, 20, 10, 15);
        let first = classify_chaos_action(config, 99, TransportLane::Raft);
        let second = classify_chaos_action(config, 99, TransportLane::Raft);
        assert_eq!(first, second);
    }

    #[test]
    fn chaos_classification_changes_with_seed() {
        let mut observed = std::collections::HashSet::new();
        for seed in 1..=16u64 {
            observed.insert(classify_chaos_action(
                TransportChaosConfig::new(seed, 30, 30, 30),
                42,
                TransportLane::Control,
            ));
        }
        assert!(
            observed.len() > 1,
            "seeded chaos should vary decisions across seeds"
        );
    }

    #[test]
    fn chaos_fuzz_sweep_is_bounded_and_exercises_actions() {
        let config = TransportChaosConfig::new(73, 25, 20, 20);
        let mut seen = [0u64; 4];

        for frame_id in 1..=2_000u64 {
            let lane = match frame_id % 4 {
                0 => TransportLane::Control,
                1 => TransportLane::Raft,
                2 => TransportLane::Snapshot,
                _ => TransportLane::Bulk,
            };
            match classify_chaos_action(config, frame_id, lane) {
                TransportChaosAction::Pass => seen[0] += 1,
                TransportChaosAction::Drop => seen[1] += 1,
                TransportChaosAction::Duplicate => seen[2] += 1,
                TransportChaosAction::Delay => seen[3] += 1,
            }
        }

        assert!(seen[1] > 0, "drop action should be reachable");
        assert!(seen[2] > 0, "duplicate action should be reachable");
        assert!(seen[3] > 0, "delay action should be reachable");
        assert_eq!(seen.iter().sum::<u64>(), 2_000);
    }

    #[test]
    fn lane_dispatch_weights_match_deterministic_schedule() {
        assert_eq!(
            TransportScheduler::<()>::lane_dispatch_weight(TransportLane::Control),
            3
        );
        assert_eq!(
            TransportScheduler::<()>::lane_dispatch_weight(TransportLane::Raft),
            2
        );
        assert_eq!(
            TransportScheduler::<()>::lane_dispatch_weight(TransportLane::Snapshot),
            1
        );
        assert_eq!(
            TransportScheduler::<()>::lane_dispatch_weight(TransportLane::Bulk),
            1
        );
    }

    #[test]
    fn scheduler_state_reports_pending_and_inflight_per_lane() {
        let mut scheduler = TransportScheduler::new(TransportFlowConfig::new(12, 12, 8));
        scheduler
            .enqueue(TransportLane::Control, "c1", 5)
            .expect("enqueue control");
        scheduler
            .enqueue(TransportLane::Raft, "r1", 4)
            .expect("enqueue raft");
        scheduler
            .enqueue(TransportLane::Snapshot, "s1", 3)
            .expect("enqueue snapshot");

        let control = scheduler.try_dispatch().expect("dispatch control");
        assert_eq!(control.lane, TransportLane::Control);

        let state = scheduler.state();
        assert_eq!(state.available_credits, 7);
        assert_eq!(state.in_flight_bytes, 5);
        assert_eq!(state.total_pending_frames, 2);
        assert_eq!(state.total_pending_bytes, 7);
        assert_eq!(
            state.lanes[TransportLane::Control.as_index()],
            TransportLaneState {
                pending_frames: 0,
                pending_bytes: 0,
                in_flight_frames: 1,
                in_flight_bytes: 5,
                dispatch_weight: 3,
            }
        );
        assert_eq!(
            state.lanes[TransportLane::Raft.as_index()],
            TransportLaneState {
                pending_frames: 1,
                pending_bytes: 4,
                in_flight_frames: 0,
                in_flight_bytes: 0,
                dispatch_weight: 2,
            }
        );
        assert_eq!(
            state.lanes[TransportLane::Snapshot.as_index()],
            TransportLaneState {
                pending_frames: 1,
                pending_bytes: 3,
                in_flight_frames: 0,
                in_flight_bytes: 0,
                dispatch_weight: 1,
            }
        );
        assert_eq!(
            state.lanes[TransportLane::Bulk.as_index()],
            TransportLaneState {
                pending_frames: 0,
                pending_bytes: 0,
                in_flight_frames: 0,
                in_flight_bytes: 0,
                dispatch_weight: 1,
            }
        );
    }
}
