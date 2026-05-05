//! Owner: Interactive TUI subsystem — application state and refresh loop
//! Proof: `cargo nextest run -p jeryu -- tui::app`
//! Invariants: UI state refreshes are bounded, non-blocking, and derived from durable control-plane state.
use crate::{
    docker::DockerCtl,
    gitlab_client::GitlabClient,
    release,
    state::{Db, JobEvent, Pool, TrackedPipeline},
};
use anyhow::Result;
use tokio::sync::mpsc;
use tokio::sync::watch;

const LIVE_LOG_MAX_BYTES: usize = 160_000;
const FEED_MAX_LINES: usize = 80;
const FEED_CYCLE_TICKS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveTab {
    #[default]
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
            1 => Some(Self::Mission),
            2 => Some(Self::Release),
            3 => Some(Self::Jobs),
            4 => Some(Self::Agents),
            5 => Some(Self::Tests),
            6 => Some(Self::Pools),
            7 => Some(Self::Cache),
            8 => Some(Self::Evidence),
            9 => Some(Self::Secrets),
            10 => Some(Self::Git),
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
    pub jeryu_db_bytes: u64,
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
    pub stale: bool,
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
    pub db: Db,
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

impl App {
    pub fn new(db: Db, docker: DockerCtl, gitlab: GitlabClient) -> Self {
        let (sync_tx, sync_rx) = mpsc::channel(4);
        let (flow_tx, flow_rx) = mpsc::channel(4);
        let (log_tx, log_rx) = mpsc::channel(8);
        let (feed_tx, feed_rx) = mpsc::channel(4);
        let (log_target_tx, _log_target_rx) = watch::channel(None);
        Self {
            db,
            docker,
            gitlab,
            state: TuiStateSnapshot::default(),
            active_tab: ActiveTab::default(),
            active_pane: ActivePane::default(),
            selected_pool_index: 0,
            selected_pipeline_index: 0,
            selected_job_index: 0,
            selected_job_id: None,
            maximize_logs: false,
            log_scroll_offset: 0,
            follow_log_tail: true,
            test_view_mode: TestViewMode::default(),
            selected_test_index: 0,
            selected_test_history: None,
            selected_evidence_index: 0,
            command_palette_open: false,
            command_palette_query: String::new(),
            selected_palette_index: 0,
            evidence_view_mode: EvidenceViewMode::default(),
            tick_count: 0,
            log_target: None,
            log_target_tx,
            feed_scroll_offset: 0,
            feed_follow_tail: true,
            feed_pinned: None,
            search_active: false,
            search_query: String::new(),
            help_overlay_open: false,
            sync_rx,
            sync_tx,
            log_rx,
            log_tx,
            flow_rx,
            flow_tx,
            feed_rx,
            feed_tx,
        }
    }

    pub fn apply_demo_fixture(&mut self) {
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();

        let attempt = crate::state::ReleaseAttempt {
            id: 42,
            project_id: release::DEFAULT_RELEASE_PROJECT_ID,
            ref_name: "main".into(),
            sha: "9c3f2d4e0b9f5d1d7cc8".into(),
            version: "v3.0.1-demo".into(),
            upstream_pipeline_id: Some(8_012),
            upstream_status: "success".into(),
            release_pipeline_id: Some(8_013),
            release_pipeline_status: Some("running".into()),
            production_pipeline_id: Some(8_014),
            production_pipeline_status: Some("pending".into()),
            canary_status: "in-flight".into(),
            canary_started_at: Some(now_str.clone()),
            canary_finished_at: None,
            canary_note: Some("Evaluating telemetry and E2E readiness".into()),
            created_at: now_str.clone(),
            updated_at: now_str.clone(),
        };

        let release_status = release::ReleaseAttemptView {
            attempt: attempt.clone(),
            release_dir: "target/demo-release".into(),
            canary_state_path: "artifacts/canary.state".into(),
            gate_remote_canary_path: "artifacts/remote-canary.txt".into(),
            gate_canary_e2e_path: "artifacts/canary-e2e.txt".into(),
            gate_canary_telemetry_path: "artifacts/canary-telemetry.txt".into(),
            telemetry_diag_path: "artifacts/telemetry-diag.json".into(),
            canary_state: "in-flight".into(),
            eligibility: "high-confidence".into(),
            phase: Some("validation".into()),
            detail: Some("Demo release is active and collecting proof.".into()),
            state_status: Some("running".into()),
            has_remote_gate: true,
            has_telemetry_gate: true,
            has_e2e_gate: true,
            has_telemetry_diag: true,
            release_identity_ok: true,
            canary_public_url: Some("https://example.invalid/jeryu/demo-canary".into()),
        };

        let flow_jobs = vec![
            JobEvent {
                job_id: 9_001,
                project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                pipeline_id: Some(8_013),
                status: "success".into(),
                job_name: Some("policy-admission".into()),
                pool_name: Some("trusted".into()),
                system_id: Some("sys-a1".into()),
                queued_duration: Some(1.6),
                received_at: now_str.clone(),
            },
            JobEvent {
                job_id: 9_002,
                project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                pipeline_id: Some(8_013),
                status: "running".into(),
                job_name: Some("build-image".into()),
                pool_name: Some("trusted".into()),
                system_id: Some("sys-a2".into()),
                queued_duration: Some(0.4),
                received_at: now_str.clone(),
            },
            JobEvent {
                job_id: 9_003,
                project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                pipeline_id: Some(8_013),
                status: "pending".into(),
                job_name: Some("integration-tests".into()),
                pool_name: Some("trusted".into()),
                system_id: Some("sys-a3".into()),
                queued_duration: None,
                received_at: now_str.clone(),
            },
            JobEvent {
                job_id: 9_004,
                project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                pipeline_id: Some(8_013),
                status: "failed".into(),
                job_name: Some("security-gate".into()),
                pool_name: Some("security".into()),
                system_id: Some("sys-s1".into()),
                queued_duration: Some(0.8),
                received_at: now_str.clone(),
            },
            JobEvent {
                job_id: 9_005,
                project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                pipeline_id: Some(8_013),
                status: "running".into(),
                job_name: Some("e2e-canary".into()),
                pool_name: Some("trusted".into()),
                system_id: Some("sys-a4".into()),
                queued_duration: Some(0.9),
                received_at: now_str.clone(),
            },
        ];
        let flow_graph = crate::tui::flow::builder::build_graph(8_013, flow_jobs.clone());
        let progress_pct: u16 = 68;

        let flow = crate::tui::flow::FlowSnapshot {
            generated_at: now,
            gitlab_online: true,
            active_pipelines: vec![crate::tui::flow::PipelineFlow {
                pipeline_id: 8_013,
                project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                ref_name: "main".into(),
                sha: Some("9c3f2d4e0b9f5d1d7cc8".into()),
                status: "running".into(),
                graph: flow_graph,
                current_blocker: Some(9_004),
                critical_path: vec![9_002, 9_003, 9_004],
                eta: Some(crate::tui::flow::EtaEstimate {
                    remaining_secs: 380,
                    confidence: crate::tui::flow::EtaConfidence::Medium,
                    reason: "security gate retry path may be needed".into(),
                }),
                progress_pct,
            }],
            stale: false,
            last_non_empty_at: Some(now),
            selected_pipeline_id: Some(8_013),
            release: Some(release_status.clone()),
            pools: vec![
                Pool {
                    name: "trusted".into(),
                    gitlab_runner_id: 9001,
                    auth_token: "token-trusted".into(),
                    tags: "linux,x86_64,trusted".into(),
                    executor: "docker".into(),
                    min_warm: 2,
                    max_managers: 10,
                    concurrent: 4,
                    request_concurrency: 4,
                    paused: false,
                    trust_tier: "trusted".into(),
                },
                Pool {
                    name: "security".into(),
                    gitlab_runner_id: 9002,
                    auth_token: "token-security".into(),
                    tags: "linux,x86_64,security".into(),
                    executor: "docker".into(),
                    min_warm: 1,
                    max_managers: 4,
                    concurrent: 2,
                    request_concurrency: 2,
                    paused: false,
                    trust_tier: "restricted".into(),
                },
                Pool {
                    name: "research".into(),
                    gitlab_runner_id: 9003,
                    auth_token: "token-research".into(),
                    tags: "linux,arm64,research".into(),
                    executor: "docker".into(),
                    min_warm: 1,
                    max_managers: 3,
                    concurrent: 1,
                    request_concurrency: 1,
                    paused: false,
                    trust_tier: "experimental".into(),
                },
            ],
            active_containers: 11,
            cache_metrics: crate::tui::flow::CacheMetrics {
                hot_usage_bytes: 24_311_008,
                hits: 1_102,
                objects: 2_900,
                singleflight_coalesced: 72,
                hit_ratio: 0.88,
                misses: 148,
                requests: 1_250,
            },
        };

        self.state = TuiStateSnapshot {
            pools: vec![
                Pool {
                    name: "trusted".into(),
                    gitlab_runner_id: 9001,
                    auth_token: "token-trusted".into(),
                    tags: "linux,x86_64,trusted".into(),
                    executor: "docker".into(),
                    min_warm: 2,
                    max_managers: 10,
                    concurrent: 4,
                    request_concurrency: 4,
                    paused: false,
                    trust_tier: "trusted".into(),
                },
                Pool {
                    name: "security".into(),
                    gitlab_runner_id: 9002,
                    auth_token: "token-security".into(),
                    tags: "linux,x86_64,security".into(),
                    executor: "docker".into(),
                    min_warm: 1,
                    max_managers: 4,
                    concurrent: 2,
                    request_concurrency: 2,
                    paused: false,
                    trust_tier: "restricted".into(),
                },
            ],
            gitlab_ready: true,
            active_containers: 11,
            recent_jobs: flow_jobs,
            pipelines: vec![
                PipelineMetrics {
                    pipeline: TrackedPipeline {
                        pipeline_id: 8_013,
                        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                        ref_name: "main".into(),
                        sha: "9c3f2d4e0b9f5d1d7cc8".into(),
                        status: "running".into(),
                        updated_at: now_str.clone(),
                    },
                    total: 5,
                    completed: 2,
                },
                PipelineMetrics {
                    pipeline: TrackedPipeline {
                        pipeline_id: 8_014,
                        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                        ref_name: "main".into(),
                        sha: "9c3f2d4e0b9f5d1d7cc8".into(),
                        status: "pending".into(),
                        updated_at: now_str.clone(),
                    },
                    total: 3,
                    completed: 0,
                },
            ],
            flow,
            live_log: LiveLogState {
                target: Some(LogTarget {
                    project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                    job_id: 9_002,
                }),
                text: "[demo] security gate failed due to stale artifact cache\n[demo] retry required with clean cache namespace\n".into(),
                updated_at: Some(now_str.clone()),
                error: None,
                stale: false,
            },
            hot_cache_usage_bytes: 2_340_000_000,
            cache_hits: 1_102,
            cache_objects_count: 2_900,
            proxy_healthy: true,
            registry_healthy: true,
            mirror_enabled: true,
            ca_mounted: true,
            singleflight_requests: 72,
            hit_ratio: 0.88,
            miss_count: 148,
            total_requests: 1_250,
            active_taint_count: 1,
            detonation_breaches: 0,
            cold_execution_downgrades: 0,
            cas_disk_bytes: 11_500_000,
            crate_cache_disk_bytes: 8_900_000,
            storage_breakdown: StorageBreakdown {
                docker_images_bytes: 7_000_000,
                docker_volumes_bytes: 3_000_000,
                docker_build_cache_bytes: 5_000_000,
                cas_bytes: 11_500_000,
                crate_cache_bytes: 8_900_000,
                runner_data_bytes: 2_600_000,
                git_repos_bytes: 1_200_000,
                rust_target_bytes: 2_300_000,
                jeryu_db_bytes: 450_000,
                total_disk_bytes: 45_000_000,
                disk_available_bytes: 120_000_000,
            },
            pipeline_eta: Some("~6m".into()),
            pipeline_progress: progress_pct,
            release_status: Some(release_status.clone()),
            release_status_generated_at: Some(now_str.clone()),
            test_bottlenecks_avg: vec![
                crate::state::TestBottleneck {
                    test_name: "integration::cache_layer".into(),
                    avg_duration_ms: 7_200.0,
                    latest_duration_ms: 7_450,
                    count: 12,
                },
                crate::state::TestBottleneck {
                    test_name: "unit::scheduler".into(),
                    avg_duration_ms: 1_100.0,
                    latest_duration_ms: 1_020,
                    count: 24,
                },
                crate::state::TestBottleneck {
                    test_name: "e2e::release_path".into(),
                    avg_duration_ms: 18_000.0,
                    latest_duration_ms: 18_720,
                    count: 4,
                },
            ],
            test_bottlenecks_latest: vec![
                crate::state::TestBottleneck {
                    test_name: "e2e::release_path".into(),
                    avg_duration_ms: 18_000.0,
                    latest_duration_ms: 18_720,
                    count: 4,
                },
                crate::state::TestBottleneck {
                    test_name: "integration::policy".into(),
                    avg_duration_ms: 5_300.0,
                    latest_duration_ms: 5_910,
                    count: 9,
                },
                crate::state::TestBottleneck {
                    test_name: "security::secret_scan".into(),
                    avg_duration_ms: 6_200.0,
                    latest_duration_ms: 6_000,
                    count: 6,
                },
            ],
            last_sync_at: Some(now),
            inspector_capsule: Some(crate::capsule::FailureCapsule {
                job_id: 9_004,
                pipeline_id: Some(8_013),
                project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                stage: "security".into(),
                exit_code: 1,
                commit_sha: "9c3f2d4e0b9f5d1d7cc8".into(),
                ref_name: "main".into(),
                working_directory: "/workspace".into(),
                log_snippet: "security gate timed out in artifact verification".into(),
                repro_script: "./tools/run-security-gate.sh".into(),
                environment: std::collections::HashMap::new(),
                failure_kind: "timeout".into(),
                summary: "security gate timeout on canary validation".into(),
                superseded_by_sha: None,
                retried_from_job_id: None,
            }),
            inspector_job_id: Some(9_004),
            recent_evidence: vec![
                crate::state::EvidenceRecord {
                    id: 11,
                    event_type: "vti_validation".into(),
                    project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                    job_id: 9_003,
                    pipeline_id: Some(8_013),
                    commit_sha: "9c3f2d4e0b9f5d1d7cc8".into(),
                    ref_name: "main".into(),
                    stage: "test".into(),
                    exit_code: 0,
                    failure_kind: "none".into(),
                    classification: "ok".into(),
                    created_at: now_str.clone(),
                    payload: "{\"scope\": \"tests\", \"cache\": \"hit\"}".into(),
                },
                crate::state::EvidenceRecord {
                    id: 12,
                    event_type: "secret_audit".into(),
                    project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                    job_id: 9_001,
                    pipeline_id: Some(8_013),
                    commit_sha: "9c3f2d4e0b9f5d1d7cc8".into(),
                    ref_name: "main".into(),
                    stage: "prepare".into(),
                    exit_code: 0,
                    failure_kind: "none".into(),
                    classification: "compliant".into(),
                    created_at: now_str.clone(),
                    payload: "{\"vault\": \"rotation_ok\"}".into(),
                },
            ],
            secret_audit_events: vec![
                crate::state::SecretAuditEvent {
                    id: Some(21),
                    repo_name: "jeryu".into(),
                    version: "v3.0.1".into(),
                    target: "release".into(),
                    action: "rotated".into(),
                    status: "ok".into(),
                    detail: "release token rotated for canary context".into(),
                    created_at: now_str.clone(),
                },
                crate::state::SecretAuditEvent {
                    id: Some(22),
                    repo_name: "jeryu".into(),
                    version: "v3.0.1".into(),
                    target: "agent".into(),
                    action: "fetched".into(),
                    status: "ok".into(),
                    detail: "agent bootstrap secret requested".into(),
                    created_at: now_str.clone(),
                },
            ],
            agent_pipelines: vec![
                TrackedPipeline {
                    pipeline_id: 8_013,
                    project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                    ref_name: "main".into(),
                    sha: "9c3f2d4e0b9f5d1d7cc8".into(),
                    status: "running".into(),
                    updated_at: now_str.clone(),
                },
                TrackedPipeline {
                    pipeline_id: 8_014,
                    project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                    ref_name: "agent/repair-hypothesis".into(),
                    sha: "e8f2d2b77a11".into(),
                    status: "success".into(),
                    updated_at: now_str.clone(),
                },
            ],
            recent_audit_events: vec![
                crate::state::EventLog {
                    id: 501,
                    event_type: "capability.granted".into(),
                    timestamp: now_str.clone(),
                    project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                    job_id: Some(9_001),
                    actor: "ci-bot".into(),
                    payload: "{\"action\": \"run_validation\"}".into(),
                },
                crate::state::EventLog {
                    id: 502,
                    event_type: "vti.receipt".into(),
                    timestamp: now_str.clone(),
                    project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                    job_id: Some(9_003),
                    actor: "validator".into(),
                    payload: "{\"status\": \"selected\", \"selector\": \"safe\"}".into(),
                },
                crate::state::EventLog {
                    id: 503,
                    event_type: "merge_gate.blocked".into(),
                    timestamp: now_str.clone(),
                    project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                    job_id: Some(9_004),
                    actor: "gatekeeper".into(),
                    payload: "{\"reason\": \"security_gate_failed\"}".into(),
                },
            ],
            recent_git_events: vec![],
            runner_feeds: vec![
                RunnerFeed {
                    runner_name: "trusted-01".into(),
                    job_id: 9_002,
                    job_name: "build-image".into(),
                    pipeline_id: 8_013,
                    status: "running".into(),
                    elapsed_secs: 134.0,
                    log_tail: "[2026-05-02 23:04:12] Compiling jeryu v3.0.1\n[2026-05-02 23:04:13] Compiling tokio v1.40\n[2026-05-02 23:04:14]   warning: unused import `std::io`\n[2026-05-02 23:04:15] Compiling sqlx v0.8\n[2026-05-02 23:04:16] Compiling ratatui v0.29\n[2026-05-02 23:04:17] Finished `release` profile in 2m14s".into(),
                    updated_at: now_str.clone(),
                },
                RunnerFeed {
                    runner_name: "trusted-02".into(),
                    job_id: 9_005,
                    job_name: "e2e-canary".into(),
                    pipeline_id: 8_013,
                    status: "running".into(),
                    elapsed_secs: 87.0,
                    log_tail: "[2026-05-02 23:04:30] Running e2e test suite...\n[2026-05-02 23:04:31] test canary::smoke_health ... ok\n[2026-05-02 23:04:32] test canary::telemetry_check ... FAILED\n[2026-05-02 23:04:33]   Error: telemetry endpoint returned 503\n[2026-05-02 23:04:34] test canary::rollback_gate ... ok".into(),
                    updated_at: now_str.clone(),
                },
                RunnerFeed {
                    runner_name: "security-01".into(),
                    job_id: 9_004,
                    job_name: "security-gate".into(),
                    pipeline_id: 8_013,
                    status: "failed".into(),
                    elapsed_secs: 45.0,
                    log_tail: "[2026-05-02 23:03:50] Running security scan...\n[2026-05-02 23:03:52] Checking artifact signatures...\n[2026-05-02 23:03:55] ERROR: Artifact verification timed out\n[2026-05-02 23:03:55] FATAL: security gate failed".into(),
                    updated_at: now_str.clone(),
                },
            ],
            active_feed_index: 0,
            feed_cycle_tick: 0,
            feed_auto_cycle: true,
            pipeline_progress_view: Some(PipelineProgressView {
                pipeline_id: 8_013,
                ref_name: "main".into(),
                sha_short: "9c3f2d4e".into(),
                stages: vec![
                    StageProgress {
                        stage_name: "build".into(),
                        total_jobs: 2, completed_jobs: 2, running_jobs: 0, failed_jobs: 0,
                        status: "success".into(),
                        avg_duration_secs: Some(180.0), elapsed_secs: Some(134.0),
                    },
                    StageProgress {
                        stage_name: "test".into(),
                        total_jobs: 3, completed_jobs: 1, running_jobs: 1, failed_jobs: 0,
                        status: "running".into(),
                        avg_duration_secs: Some(300.0), elapsed_secs: Some(87.0),
                    },
                    StageProgress {
                        stage_name: "security".into(),
                        total_jobs: 2, completed_jobs: 0, running_jobs: 0, failed_jobs: 1,
                        status: "failed".into(),
                        avg_duration_secs: Some(60.0), elapsed_secs: Some(45.0),
                    },
                    StageProgress {
                        stage_name: "deploy".into(),
                        total_jobs: 1, completed_jobs: 0, running_jobs: 0, failed_jobs: 0,
                        status: "pending".into(),
                        avg_duration_secs: Some(120.0), elapsed_secs: None,
                    },
                    StageProgress {
                        stage_name: "e2e".into(),
                        total_jobs: 2, completed_jobs: 0, running_jobs: 1, failed_jobs: 0,
                        status: "running".into(),
                        avg_duration_secs: Some(240.0), elapsed_secs: Some(87.0),
                    },
                ],
                overall_pct: 47,
                eta_remaining_secs: Some(492),
                eta_confidence: "medium".into(),
                wall_clock_secs: 862,
                started_at: Some(now_str.clone()),
            }),
            event_ticker_offset: 0,
        };

        self.selected_job_index = 0;
        self.selected_pipeline_index = 0;
        self.selected_pool_index = 0;
        self.selected_test_index = 0;
        self.selected_test_history = None;
        self.selected_evidence_index = 0;
        self.test_view_mode = TestViewMode::Average;
        self.evidence_view_mode = EvidenceViewMode::Capsules;
        self.maximize_logs = false;
        self.log_scroll_offset = 0;
        self.follow_log_tail = true;
        self.command_palette_open = false;
        self.command_palette_query.clear();
        self.selected_palette_index = 0;
        self.tick_count = 0;
        self.log_target = Some(LogTarget {
            project_id: release::DEFAULT_RELEASE_PROJECT_ID,
            job_id: 9_002,
        });
        self.log_target_tx.send(self.log_target).ok();
        self.remember_selected_job();
    }

    pub fn start_background_sync(&self) {
        let db = self.db.clone();
        let docker = self.docker.clone();
        let gitlab = self.gitlab.clone();
        let tx = self.sync_tx.clone();
        let log_tx = self.log_tx.clone();
        let mut log_rx = self.log_target_tx.subscribe();

        let db_flow = self.db.clone();
        let docker_flow = self.docker.clone();
        let gitlab_flow = self.gitlab.clone();

        let flow_tx = self.flow_tx.clone();
        let flow_log_rx = self.log_target_tx.subscribe();
        tokio::spawn(async move {
            crate::tui::flow::collector::run_collector(
                db_flow,
                docker_flow,
                gitlab_flow,
                flow_tx,
                flow_log_rx,
            )
            .await;
        });

        let gitlab_logs = self.gitlab.clone();
        tokio::spawn(async move {
            let mut current_target: Option<LogTarget> = None;
            let mut state = LiveLogState::default();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(650));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                if let Ok(true) = log_rx.has_changed() {
                    current_target = *log_rx.borrow_and_update();
                    state = LiveLogState {
                        target: current_target,
                        ..Default::default()
                    };
                }

                let Some(target) = current_target else {
                    if state.target.is_some() {
                        state = LiveLogState::default();
                        if log_tx.send(state.clone()).await.is_err() {
                            break;
                        }
                    }
                    continue;
                };

                match gitlab_logs
                    .job_trace(target.project_id, target.job_id)
                    .await
                {
                    Ok(trace_text) => {
                        state = LiveLogState {
                            target: Some(target),
                            text: retain_tail(&trace_text, LIVE_LOG_MAX_BYTES),
                            updated_at: Some(chrono::Utc::now().to_rfc3339()),
                            error: None,
                            stale: false,
                        };
                    }
                    Err(error) => {
                        state.target = Some(target);
                        state
                            .updated_at
                            .get_or_insert_with(|| chrono::Utc::now().to_rfc3339());
                        state.error = Some(error.to_string());
                        state.stale = true;
                    }
                }

                if log_tx.send(state.clone()).await.is_err() {
                    break;
                }
            }
        });

        // TUI v2 — Live Runner Feed background sync
        let db_feed = self.db.clone();
        let gitlab_feed = self.gitlab.clone();
        let feed_tx = self.feed_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(2000));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                // Find running jobs
                let running_jobs = db_feed
                    .recent_job_events(50)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|j| j.status == "running")
                    .take(5)
                    .collect::<Vec<_>>();

                let mut feeds = Vec::new();
                for job in &running_jobs {
                    let log_tail = match gitlab_feed.job_trace(job.project_id, job.job_id).await {
                        Ok(trace) => {
                            let lines: Vec<&str> = trace.lines().collect();
                            let start = lines.len().saturating_sub(FEED_MAX_LINES);
                            lines[start..].join("\n")
                        }
                        Err(_) => String::new(),
                    };

                    let elapsed = chrono::DateTime::parse_from_rfc3339(&job.received_at)
                        .map(|t| chrono::Utc::now().signed_duration_since(t).num_seconds() as f64)
                        .unwrap_or(0.0);

                    feeds.push(RunnerFeed {
                        runner_name: job.pool_name.clone().unwrap_or_else(|| "unknown".into()),
                        job_id: job.job_id,
                        job_name: job
                            .job_name
                            .clone()
                            .unwrap_or_else(|| format!("job-{}", job.job_id)),
                        pipeline_id: job.pipeline_id.unwrap_or(0),
                        status: job.status.clone(),
                        elapsed_secs: elapsed,
                        log_tail,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    });
                }

                if feed_tx.send(feeds).await.is_err() {
                    break;
                }
            }
        });

        // TUI snapshot sync loop
        tokio::spawn(async move {
            loop {
                let mut snap = TuiStateSnapshot::default();
                Self::hydrate_core_snapshot(&mut snap, &db, &docker, &gitlab).await;

                if let Ok(pipes) = db.list_tracked_pipelines(10).await {
                    let mut pipe_metrics = Vec::new();
                    let mut max_progress = 0;
                    let mut latest_eta: Option<String> = None;

                    for p in pipes {
                        // Count actual job events for this pipeline from DB
                        let total = sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(DISTINCT job_id) FROM job_events WHERE pipeline_id = ?",
                        )
                        .bind(p.pipeline_id)
                        .fetch_one(&db.pool())
                        .await
                        .unwrap_or(0);
                        let completed = sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(DISTINCT job_id) FROM job_events WHERE pipeline_id = ? AND status IN ('success', 'failed', 'canceled')"
                        )
                        .bind(p.pipeline_id)
                        .fetch_one(&db.pool())
                        .await
                        .unwrap_or(0);

                        let running = sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(DISTINCT job_id) FROM job_events WHERE pipeline_id = ? AND status = 'running'"
                        )
                        .bind(p.pipeline_id)
                        .fetch_one(&db.pool())
                        .await
                        .unwrap_or(0);

                        let pct = if total > 0 {
                            let effective = completed as f64 + (running as f64 * 0.5);
                            ((effective / total as f64) * 100.0) as u16
                        } else {
                            0
                        };

                        if (p.status == "running" || p.status == "pending" || p.status == "created")
                            && pct >= max_progress
                        {
                            max_progress = pct;
                            let remaining = total - completed;
                            // Estimate ~45 seconds per job (adjust heuristically based on audit suites)
                            let secs = remaining * 45;
                            latest_eta = Some(if secs > 3600 {
                                format!("~{}h {}m remaining", secs / 3600, (secs % 3600) / 60)
                            } else if secs > 60 {
                                format!("~{}m {}s remaining", secs / 60, secs % 60)
                            } else {
                                format!("~{}s remaining", secs)
                            });
                        }

                        pipe_metrics.push(PipelineMetrics {
                            pipeline: p.clone(),
                            total: total as usize,
                            completed: completed as usize,
                        });
                    }
                    snap.pipelines = pipe_metrics;
                    snap.pipeline_progress = max_progress;
                    snap.pipeline_eta = latest_eta;
                }

                snap.gitlab_ready = gitlab.is_ready().await;

                if let Ok(report) = release::build_release_status_report(
                    &db,
                    release::ReleaseStatusQuery {
                        project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                        ref_name: Some("main".into()),
                        sha: None,
                        limit: 1,
                    },
                )
                .await
                {
                    snap.release_status_generated_at = Some(report.generated_at);
                    snap.release_status = report.latest;
                }

                // Observability metrics for Cache
                let proxy_addr = format!("127.0.0.1:{}", crate::config::CACHE_PROXY_PORT);
                let registry_addr = format!("127.0.0.1:{}", crate::config::CACHE_REGISTRY_PORT);

                snap.proxy_healthy = tokio::net::TcpStream::connect(&proxy_addr).await.is_ok();
                snap.registry_healthy =
                    tokio::net::TcpStream::connect(&registry_addr).await.is_ok();

                let daemon_path = std::path::Path::new("/etc/docker/daemon.json");
                snap.mirror_enabled = if daemon_path.exists() {
                    let content = std::fs::read_to_string(daemon_path).unwrap_or_default();
                    content.contains(&registry_addr)
                        || content.contains(&crate::config::CACHE_REGISTRY_PORT.to_string())
                } else {
                    false
                };

                snap.ca_mounted =
                    std::path::Path::new("/etc/ssl/certs/ca-certificates.crt").exists();

                if let Ok(metrics) = db.get_cache_metrics().await {
                    snap.hot_cache_usage_bytes = metrics.bytes_served;
                    snap.cache_hits = metrics.hit_count;
                    snap.cache_objects_count = metrics.object_count;
                    snap.singleflight_requests = metrics.singleflight_coalesced;
                    snap.hit_ratio = metrics.hit_ratio;
                    snap.miss_count = metrics.miss_count;
                    snap.total_requests = metrics.total_requests;
                }

                // Real taint and verdict counts from DB
                if let Ok(count) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cache_taints")
                    .fetch_one(&db.pool())
                    .await
                {
                    snap.active_taint_count = count;
                }
                if let Ok(count) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cache_taints WHERE reason LIKE '%Tripwire%' OR reason LIKE '%tripwire%'")
                    .fetch_one(&db.pool()).await {
                    snap.detonation_breaches = count;
                }
                if let Ok(count) = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM cache_verdicts WHERE verdict LIKE '%Miss%'",
                )
                .fetch_one(&db.pool())
                .await
                {
                    snap.cold_execution_downgrades = count;
                }

                // Real disk usage for CAS and crate cache
                let cas_dir = crate::config::data_dir().join("cas");
                let crate_dir = crate::config::data_dir().join("cache").join("crates");
                snap.cas_disk_bytes = dir_size_bytes(&cas_dir).await;
                snap.crate_cache_disk_bytes = dir_size_bytes(&crate_dir).await;

                if let Ok(avg) = db.get_test_bottlenecks("average", 50).await {
                    snap.test_bottlenecks_avg = avg;
                }
                if let Ok(lat) = db.get_test_bottlenecks("latest", 50).await {
                    snap.test_bottlenecks_latest = lat;
                }

                if let Ok(evidence) = db.recent_evidence_all(30).await {
                    snap.recent_evidence = evidence;
                }
                if let Ok(secrets) = db.all_recent_secret_audit_events(20).await {
                    snap.secret_audit_events = secrets;
                }
                if let Ok(agent_pipes) = db.list_agent_pipelines().await {
                    snap.agent_pipelines = agent_pipes;
                }
                if let Ok(events) = db.get_events(50).await {
                    snap.recent_audit_events = events;
                }
                if let Ok(events) = db.recent_git_command_events(30).await {
                    snap.recent_git_events = events;
                }
                snap.last_sync_at = Some(chrono::Utc::now());

                // Storage Metrics background queries
                if let Ok(df_output) = tokio::process::Command::new("df")
                    .args(["-k", "/"])
                    .output()
                    .await
                {
                    let s = String::from_utf8_lossy(&df_output.stdout);
                    if let Some(line) = s.lines().nth(1) {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 4 {
                            snap.storage_breakdown.total_disk_bytes =
                                parts[1].parse::<u64>().unwrap_or(0) * 1024;
                            snap.storage_breakdown.disk_available_bytes =
                                parts[3].parse::<u64>().unwrap_or(0) * 1024;
                        }
                    }
                }

                // Fetch other storage queries roughly. (Since they are heavy, in a real app would cache them and do them every 60s instead of 1s, but this is a POC)
                snap.storage_breakdown.cas_bytes = snap.cas_disk_bytes as u64;
                snap.storage_breakdown.crate_cache_bytes = snap.crate_cache_disk_bytes as u64;
                snap.storage_breakdown.jeryu_db_bytes =
                    std::fs::metadata(crate::config::data_dir().join("jeryu.db"))
                        .map(|m| m.len())
                        .unwrap_or(0);

                snap.storage_breakdown.runner_data_bytes =
                    dir_size_bytes(&crate::config::data_dir().join("runners")).await as u64;

                // Docker generic estimations (system df can be slow so we skip parsing detailed system df for this tight loop, substituting some approximations or just keeping them 0 for now)

                if tx.send(snap).await.is_err() {
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            }
        });
    }

    pub async fn refresh_now(&mut self) {
        let mut snap = TuiStateSnapshot::default();
        Self::hydrate_core_snapshot(&mut snap, &self.db, &self.docker, &self.gitlab).await;
        self.state = snap;
    }

    async fn hydrate_core_snapshot(
        snap: &mut TuiStateSnapshot,
        db: &Db,
        docker: &DockerCtl,
        gitlab: &GitlabClient,
    ) {
        if let Ok(pools) = db.list_pools().await {
            snap.pools = pools;
        }

        if let Ok(managed) = docker.list_managed_containers().await {
            snap.active_containers = managed.len();
        }

        let mut jobs = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        if let Ok(active_jobs) = gitlab
            .list_jobs(
                release::DEFAULT_RELEASE_PROJECT_ID,
                &["running", "pending", "created"],
            )
            .await
        {
            let now = chrono::Utc::now().to_rfc3339();
            for job in active_jobs {
                seen.insert(job.id);
                let pool_name = job
                    .runner
                    .and_then(|runner| runner.description)
                    .or(Some(job.stage));
                jobs.push(JobEvent {
                    job_id: job.id,
                    project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                    pipeline_id: None,
                    status: job.status,
                    job_name: Some(job.name),
                    pool_name,
                    system_id: None,
                    queued_duration: job.queued_duration,
                    received_at: job.started_at.unwrap_or_else(|| now.clone()),
                });
            }
        }

        if let Ok(db_jobs) = db.recent_job_events(50).await {
            jobs.extend(
                db_jobs
                    .into_iter()
                    .filter(|job| seen.insert(job.job_id))
                    .take(50usize.saturating_sub(jobs.len())),
            );
        }

        jobs.sort_by(|left, right| {
            live_job_status_rank(&right.status)
                .cmp(&live_job_status_rank(&left.status))
                .then_with(|| right.received_at.cmp(&left.received_at))
                .then_with(|| right.job_id.cmp(&left.job_id))
        });
        snap.recent_jobs = jobs;
        snap.gitlab_ready = gitlab.is_ready().await;
    }

    pub async fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        while let Ok(mut state) = self.sync_rx.try_recv() {
            // Preserve live sub-state that is updated on separate channels
            state.flow = self.state.flow.clone();
            state.live_log = self.state.live_log.clone();
            state.inspector_capsule = self.state.inspector_capsule.clone();
            state.inspector_job_id = self.state.inspector_job_id;
            // Preserve TUI v2 feed state
            state.runner_feeds = self.state.runner_feeds.clone();
            state.active_feed_index = self.state.active_feed_index;
            state.feed_cycle_tick = self.state.feed_cycle_tick;
            state.feed_auto_cycle = self.state.feed_auto_cycle;
            state.pipeline_progress_view = self.state.pipeline_progress_view.clone();
            state.event_ticker_offset = self.state.event_ticker_offset;
            self.state = state;
        }

        while let Ok(flow_snap) = self.flow_rx.try_recv() {
            self.apply_flow_snapshot(flow_snap);
        }

        while let Ok(log_state) = self.log_rx.try_recv() {
            self.state.live_log = log_state;
        }

        // TUI v2 — consume runner feed updates
        while let Ok(feeds) = self.feed_rx.try_recv() {
            self.state.runner_feeds = feeds;
        }

        // TUI v2 — auto-cycle runner feed every FEED_CYCLE_TICKS (5s at 250ms tick)
        if !self.state.runner_feeds.is_empty() && self.feed_pinned.is_none() {
            self.state.feed_cycle_tick = self.state.feed_cycle_tick.wrapping_add(1);
            if self.state.feed_cycle_tick % FEED_CYCLE_TICKS == 0 {
                self.state.active_feed_index =
                    (self.state.active_feed_index + 1) % self.state.runner_feeds.len();
                self.state.feed_auto_cycle = true;
                self.feed_scroll_offset = 0;
                self.feed_follow_tail = true;
            }
        }
        if let Some(pinned) = self.feed_pinned {
            self.state.active_feed_index =
                pinned.min(self.state.runner_feeds.len().saturating_sub(1));
            self.state.feed_auto_cycle = false;
        }

        // TUI v2 — advance event ticker
        if self.tick_count % 2 == 0 {
            self.state.event_ticker_offset = self.state.event_ticker_offset.wrapping_add(1);
        }

        // Clamp indices
        if self.selected_pool_index >= self.state.pools.len() && !self.state.pools.is_empty() {
            self.selected_pool_index = self.state.pools.len() - 1;
        }
        if self.selected_pipeline_index >= self.state.pipelines.len()
            && !self.state.pipelines.is_empty()
        {
            self.selected_pipeline_index = self.state.pipelines.len() - 1;
        }
        self.sync_selected_job_index();
        self.update_log_target();

        // Fetch inspector capsule when selected job changes
        let current_job_id = self.selected_job_id;
        if current_job_id != self.state.inspector_job_id {
            self.state.inspector_job_id = current_job_id;
            if let Some(jid) = current_job_id {
                self.state.inspector_capsule =
                    self.db.latest_evidence_by_job_id(jid).await.ok().flatten();
            } else {
                self.state.inspector_capsule = None;
            }
        }
    }

    fn apply_flow_snapshot(&mut self, mut flow_snap: crate::tui::flow::FlowSnapshot) {
        if flow_snap.active_pipelines.is_empty() && !self.state.flow.active_pipelines.is_empty() {
            flow_snap.active_pipelines = self.state.flow.active_pipelines.clone();
            flow_snap.stale = true;
            flow_snap.last_non_empty_at = self
                .state
                .flow
                .last_non_empty_at
                .or(Some(self.state.flow.generated_at));
            flow_snap.selected_pipeline_id = self.state.flow.selected_pipeline_id;
        } else if flow_snap.active_pipelines.is_empty()
            && let Some(fallback) = self.flow_from_recent_jobs(flow_snap.generated_at)
        {
            flow_snap.active_pipelines = vec![fallback];
            flow_snap.stale = true;
            flow_snap.last_non_empty_at = Some(flow_snap.generated_at);
        } else if !flow_snap.active_pipelines.is_empty() {
            flow_snap.last_non_empty_at =
                flow_snap.last_non_empty_at.or(Some(flow_snap.generated_at));
        }

        self.state.flow = flow_snap;
    }

    fn flow_from_recent_jobs(
        &self,
        _generated_at: chrono::DateTime<chrono::Utc>,
    ) -> Option<crate::tui::flow::PipelineFlow> {
        let release = self.state.release_status.as_ref();
        let pipeline_id = release
            .and_then(|view| view.attempt.release_pipeline_id)
            .or_else(|| {
                self.state
                    .pipelines
                    .get(self.selected_pipeline_index)
                    .map(|metrics| metrics.pipeline.pipeline_id)
            })
            .or_else(|| {
                self.state
                    .recent_jobs
                    .iter()
                    .find_map(|job| job.pipeline_id)
            })?;

        let project_id = release
            .map(|view| view.attempt.project_id)
            .or_else(|| {
                self.state
                    .recent_jobs
                    .iter()
                    .find(|job| job.pipeline_id == Some(pipeline_id))
                    .map(|job| job.project_id)
            })
            .unwrap_or(release::DEFAULT_RELEASE_PROJECT_ID);

        let ref_name = release
            .map(|view| view.attempt.ref_name.clone())
            .or_else(|| {
                self.state
                    .pipelines
                    .iter()
                    .find(|metrics| metrics.pipeline.pipeline_id == pipeline_id)
                    .map(|metrics| metrics.pipeline.ref_name.clone())
            })
            .unwrap_or_else(|| "main".to_string());

        let sha = release.map(|view| view.attempt.sha.clone()).or_else(|| {
            self.state
                .pipelines
                .iter()
                .find(|metrics| metrics.pipeline.pipeline_id == pipeline_id)
                .map(|metrics| metrics.pipeline.sha.clone())
        });

        let status = release
            .and_then(|view| view.attempt.release_pipeline_status.clone())
            .or_else(|| {
                self.state
                    .pipelines
                    .iter()
                    .find(|metrics| metrics.pipeline.pipeline_id == pipeline_id)
                    .map(|metrics| metrics.pipeline.status.clone())
            })
            .unwrap_or_else(|| "unknown".to_string());

        let jobs = self
            .state
            .recent_jobs
            .iter()
            .filter(|job| job.pipeline_id == Some(pipeline_id))
            .cloned()
            .collect::<Vec<_>>();

        if jobs.is_empty() {
            return None;
        }

        let graph = crate::tui::flow::build_graph(pipeline_id, jobs);
        let total = graph.nodes.len();
        let completed = graph
            .nodes
            .iter()
            .filter(|node| matches!(node.status.as_str(), "success" | "failed" | "canceled"))
            .count();
        let running = graph
            .nodes
            .iter()
            .filter(|node| node.status == "running")
            .count();
        let progress_pct = if total > 0 {
            (((completed as f64 + running as f64 * 0.5) / total as f64) * 100.0) as u16
        } else {
            0
        };
        let current_blocker = graph
            .nodes
            .iter()
            .filter(|node| node.status == "running" || node.status == "failed")
            .max_by_key(|node| node.elapsed_secs)
            .and_then(|node| node.job_id);

        Some(crate::tui::flow::PipelineFlow {
            pipeline_id,
            project_id,
            ref_name,
            sha,
            status,
            graph,
            current_blocker,
            critical_path: Vec::new(),
            eta: None,
            progress_pct,
        })
    }

    pub fn cycle_tab_next(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Mission => ActiveTab::Release,
            ActiveTab::Release => ActiveTab::Jobs,
            ActiveTab::Jobs => ActiveTab::Agents,
            ActiveTab::Agents => ActiveTab::Tests,
            ActiveTab::Tests => ActiveTab::Pools,
            ActiveTab::Pools => ActiveTab::Cache,
            ActiveTab::Cache => ActiveTab::Evidence,
            ActiveTab::Evidence => ActiveTab::Secrets,
            ActiveTab::Secrets => ActiveTab::Git,
            ActiveTab::Git => ActiveTab::Mission,
        };
    }

    pub fn cycle_pane_next(&mut self) {
        // Only Jobs is currently rendered; cycling to Pools/Pipelines would silently
        // focus invisible panes. Expand this when those panes are visible.
        self.active_pane = ActivePane::Jobs;
        self.update_log_target();
    }

    pub fn cycle_pane_prev(&mut self) {
        self.active_pane = ActivePane::Jobs;
        self.update_log_target();
    }

    pub fn up(&mut self) {
        if self.active_tab == ActiveTab::Tests {
            let limit = match self.test_view_mode {
                TestViewMode::Average => self.state.test_bottlenecks_avg.len(),
                TestViewMode::Latest => self.state.test_bottlenecks_latest.len(),
            };
            if limit > 0 {
                if self.selected_test_index > 0 {
                    self.selected_test_index -= 1;
                } else {
                    self.selected_test_index = limit - 1;
                }
                self.selected_test_history = None; // clear history when moving
            }
            return;
        }

        match self.active_pane {
            ActivePane::Pools => {
                if !self.state.pools.is_empty() {
                    if self.selected_pool_index > 0 {
                        self.selected_pool_index -= 1;
                    } else {
                        self.selected_pool_index = self.state.pools.len() - 1;
                    }
                }
            }
            ActivePane::Pipelines => {
                if !self.state.pipelines.is_empty() {
                    if self.selected_pipeline_index > 0 {
                        self.selected_pipeline_index -= 1;
                    } else {
                        self.selected_pipeline_index = self.state.pipelines.len() - 1;
                    }
                }
            }
            ActivePane::Jobs => {
                if !self.state.recent_jobs.is_empty() {
                    if self.selected_job_index > 0 {
                        self.selected_job_index -= 1;
                    } else {
                        self.selected_job_index = self.state.recent_jobs.len() - 1;
                    }
                    self.remember_selected_job();
                }
            }
        }
        self.update_log_target();
    }

    pub fn down(&mut self) {
        if self.active_tab == ActiveTab::Tests {
            let limit = match self.test_view_mode {
                TestViewMode::Average => self.state.test_bottlenecks_avg.len(),
                TestViewMode::Latest => self.state.test_bottlenecks_latest.len(),
            };
            if limit > 0 {
                self.selected_test_index = (self.selected_test_index + 1) % limit;
                self.selected_test_history = None; // clear history when moving
            }
            return;
        }

        match self.active_pane {
            ActivePane::Pools => {
                if !self.state.pools.is_empty() {
                    self.selected_pool_index =
                        (self.selected_pool_index + 1) % self.state.pools.len();
                }
            }
            ActivePane::Pipelines => {
                if !self.state.pipelines.is_empty() {
                    self.selected_pipeline_index =
                        (self.selected_pipeline_index + 1) % self.state.pipelines.len();
                }
            }
            ActivePane::Jobs => {
                if !self.state.recent_jobs.is_empty() {
                    self.selected_job_index =
                        (self.selected_job_index + 1) % self.state.recent_jobs.len();
                    self.remember_selected_job();
                }
            }
        }
        self.update_log_target();
    }

    fn update_log_target(&mut self) {
        if self.maximize_logs
            && let Some(job) = self.selected_job()
        {
            let target = Some(LogTarget {
                project_id: job.project_id,
                job_id: job.job_id,
            });
            if self.log_target != target {
                self.log_target = target;
                let _ = self.log_target_tx.send(target);
            }
            return;
        }
        if self.log_target.is_some() {
            self.log_target = None;
            let _ = self.log_target_tx.send(None);
        }
    }

    fn sync_selected_job_index(&mut self) {
        if self.state.recent_jobs.is_empty() {
            self.selected_job_index = 0;
            self.selected_job_id = None;
            return;
        }

        if let Some(job_id) = self.selected_job_id
            && let Some(index) = self
                .state
                .recent_jobs
                .iter()
                .position(|job| job.job_id == job_id)
        {
            self.selected_job_index = index;
            return;
        }

        if self.selected_job_index >= self.state.recent_jobs.len() {
            self.selected_job_index = self.state.recent_jobs.len() - 1;
        }
        self.remember_selected_job();
    }

    fn remember_selected_job(&mut self) {
        self.selected_job_id = self.selected_job().map(|job| job.job_id);
    }

    pub fn selected_job(&self) -> Option<&JobEvent> {
        self.state.recent_jobs.get(self.selected_job_index)
    }

    pub fn open_selected_job_log(&mut self) {
        self.active_pane = ActivePane::Jobs;
        self.remember_selected_job();
        self.maximize_logs = true;
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
        self.update_log_target();
    }

    pub fn close_log_view(&mut self) {
        self.maximize_logs = false;
        self.update_log_target();
    }

    pub fn scroll_logs_up(&mut self, amount: u16) {
        self.follow_log_tail = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_logs_down(&mut self, amount: u16) {
        self.follow_log_tail = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_add(amount);
    }

    pub fn follow_logs(&mut self) {
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
    }

    pub fn jump_logs_top(&mut self) {
        self.follow_log_tail = false;
        self.log_scroll_offset = 0;
    }

    pub async fn toggle_pool_paused(&mut self) -> Result<()> {
        if let Some(pool) = self.state.pools.get(self.selected_pool_index) {
            if pool.paused {
                crate::pool::resume_pool(&self.db, &self.gitlab, &pool.name).await?;
            } else {
                crate::pool::pause_pool(&self.db, &self.gitlab, &pool.name).await?;
            }
        }
        Ok(())
    }

    pub async fn delete_selected_item(&mut self) -> Result<()> {
        match self.active_pane {
            ActivePane::Pipelines => {
                if let Some(pm) = self.state.pipelines.get(self.selected_pipeline_index) {
                    let pid = pm.pipeline.pipeline_id;
                    self.db.delete_pipeline(pid).await?;
                    // Remove from local state immediately for snappy UX
                    self.state.pipelines.remove(self.selected_pipeline_index);
                    if self.selected_pipeline_index > 0 {
                        self.selected_pipeline_index -= 1;
                    }
                }
            }
            ActivePane::Jobs => {
                if let Some(j) = self.state.recent_jobs.get(self.selected_job_index) {
                    let jid = j.job_id;
                    self.db.delete_job_event(jid).await?;
                    self.state.recent_jobs.remove(self.selected_job_index);
                    if self.selected_job_index > 0 {
                        self.selected_job_index -= 1;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn retry_selected_job(&mut self) -> Result<()> {
        if self.active_pane == ActivePane::Jobs
            && let Some(j) = self.state.recent_jobs.get(self.selected_job_index)
            && j.status == "failed"
        {
            self.gitlab.retry_job(j.project_id, j.job_id).await?;
        }
        Ok(())
    }

    pub fn toggle_test_view_mode(&mut self) {
        self.test_view_mode = match self.test_view_mode {
            TestViewMode::Average => TestViewMode::Latest,
            TestViewMode::Latest => TestViewMode::Average,
        };
        self.selected_test_index = 0;
        self.selected_test_history = None;
    }

    pub async fn fetch_selected_test_history(&mut self) {
        let bottlenecks = match self.test_view_mode {
            TestViewMode::Average => &self.state.test_bottlenecks_avg,
            TestViewMode::Latest => &self.state.test_bottlenecks_latest,
        };
        if let Some(b) = bottlenecks.get(self.selected_test_index)
            && let Ok(hist) = self.db.get_test_history(&b.test_name, 50).await
        {
            self.selected_test_history = Some(hist);
        }
    }

    // -----------------------------------------------------------------------
    // TUI v2 — Runner feed controls
    // -----------------------------------------------------------------------

    pub fn feed_next(&mut self) {
        if !self.state.runner_feeds.is_empty() {
            self.state.active_feed_index =
                (self.state.active_feed_index + 1) % self.state.runner_feeds.len();
            self.feed_scroll_offset = 0;
            self.feed_follow_tail = true;
        }
    }

    pub fn feed_prev(&mut self) {
        if !self.state.runner_feeds.is_empty() {
            if self.state.active_feed_index > 0 {
                self.state.active_feed_index -= 1;
            } else {
                self.state.active_feed_index = self.state.runner_feeds.len() - 1;
            }
            self.feed_scroll_offset = 0;
            self.feed_follow_tail = true;
        }
    }

    pub fn feed_toggle_pin(&mut self) {
        if self.feed_pinned.is_some() {
            self.feed_pinned = None;
        } else {
            self.feed_pinned = Some(self.state.active_feed_index);
        }
    }

    pub fn feed_follow_toggle(&mut self) {
        self.feed_follow_tail = !self.feed_follow_tail;
        if self.feed_follow_tail {
            self.feed_scroll_offset = u16::MAX;
        }
    }

    // TUI v2 — Interactive actions

    pub async fn cancel_selected_job(&mut self) -> Result<()> {
        if let Some(j) = self.state.recent_jobs.get(self.selected_job_index) {
            self.gitlab.cancel_job(j.project_id, j.job_id).await?;
        }
        Ok(())
    }

    pub async fn force_refresh(&mut self) {
        self.refresh_now().await;
    }
}

fn live_job_status_rank(status: &str) -> u8 {
    match status {
        "running" => 4,
        "waiting_for_resource" | "preparing" => 3,
        "pending" => 2,
        "created" => 1,
        _ => 0,
    }
}

fn retain_tail(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }

    let mut start = input.len().saturating_sub(max_bytes);
    while !input.is_char_boundary(start) {
        start += 1;
    }

    format!("... (truncated)\n{}", &input[start..])
}

/// Recursively calculate the size of a directory in bytes.
async fn dir_size_bytes(path: &std::path::Path) -> i64 {
    let mut total: i64 = 0;
    if let Ok(mut entries) = tokio::fs::read_dir(path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(meta) = entry.metadata().await {
                if meta.is_file() {
                    total += meta.len() as i64;
                } else if meta.is_dir() {
                    total += Box::pin(dir_size_bytes(&entry.path())).await;
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::{App, LiveLogState, live_job_status_rank};
    use crate::state::JobEvent;
    use crate::tui::flow::{FlowGraph, FlowSnapshot, PipelineFlow};
    use anyhow::Result;

    async fn test_app() -> Result<App> {
        let db = crate::state::Db::open_memory().await?;
        let docker = crate::docker::DockerCtl::connect()?;
        let gitlab = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:9", None);
        Ok(App::new(db, docker, gitlab))
    }

    fn job(job_id: i64, status: &str, received_at: &str) -> JobEvent {
        JobEvent {
            job_id,
            project_id: 2,
            pipeline_id: Some(10),
            status: status.into(),
            job_name: Some(format!("test-job-{job_id}")),
            pool_name: Some("default".into()),
            system_id: None,
            queued_duration: None,
            received_at: received_at.into(),
        }
    }

    fn pipeline_flow(pipeline_id: i64) -> PipelineFlow {
        PipelineFlow {
            pipeline_id,
            project_id: 2,
            ref_name: "main".into(),
            sha: Some("abc123".into()),
            status: "running".into(),
            graph: FlowGraph::default(),
            current_blocker: None,
            critical_path: Vec::new(),
            eta: None,
            progress_pct: 50,
        }
    }

    #[test]
    fn live_jobs_sort_running_ahead_of_created_and_pending() {
        let mut jobs = [
            JobEvent {
                job_id: 1,
                project_id: 2,
                pipeline_id: None,
                status: "created".into(),
                job_name: Some("build-enclave-server".into()),
                pool_name: Some("x86-64".into()),
                system_id: None,
                queued_duration: None,
                received_at: "2026-04-23T19:00:00Z".into(),
            },
            JobEvent {
                job_id: 2,
                project_id: 2,
                pipeline_id: None,
                status: "running".into(),
                job_name: Some("test-rust-nextest-1".into()),
                pool_name: Some("build".into()),
                system_id: None,
                queued_duration: None,
                received_at: "2026-04-23T19:01:00Z".into(),
            },
            JobEvent {
                job_id: 3,
                project_id: 2,
                pipeline_id: None,
                status: "pending".into(),
                job_name: Some("test-rust-nextest-4".into()),
                pool_name: Some("build".into()),
                system_id: None,
                queued_duration: None,
                received_at: "2026-04-23T19:02:00Z".into(),
            },
        ];

        jobs.sort_by(|left, right| {
            live_job_status_rank(&right.status)
                .cmp(&live_job_status_rank(&left.status))
                .then_with(|| right.received_at.cmp(&left.received_at))
                .then_with(|| right.job_id.cmp(&left.job_id))
        });

        let statuses: Vec<_> = jobs.iter().map(|job| job.status.as_str()).collect();
        assert_eq!(statuses, vec!["running", "pending", "created"]);
    }

    #[tokio::test]
    async fn core_snapshot_preserves_flow_and_live_log_state() -> Result<()> {
        let mut app = test_app().await?;
        app.state.flow.stale = true;
        app.state.live_log = LiveLogState {
            text: "running test output".into(),
            ..Default::default()
        };

        app.sync_tx
            .send(super::TuiStateSnapshot::default())
            .await
            .unwrap();
        app.tick().await;

        assert!(app.state.flow.stale);
        assert_eq!(app.state.live_log.text, "running test output");
        Ok(())
    }

    #[tokio::test]
    async fn empty_flow_snapshot_does_not_blank_existing_board() -> Result<()> {
        let mut app = test_app().await?;
        let generated_at = chrono::Utc::now();
        app.state.flow = FlowSnapshot {
            generated_at,
            active_pipelines: vec![pipeline_flow(42)],
            last_non_empty_at: Some(generated_at),
            ..Default::default()
        };

        app.flow_tx
            .send(FlowSnapshot {
                generated_at: generated_at + chrono::Duration::seconds(5),
                gitlab_online: true,
                ..Default::default()
            })
            .await
            .unwrap();
        app.tick().await;

        assert_eq!(app.state.flow.active_pipelines.len(), 1);
        assert_eq!(app.state.flow.active_pipelines[0].pipeline_id, 42);
        assert!(app.state.flow.stale);
        assert_eq!(app.state.flow.last_non_empty_at, Some(generated_at));
        assert!(app.state.flow.gitlab_online);
        Ok(())
    }

    #[tokio::test]
    async fn empty_flow_snapshot_uses_recent_jobs_before_collector_graph_arrives() -> Result<()> {
        let mut app = test_app().await?;
        let generated_at = chrono::Utc::now();
        app.state.recent_jobs = vec![
            JobEvent {
                job_id: 7,
                project_id: 2,
                pipeline_id: Some(55),
                status: "running".into(),
                job_name: Some("test-frontend-nht".into()),
                pool_name: Some("default".into()),
                system_id: None,
                queued_duration: None,
                received_at: "2026-04-23T19:00:00Z".into(),
            },
            JobEvent {
                job_id: 8,
                project_id: 2,
                pipeline_id: Some(55),
                status: "created".into(),
                job_name: Some("test-local-rc".into()),
                pool_name: Some("build".into()),
                system_id: None,
                queued_duration: None,
                received_at: "2026-04-23T19:01:00Z".into(),
            },
        ];

        app.flow_tx
            .send(FlowSnapshot {
                generated_at,
                gitlab_online: true,
                ..Default::default()
            })
            .await
            .unwrap();
        app.tick().await;

        assert_eq!(app.state.flow.active_pipelines.len(), 1);
        assert_eq!(app.state.flow.active_pipelines[0].pipeline_id, 55);
        assert_eq!(app.state.flow.active_pipelines[0].graph.nodes.len(), 2);
        assert!(app.state.flow.stale);
        Ok(())
    }

    #[tokio::test]
    async fn selected_job_survives_refresh_reorder() -> Result<()> {
        let mut app = test_app().await?;
        app.state.recent_jobs = vec![
            job(1, "running", "2026-04-23T19:00:00Z"),
            job(2, "pending", "2026-04-23T19:01:00Z"),
        ];
        app.selected_job_index = 1;
        app.remember_selected_job();

        let snap = super::TuiStateSnapshot {
            recent_jobs: vec![
                job(2, "running", "2026-04-23T19:02:00Z"),
                job(1, "success", "2026-04-23T19:03:00Z"),
            ],
            ..Default::default()
        };
        app.sync_tx.send(snap).await.unwrap();
        app.tick().await;

        assert_eq!(app.selected_job_index, 0);
        assert_eq!(app.selected_job().map(|job| job.job_id), Some(2));
        assert_eq!(app.log_target.map(|target| target.job_id), None);
        Ok(())
    }

    #[tokio::test]
    async fn opening_and_scrolling_logs_controls_follow_mode() -> Result<()> {
        let mut app = test_app().await?;
        app.state.recent_jobs = vec![job(7, "running", "2026-04-23T19:00:00Z")];

        app.open_selected_job_log();
        assert!(app.maximize_logs);
        assert!(app.follow_log_tail);
        assert_eq!(app.log_target.map(|target| target.job_id), Some(7));

        app.scroll_logs_up(1);
        assert!(!app.follow_log_tail);

        app.follow_logs();
        assert!(app.follow_log_tail);
        assert_eq!(app.log_scroll_offset, u16::MAX);
        Ok(())
    }
}
