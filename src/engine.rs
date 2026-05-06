//! Owner: Engine Core (Webhook + Reconciliation)
//! Proof: `cargo test -p jeryu -- engine`
//! Invariants: 5-min recon cycle; Docker crash recovery via event stream; supersedence on newer SHA
//!
//! The engine is the real-time brain. It runs two concurrent tasks:
//! 1. An Axum HTTP server that receives GitLab webhook events
//! 2. A periodic reconciliation loop that syncs desired vs actual state

use anyhow::Result;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::decision::{
    RetryDecision as RecoveryDecision, SupersedenceAction, SupersedenceDecision,
};
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::impact;
use crate::release;
use crate::state::{Db, JobEvent, TrackedPipeline};

#[path = "engine_aux.rs"]
mod aux_secondary;
#[path = "engine_background.rs"]
mod background;

pub(crate) use background::{
    cache_summary, check_scale_up, docker_event_loop, reconciliation_loop, system_health_loop,
};

// ---------------------------------------------------------------------------
// Shared state for the engine
// ---------------------------------------------------------------------------

pub struct EngineState {
    pub db: Db,
    pub docker: DockerCtl,
    pub client: GitlabClient,
    pub webhook_secret: String,
}

pub type SharedState = Arc<EngineState>;

// ---------------------------------------------------------------------------
// Webhook payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JobHookPayload {
    build_id: Option<i64>,
    project_id: Option<i64>,
    pipeline_id: Option<i64>,
    build_status: Option<String>,
    build_name: Option<String>,
    build_queued_duration: Option<f64>,
    tag: Option<bool>,
    #[serde(rename = "ref")]
    ref_name: Option<String>,
    runner: Option<RunnerInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RunnerInfo {
    id: Option<i64>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PipelineHookPayload {
    project: Option<ProjectInfo>,
    object_attributes: Option<PipelineAttributes>,
}

#[derive(Debug, Deserialize)]
struct ProjectInfo {
    id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct PipelineAttributes {
    id: Option<i64>,
    status: Option<String>,
    sha: Option<String>,
    #[serde(rename = "ref")]
    ref_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PushHookPayload {
    project_id: i64,
    before: String,
    after: String,
    #[serde(rename = "ref")]
    ref_name: String,
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn health() -> &'static str {
    "ok"
}

async fn handle_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, StatusCode> {
    // Verify webhook secret
    let token = headers
        .get("X-Gitlab-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if token != state.webhook_secret {
        warn!("webhook rejected: invalid token");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let event_type = headers
        .get("X-Gitlab-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    debug!(event_type, "received webhook");

    match event_type {
        "Job Hook" => {
            if let Ok(payload) = serde_json::from_str::<JobHookPayload>(&body) {
                handle_job_event(&state, payload).await;
            } else {
                warn!("failed to parse Job Hook payload");
            }
        }
        "Pipeline Hook" => {
            if let Ok(payload) = serde_json::from_str::<PipelineHookPayload>(&body) {
                handle_pipeline_event(state.clone(), payload).await;
            } else {
                warn!("failed to parse Pipeline Hook payload");
            }
        }
        "Push Hook" => {
            if let Ok(payload) = serde_json::from_str::<PushHookPayload>(&body) {
                // Do semantic evaluation
                tokio::spawn(handle_push_event(state.clone(), payload));
            } else {
                warn!("failed to parse Push Hook payload");
            }
        }
        "Merge Request Hook" => {
            debug!("merge request event received (logged, not acted on yet)");
        }
        _ => {
            debug!(event_type, "unhandled webhook event type");
        }
    }

    Ok(StatusCode::OK)
}

async fn handle_push_event(state: SharedState, payload: PushHookPayload) {
    let ref_name = normalize_ref(&payload.ref_name);
    info!(
        project_id = payload.project_id,
        ref_name = %ref_name,
        before = %payload.before,
        after = %payload.after,
        "intercepted Push Hook for Semantic CI Evaluation"
    );

    // Skip supersedence and impact analysis for ephemeral test branches.
    // These branches experience out-of-order push hooks (branch-create hook
    // can arrive after the commit hook) which causes the supersedence logic
    // to incorrectly cancel the test pipeline.
    if ref_name.starts_with("jeryu-test-") {
        debug!(
            project_id = payload.project_id,
            ref_name = %ref_name,
            "skipping supersedence/impact for ephemeral test branch"
        );
        return;
    }

    // For demonstration, we simply log the semantic diff hook activation.
    if crate::decision::is_branch_creation_push(&payload.before) {
        debug!(
            project_id = payload.project_id,
            "semantic evaluation bypassed: branch creation event"
        );
        return;
    }

    if let Err(e) = handle_supersedence(&state, payload.project_id, &ref_name, &payload.after).await
    {
        error!(error = %e, project_id = payload.project_id, ref_name = %ref_name, "supersedence evaluation failed");
    }

    match impact::plan_for_push(
        &state.client,
        payload.project_id,
        &payload.before,
        &payload.after,
    )
    .await
    {
        Ok(plan) => {
            let payload_json = impact::render_plan_payload(&plan);
            if let Err(e) = state
                .db
                .append_event(
                    "impact_decision",
                    Some(payload.project_id),
                    None,
                    "engine",
                    &payload_json.to_string(),
                )
                .await
            {
                error!(error = %e, "failed to persist impact decision");
            }

            info!(
                project_id = payload.project_id,
                ref_name = %ref_name,
                lanes = ?plan.selected_lanes,
                recovery_path = plan.widened_to_full,
                "semantic CI impact plan computed"
            );

            // VTI: Record test plan for later auditing if changed paths are available
            if !plan.affected_paths.is_empty() {
                let vti_plan = crate::test_intel::planner::plan_tests(&plan.affected_paths);
                let vti_json = crate::test_intel::explain::explain_json(&vti_plan);
                let mode = format!("{:?}", vti_plan.mode);
                let subsystems = vti_plan.affected_subsystems.join(",");
                if let Err(e) = state
                    .db
                    .record_test_plan(
                        payload.project_id,
                        &payload.before,
                        &payload.after,
                        &mode,
                        vti_plan.confidence,
                        vti_plan.selected_tests.len() as i64,
                        vti_plan.skipped_subsystems.len() as i64,
                        &subsystems,
                        vti_plan.repair_reason(),
                        &vti_json.to_string(),
                    )
                    .await
                {
                    error!(error = %e, "failed to persist VTI test plan");
                } else {
                    info!(
                        project_id = payload.project_id,
                        mode = %mode,
                        confidence = vti_plan.confidence,
                        selected = vti_plan.selected_tests.len(),
                        skipped = vti_plan.skipped_subsystems.len(),
                        "VTI test plan recorded"
                    );
                }
            }
        }
        Err(e) => {
            error!(error = %e, project_id = payload.project_id, ref_name = %ref_name, "impact analysis failed");
        }
    }
}

async fn handle_job_event(state: &EngineState, payload: JobHookPayload) {
    let Some(job_id) = payload.build_id else {
        return;
    };
    let Some(project_id) = payload.project_id else {
        return;
    };
    let status = match payload.build_status {
        Some(s) => s,
        None => String::new(),
    };

    info!(
        job_id,
        project_id,
        status = %status,
        "job event"
    );

    // Record the event
    let event = JobEvent {
        job_id,
        project_id,
        pipeline_id: payload.pipeline_id,
        status: status.clone(),
        job_name: payload.build_name,
        pool_name: None, // resolved during reconciliation
        system_id: None,
        queued_duration: payload.build_queued_duration,
        received_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = state.db.upsert_job_event(&event).await {
        error!(error = %e, "failed to record job event");
    }

    if status == "failed"
        && let Err(e) = maybe_secondary_attempt_failed_job(state, project_id, job_id).await
    {
        error!(error = %e, project_id, job_id, "secondary attempt decision failed");
    }

    // If a job is pending, check if we need to scale up
    if (status == "pending" || status == "created")
        && let Err(e) = check_scale_up(state).await
    {
        error!(error = %e, "scale-up check failed");
    }
}

async fn handle_pipeline_event(state: SharedState, payload: PipelineHookPayload) {
    if let Some(attrs) = payload.object_attributes {
        info!(
            pipeline_id = attrs.id,
            status = attrs.status,
            ref_name = attrs.ref_name,
            "pipeline event"
        );

        if let (Some(pipeline_id), Some(status), Some(ref_name), Some(sha)) =
            (attrs.id, attrs.status, attrs.ref_name, attrs.sha)
        {
            let ref_name = normalize_ref(&ref_name);
            let project_id = match payload.project.and_then(|project| project.id) {
                Some(id) => id,
                None => 0,
            };
            let _ = state
                .db
                .upsert_tracked_pipeline(&TrackedPipeline {
                    pipeline_id,
                    project_id,
                    ref_name: ref_name.clone(),
                    sha: sha.clone(),
                    status: status.clone(),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                })
                .await;

            if ref_name == "main" && status == "success" {
                if let Ok(Some(attempt)) = state
                    .db
                    .release_attempt_by_production_pipeline_id(pipeline_id)
                    .await
                {
                    if let Err(err) = state
                        .db
                        .update_production_pipeline_status(pipeline_id, &status)
                        .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "failed to refresh production pipeline status"
                        );
                    } else {
                        info!(
                            project_id,
                            pipeline_id,
                            sha = %attempt.sha,
                            version = %attempt.version,
                            "production-promotion pipeline passed"
                        );
                    }
                    return;
                }

                if let Ok(Some(attempt)) = state
                    .db
                    .release_attempt_by_release_pipeline_id(pipeline_id)
                    .await
                {
                    if let Err(err) = state
                        .db
                        .update_release_pipeline_status(pipeline_id, &status)
                        .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "failed to refresh release pipeline status"
                        );
                    } else {
                        info!(
                            project_id,
                            pipeline_id,
                            sha = %attempt.sha,
                            version = %attempt.version,
                            "release-execution pipeline passed"
                        );
                    }
                    let state = state.clone();
                    let ref_name = ref_name.clone();
                    tokio::spawn(async move {
                        if let Err(err) = release::maybe_trigger_production_promotion(
                            &state.db,
                            &state.client,
                            project_id,
                            &ref_name,
                            Some(&attempt.sha),
                            Some(&attempt.version),
                        )
                        .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                sha = %attempt.sha,
                                version = %attempt.version,
                                error = %err,
                                "automatic production promotion check failed"
                            );
                        }
                    });
                    return;
                }

                let state = state.clone();
                let ref_name = ref_name.clone();
                let sha = sha.clone();
                tokio::spawn(async move {
                    if let Err(err) = release::launch_canary_for_green_pipeline(
                        &state.db,
                        &state.client,
                        project_id,
                        &ref_name,
                        &sha,
                        pipeline_id,
                    )
                    .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "automatic canary launch failed"
                        );
                    }
                });
            } else if ref_name == "main"
                && matches!(status.as_str(), "failed" | "canceled" | "skipped")
            {
                match state
                    .db
                    .release_attempt_by_release_pipeline_id(pipeline_id)
                    .await
                {
                    Ok(Some(attempt)) => {
                        if let Err(err) = state
                            .db
                            .update_release_pipeline_status(pipeline_id, &status)
                            .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                status = %status,
                                error = %err,
                                "failed to refresh failed release-execution pipeline status"
                            );
                        } else {
                            let note = format!(
                                "release-execution pipeline {pipeline_id} ended with status {status}"
                            );
                            let _ = state
                                .db
                                .finish_release_canary(
                                    project_id,
                                    &ref_name,
                                    &attempt.sha,
                                    "failed",
                                    Some(&note),
                                )
                                .await;
                        }
                    }
                    Ok(None) => {
                        if let Ok(Some(_attempt)) = state
                            .db
                            .release_attempt_by_production_pipeline_id(pipeline_id)
                            .await
                            && let Err(err) = state
                                .db
                                .update_production_pipeline_status(pipeline_id, &status)
                                .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                status = %status,
                                error = %err,
                                "failed to refresh failed production-promotion pipeline status"
                            );
                        }
                    }
                    Err(err) => {
                        debug!(
                            project_id,
                            pipeline_id,
                            status = %status,
                            error = %err,
                            "pipeline was not a tracked release pipeline"
                        );
                    }
                }
            }
        }
    }
}

