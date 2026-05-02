//! Owner: Interactive TUI subsystem — flow data model
//! Proof: `cargo nextest run -p jeryu -- tui::flow`
//! Invariants: Flow model types remain serializable, cloneable snapshots of observed control-plane state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct FlowSnapshot {
    pub generated_at: DateTime<Utc>,
    pub gitlab_online: bool,
    pub active_pipelines: Vec<PipelineFlow>,
    pub stale: bool,
    pub last_non_empty_at: Option<DateTime<Utc>>,
    pub selected_pipeline_id: Option<i64>,
    pub release: Option<crate::release::ReleaseAttemptView>,
    pub pools: Vec<crate::state::Pool>,
    // For other parts of TUI state:
    pub active_containers: usize,
    pub cache_metrics: CacheMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheMetrics {
    pub hot_usage_bytes: i64,
    pub hits: i64,
    pub objects: i64,
    pub singleflight_coalesced: i64,
    pub hit_ratio: f64,
    pub misses: i64,
    pub requests: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineFlow {
    pub pipeline_id: i64,
    pub project_id: i64,
    pub ref_name: String,
    pub sha: Option<String>,
    pub status: String,
    pub graph: FlowGraph,
    pub current_blocker: Option<i64>,
    pub critical_path: Vec<i64>,
    pub eta: Option<EtaEstimate>,
    pub progress_pct: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlowGraph {
    pub columns: Vec<FlowColumn>,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowColumn {
    pub key: FlowColumnKind,
    pub title: String,
    pub status: String,
    pub eta: Option<EtaEstimate>,
    pub lane_groups: Vec<LaneGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaneGroup {
    pub lane: LaneKind,
    pub title: String,
    pub node_ids: Vec<i64>, // references FlowNode.id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: i64,
    pub job_id: Option<i64>, // GitLab job ID if it exists
    pub label: String,
    pub column: FlowColumnKind,
    pub lane: LaneKind,
    pub status: String,
    pub progress_pct: u16,
    pub eta: Option<EtaEstimate>,
    pub is_required: bool,
    pub is_critical_path: bool,
    pub backend: Option<BackendRef>,
    pub elapsed_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEdge {
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FlowColumnKind {
    Commit,
    Admission,
    Impact,
    Pipeline,
    Build,
    Tests,
    Security,
    Package,
    ReleaseGates,
    Canary,
    Production,
    Other,
}

impl FlowColumnKind {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Commit => "Commit",
            Self::Admission => "Admission",
            Self::Impact => "Impact",
            Self::Pipeline => "Pipeline",
            Self::Build => "Build",
            Self::Tests => "Tests",
            Self::Security => "Security",
            Self::Package => "Package",
            Self::ReleaseGates => "Gates",
            Self::Canary => "Canary",
            Self::Production => "Prod",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LaneKind {
    Git,
    Admission,
    Impact,
    Build,
    Unit,
    Integration,
    Security,
    Extended,
    Research,
    ReleaseCritical,
    ReleaseExecution,
    Other,
}

impl LaneKind {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Git => "Git",
            Self::Admission => "Admission",
            Self::Impact => "Impact",
            Self::Build => "Build",
            Self::Unit => "Unit",
            Self::Integration => "Integration",
            Self::Security => "Security",
            Self::Extended => "Extended",
            Self::Research => "Research",
            Self::ReleaseCritical => "Rel-Critical",
            Self::ReleaseExecution => "Release",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtaEstimate {
    pub remaining_secs: i64,
    pub confidence: EtaConfidence,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EtaConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendRef {
    pub project_id: i64,
    pub job_id: i64,
    pub pipeline_id: Option<i64>,
    pub status: String,
    pub queued_duration: Option<f64>,
    pub received_at: String,
}
