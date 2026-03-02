use crate::db::raft::message::LogEntry;

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
    pub commit_index: u64,
    pub log: Vec<LogEntry>,
}

impl NodeState {
    pub fn new(node_id: u64) -> Self {
        Self::with_timing(node_id, 0, 10)
    }

    pub fn with_timing(node_id: u64, now_tick: u64, election_timeout_ticks: u64) -> Self {
        let base = election_timeout_ticks.max(1);
        let jitter = (node_id
            .wrapping_add(now_tick)
            .wrapping_mul(0x9e3779b97f4a7c15_u64))
            % base;
        let election_deadline_tick = now_tick.saturating_add(base).saturating_add(jitter);
        Self {
            node_id,
            current_term: 0,
            voted_for: None,
            role: Role::Follower,
            election_deadline_tick,
            last_heartbeat_tick: now_tick,
            commit_index: 0,
            log: Vec::new(),
        }
    }

    pub fn election_timed_out(&self, now_tick: u64) -> bool {
        now_tick >= self.election_deadline_tick
    }

    /// Resets the election deadline with randomized jitter in [timeout, 2*timeout) per Raft.
    /// Jitter is derived from node_id and now_tick so it is deterministic (tests) and differs across nodes.
    pub fn reset_election_deadline(&mut self, now_tick: u64, election_timeout_ticks: u64) {
        let base = election_timeout_ticks.max(1);
        let jitter = (self
            .node_id
            .wrapping_add(now_tick)
            .wrapping_mul(0x9e3779b97f4a7c15_u64))
            % base;
        self.election_deadline_tick = now_tick.saturating_add(base).saturating_add(jitter);
    }

    pub fn last_log_index(&self) -> u64 {
        self.log.last().map(|entry| entry.index).unwrap_or(0)
    }

    pub fn last_log_term(&self) -> u64 {
        self.log.last().map(|entry| entry.term).unwrap_or(0)
    }

    pub fn log_term_at(&self, index: u64) -> Option<u64> {
        if index == 0 {
            return Some(0);
        }
        let position = (index - 1) as usize;
        self.log.get(position).map(|entry| entry.term)
    }

    pub fn truncate_log_from(&mut self, from_index: u64) {
        if from_index == 0 {
            self.log.clear();
            self.commit_index = 0;
            return;
        }
        let keep_len = (from_index - 1) as usize;
        if keep_len < self.log.len() {
            self.log.truncate(keep_len);
        }
        self.commit_index = self.commit_index.min(self.last_log_index());
    }

    pub fn append_log_entry_checked(&mut self, entry: LogEntry) -> Result<(), &'static str> {
        let expected = self.last_log_index().saturating_add(1);
        if entry.index != expected {
            return Err("non-contiguous log index");
        }
        self.log.push(entry);
        Ok(())
    }

    pub fn append_log_entry(&mut self, entry: LogEntry) {
        let _ = self.append_log_entry_checked(entry);
    }

    pub fn restore_log_contiguous(&mut self, restored: Vec<LogEntry>) {
        self.log.clear();
        for entry in restored {
            if self.append_log_entry_checked(entry).is_err() {
                break;
            }
        }
        self.commit_index = self.commit_index.min(self.last_log_index());
    }
}
