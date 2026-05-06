use super::*;

pub(crate) async fn explain_blockers(
    project_id: i64,
    ref_name: String,
    sha: Option<String>,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    match crate::release::build_release_status_report(
        &db,
        crate::release::ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.clone()),
            sha,
            limit: 20,
        },
    )
    .await
    {
        Ok(report) => CapabilityResponse {
            success: true,
            message: "release blockers resolved".into(),
            data: Some(serde_json::to_value(report).ok().unwrap_or_default()),
        },
        Err(e) => err(&format!("release status: {}", e)),
    }
}

pub(crate) async fn get_system_snapshot(
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    let snapshot = crate::state::system_snapshot(&db, client).await;
    CapabilityResponse {
        success: true,
        message: "system snapshot".into(),
        data: Some(serde_json::to_value(snapshot).ok().unwrap_or_default()),
    }
}

pub(crate) async fn get_pipeline_jobs(
    project_id: i64,
    pipeline_id: i64,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    match client.list_pipeline_jobs_with_downstream(project_id, pipeline_id).await {
        Ok(jobs) => CapabilityResponse {
            success: true,
            message: "pipeline jobs".into(),
            data: Some(serde_json::to_value(jobs).ok().unwrap_or_default()),
        },
        Err(e) => err(&format!("pipeline jobs: {}", e)),
    }
}

pub(crate) async fn get_ci_bottlenecks(
    project_id: i64,
    ref_name: String,
    limit: usize,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    match db
        .ci_job_bottlenecks(project_id, Some(&ref_name), limit as i64)
        .await
    {
        Ok(rows) => CapabilityResponse {
            success: true,
            message: "ci bottlenecks".into(),
            data: Some(serde_json::to_value(rows).ok().unwrap_or_default()),
        },
        Err(e) => err(&format!("ci bottlenecks: {}", e)),
    }
}

pub(crate) fn list_allowed_actions() -> CapabilityResponse {
    CapabilityResponse {
        success: true,
        message: "allowed actions".into(),
        data: Some(serde_json::json!({
            "actions": ["FetchCapsule", "RunTests", "ProposePatch", "RacePatches", "RequestMerge", "ExplainBlockers", "GetSystemSnapshot", "GetPipelineJobs", "GetCiBottlenecks", "ListAllowedActions", "PlanValidation"]
        })),
    }
}

pub(crate) async fn plan_validation(
    project_id: i64,
    ref_name: String,
    test_ids: Vec<String>,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    let since = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::hours(24))
        .unwrap_or(chrono::Utc::now())
        .to_rfc3339();
    let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);

    let valid = miss_count == 0;
    CapabilityResponse {
        success: valid,
        message: if valid {
            format!(
                "Plan for ref '{}' with {} tests is valid",
                ref_name,
                test_ids.len()
            )
        } else {
            format!(
                "Plan invalid: {} unresolved selector miss(es) in last 24h",
                miss_count
            )
        },
        data: Some(serde_json::json!({
            "valid": valid,
            "test_count": test_ids.len(),
            "ref_name": ref_name,
            "selector_misses": miss_count,
        })),
    }
}

fn err(msg: &str) -> CapabilityResponse {
    CapabilityResponse {
        success: false,
        message: msg.to_string(),
        data: None,
    }
}
