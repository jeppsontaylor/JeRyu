//! Owner: Interactive TUI subsystem — workflow DAG model
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::model`
//! Invariants: WorkflowSnapshot is read-only; built by builder, consumed by widget.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::snapshot::{CacheVerdict, VtiStatus};

/// Canonical status for every workflow node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    #[default]
    Waiting,
    Running,
    Ran,
    Error,
    Skipped,
    Cached,
    Blocked,
    Unknown,
}

impl WorkflowStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Waiting => "WAIT",
            Self::Running => "RUN",
            Self::Ran => "RAN",
            Self::Error => "ERR",
            Self::Skipped => "SKIP",
            Self::Cached => "CACHE",
            Self::Blocked => "BLOCK",
            Self::Unknown => "?",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::Waiting => "○",
            Self::Running => "●",
            Self::Ran => "✓",
            Self::Error => "✗",
            Self::Skipped => "⊘",
            Self::Cached => "◈",
            Self::Blocked => "▪",
            Self::Unknown => "◇",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Ran | Self::Error | Self::Skipped | Self::Cached)
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Classification of workflow nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Check,
    Build,
    Lint,
    UnitTest,
    IntegrationTest,
    SecurityGate,
    ReleaseGate,
    VtiPlan,
    Sentinel,
    #[default]
    Custom,
}

impl WorkflowNodeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Build => "build",
            Self::Lint => "lint",
            Self::UnitTest => "unit",
            Self::IntegrationTest => "integration",
            Self::SecurityGate => "security",
            Self::ReleaseGate => "release-gate",
            Self::VtiPlan => "vti-plan",
            Self::Sentinel => "sentinel",
            Self::Custom => "custom",
        }
    }
}

/// A single node in the workflow DAG — one test, check, or gate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub label: String,
    pub command: Option<String>,
    pub kind: WorkflowNodeKind,
    pub status: WorkflowStatus,
    pub required: bool,
    pub critical_path: bool,
    pub deps: Vec<String>,
    pub duration_secs: Option<f64>,
    pub eta_secs: Option<u64>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub backend: Option<WorkflowBackendRef>,
    pub reason: Option<String>,
    pub vti_status: Option<VtiStatus>,
    pub cache_verdict: Option<CacheVerdict>,
    pub progress_pct: Option<u16>,
    pub tags: Vec<String>,
}

/// Where a node's live status comes from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowBackendRef {
    GitlabJob {
        project_id: i64,
        pipeline_id: i64,
        job_id: i64,
    },
    VtiPlanItem {
        plan_id: i64,
        test_id: String,
    },
    LocalProofLane {
        lane: String,
    },
}

/// A dependency edge in the workflow DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    pub kind: WorkflowEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEdgeKind {
    Dependency,
    StageOrder,
    VtiSkip,
}

/// A horizontal row of parallel nodes at the same dependency depth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    pub id: String,
    pub title: String,
    pub depth: u32,
    pub node_ids: Vec<String>,
}

/// Aggregate counts for the workflow summary banner.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub total: u32,
    pub passed: u32,
    pub running: u32,
    pub waiting: u32,
    pub error: u32,
    pub skipped: u32,
    pub cached: u32,
    pub blocked: u32,
    pub overall_pct: f64,
    pub eta_secs: Option<u64>,
}

impl WorkflowSummary {
    /// Build summary from node statuses.
    pub fn from_nodes(nodes: &[WorkflowNode]) -> Self {
        let mut s = Self { total: nodes.len() as u32, ..Default::default() };
        for n in nodes {
            match n.status {
                WorkflowStatus::Ran => s.passed += 1,
                WorkflowStatus::Running => s.running += 1,
                WorkflowStatus::Waiting => s.waiting += 1,
                WorkflowStatus::Error => s.error += 1,
                WorkflowStatus::Skipped => s.skipped += 1,
                WorkflowStatus::Cached => s.cached += 1,
                WorkflowStatus::Blocked => s.blocked += 1,
                WorkflowStatus::Unknown => {}
            }
        }
        let terminal = s.passed + s.error + s.skipped + s.cached;
        s.overall_pct = if s.total > 0 {
            (terminal as f64 / s.total as f64) * 100.0
        } else { 0.0 };
        s
    }
}

/// Where the workflow data came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSource {
    LatestDbPlan,
    CurrentDiff,
    LivePipeline,
    #[default]
    Demo,
}

/// The complete workflow DAG snapshot consumed by the widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSnapshot {
    pub generated_at: DateTime<Utc>,
    pub title: String,
    pub source: WorkflowSource,
    pub mode: String,
    pub confidence: f64,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub phases: Vec<WorkflowPhase>,
    pub summary: WorkflowSummary,
    pub selected_node_id: Option<String>,
    pub outdated: bool,
}

impl WorkflowSnapshot {
    /// Create an empty snapshot with no active workflow data.
    pub fn empty() -> Self {
        Self {
            generated_at: Utc::now(), title: "No active workflow".into(),
            source: WorkflowSource::Demo, mode: "none".into(), confidence: 0.0,
            nodes: Vec::new(), edges: Vec::new(), phases: Vec::new(),
            summary: WorkflowSummary::default(), selected_node_id: None, outdated: false,
        }
    }

    /// Look up a node by ID.
    pub fn node(&self, id: &str) -> Option<&WorkflowNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Find nodes in a specific phase.
    pub fn phase_nodes(&self, phase_idx: usize) -> Vec<&WorkflowNode> {
        match self.phases.get(phase_idx) {
            Some(p) => p.node_ids.iter().filter_map(|id| self.node(id)).collect(),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_unique() {
        let all = [WorkflowStatus::Waiting, WorkflowStatus::Running, WorkflowStatus::Ran,
            WorkflowStatus::Error, WorkflowStatus::Skipped, WorkflowStatus::Cached,
            WorkflowStatus::Blocked, WorkflowStatus::Unknown];
        let labels: Vec<_> = all.iter().map(|s| s.label()).collect();
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len());
    }

    #[test]
    fn status_terminal_vs_active() {
        assert!(WorkflowStatus::Ran.is_terminal());
        assert!(!WorkflowStatus::Running.is_terminal());
        assert!(WorkflowStatus::Running.is_active());
    }

    #[test]
    fn summary_from_nodes() {
        let nodes = vec![
            WorkflowNode { status: WorkflowStatus::Ran, ..Default::default() },
            WorkflowNode { status: WorkflowStatus::Running, ..Default::default() },
            WorkflowNode { status: WorkflowStatus::Waiting, ..Default::default() },
            WorkflowNode { status: WorkflowStatus::Error, ..Default::default() },
        ];
        let s = WorkflowSummary::from_nodes(&nodes);
        assert_eq!(s.total, 4);
        assert_eq!(s.passed, 1);
        assert!((s.overall_pct - 50.0).abs() < 0.1);
    }

    #[test]
    fn empty_snapshot_is_demo() {
        let snap = WorkflowSnapshot::empty();
        assert_eq!(snap.source, WorkflowSource::Demo);
        assert!(snap.nodes.is_empty());
    }

    #[test]
    fn node_lookup() {
        let mut snap = WorkflowSnapshot::empty();
        snap.nodes.push(WorkflowNode { id: "x".into(), ..Default::default() });
        assert!(snap.node("x").is_some());
        assert!(snap.node("y").is_none());
    }
}
