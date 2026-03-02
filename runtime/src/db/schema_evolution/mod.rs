use std::collections::BTreeMap;
use std::fmt::Write;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvolutionPhase {
    Draft,
    Backfill,
    Validate,
    Cutover,
    Complete,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaEvolution {
    phase: EvolutionPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    InvalidTransition {
        from: EvolutionPhase,
        to: EvolutionPhase,
    },
}

impl SchemaEvolution {
    pub fn new() -> Self {
        Self {
            phase: EvolutionPhase::Draft,
        }
    }

    pub fn phase(&self) -> EvolutionPhase {
        self.phase
    }

    pub fn transition_to(&mut self, next: EvolutionPhase) -> Result<(), TransitionError> {
        if self.is_valid_transition(next) {
            self.phase = next;
            return Ok(());
        }

        Err(TransitionError::InvalidTransition {
            from: self.phase,
            to: next,
        })
    }

    fn is_valid_transition(&self, next: EvolutionPhase) -> bool {
        match (self.phase, next) {
            (EvolutionPhase::Draft, EvolutionPhase::Backfill)
            | (EvolutionPhase::Backfill, EvolutionPhase::Validate)
            | (EvolutionPhase::Validate, EvolutionPhase::Cutover)
            | (EvolutionPhase::Cutover, EvolutionPhase::Complete) => true,
            (EvolutionPhase::Draft, EvolutionPhase::Aborted)
            | (EvolutionPhase::Backfill, EvolutionPhase::Aborted)
            | (EvolutionPhase::Validate, EvolutionPhase::Aborted)
            | (EvolutionPhase::Cutover, EvolutionPhase::Aborted) => true,
            _ => false,
        }
    }
}

impl Default for SchemaEvolution {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackfillProgress {
    total_rows: u64,
    completed_rows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressError {
    NonMonotonic { current: u64, attempted: u64 },
    ExceedsTotal { total: u64, attempted: u64 },
}

impl BackfillProgress {
    pub fn new(total_rows: u64) -> Self {
        Self {
            total_rows,
            completed_rows: 0,
        }
    }

    pub fn total_rows(&self) -> u64 {
        self.total_rows
    }

    pub fn completed_rows(&self) -> u64 {
        self.completed_rows
    }

    pub fn is_complete(&self) -> bool {
        self.completed_rows == self.total_rows
    }

    pub fn record_completed_rows(&mut self, completed_rows: u64) -> Result<(), ProgressError> {
        if completed_rows < self.completed_rows {
            return Err(ProgressError::NonMonotonic {
                current: self.completed_rows,
                attempted: completed_rows,
            });
        }
        if completed_rows > self.total_rows {
            return Err(ProgressError::ExceedsTotal {
                total: self.total_rows,
                attempted: completed_rows,
            });
        }

        self.completed_rows = completed_rows;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReindexWorkerConfig {
    max_batch_rows: u64,
    max_in_flight: u64,
}

impl ReindexWorkerConfig {
    pub fn new(max_batch_rows: u64, max_in_flight: u64) -> Self {
        Self {
            max_batch_rows,
            max_in_flight,
        }
    }

    pub fn max_batch_rows(&self) -> u64 {
        self.max_batch_rows
    }

    pub fn max_in_flight(&self) -> u64 {
        self.max_in_flight
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResumeToken(String);

impl ResumeToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReindexWorkerState {
    progress_cursor: u64,
    in_flight_rows: u64,
    resume_token: ResumeToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerStateError {
    InvalidResumeTokenFormat,
    InvalidResumeTokenCursor,
    InvalidResumeTokenInFlight,
    InvalidAckCount {
        in_flight_rows: u64,
        acked_rows: u64,
    },
}

impl ReindexWorkerState {
    pub fn new() -> Self {
        Self::from_parts(0, 0)
    }

    pub fn from_resume_token(token: ResumeToken) -> Result<Self, WorkerStateError> {
        let Some((cursor_raw, in_flight_raw)) = token.as_str().split_once(':') else {
            return Err(WorkerStateError::InvalidResumeTokenFormat);
        };

        let progress_cursor = cursor_raw
            .parse::<u64>()
            .map_err(|_| WorkerStateError::InvalidResumeTokenCursor)?;
        let in_flight_rows = in_flight_raw
            .parse::<u64>()
            .map_err(|_| WorkerStateError::InvalidResumeTokenInFlight)?;

        Ok(Self::from_parts(progress_cursor, in_flight_rows))
    }

    pub fn progress_cursor(&self) -> u64 {
        self.progress_cursor
    }

    pub fn in_flight_rows(&self) -> u64 {
        self.in_flight_rows
    }

    pub fn resume_token(&self) -> &ResumeToken {
        &self.resume_token
    }

    pub fn ack_completed_rows(&mut self, acked_rows: u64) -> Result<(), WorkerStateError> {
        if acked_rows > self.in_flight_rows {
            return Err(WorkerStateError::InvalidAckCount {
                in_flight_rows: self.in_flight_rows,
                acked_rows,
            });
        }
        self.in_flight_rows -= acked_rows;
        self.resume_token = Self::encode_token(self.progress_cursor, self.in_flight_rows);
        Ok(())
    }

    fn advance(&mut self, assigned_rows: u64) {
        self.progress_cursor += assigned_rows;
        self.in_flight_rows += assigned_rows;
        self.resume_token = Self::encode_token(self.progress_cursor, self.in_flight_rows);
    }

    fn from_parts(progress_cursor: u64, in_flight_rows: u64) -> Self {
        Self {
            progress_cursor,
            in_flight_rows,
            resume_token: Self::encode_token(progress_cursor, in_flight_rows),
        }
    }

    fn encode_token(progress_cursor: u64, in_flight_rows: u64) -> ResumeToken {
        ResumeToken(format!("{progress_cursor}:{in_flight_rows}"))
    }
}

impl Default for ReindexWorkerState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReindexStep {
    assigned_rows: u64,
    next_cursor: u64,
    in_flight_rows: u64,
}

impl ReindexStep {
    pub fn assigned_rows(&self) -> u64 {
        self.assigned_rows
    }

    pub fn next_cursor(&self) -> u64 {
        self.next_cursor
    }

    pub fn in_flight_rows(&self) -> u64 {
        self.in_flight_rows
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReindexWorker {
    config: ReindexWorkerConfig,
}

impl ReindexWorker {
    pub fn new(config: ReindexWorkerConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> ReindexWorkerConfig {
        self.config
    }

    pub fn step(&self, state: &mut ReindexWorkerState, total_rows: u64) -> ReindexStep {
        let remaining_rows = total_rows.saturating_sub(state.progress_cursor);
        let available_in_flight = self
            .config
            .max_in_flight
            .saturating_sub(state.in_flight_rows);
        let assigned_rows = remaining_rows
            .min(self.config.max_batch_rows)
            .min(available_in_flight);
        state.advance(assigned_rows);

        ReindexStep {
            assigned_rows,
            next_cursor: state.progress_cursor,
            in_flight_rows: state.in_flight_rows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ValidationObservation {
    SourceRowMissing {
        row_key: String,
    },
    ValueHashMismatch {
        row_key: String,
        expected_hash: String,
        actual_hash: String,
    },
    UnexpectedIndexEntry {
        index_key: String,
    },
    IndexEntryMissing {
        index_key: String,
        row_key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ValidationMismatch {
    MissingRow {
        row_key: String,
    },
    DivergentValue {
        row_key: String,
        expected_hash: String,
        actual_hash: String,
    },
    ExtraIndexEntry {
        index_key: String,
    },
    MissingIndexEntry {
        index_key: String,
        row_key: String,
    },
}

pub fn classify_mismatch(observation: ValidationObservation) -> ValidationMismatch {
    match observation {
        ValidationObservation::SourceRowMissing { row_key } => {
            ValidationMismatch::MissingRow { row_key }
        }
        ValidationObservation::ValueHashMismatch {
            row_key,
            expected_hash,
            actual_hash,
        } => ValidationMismatch::DivergentValue {
            row_key,
            expected_hash,
            actual_hash,
        },
        ValidationObservation::UnexpectedIndexEntry { index_key } => {
            ValidationMismatch::ExtraIndexEntry { index_key }
        }
        ValidationObservation::IndexEntryMissing { index_key, row_key } => {
            ValidationMismatch::MissingIndexEntry { index_key, row_key }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RemediationAction {
    BackfillRow {
        row_key: String,
    },
    RepairRowValue {
        row_key: String,
        expected_hash: String,
    },
    RemoveIndexEntry {
        index_key: String,
    },
    UpsertIndexEntry {
        index_key: String,
        row_key: String,
    },
}

pub fn plan_remediation_actions(mismatches: &[ValidationMismatch]) -> Vec<RemediationAction> {
    let mut actions: Vec<_> = mismatches
        .iter()
        .map(|mismatch| match mismatch {
            ValidationMismatch::MissingRow { row_key } => RemediationAction::BackfillRow {
                row_key: row_key.clone(),
            },
            ValidationMismatch::DivergentValue {
                row_key,
                expected_hash,
                ..
            } => RemediationAction::RepairRowValue {
                row_key: row_key.clone(),
                expected_hash: expected_hash.clone(),
            },
            ValidationMismatch::ExtraIndexEntry { index_key } => {
                RemediationAction::RemoveIndexEntry {
                    index_key: index_key.clone(),
                }
            }
            ValidationMismatch::MissingIndexEntry { index_key, row_key } => {
                RemediationAction::UpsertIndexEntry {
                    index_key: index_key.clone(),
                    row_key: row_key.clone(),
                }
            }
        })
        .collect();
    actions.sort();
    actions.dedup();
    actions
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CutoverReadinessReason {
    BackfillIncomplete { remaining_rows: u64 },
    ReindexWorkInFlight { in_flight_rows: u64 },
    ValidationMismatchesPending { mismatch_count: usize },
    RemediationPending { action_count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CutoverReadiness {
    Ready,
    NotReady {
        reasons: Vec<CutoverReadinessReason>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutoverReadinessGateInput {
    pub backfill_progress: BackfillProgress,
    pub reindex_state: ReindexWorkerState,
    pub pending_mismatches: Vec<ValidationMismatch>,
    pub pending_actions: Vec<RemediationAction>,
}

pub fn evaluate_cutover_readiness(input: &CutoverReadinessGateInput) -> CutoverReadiness {
    let mut reasons = Vec::new();
    if !input.backfill_progress.is_complete() {
        reasons.push(CutoverReadinessReason::BackfillIncomplete {
            remaining_rows: input
                .backfill_progress
                .total_rows()
                .saturating_sub(input.backfill_progress.completed_rows()),
        });
    }
    if input.reindex_state.in_flight_rows() > 0 {
        reasons.push(CutoverReadinessReason::ReindexWorkInFlight {
            in_flight_rows: input.reindex_state.in_flight_rows(),
        });
    }
    if !input.pending_mismatches.is_empty() {
        reasons.push(CutoverReadinessReason::ValidationMismatchesPending {
            mismatch_count: input.pending_mismatches.len(),
        });
    }
    if !input.pending_actions.is_empty() {
        reasons.push(CutoverReadinessReason::RemediationPending {
            action_count: input.pending_actions.len(),
        });
    }

    if reasons.is_empty() {
        CutoverReadiness::Ready
    } else {
        CutoverReadiness::NotReady { reasons }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaJobId(String);

impl SchemaJobId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn from_request(request: &SchemaJobRequest) -> Result<Self, PersistenceError> {
        let canonical = serde_json::to_vec(request)
            .map_err(|err| PersistenceError::Serialization(err.to_string()))?;
        let mut hasher = Sha256::new();
        hasher.update(canonical);
        let digest = hasher.finalize();

        let mut id = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(id, "{byte:02x}");
        }
        Ok(Self(id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaJobRequest {
    pub schema_name: String,
    pub from_version: u64,
    pub to_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaJobStatus {
    Draft,
    Backfill,
    Validate,
    Cutover,
    Complete,
    Canceled,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleEventKind {
    Created,
    PhaseChanged {
        from: EvolutionPhase,
        to: EvolutionPhase,
    },
    Canceled,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub sequence: u64,
    pub kind: LifecycleEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum TerminalState {
    Canceled,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SchemaJobState {
    request: SchemaJobRequest,
    evolution: SchemaEvolution,
    terminal_state: Option<TerminalState>,
    lifecycle_events: Vec<LifecycleEvent>,
    next_sequence: u64,
}

impl SchemaJobState {
    fn new(request: SchemaJobRequest) -> Self {
        let mut state = Self {
            request,
            evolution: SchemaEvolution::new(),
            terminal_state: None,
            lifecycle_events: Vec::new(),
            next_sequence: 0,
        };
        state.record_event(LifecycleEventKind::Created);
        state
    }

    fn status(&self) -> SchemaJobStatus {
        match self.terminal_state {
            Some(TerminalState::Canceled) => SchemaJobStatus::Canceled,
            Some(TerminalState::RolledBack) => SchemaJobStatus::RolledBack,
            None => match self.evolution.phase() {
                EvolutionPhase::Draft => SchemaJobStatus::Draft,
                EvolutionPhase::Backfill => SchemaJobStatus::Backfill,
                EvolutionPhase::Validate => SchemaJobStatus::Validate,
                EvolutionPhase::Cutover => SchemaJobStatus::Cutover,
                EvolutionPhase::Complete => SchemaJobStatus::Complete,
                EvolutionPhase::Aborted => SchemaJobStatus::Canceled,
            },
        }
    }

    fn phase(&self) -> EvolutionPhase {
        self.evolution.phase()
    }

    fn record_event(&mut self, kind: LifecycleEventKind) {
        self.lifecycle_events.push(LifecycleEvent {
            sequence: self.next_sequence,
            kind,
        });
        self.next_sequence += 1;
    }

    fn transition_to(&mut self, next: EvolutionPhase) -> Result<(), TransitionError> {
        let from = self.evolution.phase();
        self.evolution.transition_to(next)?;
        self.record_event(LifecycleEventKind::PhaseChanged { from, to: next });
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobError {
    JobNotFound {
        job_id: SchemaJobId,
    },
    IllegalCancel {
        status: SchemaJobStatus,
    },
    IllegalRollback {
        status: SchemaJobStatus,
    },
    IllegalTransition {
        from: EvolutionPhase,
        to: EvolutionPhase,
    },
    Persistence(PersistenceError),
}

impl From<PersistenceError> for JobError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceError {
    Serialization(String),
    Deserialization(String),
    DuplicateJobId { job_id: SchemaJobId },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaJobStore {
    jobs: BTreeMap<SchemaJobId, SchemaJobState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CanonicalStore {
    jobs: Vec<CanonicalJobEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CanonicalJobEntry {
    job_id: SchemaJobId,
    state: SchemaJobState,
}

impl SchemaJobStore {
    pub fn new() -> Self {
        Self {
            jobs: BTreeMap::new(),
        }
    }

    pub fn create_job(&mut self, request: SchemaJobRequest) -> Result<SchemaJobId, JobError> {
        let job_id = SchemaJobId::from_request(&request)?;
        self.jobs
            .entry(job_id.clone())
            .or_insert_with(|| SchemaJobState::new(request));
        Ok(job_id)
    }

    pub fn status(&self, job_id: &SchemaJobId) -> Option<SchemaJobStatus> {
        self.jobs.get(job_id).map(SchemaJobState::status)
    }

    pub fn cancel_job(&mut self, job_id: &SchemaJobId) -> Result<(), JobError> {
        let Some(job) = self.jobs.get_mut(job_id) else {
            return Err(JobError::JobNotFound {
                job_id: job_id.clone(),
            });
        };

        let status = job.status();
        if !matches!(
            status,
            SchemaJobStatus::Draft
                | SchemaJobStatus::Backfill
                | SchemaJobStatus::Validate
                | SchemaJobStatus::Cutover
        ) {
            return Err(JobError::IllegalCancel { status });
        }

        job.transition_to(EvolutionPhase::Aborted)
            .map_err(|err| match err {
                TransitionError::InvalidTransition { from, to } => {
                    JobError::IllegalTransition { from, to }
                }
            })?;
        job.terminal_state = Some(TerminalState::Canceled);
        job.record_event(LifecycleEventKind::Canceled);
        Ok(())
    }

    pub fn rollback_job(&mut self, job_id: &SchemaJobId) -> Result<(), JobError> {
        let Some(job) = self.jobs.get_mut(job_id) else {
            return Err(JobError::JobNotFound {
                job_id: job_id.clone(),
            });
        };

        let status = job.status();
        if !matches!(status, SchemaJobStatus::Validate | SchemaJobStatus::Cutover) {
            return Err(JobError::IllegalRollback { status });
        }

        job.transition_to(EvolutionPhase::Aborted)
            .map_err(|err| match err {
                TransitionError::InvalidTransition { from, to } => {
                    JobError::IllegalTransition { from, to }
                }
            })?;
        job.terminal_state = Some(TerminalState::RolledBack);
        job.record_event(LifecycleEventKind::RolledBack);
        Ok(())
    }

    pub fn transition_job_phase(
        &mut self,
        job_id: &SchemaJobId,
        next: EvolutionPhase,
    ) -> Result<(), JobError> {
        let Some(job) = self.jobs.get_mut(job_id) else {
            return Err(JobError::JobNotFound {
                job_id: job_id.clone(),
            });
        };

        if job.terminal_state.is_some() {
            return Err(JobError::IllegalTransition {
                from: job.phase(),
                to: next,
            });
        }

        job.transition_to(next).map_err(|err| match err {
            TransitionError::InvalidTransition { from, to } => {
                JobError::IllegalTransition { from, to }
            }
        })
    }

    pub fn events(&self, job_id: &SchemaJobId) -> Option<&[LifecycleEvent]> {
        self.jobs
            .get(job_id)
            .map(|job| job.lifecycle_events.as_slice())
    }

    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, PersistenceError> {
        let canonical = CanonicalStore {
            jobs: self
                .jobs
                .iter()
                .map(|(job_id, state)| CanonicalJobEntry {
                    job_id: job_id.clone(),
                    state: state.clone(),
                })
                .collect(),
        };

        serde_json::to_vec(&canonical)
            .map_err(|err| PersistenceError::Serialization(err.to_string()))
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, PersistenceError> {
        let canonical: CanonicalStore = serde_json::from_slice(bytes)
            .map_err(|err| PersistenceError::Deserialization(err.to_string()))?;

        let mut jobs = BTreeMap::new();
        for entry in canonical.jobs {
            let previous = jobs.insert(entry.job_id.clone(), entry.state);
            if previous.is_some() {
                return Err(PersistenceError::DuplicateJobId {
                    job_id: entry.job_id,
                });
            }
        }

        Ok(Self { jobs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(name: &str, from: u64, to: u64) -> SchemaJobRequest {
        SchemaJobRequest {
            schema_name: name.to_string(),
            from_version: from,
            to_version: to,
        }
    }

    #[test]
    fn legal_transitions_follow_contract() {
        let mut evolution = SchemaEvolution::new();
        assert_eq!(evolution.phase(), EvolutionPhase::Draft);

        evolution
            .transition_to(EvolutionPhase::Backfill)
            .expect("draft -> backfill must be legal");
        assert_eq!(evolution.phase(), EvolutionPhase::Backfill);

        evolution
            .transition_to(EvolutionPhase::Validate)
            .expect("backfill -> validate must be legal");
        assert_eq!(evolution.phase(), EvolutionPhase::Validate);

        evolution
            .transition_to(EvolutionPhase::Cutover)
            .expect("validate -> cutover must be legal");
        assert_eq!(evolution.phase(), EvolutionPhase::Cutover);

        evolution
            .transition_to(EvolutionPhase::Complete)
            .expect("cutover -> complete must be legal");
        assert_eq!(evolution.phase(), EvolutionPhase::Complete);
    }

    #[test]
    fn illegal_transitions_return_typed_error() {
        let mut evolution = SchemaEvolution::new();

        let err = evolution
            .transition_to(EvolutionPhase::Validate)
            .expect_err("draft -> validate must be illegal");
        assert_eq!(
            err,
            TransitionError::InvalidTransition {
                from: EvolutionPhase::Draft,
                to: EvolutionPhase::Validate,
            }
        );
        assert_eq!(evolution.phase(), EvolutionPhase::Draft);
    }

    #[test]
    fn abort_is_allowed_before_completion_and_is_terminal() {
        let mut evolution = SchemaEvolution::new();
        evolution
            .transition_to(EvolutionPhase::Backfill)
            .expect("draft -> backfill must be legal");
        evolution
            .transition_to(EvolutionPhase::Aborted)
            .expect("backfill -> aborted must be legal");
        assert_eq!(evolution.phase(), EvolutionPhase::Aborted);

        let err = evolution
            .transition_to(EvolutionPhase::Validate)
            .expect_err("aborted must be terminal");
        assert_eq!(
            err,
            TransitionError::InvalidTransition {
                from: EvolutionPhase::Aborted,
                to: EvolutionPhase::Validate,
            }
        );
    }

    #[test]
    fn abort_after_complete_is_rejected() {
        let mut evolution = SchemaEvolution::new();
        evolution
            .transition_to(EvolutionPhase::Backfill)
            .expect("draft -> backfill must be legal");
        evolution
            .transition_to(EvolutionPhase::Validate)
            .expect("backfill -> validate must be legal");
        evolution
            .transition_to(EvolutionPhase::Cutover)
            .expect("validate -> cutover must be legal");
        evolution
            .transition_to(EvolutionPhase::Complete)
            .expect("cutover -> complete must be legal");

        let err = evolution
            .transition_to(EvolutionPhase::Aborted)
            .expect_err("complete must be terminal");
        assert_eq!(
            err,
            TransitionError::InvalidTransition {
                from: EvolutionPhase::Complete,
                to: EvolutionPhase::Aborted,
            }
        );
    }

    #[test]
    fn backfill_progress_is_monotonic_and_completion_is_exact() {
        let mut progress = BackfillProgress::new(10);
        assert_eq!(progress.total_rows(), 10);
        assert_eq!(progress.completed_rows(), 0);
        assert!(!progress.is_complete());

        progress
            .record_completed_rows(4)
            .expect("progress can move forward");
        assert_eq!(progress.completed_rows(), 4);
        assert!(!progress.is_complete());

        let err = progress
            .record_completed_rows(3)
            .expect_err("progress cannot go backward");
        assert_eq!(
            err,
            ProgressError::NonMonotonic {
                current: 4,
                attempted: 3,
            }
        );

        let err = progress
            .record_completed_rows(11)
            .expect_err("progress cannot exceed total");
        assert_eq!(
            err,
            ProgressError::ExceedsTotal {
                total: 10,
                attempted: 11,
            }
        );

        progress
            .record_completed_rows(10)
            .expect("progress can reach total");
        assert!(progress.is_complete());
    }

    #[test]
    fn zero_total_backfill_is_immediately_complete() {
        let mut progress = BackfillProgress::new(0);
        assert!(progress.is_complete());
        progress
            .record_completed_rows(0)
            .expect("zero remains valid and complete");
        assert!(progress.is_complete());
    }

    #[test]
    fn create_job_returns_deterministic_id_and_is_idempotent() {
        let mut store = SchemaJobStore::new();
        let req = request("users", 1, 2);

        let id_a = store
            .create_job(req.clone())
            .expect("first create should work");
        let id_b = store.create_job(req).expect("recreate should work");
        assert_eq!(id_a, id_b);

        let events = store.events(&id_a).expect("job should exist");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, LifecycleEventKind::Created);
        assert_eq!(store.status(&id_a), Some(SchemaJobStatus::Draft));
    }

    #[test]
    fn crash_resume_roundtrip_is_stable_and_deterministic() {
        let mut store_a = SchemaJobStore::new();
        let id_users = store_a
            .create_job(request("users", 1, 2))
            .expect("create users job");
        store_a
            .transition_job_phase(&id_users, EvolutionPhase::Backfill)
            .expect("advance users job");
        store_a
            .transition_job_phase(&id_users, EvolutionPhase::Validate)
            .expect("advance users job");
        store_a.rollback_job(&id_users).expect("rollback users job");

        let id_accounts = store_a
            .create_job(request("accounts", 7, 8))
            .expect("create accounts job");
        store_a
            .transition_job_phase(&id_accounts, EvolutionPhase::Backfill)
            .expect("advance accounts job");
        store_a
            .cancel_job(&id_accounts)
            .expect("cancel accounts job");

        let bytes_a = store_a
            .to_canonical_bytes()
            .expect("serialize canonical bytes");
        let restored =
            SchemaJobStore::from_canonical_bytes(&bytes_a).expect("deserialize canonical bytes");
        let bytes_restored = restored
            .to_canonical_bytes()
            .expect("reserialize canonical bytes");

        assert_eq!(bytes_a, bytes_restored);
        assert_eq!(
            restored.status(&id_users),
            Some(SchemaJobStatus::RolledBack)
        );
        assert_eq!(
            restored.status(&id_accounts),
            Some(SchemaJobStatus::Canceled)
        );

        let mut store_b = SchemaJobStore::new();
        let id_accounts_b = store_b
            .create_job(request("accounts", 7, 8))
            .expect("create accounts job in different order");
        store_b
            .transition_job_phase(&id_accounts_b, EvolutionPhase::Backfill)
            .expect("advance accounts job");
        store_b
            .cancel_job(&id_accounts_b)
            .expect("cancel accounts job");

        let id_users_b = store_b
            .create_job(request("users", 1, 2))
            .expect("create users job in different order");
        store_b
            .transition_job_phase(&id_users_b, EvolutionPhase::Backfill)
            .expect("advance users job");
        store_b
            .transition_job_phase(&id_users_b, EvolutionPhase::Validate)
            .expect("advance users job");
        store_b
            .rollback_job(&id_users_b)
            .expect("rollback users job");

        let bytes_b = store_b
            .to_canonical_bytes()
            .expect("serialize canonical bytes for comparison");
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn cancel_transitions_enforce_legal_and_illegal_states() {
        let mut store = SchemaJobStore::new();
        let id = store
            .create_job(request("orders", 11, 12))
            .expect("create job");

        store.cancel_job(&id).expect("cancel from draft is legal");
        assert_eq!(store.status(&id), Some(SchemaJobStatus::Canceled));

        let events = store.events(&id).expect("events must exist");
        assert_eq!(events[0].kind, LifecycleEventKind::Created);
        assert_eq!(
            events[1].kind,
            LifecycleEventKind::PhaseChanged {
                from: EvolutionPhase::Draft,
                to: EvolutionPhase::Aborted,
            }
        );
        assert_eq!(events[2].kind, LifecycleEventKind::Canceled);

        let err = store
            .cancel_job(&id)
            .expect_err("canceling canceled job must fail");
        assert_eq!(
            err,
            JobError::IllegalCancel {
                status: SchemaJobStatus::Canceled
            }
        );

        let complete_id = store
            .create_job(request("payments", 1, 2))
            .expect("create second job");
        store
            .transition_job_phase(&complete_id, EvolutionPhase::Backfill)
            .expect("draft -> backfill");
        store
            .transition_job_phase(&complete_id, EvolutionPhase::Validate)
            .expect("backfill -> validate");
        store
            .transition_job_phase(&complete_id, EvolutionPhase::Cutover)
            .expect("validate -> cutover");
        store
            .transition_job_phase(&complete_id, EvolutionPhase::Complete)
            .expect("cutover -> complete");

        let err = store
            .cancel_job(&complete_id)
            .expect_err("canceling complete job must fail");
        assert_eq!(
            err,
            JobError::IllegalCancel {
                status: SchemaJobStatus::Complete
            }
        );
    }

    #[test]
    fn rollback_transitions_enforce_legal_and_illegal_states() {
        let mut store = SchemaJobStore::new();
        let id = store
            .create_job(request("invoices", 2, 3))
            .expect("create job");

        let err = store
            .rollback_job(&id)
            .expect_err("rollback from draft must fail");
        assert_eq!(
            err,
            JobError::IllegalRollback {
                status: SchemaJobStatus::Draft
            }
        );

        store
            .transition_job_phase(&id, EvolutionPhase::Backfill)
            .expect("draft -> backfill");
        store
            .transition_job_phase(&id, EvolutionPhase::Validate)
            .expect("backfill -> validate");
        store
            .rollback_job(&id)
            .expect("rollback from validate must be legal");
        assert_eq!(store.status(&id), Some(SchemaJobStatus::RolledBack));

        let err = store
            .rollback_job(&id)
            .expect_err("rollbacking rolled back job must fail");
        assert_eq!(
            err,
            JobError::IllegalRollback {
                status: SchemaJobStatus::RolledBack
            }
        );
    }

    #[test]
    fn status_lookup_is_stable_across_lifecycle() {
        let mut store = SchemaJobStore::new();
        let id = store
            .create_job(request("audit", 4, 5))
            .expect("create job");

        assert_eq!(store.status(&id), Some(SchemaJobStatus::Draft));

        store
            .transition_job_phase(&id, EvolutionPhase::Backfill)
            .expect("draft -> backfill");
        assert_eq!(store.status(&id), Some(SchemaJobStatus::Backfill));

        store
            .transition_job_phase(&id, EvolutionPhase::Validate)
            .expect("backfill -> validate");
        assert_eq!(store.status(&id), Some(SchemaJobStatus::Validate));

        store
            .transition_job_phase(&id, EvolutionPhase::Cutover)
            .expect("validate -> cutover");
        assert_eq!(store.status(&id), Some(SchemaJobStatus::Cutover));

        store
            .transition_job_phase(&id, EvolutionPhase::Complete)
            .expect("cutover -> complete");
        assert_eq!(store.status(&id), Some(SchemaJobStatus::Complete));
    }

    #[test]
    fn reindex_worker_step_is_bounded_and_resumable() {
        let worker = ReindexWorker::new(ReindexWorkerConfig::new(4, 6));
        let mut state = ReindexWorkerState::new();
        let total_rows = 11;

        let step_1 = worker.step(&mut state, total_rows);
        assert_eq!(step_1.assigned_rows(), 4);
        assert_eq!(step_1.next_cursor(), 4);
        assert_eq!(step_1.in_flight_rows(), 4);
        assert!(step_1.assigned_rows() <= worker.config().max_batch_rows());
        assert!(step_1.in_flight_rows() <= worker.config().max_in_flight());

        state
            .ack_completed_rows(2)
            .expect("acking less than in-flight should succeed");
        assert_eq!(state.in_flight_rows(), 2);

        let step_2 = worker.step(&mut state, total_rows);
        assert_eq!(step_2.assigned_rows(), 4);
        assert_eq!(step_2.next_cursor(), 8);
        assert_eq!(step_2.in_flight_rows(), 6);
        assert!(step_2.assigned_rows() <= worker.config().max_batch_rows());
        assert!(step_2.in_flight_rows() <= worker.config().max_in_flight());

        let resumed_token = state.resume_token().clone();
        let mut resumed = ReindexWorkerState::from_resume_token(resumed_token)
            .expect("token generated by state should always decode");
        assert_eq!(resumed.progress_cursor(), state.progress_cursor());
        assert_eq!(resumed.in_flight_rows(), state.in_flight_rows());

        resumed
            .ack_completed_rows(6)
            .expect("all in-flight rows can be acked");
        let step_3 = worker.step(&mut resumed, total_rows);
        assert_eq!(step_3.assigned_rows(), 3);
        assert_eq!(step_3.next_cursor(), 11);
        assert_eq!(step_3.in_flight_rows(), 3);
        assert!(step_3.assigned_rows() <= worker.config().max_batch_rows());
        assert!(step_3.in_flight_rows() <= worker.config().max_in_flight());

        let step_4 = worker.step(&mut resumed, total_rows);
        assert_eq!(step_4.assigned_rows(), 0);
        assert_eq!(step_4.next_cursor(), 11);
        assert_eq!(step_4.in_flight_rows(), 3);
        assert!(step_4.assigned_rows() <= worker.config().max_batch_rows());
        assert!(step_4.in_flight_rows() <= worker.config().max_in_flight());
    }

    #[test]
    fn mismatch_classification_and_remediation_plan_are_deterministic() {
        let observations = vec![
            ValidationObservation::UnexpectedIndexEntry {
                index_key: "idx/orders/old".to_string(),
            },
            ValidationObservation::SourceRowMissing {
                row_key: "orders/42".to_string(),
            },
            ValidationObservation::ValueHashMismatch {
                row_key: "orders/24".to_string(),
                expected_hash: "abc".to_string(),
                actual_hash: "def".to_string(),
            },
            ValidationObservation::IndexEntryMissing {
                index_key: "idx/orders/new".to_string(),
                row_key: "orders/42".to_string(),
            },
            ValidationObservation::SourceRowMissing {
                row_key: "orders/42".to_string(),
            },
        ];

        let classified: Vec<_> = observations
            .iter()
            .cloned()
            .map(classify_mismatch)
            .collect();
        assert_eq!(
            classified[0],
            ValidationMismatch::ExtraIndexEntry {
                index_key: "idx/orders/old".to_string()
            }
        );
        assert_eq!(
            classified[1],
            ValidationMismatch::MissingRow {
                row_key: "orders/42".to_string()
            }
        );
        assert_eq!(
            classified[2],
            ValidationMismatch::DivergentValue {
                row_key: "orders/24".to_string(),
                expected_hash: "abc".to_string(),
                actual_hash: "def".to_string(),
            }
        );
        assert_eq!(
            classified[3],
            ValidationMismatch::MissingIndexEntry {
                index_key: "idx/orders/new".to_string(),
                row_key: "orders/42".to_string(),
            }
        );

        let mut reversed = classified.clone();
        reversed.reverse();

        let planned = plan_remediation_actions(&classified);
        let planned_reversed = plan_remediation_actions(&reversed);
        assert_eq!(planned, planned_reversed);
        assert_eq!(
            planned,
            vec![
                RemediationAction::BackfillRow {
                    row_key: "orders/42".to_string(),
                },
                RemediationAction::RepairRowValue {
                    row_key: "orders/24".to_string(),
                    expected_hash: "abc".to_string(),
                },
                RemediationAction::RemoveIndexEntry {
                    index_key: "idx/orders/old".to_string(),
                },
                RemediationAction::UpsertIndexEntry {
                    index_key: "idx/orders/new".to_string(),
                    row_key: "orders/42".to_string(),
                },
            ]
        );
    }

    #[test]
    fn cutover_readiness_gate_returns_typed_reasons() {
        let mut backfill = BackfillProgress::new(10);
        backfill
            .record_completed_rows(7)
            .expect("progress update should succeed");

        let input = CutoverReadinessGateInput {
            backfill_progress: backfill,
            reindex_state: ReindexWorkerState::from_resume_token(ResumeToken("7:2".to_string()))
                .expect("token should parse"),
            pending_mismatches: vec![ValidationMismatch::MissingRow {
                row_key: "orders/42".to_string(),
            }],
            pending_actions: vec![RemediationAction::BackfillRow {
                row_key: "orders/42".to_string(),
            }],
        };

        assert_eq!(
            evaluate_cutover_readiness(&input),
            CutoverReadiness::NotReady {
                reasons: vec![
                    CutoverReadinessReason::BackfillIncomplete { remaining_rows: 3 },
                    CutoverReadinessReason::ReindexWorkInFlight { in_flight_rows: 2 },
                    CutoverReadinessReason::ValidationMismatchesPending { mismatch_count: 1 },
                    CutoverReadinessReason::RemediationPending { action_count: 1 },
                ],
            }
        );

        let mut complete_backfill = BackfillProgress::new(3);
        complete_backfill
            .record_completed_rows(3)
            .expect("progress update should succeed");
        let ready_input = CutoverReadinessGateInput {
            backfill_progress: complete_backfill,
            reindex_state: ReindexWorkerState::new(),
            pending_mismatches: Vec::new(),
            pending_actions: Vec::new(),
        };
        assert_eq!(
            evaluate_cutover_readiness(&ready_input),
            CutoverReadiness::Ready
        );
    }
}
