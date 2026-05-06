use super::*;

pub(crate) fn effective_progress_status<'a>(
    statuses: &'a HashMap<String, String>,
    job_id: &str,
    pipeline_status: &str,
) -> &'a str {
    match statuses.get(job_id) {
        Some(status) => status.as_str(),
        None => match pipeline_status {
            "success" | "failed" | "canceled" | "skipped" => "omitted",
            _ => "pending",
        },
    }
}

pub(crate) fn read_punchlist_summary(root: &Path) -> Option<serde_json::Value> {
    let path = root.join("testing/status/ci/punchlist_summary_latest.json");
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub(crate) fn punchlist_freshness(
    root: &Path,
    winning_sha: Option<&str>,
    version: Option<&str>,
) -> String {
    let Some(value) = read_punchlist_summary(root) else {
        return "missing".to_string();
    };
    let summary_sha = value
        .get("winning_sha")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let summary_version = value
        .get("expected_release_version")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let freshness = value
        .get("punchlist_freshness")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    match (winning_sha, version) {
        (Some(winning_sha), Some(version))
            if summary_sha == winning_sha && summary_version == version =>
        {
            freshness.to_string()
        }
        (Some(_), Some(_)) => "outdated-for-sha".to_string(),
        _ => "missing-winning-sha".to_string(),
    }
}

pub(crate) fn summary_lane_progress(
    summary: &serde_json::Value,
    key: &str,
) -> Option<LaneProgress> {
    let section = summary.get(key)?;
    Some(LaneProgress {
        passed: section.get("passed")?.as_u64()? as usize,
        total: section.get("total")?.as_u64()? as usize,
        percent: section.get("percent")?.as_f64()?,
    })
}

pub(crate) fn summary_job_items(
    summary: &serde_json::Value,
    release_blocking: bool,
    failed_only: bool,
) -> Vec<String> {
    let Some(items) = summary.get("milestones").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if item
            .get("release_blocking")
            .and_then(|value| value.as_bool())
            != Some(release_blocking)
        {
            continue;
        }
        let Some(evidence) = item.get("evidence").and_then(|value| value.as_str()) else {
            continue;
        };
        for token in evidence.split(", ") {
            let Some((job, status)) = token.rsplit_once(": ") else {
                continue;
            };
            let include = if failed_only {
                matches!(status, "failed" | "canceled")
            } else {
                status != "passed"
            };
            if include {
                out.push(job.to_string());
            }
        }
    }
    out
}

pub(crate) fn release_execution_percent(progress: &ReleaseExecutionProgress) -> f64 {
    if progress.e2e_gate && progress.punchlist_current {
        100.0
    } else if progress.e2e_gate {
        80.0
    } else if progress.telemetry_gate {
        60.0
    } else if progress.remote_gate {
        40.0
    } else if progress.attempt_exists {
        20.0
    } else {
        0.0
    }
}
