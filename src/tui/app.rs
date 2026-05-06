//! Owner: Interactive TUI subsystem — application state and refresh loop
//! Proof: `cargo nextest run -p jeryu -- tui::app`
//! Invariants: UI state refreshes are bounded, non-blocking, and derived from durable control-plane state.
use crate::{
    docker::DockerCtl,
    gitlab_client::GitlabClient,
    release,
    state::{JobEvent, Pool, TrackedPipeline, TuiSession}, // allowlist: TUI session import
};
use anyhow::Result;
use tokio::sync::mpsc;
use tokio::sync::watch;

fn demo_pool(
    name: &str,
    gitlab_runner_id: i64,
    auth_token: &str,
    tags: &str,
    min_warm: i64,
    max_managers: i64,
    concurrent: i64,
    request_concurrency: i64,
    trust_tier: &str,
) -> Pool {
    Pool {
        name: name.into(),
        gitlab_runner_id,
        auth_token: auth_token.into(),
        tags: tags.into(),
        executor: "docker".into(),
        min_warm,
        max_managers,
        concurrent,
        request_concurrency,
        paused: false,
        trust_tier: trust_tier.into(),
    }
}

fn demo_pipeline(pipeline_id: i64, status: &str, updated_at: String) -> TrackedPipeline {
    TrackedPipeline {
        pipeline_id,
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        ref_name: "main".into(),
        sha: "9c3f2d4e0b9f5d1d7cc8".into(),
        status: status.into(),
        updated_at,
    }
}

fn demo_job_event(
    job_id: i64,
    status: &str,
    job_name: &str,
    pool_name: &str,
    system_id: &str,
    queued_duration: Option<f64>,
    received_at: String,
) -> JobEvent {
    JobEvent {
        job_id,
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        pipeline_id: Some(8_013),
        status: status.into(),
        job_name: Some(job_name.into()),
        pool_name: Some(pool_name.into()),
        system_id: Some(system_id.into()),
        queued_duration,
        received_at,
    }
}

fn demo_evidence_record(
    id: i64,
    event_type: &str,
    job_id: i64,
    stage: &str,
    classification: &str,
    payload: &str,
    created_at: String,
) -> crate::state::EvidenceRecord {
    crate::state::EvidenceRecord {
        id,
        event_type: event_type.into(),
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        job_id,
        pipeline_id: Some(8_013),
        commit_sha: "9c3f2d4e0b9f5d1d7cc8".into(),
        ref_name: "main".into(),
        stage: stage.into(),
        exit_code: 0,
        failure_kind: "none".into(),
        classification: classification.into(),
        created_at,
        payload: payload.into(),
    }
}

fn demo_secret_audit_event(
    id: i64,
    target: &str,
    action: &str,
    detail: &str,
    created_at: String,
) -> crate::state::SecretAuditEvent {
    crate::state::SecretAuditEvent {
        id: Some(id),
        repo_name: "jeryu".into(),
        version: "v3.0.1".into(),
        target: target.into(),
        action: action.into(),
        status: "ok".into(),
        detail: detail.into(),
        created_at,
    }
}

const LIVE_LOG_MAX_BYTES: usize = 160_000;
const FEED_MAX_LINES: usize = 80;
const FEED_CYCLE_TICKS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveTab {
    #[default]
    Workflow,
    Mission,
    Release,
    Jobs,
    Agents,
    Tests,
    Pools,
    Cache,
    Evidence,
    Git,
    Secrets,
}

