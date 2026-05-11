use anyhow::Result;
use tracing::info;

use super::support::env_string_or_default;

/// Handles `jeryu exec cleanup`
/// Tears down the sandbox.
pub async fn run_cleanup() -> Result<()> {
    let job_id = env_string_or_default("CUSTOM_ENV_CI_JOB_ID", "unknown");
    let project_id_str = env_string_or_default("CUSTOM_ENV_CI_PROJECT_ID", "");
    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");

    info!(job_id, "Driver: cleaning up sandbox");

    let sandbox_path = format!("{}-sandbox", project_dir);
    let quarantine_marker = std::path::Path::new(&sandbox_path).join(".jeryu_quarantine");

    if quarantine_marker.exists() {
        tracing::error!(
            "🚨 Sandbox {} is quarantined. Skipping workspace destruction for forensics.",
            sandbox_path
        );

        let db = crate::state::Db::open().await?;
        let payload = serde_json::json!({
            "action": "quarantine_skip",
            "sandbox_path": sandbox_path,
        });
        db.append_event(
            "executor_cleanup_quarantined",
            project_id_str.parse().ok(),
            job_id.parse().ok(),
            "jeryu-exec",
            &payload.to_string(),
        )
        .await?;

        return Ok(());
    }

    if std::path::Path::new(&sandbox_path).exists() {
        let _ = std::fs::remove_dir_all(&sandbox_path);
        info!("removed sandbox fast clone at {}", sandbox_path);
    }

    let db = crate::state::Db::open().await?;
    let payload = serde_json::json!({
        "action": "cleanup",
        "sandbox_path": sandbox_path,
        "build_failure_exit_code": env_string_or_default("BUILD_FAILURE_EXIT_CODE", ""),
        "system_failure_exit_code": env_string_or_default("SYSTEM_FAILURE_EXIT_CODE", ""),
    });

    db.append_event(
        "executor_cleanup",
        project_id_str.parse().ok(),
        job_id.parse().ok(),
        "jeryu-exec",
        &payload.to_string(),
    )
    .await?;

    Ok(())
}
