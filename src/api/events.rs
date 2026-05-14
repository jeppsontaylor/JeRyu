//! Owner: TUI Control-Plane API — durable event stream
//! Proof: `cargo nextest run -p jeryu -- api::events`
//! Invariants: Events have monotonic sequence numbers; freshness windows are always set;
//!             every event references at least one entity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use super::entity::{ActionRef, EntityRef, Severity};

// ── TUI Event ───────────────────────────────────────────────────────────

/// A single fact emitted by the control plane. The TUI renders from
/// a stream of these events instead of periodically re-scraping state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiEvent {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub kind: TuiEventKind,
    pub severity: Severity,
    /// Primary entity this event concerns.
    pub entity: EntityRef,
    /// Parent entity (e.g. pipeline for a job event).
    pub parent: Option<EntityRef>,
    /// One-line human-readable summary.
    pub summary: String,
    /// Correlation ID for tracing across subsystems.
    pub correlation_id: Option<String>,
    /// Evidence capsules / proof packets linked to this event.
    pub evidence_refs: Vec<String>,
    /// Suggested next actions in response to this event.
    pub next_actions: Vec<ActionRef>,
    /// Milliseconds after which this event's data may be considered aged.
    pub stale_after_ms: u64,
}

// ── Event Kinds ─────────────────────────────────────────────────────────

/// Exhaustive taxonomy of control-plane events.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TuiEventKind {
    // System
    SystemHealthUpdated,

    // Pipeline lifecycle
    PipelineCreated,
    PipelineUpdated,
    PipelineCompleted,
    PipelineSuperseded,

    // Job lifecycle
    JobCreated,
    JobStarted,
    JobUpdated,
    JobCompleted,
    JobFailed,
    JobRetried,

    // Logs
    JobLogChunk,
    JobLogAnnotation,
    JobFailureCapsuleCreated,

    // Test intelligence
    TestPlanCreated,
    TestSelectorMissCreated,
    TestVtiAccelerated,
    TestVtiSkipped,

    // Agent lifecycle
    AgentSessionCreated,
    AgentIntentStarted,
    AgentIntentFinished,
    AgentPatchProposed,
    AgentRaceCreated,
    AgentRaceWinnerSelected,

    // Grants & admission
    AgentGrantCreated,
    AgentGrantExpired,
    AgentGrantDenied,
    AdmissionDecisionCreated,

    // Cache
    CacheTaintCreated,
    CacheTaintCleared,
    CacheGcPlanCreated,

    // Release
    ReleaseGateUpdated,
    ReleaseCandidateCreated,
    ReleasePromoted,
    ReleaseRolledBack,

    // Security
    SecretAuditCreated,
    SecretAccessDenied,
    PolicyViolation,

    // Action lifecycle
    ActionPreviewed,
    ActionExecuted,
    ActionFailed,

    // Meta
    NextActionUpdated,
    SnapshotRefreshed,
}

impl TuiEventKind {
    /// Human-readable dot-separated label for display.
    pub fn label(self) -> &'static str {
        match self {
            Self::SystemHealthUpdated => "system.health.updated",
            Self::PipelineCreated => "pipeline.created",
            Self::PipelineUpdated => "pipeline.updated",
            Self::PipelineCompleted => "pipeline.completed",
            Self::PipelineSuperseded => "pipeline.superseded",
            Self::JobCreated => "job.created",
            Self::JobStarted => "job.started",
            Self::JobUpdated => "job.updated",
            Self::JobCompleted => "job.completed",
            Self::JobFailed => "job.failed",
            Self::JobRetried => "job.retried",
            Self::JobLogChunk => "job.log.chunk",
            Self::JobLogAnnotation => "job.log.annotation",
            Self::JobFailureCapsuleCreated => "job.capsule.created",
            Self::TestPlanCreated => "test.plan.created",
            Self::TestSelectorMissCreated => "test.selector_miss.created",
            Self::TestVtiAccelerated => "test.vti.accelerated",
            Self::TestVtiSkipped => "test.vti.skipped",
            Self::AgentSessionCreated => "agent.session.created",
            Self::AgentIntentStarted => "agent.intent.started",
            Self::AgentIntentFinished => "agent.intent.finished",
            Self::AgentPatchProposed => "agent.patch.proposed",
            Self::AgentRaceCreated => "agent.race.created",
            Self::AgentRaceWinnerSelected => "agent.race.winner",
            Self::AgentGrantCreated => "agent.grant.created",
            Self::AgentGrantExpired => "agent.grant.expired",
            Self::AgentGrantDenied => "agent.grant.denied",
            Self::AdmissionDecisionCreated => "admission.decision.created",
            Self::CacheTaintCreated => "cache.taint.created",
            Self::CacheTaintCleared => "cache.taint.cleared",
            Self::CacheGcPlanCreated => "cache.gc.plan.created",
            Self::ReleaseGateUpdated => "release.gate.updated",
            Self::ReleaseCandidateCreated => "release.candidate.created",
            Self::ReleasePromoted => "release.promoted",
            Self::ReleaseRolledBack => "release.rolled_back",
            Self::SecretAuditCreated => "secret.audit.created",
            Self::SecretAccessDenied => "secret.access.denied",
            Self::PolicyViolation => "policy.violation",
            Self::ActionPreviewed => "action.previewed",
            Self::ActionExecuted => "action.executed",
            Self::ActionFailed => "action.failed",
            Self::NextActionUpdated => "next_action.updated",
            Self::SnapshotRefreshed => "snapshot.refreshed",
        }
    }
}

