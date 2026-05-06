//! Owner: Release Pipeline
//! Proof: `cargo test -p jeryu -- release`
//! Invariants: Exact-SHA evidence matching, canary gate ladder, immutable evidence dirs

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::process::Command;
use tracing::{info, warn};

use crate::gitlab_client::{GitlabClient, Job, Pipeline};
use crate::state::{Db, ReleaseAttempt};

/// Typed release pipeline errors for programmatic failure classification.
#[derive(Debug, Error)]
pub enum ReleaseError {
    #[error("canary gate rejected for {version}: state is {state} (expected e2e-passed)")]
    CanaryGateRejected { version: String, state: String },

    #[error("missing C artifact handoff for {version} at {path}")]
    MissingHandoff { version: String, path: PathBuf },

    #[error("missing C validation artifact for {version} at {path}")]
    MissingValidation { version: String, path: PathBuf },

    #[error("CI schema command failed: {stderr}")]
    CiSchemaFailed { stderr: String },
}

pub const DEFAULT_RELEASE_PROJECT_ID: i64 = 2;

fn render_release_version(sha: &str) -> String {
    format!("ci-{}", sha.chars().take(12).collect::<String>())
}

fn release_dir(version: &str) -> PathBuf {
    crate::settings::release_repo_root()
        .join("ops/releases")
        .join(version)
}

fn canary_state_path(version: &str) -> PathBuf {
    release_dir(version).join("deploy-canary-c-state.json")
}

fn gate_remote_canary_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-remote-canary.json")
}

fn gate_canary_e2e_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-e2e.json")
}

fn gate_canary_telemetry_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-telemetry.json")
}

fn gate_prod_promotion_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-prod-promotion.json")
}

fn telemetry_diag_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-telemetry-diagnostics.json")
}

fn c_handoff_path(version: &str) -> PathBuf {
    release_dir(version).join("rendered/c-handoff.json")
}

fn c_validation_path(version: &str) -> PathBuf {
    release_dir(version).join("c-validation.json")
}

/// Download gate files and handoff artifacts from the deploy-canary-final job
/// of a release-execution pipeline to local disk. Non-fatal: logs and returns Ok
/// if the job is not found or individual artifacts are missing.
async fn sync_canary_artifacts(
    client: &GitlabClient,
    project_id: i64,
    release_pipeline_id: i64,
    version: &str,
) -> Result<()> {
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, release_pipeline_id)
        .await?;
    let Some(canary_job) = jobs
        .iter()
        .find(|j| j.name == "deploy-canary-final" && j.status == "success")
    else {
        return Ok(());
    };
    let release_root = release_dir(version);
    if let Err(err) = fs::create_dir_all(&release_root) {
        warn!(version, error = %err, "could not create release dir for artifact sync");
        return Ok(());
    }
    let _ = fs::create_dir_all(release_root.join("rendered"));
    let artifacts = [
        (
            format!("ops/releases/{version}/gate-remote-canary.json"),
            "gate-remote-canary.json",
        ),
        (
            format!("ops/releases/{version}/gate-canary-telemetry.json"),
            "gate-canary-telemetry.json",
        ),
        (
            format!("ops/releases/{version}/gate-canary-e2e.json"),
            "gate-canary-e2e.json",
        ),
        (
            format!("ops/releases/{version}/c-validation.json"),
            "c-validation.json",
        ),
        (
            format!("ops/releases/{version}/deploy-canary-c-state.json"),
            "deploy-canary-c-state.json",
        ),
        (
            format!("ops/releases/{version}/release.json"),
            "release.json",
        ),
        (
            format!("ops/releases/{version}/release.json.sig"),
            "release.json.sig",
        ),
        (
            format!("ops/releases/{version}/release-contract.json"),
            "release-contract.json",
        ),
        (format!("ops/releases/{version}/image.env"), "image.env"),
        (
            format!("ops/releases/{version}/payload-manifest.json"),
            "payload-manifest.json",
        ),
        (format!("ops/releases/{version}/deks.env"), "deks.env"),
        (
            format!("ops/releases/{version}/rendered/c-handoff.json"),
            "rendered/c-handoff.json",
        ),
        (
            format!("ops/releases/{version}/rendered/c-slave.env"),
            "rendered/c-slave.env",
        ),
    ];
    for (artifact_path, local_name) in &artifacts {
        let dest = release_root.join(local_name);
        match client
            .job_artifact_file(project_id, canary_job.id, artifact_path)
            .await
        {
            Ok(content) => {
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(err) = fs::write(&dest, content.as_bytes()) {
                    warn!(version, artifact = local_name, error = %err, "could not write synced artifact");
                } else {
                    info!(
                        version,
                        artifact = local_name,
                        "synced canary artifact from CI"
                    );
                }
            }
            Err(err) => {
                warn!(version, artifact = local_name, error = %err, "canary artifact not available in CI");
            }
        }
    }
    Ok(())
}

