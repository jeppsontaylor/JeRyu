//! Owner: Logging & Observability subsystem
//! Proof: `cargo nextest run -p vgit -- logs`
//! Invariants: Log collection remains bounded, non-secret, and attributable to the owning job or component.
//! Log aggregation: three-stream model.
//!
//! Stream 1: Runner-manager container logs (via Docker)
//! Stream 2: GitLab job traces (via API)
//! Stream 3: Runner Prometheus metrics (via HTTP scrape)

use anyhow::Result;

use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::state::Db;

// ---------------------------------------------------------------------------
// Manager logs (Docker container stdout/stderr)
// ---------------------------------------------------------------------------

/// Get recent log lines from a runner manager container.
pub async fn tail_manager(
    db: &Db,
    docker: &DockerCtl,
    manager_id: &str,
    lines: usize,
) -> Result<Vec<String>> {
    let manager = db
        .get_manager(manager_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("manager '{}' not found", manager_id))?;

    let logs = docker
        .manager_logs(&manager.docker_container_id, lines)
        .await?;

    Ok(logs)
}

// ---------------------------------------------------------------------------
// Job trace (GitLab API)
// ---------------------------------------------------------------------------

/// Get the full trace (log output) for a GitLab CI job.
pub async fn get_job_trace(client: &GitlabClient, project_id: i64, job_id: i64) -> Result<String> {
    let trace = client.job_trace(project_id, job_id).await?;
    Ok(trace)
}

// ---------------------------------------------------------------------------
// Print helpers
// ---------------------------------------------------------------------------

/// Print job trace to stdout with a header.
pub fn print_trace(project_id: i64, job_id: i64, trace: &str) {
    println!("━━━ Job Trace: project={} job={} ━━━", project_id, job_id);
    println!("{}", trace);
    println!("━━━ End Trace ━━━");
}

/// Print manager logs to stdout with a header.
pub fn print_manager_logs(manager_id: &str, logs: &[String]) {
    println!("━━━ Manager Logs: {} ━━━", manager_id);
    for line in logs {
        println!("{}", line);
    }
    println!("━━━ End Logs ━━━");
}