// ── Event Store ─────────────────────────────────────────────────────────

/// Bounded in-memory ring buffer for TUI event timeline rendering.
/// Events are appended by the control plane and consumed by the
/// bottom event timeline widget.
pub struct EventStore {
    events: VecDeque<TuiEvent>,
    capacity: usize,
    next_seq: u64,
}

impl EventStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
            next_seq: 1,
        }
    }

    /// Append a new event, assigning it the next sequence number.
    pub fn push(&mut self, mut event: TuiEvent) -> u64 {
        let seq = self.next_seq;
        event.seq = seq;
        self.next_seq += 1;

        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);
        seq
    }

    /// All events in order (oldest first).
    pub fn all(&self) -> impl Iterator<Item = &TuiEvent> {
        self.events.iter()
    }

    /// Events since a given cursor (exclusive).
    pub fn since(&self, cursor: u64) -> impl Iterator<Item = &TuiEvent> {
        self.events.iter().filter(move |e| e.seq > cursor)
    }

    /// Most recent N events (newest first).
    pub fn recent(&self, n: usize) -> Vec<&TuiEvent> {
        self.events.iter().rev().take(n).collect()
    }

    /// Current cursor (sequence of the last event).
    pub fn cursor(&self) -> u64 {
        self.next_seq.saturating_sub(1)
    }

    /// Number of events stored.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new(1000)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{EntityKind, EntityRef};

    fn make_event(kind: TuiEventKind, summary: &str) -> TuiEvent {
        TuiEvent {
            seq: 0, // will be assigned by EventStore::push
            timestamp: Utc::now(),
            kind,
            severity: Severity::Info,
            entity: EntityRef::new(EntityKind::System, "test"),
            parent: None,
            summary: summary.into(),
            correlation_id: None,
            evidence_refs: Vec::new(),
            next_actions: Vec::new(),
            stale_after_ms: 5000,
        }
    }

    #[test]
    fn event_store_assigns_monotonic_seqs() {
        let mut store = EventStore::new(100);
        let s1 = store.push(make_event(TuiEventKind::SystemHealthUpdated, "a"));
        let s2 = store.push(make_event(TuiEventKind::JobFailed, "b"));
        let s3 = store.push(make_event(TuiEventKind::AgentSessionCreated, "c"));
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
        assert_eq!(store.cursor(), 3);
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn event_store_respects_capacity() {
        let mut store = EventStore::new(2);
        store.push(make_event(TuiEventKind::JobStarted, "first"));
        store.push(make_event(TuiEventKind::JobFailed, "second"));
        store.push(make_event(TuiEventKind::JobRetried, "third"));
        assert_eq!(store.len(), 2);
        let events: Vec<_> = store.all().collect();
        assert_eq!(events[0].summary, "second");
        assert_eq!(events[1].summary, "third");
    }

    #[test]
    fn event_store_since_filters_correctly() {
        let mut store = EventStore::new(100);
        store.push(make_event(TuiEventKind::JobStarted, "a"));
        store.push(make_event(TuiEventKind::JobFailed, "b"));
        store.push(make_event(TuiEventKind::JobRetried, "c"));
        let since_1: Vec<_> = store.since(1).collect();
        assert_eq!(since_1.len(), 2);
        assert_eq!(since_1[0].summary, "b");
        assert_eq!(since_1[1].summary, "c");
    }

    #[test]
    fn event_store_recent_returns_newest_first() {
        let mut store = EventStore::new(100);
        store.push(make_event(TuiEventKind::JobStarted, "prior"));
        store.push(make_event(TuiEventKind::JobFailed, "mid"));
        store.push(make_event(TuiEventKind::JobRetried, "new"));
        let recent = store.recent(2);
        assert_eq!(recent[0].summary, "new");
        assert_eq!(recent[1].summary, "mid");
    }

    #[test]
    fn event_kind_labels_are_dot_separated() {
        assert_eq!(TuiEventKind::JobFailed.label(), "job.failed");
        assert_eq!(
            TuiEventKind::AgentRaceWinnerSelected.label(),
            "agent.race.winner"
        );
        assert_eq!(
            TuiEventKind::TestVtiAccelerated.label(),
            "test.vti.accelerated"
        );
    }
}
