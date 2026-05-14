use super::*;

#[path = "lifecycle_checks.rs"]
mod checks;
#[path = "lifecycle_support.rs"]
mod support;
#[path = "lifecycle_launch.rs"]
mod launch;

pub(crate) use checks::{
    DoctorBlocker, DoctorReport, PreflightBlocker, PreflightReport, ReleaseLock, release_lock_path,
    write_release_lock,
};
pub use checks::{release_doctor, release_preflight};
pub(crate) use support::{
    UpstreamImageHandoff, parse_image_env, pipeline_has_release_execution_jobs,
    release_impacting_change, upstream_image_handoff,
};
pub use launch::launch_canary_for_green_pipeline;

pub async fn reconcile_release_for_ref(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
) -> Result<ReleaseStatusReport> {
    let Some(pipeline) =
        latest_release_candidate_pipeline_for_ref(client, project_id, ref_name).await?
    else {
        return build_release_status_report(
            db,
            ReleaseStatusQuery {
                project_id: Some(project_id),
                ref_name: Some(ref_name.to_string()),
                sha: None,
                limit: 5,
            },
        )
        .await;
    };

    let version = render_release_version(&pipeline.sha);
    let mut existing = db
        .get_release_attempt(project_id, ref_name, &pipeline.sha)
        .await?;
    if let Some(attempt) = existing.as_ref()
        && let Some(release_pipeline_id) = attempt.release_pipeline_id
    {
        let release_pipeline = client
            .get_pipeline(project_id, release_pipeline_id)
            .await
            .with_context(|| {
                format!("refresh release pipeline {release_pipeline_id} before reconcile")
            })?;
        if attempt.release_pipeline_status.as_deref() != Some(release_pipeline.status.as_str()) {
            existing = db
                .update_release_pipeline_status(release_pipeline_id, &release_pipeline.status)
                .await?;
        }
        if matches!(release_pipeline.status.as_str(), "failed" | "canceled")
            && existing
                .as_ref()
                .map(|attempt| attempt.canary_status.as_str())
                == Some("running")
        {
            let note = format!(
                "release-execution pipeline {release_pipeline_id} ended with status {}",
                release_pipeline.status
            );
            db.finish_release_canary(project_id, ref_name, &pipeline.sha, "failed", Some(&note))
                .await?;
            existing = db
                .get_release_attempt(project_id, ref_name, &pipeline.sha)
                .await?;
        }
    }
    let mut existing_canary_status = existing
        .as_ref()
        .map(|attempt| attempt.canary_status.as_str())
        .unwrap_or("pending");
    if existing_canary_status == "passed"
        && !has_complete_canary_evidence(&release_evidence(&version, &pipeline.sha)?)
    {
        let note = "release-execution pipeline ended without required canary gate evidence";
        db.finish_release_canary(project_id, ref_name, &pipeline.sha, "failed", Some(note))
            .await?;
        existing_canary_status = "failed";
        existing = db
            .get_release_attempt(project_id, ref_name, &pipeline.sha)
            .await?;
    }
    let needs_upsert = existing
        .as_ref()
        .map(|attempt| {
            attempt.upstream_pipeline_id != Some(pipeline.id)
                || attempt.upstream_status != "success"
                || attempt.version != version
        })
        .unwrap_or(true);
    if needs_upsert {
        db.upsert_release_attempt(
            project_id,
            ref_name,
            &pipeline.sha,
            &version,
            Some(pipeline.id),
            "success",
            existing_canary_status,
        )
        .await?;
    }

    if !matches!(existing_canary_status, "running" | "passed" | "skipped") {
        launch_canary_for_green_pipeline(
            db,
            client,
            project_id,
            ref_name,
            &pipeline.sha,
            pipeline.id,
        )
        .await?;
    }

    let report = build_release_status_report(
        db,
        ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.to_string()),
            sha: Some(pipeline.sha),
            limit: 5,
        },
    )
    .await?;

    if let Some(latest) = report.latest.as_ref()
        && maybe_trigger_production_promotion(
            db,
            client,
            project_id,
            ref_name,
            Some(&latest.attempt.sha),
            Some(&latest.attempt.version),
        )
        .await?
        .is_some()
    {
        return build_release_status_report(
            db,
            ReleaseStatusQuery {
                project_id: Some(project_id),
                ref_name: Some(ref_name.to_string()),
                sha: Some(latest.attempt.sha.clone()),
                limit: 5,
            },
        )
        .await;
    }

    Ok(report)
}