impl ActiveTab {
    pub fn from_number(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Workflow),
            1 => Some(Self::Mission),
            2 => Some(Self::Release),
            3 => Some(Self::Jobs),
            4 => Some(Self::Agents),
            5 => Some(Self::Tests),
            6 => Some(Self::Pools),
            7 => Some(Self::Cache),
            8 => Some(Self::Evidence),
            9 => Some(Self::Secrets),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestViewMode {
    #[default]
    Average,
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EvidenceViewMode {
    #[default]
    Capsules,
    AuditLedger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePane {
    Pools,
    Pipelines,
    #[default]
    Jobs,
}

#[derive(Default, Debug, Clone)]
pub struct StorageBreakdown {
    pub docker_images_bytes: u64,
    pub docker_volumes_bytes: u64,
    pub docker_build_cache_bytes: u64,
    pub cas_bytes: u64,
    pub crate_cache_bytes: u64,
    pub runner_data_bytes: u64,
    pub git_repos_bytes: u64,
    pub rust_target_bytes: u64,
    pub state_store_bytes: u64,
    pub total_disk_bytes: u64,
    pub disk_available_bytes: u64,
}

pub struct PipelineMetrics {
    pub pipeline: TrackedPipeline,
    pub total: usize,
    pub completed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogTarget {
    pub project_id: i64,
    pub job_id: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LiveLogState {
    pub target: Option<LogTarget>,
    pub text: String,
    pub updated_at: Option<String>,
    pub error: Option<String>,
    pub outdated: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RunnerFeed {
    pub runner_name: String,
    pub job_id: i64,
    pub job_name: String,
    pub pipeline_id: i64,
    pub status: String,
    pub elapsed_secs: f64,
    pub log_tail: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct StageProgress {
    pub stage_name: String,
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub running_jobs: usize,
    pub failed_jobs: usize,
    pub status: String,
    pub avg_duration_secs: Option<f64>,
    pub elapsed_secs: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct PipelineProgressView {
    pub pipeline_id: i64,
    pub ref_name: String,
    pub sha_short: String,
    pub stages: Vec<StageProgress>,
    pub overall_pct: u16,
    pub eta_remaining_secs: Option<u64>,
    pub eta_confidence: String,
    pub wall_clock_secs: u64,
    pub started_at: Option<String>,
}

#[derive(Default)]
pub struct TuiStateSnapshot {
    pub pools: Vec<Pool>,
    pub gitlab_ready: bool,
    pub active_containers: usize,
    pub recent_jobs: Vec<JobEvent>,
    pub pipelines: Vec<PipelineMetrics>,
    pub flow: crate::tui::flow::FlowSnapshot,
    pub live_log: LiveLogState,
    pub hot_cache_usage_bytes: i64,
    pub cache_hits: i64,
    pub cache_objects_count: i64,
    pub proxy_healthy: bool,
    pub registry_healthy: bool,
    pub mirror_enabled: bool,
    pub ca_mounted: bool,
    pub singleflight_requests: i64,
    pub hit_ratio: f64,
    pub miss_count: i64,
    pub total_requests: i64,
    pub active_taint_count: i64,
    pub detonation_breaches: i64,
    pub cold_execution_downgrades: i64,
    pub cas_disk_bytes: i64,
    pub crate_cache_disk_bytes: i64,
    pub storage_breakdown: StorageBreakdown,
    pub pipeline_eta: Option<String>,
    pub pipeline_progress: u16,
    pub release_status: Option<release::ReleaseAttemptView>,
    pub release_status_generated_at: Option<String>,
    pub test_bottlenecks_avg: Vec<crate::state::TestBottleneck>,
    pub test_bottlenecks_latest: Vec<crate::state::TestBottleneck>,
    // State sync:
    pub last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    pub inspector_capsule: Option<crate::capsule::FailureCapsule>,
    pub inspector_job_id: Option<i64>,
    pub recent_evidence: Vec<crate::state::EvidenceRecord>,
    pub secret_audit_events: Vec<crate::state::SecretAuditEvent>,
    pub agent_pipelines: Vec<crate::state::TrackedPipeline>,
    pub recent_audit_events: Vec<crate::state::EventLog>,
    pub recent_git_events: Vec<crate::state::GitCommandEventRecord>,
    // TUI v2 — live runner feeds:
    pub runner_feeds: Vec<RunnerFeed>,
    pub active_feed_index: usize,
    pub feed_cycle_tick: u64,
    pub feed_auto_cycle: bool,
    // TUI v2 — pipeline progress:
    pub pipeline_progress_view: Option<PipelineProgressView>,
    // TUI v2 — event ticker:
    pub event_ticker_offset: usize,
}

pub struct App {
    pub store: TuiSession,
    pub docker: DockerCtl,
    pub gitlab: GitlabClient,
    pub state: TuiStateSnapshot,

    pub active_tab: ActiveTab,
    pub active_pane: ActivePane,
    pub selected_pool_index: usize,
    pub selected_pipeline_index: usize,
    pub selected_job_index: usize,
    pub selected_job_id: Option<i64>,

    pub maximize_logs: bool,
    pub log_scroll_offset: u16,
    pub follow_log_tail: bool,

    pub test_view_mode: TestViewMode,
    pub selected_test_index: usize,
    pub selected_test_history: Option<Vec<crate::state::TestExecution>>,

    pub selected_evidence_index: usize,
    pub selected_palette_index: usize,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub evidence_view_mode: EvidenceViewMode,

    pub tick_count: u64,

    pub log_target: Option<LogTarget>,
    pub log_target_tx: watch::Sender<Option<LogTarget>>,

    // TUI v2 — runner feed controls:
    pub feed_scroll_offset: u16,
    pub feed_follow_tail: bool,
    pub feed_pinned: Option<usize>,
    // TUI v2 — interactive:
    pub search_active: bool,
    pub search_query: String,
    pub help_overlay_open: bool,

    sync_rx: mpsc::Receiver<TuiStateSnapshot>,
    sync_tx: mpsc::Sender<TuiStateSnapshot>,

    log_rx: mpsc::Receiver<LiveLogState>,
    log_tx: mpsc::Sender<LiveLogState>,

    flow_rx: mpsc::Receiver<crate::tui::flow::FlowSnapshot>,
    pub flow_tx: mpsc::Sender<crate::tui::flow::FlowSnapshot>,

    feed_rx: mpsc::Receiver<Vec<RunnerFeed>>,
    feed_tx: mpsc::Sender<Vec<RunnerFeed>>,
}

#[path = "app_runtime.rs"]
mod app_runtime;
#[cfg(test)]
pub(crate) use app_runtime::test_app;
