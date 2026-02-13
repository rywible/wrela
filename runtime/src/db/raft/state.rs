#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone)]
pub struct NodeState {
    pub node_id: u64,
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub role: Role,
    pub election_deadline_tick: u64,
    pub last_heartbeat_tick: u64,
}

impl NodeState {
    pub fn new(node_id: u64) -> Self {
        Self::with_timing(node_id, 0, 10)
    }

    pub fn with_timing(node_id: u64, now_tick: u64, election_timeout_ticks: u64) -> Self {
        Self {
            node_id,
            current_term: 0,
            voted_for: None,
            role: Role::Follower,
            election_deadline_tick: now_tick.saturating_add(election_timeout_ticks.max(1)),
            last_heartbeat_tick: now_tick,
        }
    }

    pub fn election_timed_out(&self, now_tick: u64) -> bool {
        now_tick >= self.election_deadline_tick
    }

    pub fn reset_election_deadline(&mut self, now_tick: u64, election_timeout_ticks: u64) {
        self.election_deadline_tick = now_tick.saturating_add(election_timeout_ticks.max(1));
    }
}
