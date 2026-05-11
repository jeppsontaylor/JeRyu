use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use tracing::{debug, warn};

use super::SharedState;

#[path = "engine_webhook_jobs.rs"]
mod jobs_impl;
#[path = "engine_webhook_pipeline.rs"]
mod pipeline_impl;
#[path = "engine_webhook_push.rs"]
mod push_impl;

pub(crate) async fn health() -> &'static str {
    "ok"
}

pub(crate) async fn handle_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, StatusCode> {
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
            if let Err(e) = jobs_impl::handle_job_event_from_body(&state, &body).await {
                warn!(error = %e, "failed to handle Job Hook payload");
            }
        }
        "Pipeline Hook" => {
            if let Err(e) = pipeline_impl::handle_pipeline_event_from_body(state.clone(), &body).await {
                warn!(error = %e, "failed to handle Pipeline Hook payload");
            }
        }
        "Push Hook" => {
            if let Err(e) = push_impl::handle_push_event_from_body(state.clone(), &body).await {
                warn!(error = %e, "failed to handle Push Hook payload");
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