fn canary_public_url(version: &str) -> Option<String> {
    let raw = fs::read_to_string(c_handoff_path(version)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    for key in [
        "target_url",
        "release_unique_url",
        "unique_canary_url",
        "canary_url",
        "public_url",
    ] {
        if let Some(url) = value.get(key).and_then(|v| v.as_str()) {
            return Some(url.to_string());
        }
    }
    None
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseAttemptView {
    pub attempt: ReleaseAttempt,
    pub release_dir: String,
    pub canary_state_path: String,
    pub gate_remote_canary_path: String,
    pub gate_canary_e2e_path: String,
    pub gate_canary_telemetry_path: String,
    pub telemetry_diag_path: String,
    pub canary_state: String,
    pub eligibility: String,
    pub phase: Option<String>,
    pub detail: Option<String>,
    pub state_status: Option<String>,
    pub has_remote_gate: bool,
    pub has_telemetry_gate: bool,
    pub has_e2e_gate: bool,
    pub has_telemetry_diag: bool,
    pub release_identity_ok: bool,
    pub canary_public_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseStatusReport {
    pub generated_at: String,
    pub project_id: Option<i64>,
    pub ref_name: Option<String>,
    pub sha: Option<String>,
    pub limit: usize,
    pub total_attempts: usize,
    pub latest: Option<ReleaseAttemptView>,
    pub recent: Vec<ReleaseAttemptView>,
}

#[derive(Debug, Clone)]
pub struct ReleaseStatusQuery {
    pub project_id: Option<i64>,
    pub ref_name: Option<String>,
    pub sha: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct CiSchema {
    jobs: Vec<CiSchemaJob>,
    #[serde(default)]
    milestones: Vec<CiSchemaMilestone>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct CiSchemaJob {
    id: String,
    lane: String,
    release_blocking: bool,
    #[serde(default)]
    section: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    runner_tags: String,
    #[serde(default)]
    runner_pool: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    component: String,
    #[serde(default)]
    pipeline_product: String,
    #[serde(default)]
    evidence_driven: bool,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    evidence_outputs: Vec<String>,
    #[serde(default)]
    estimated_cost: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct CiSchemaMilestone {
    id: String,
    title: String,
    lane: String,
    release_blocking: bool,
    #[serde(default)]
    pipeline_product: String,
    jobs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LaneProgress {
    pub passed: usize,
    pub total: usize,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ReleaseExecutionProgress {
    pub percent: f64,
    pub attempt_exists: bool,
    pub remote_gate: bool,
    pub telemetry_gate: bool,
    pub e2e_gate: bool,
    pub punchlist_current: bool,
    pub latest_attempt_sha: Option<String>,
    pub latest_attempt_state: Option<String>,
    pub phase: Option<String>,
    pub eligibility: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressReport {
    pub generated_at: String,
    pub project_id: i64,
    pub ref_name: String,
    pub latest_pipeline_id: Option<i64>,
    pub latest_pipeline_status: Option<String>,
    pub latest_pipeline_sha: Option<String>,
    pub winning_pipeline_id: Option<i64>,
    pub winning_sha: Option<String>,
    pub expected_release_version: Option<String>,
    pub release_critical: LaneProgress,
    pub extended: LaneProgress,
    pub research: LaneProgress,
    pub release_execution: ReleaseExecutionProgress,
    pub blocking_remaining: Vec<String>,
    pub non_blocking_failed: Vec<String>,
    pub current_blocker: Option<String>,
    pub punchlist_freshness: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineExplainItem {
    pub id: String,
    pub status: String,
    pub stage: Option<String>,
    pub runner_pool: String,
    pub kind: String,
    pub component: String,
    pub evidence_driven: bool,
    pub estimated_cost: String,
    pub evidence_outputs: Vec<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineExplainMilestone {
    pub id: String,
    pub title: String,
    pub status: String,
    pub lane: String,
    pub jobs: Vec<String>,
    pub incomplete_jobs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineExplainReport {
    pub generated_at: String,
    pub project_id: i64,
    pub pipeline_id: i64,
    pub pipeline_sha: String,
    pub pipeline_ref: String,
    pub pipeline_status: String,
    pub release_critical: LaneProgress,
    pub extended: LaneProgress,
    pub research: LaneProgress,
    pub release_execution: LaneProgress,
    pub current_blocker: Option<String>,
    pub release_eligible: bool,
    pub blocking_failed: Vec<PipelineExplainItem>,
    pub blocking_pending: Vec<PipelineExplainItem>,
    pub non_blocking_failed: Vec<PipelineExplainItem>,
    pub non_blocking_pending: Vec<PipelineExplainItem>,
    pub incomplete_milestones: Vec<PipelineExplainMilestone>,
    pub untracked_jobs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineDoctorJob {
    pub id: i64,
    pub name: String,
    pub canonical_name: String,
    pub status: String,
    pub stage: String,
    pub runner_pool: String,
    pub runner: Option<String>,
    pub started_at: Option<String>,
    pub duration_secs: Option<f64>,
    pub queued_duration_secs: Option<f64>,
    pub historical_avg_duration_secs: Option<f64>,
    pub historical_max_duration_secs: Option<f64>,
    pub historical_runs: Option<i64>,
    pub slow_factor: Option<f64>,
    pub queue_factor: Option<f64>,
    pub trace_bytes: Option<usize>,
    pub trace_tail: Option<String>,
    pub stuck_suspected: bool,
    pub trace_age_suspected: bool,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineDoctorReport {
    pub generated_at: String,
    pub project_id: i64,
    pub pipeline_id: i64,
    pub pipeline_sha: String,
    pub pipeline_ref: String,
    pub pipeline_status: String,
    pub jobs: Vec<PipelineDoctorJob>,
    pub stuck_suspected: Vec<PipelineDoctorJob>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
enum ReleaseHealth {
    Blocked,
    Ready,
    Running,
    RemotePassed,
    E2ePassed,
    Failed,
    Outdated,
}

mod lifecycle;
mod pipeline;
mod progress;
mod status;
#[cfg(test)]
mod tests;

pub use lifecycle::*;
pub use pipeline::*;
pub use progress::*;
pub use status::*;