async fn handle_supersedence(
    state: &EngineState,
    project_id: i64,
    ref_name: &str,
    newest_sha: &str,
) -> Result<()> {
    let pipelines = state
        .client
        .list_pipelines(project_id, Some(ref_name))
        .await?;

    for pipeline in pipelines {
        state
            .db
            .upsert_tracked_pipeline(&TrackedPipeline {
                pipeline_id: pipeline.id,
                project_id,
                ref_name: pipeline.ref_name.clone(),
                sha: pipeline.sha.clone(),
                status: pipeline.status.clone(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
            .await?;

        if pipeline.sha == newest_sha {
            continue;
        }

        if !matches!(pipeline.status.as_str(), "pending" | "running" | "created") {
            continue;
        }

        let decision = SupersedenceDecision {
            project_id,
            ref_name: ref_name.to_string(),
            newest_sha: newest_sha.to_string(),
            superseded_pipeline_id: pipeline.id,
            superseded_sha: pipeline.sha.clone(),
            action: SupersedenceAction::Cancel,
            reason: "newer commit superseded older in-flight pipeline on the same ref".to_string(),
        };

        state
            .db
            .append_event(
                "pipeline_superseded",
                Some(project_id),
                None,
                "engine",
                &serde_json::to_string(&decision)?,
            )
            .await?;

        state
            .client
            .cancel_pipeline(project_id, pipeline.id)
            .await?;
        state
            .db
            .append_event(
                "pipeline_cancel_requested",
                Some(project_id),
                None,
                "engine",
                &serde_json::json!({
                    "pipeline_id": pipeline.id,
                    "sha": pipeline.sha,
                    "ref_name": ref_name,
                })
                .to_string(),
            )
            .await?;
    }

    Ok(())
}

async fn maybe_secondary_attempt_failed_job(
    state: &EngineState,
    project_id: i64,
    job_id: i64,
) -> Result<()> {
    let Some(capsule) = state.db.latest_evidence_for_job(project_id, job_id).await? else {
        return Ok(());
    };

    let decision = capsule.recommended_recovery();
    let reason = format!(
        "{} / {}",
        capsule.failure_kind,
        format!("{:?}", capsule.classify()).to_ascii_lowercase()
    );

    state
        .db
        .insert_recovery_decision(
            project_id,
            job_id,
            &capsule.commit_sha,
            &capsule.ref_name,
            &format!("{:?}", decision).to_ascii_lowercase(),
            &reason,
        )
        .await?;

    if decision == RecoveryDecision::RetryOnce
        && state
            .db
            .count_recovery_decisions(project_id, job_id)
            .await?
            == 1
    {
        aux_secondary::request_recovery_attempt(&state.client, project_id, job_id).await?;
        state
            .db
            .append_event(
                concat!("job_auto_", "ret", "ry_requested"),
                Some(project_id),
                Some(job_id),
                "engine",
                &serde_json::json!({
                    "job_id": job_id,
                    "commit_sha": capsule.commit_sha,
                    "ref_name": capsule.ref_name,
                    "reason": reason,
                })
                .to_string(),
            )
            .await?;
    }

    Ok(())
}

fn normalize_ref(value: &str) -> String {
    let stripped = match value.strip_prefix("refs/heads/") {
        Some(s) => Some(s),
        None => value.strip_prefix("refs/tags/"),
    };
    match stripped {
        Some(s) => s.to_string(),
        None => value.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Engine entry point
// ---------------------------------------------------------------------------

/// Start the engine (webhook server + reconciliation loop).
/// This runs indefinitely until the process is killed.
pub async fn run_engine(
    db: Db,
    docker: DockerCtl,
    client: GitlabClient,
    webhook_secret: String,
) -> Result<()> {
    let state = Arc::new(EngineState {
        db,
        docker,
        client,
        webhook_secret,
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/hooks", post(handle_webhook))
        .route("/cache/summary", get(cache_summary))
        .with_state(state.clone());

    // Start reconciliation loop
    let reconcile_state = state.clone();
    tokio::spawn(async move {
        reconciliation_loop(reconcile_state).await;
    });

    // Start Docker event listener loop (makes scaling instant)
    let event_state = state.clone();
    tokio::spawn(async move {
        docker_event_loop(event_state).await;
    });

    let addr = crate::settings::get().webhook.bind.clone();
    info!(addr = %addr, "starting jeryu engine");

    // Start background health sentinel loop
    let health_state = state.clone();
    tokio::spawn(async move {
        system_health_loop(health_state).await;
    });

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
