use super::*;

pub(crate) fn parse_state_json(version: &str) -> Result<Option<serde_json::Value>> {
    let path = canary_state_path(version);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

pub(crate) fn json_release_identity_ok(path: &Path, version: &str, expected_sha: &str) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    value
        .get("git_sha")
        .and_then(|value| value.as_str())
        .map(|value| value == expected_sha)
        .unwrap_or(false)
        && value
            .get("release_version")
            .and_then(|value| value.as_str())
            .map(|value| value == version)
            .unwrap_or(false)
}

pub(crate) fn release_lock_identity_ok(version: &str, expected_sha: &str) -> bool {
    let Ok(raw) = fs::read_to_string(release_lock_path(version)) else {
        return false;
    };
    let Ok(lock) = serde_json::from_str::<ReleaseLock>(&raw) else {
        return false;
    };
    lock.product_sha == expected_sha && lock.release_version == version
}

pub(crate) fn release_identity_ok(version: &str, expected_sha: &str) -> bool {
    let release_json = release_dir(version).join("release.json");
    let contract_json = release_dir(version).join("release-contract.json");
    release_lock_identity_ok(version, expected_sha)
        && json_release_identity_ok(&release_json, version, expected_sha)
        && json_release_identity_ok(&contract_json, version, expected_sha)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CanaryGateFiles {
    pub(crate) remote: bool,
    pub(crate) telemetry: bool,
    pub(crate) e2e: bool,
    pub(crate) validation: bool,
    pub(crate) handoff: bool,
    pub(crate) telemetry_diag: bool,
}

impl CanaryGateFiles {
    pub(crate) fn canary_complete(self) -> bool {
        self.remote && self.telemetry && self.e2e && self.validation
    }

    pub(crate) fn promotion_ready(self) -> bool {
        self.canary_complete() && self.handoff
    }
}

pub(crate) fn canary_gate_files(version: &str) -> CanaryGateFiles {
    CanaryGateFiles {
        remote: gate_remote_canary_path(version).is_file(),
        telemetry: gate_canary_telemetry_path(version).is_file(),
        e2e: gate_canary_e2e_path(version).is_file(),
        validation: c_validation_path(version).is_file(),
        handoff: c_handoff_path(version).is_file(),
        telemetry_diag: telemetry_diag_path(version).is_file(),
    }
}

pub(crate) fn release_evidence(version: &str, expected_sha: &str) -> Result<ReleaseEvidence> {
    let state_value = parse_state_json(version)?;
    let gate_files = canary_gate_files(version);
    Ok(ReleaseEvidence {
        state_phase: state_value
            .as_ref()
            .and_then(|value| value.get("phase"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        state_status: state_value
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        state_detail: state_value
            .as_ref()
            .and_then(|value| value.get("detail"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        has_remote_gate: gate_files.remote,
        has_telemetry_gate: gate_files.telemetry,
        has_e2e_gate: gate_files.e2e,
        has_c_validation: gate_files.validation,
        has_c_handoff: gate_files.handoff,
        has_telemetry_diag: gate_files.telemetry_diag,
        release_identity_ok: release_identity_ok(version, expected_sha),
    })
}

pub(crate) fn has_complete_canary_evidence(evidence: &ReleaseEvidence) -> bool {
    evidence.has_remote_gate
        && evidence.has_telemetry_gate
        && evidence.has_e2e_gate
        && evidence.has_c_validation
        && evidence.has_c_handoff
        && evidence.release_identity_ok
}

pub(crate) fn is_outdated_attempt(attempt: &ReleaseAttempt, evidence: &ReleaseEvidence) -> bool {
    if evidence.has_e2e_gate {
        return false;
    }

    let ts = attempt
        .canary_started_at
        .as_deref()
        .or(attempt.canary_finished_at.as_deref())
        .or(Some(attempt.updated_at.as_str()));
    let Some(ts) = ts else {
        return false;
    };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return false;
    };
    let age = chrono::Utc::now().signed_duration_since(parsed.with_timezone(&chrono::Utc));
    age > chrono::Duration::minutes(30)
}

pub(crate) fn derive_release_health(
    attempt: &ReleaseAttempt,
    evidence: &ReleaseEvidence,
) -> ReleaseHealth {
    if attempt.upstream_status != "success" {
        return ReleaseHealth::Blocked;
    }
    if attempt.canary_status == "passed"
        && attempt.release_pipeline_status.as_deref() == Some("success")
        && has_complete_canary_evidence(evidence)
    {
        return ReleaseHealth::E2ePassed;
    }
    if !evidence.release_identity_ok {
        return ReleaseHealth::Outdated;
    }
    if matches!(evidence.state_status.as_deref(), Some("failed"))
        || attempt.canary_status == "failed"
    {
        return ReleaseHealth::Failed;
    }
    if evidence.has_e2e_gate && has_complete_canary_evidence(evidence) {
        return ReleaseHealth::E2ePassed;
    }
    if evidence.has_remote_gate {
        return ReleaseHealth::RemotePassed;
    }
    if matches!(evidence.state_status.as_deref(), Some("passed")) && !evidence.has_e2e_gate {
        return ReleaseHealth::Outdated;
    }
    if matches!(evidence.state_status.as_deref(), Some("running"))
        || attempt.canary_status == "running"
    {
        return if is_outdated_attempt(attempt, evidence) {
            ReleaseHealth::Outdated
        } else {
            ReleaseHealth::Running
        };
    }
    if attempt.canary_status == "pending" {
        return ReleaseHealth::Ready;
    }
    ReleaseHealth::Ready
}

pub(crate) fn derived_note(
    attempt: &ReleaseAttempt,
    evidence: &ReleaseEvidence,
    health: ReleaseHealth,
) -> Option<String> {
    if let Some(detail) = evidence
        .state_detail
        .as_ref()
        .filter(|detail| !detail.trim().is_empty())
    {
        let phase = evidence.state_phase.as_deref().unwrap_or("unknown-phase");
        return Some(format!("{phase}: {detail}"));
    }
    if let Some(note) = attempt
        .canary_note
        .as_ref()
        .filter(|note| !note.trim().is_empty())
    {
        return Some(note.clone());
    }
    if health == ReleaseHealth::Outdated {
        return Some("release evidence is incomplete for this version".to_string());
    }
    None
}

pub(crate) fn view_attempt(attempt: ReleaseAttempt) -> Result<ReleaseAttemptView> {
    let version = attempt.version.clone();
    let evidence = release_evidence(&version, &attempt.sha)?;
    let health = derive_release_health(&attempt, &evidence);
    let detail = derived_note(&attempt, &evidence, health);
    Ok(ReleaseAttemptView {
        attempt,
        release_dir: release_dir(&version).display().to_string(),
        canary_state_path: canary_state_path(&version).display().to_string(),
        gate_remote_canary_path: gate_remote_canary_path(&version).display().to_string(),
        gate_canary_e2e_path: gate_canary_e2e_path(&version).display().to_string(),
        gate_canary_telemetry_path: gate_canary_telemetry_path(&version).display().to_string(),
        telemetry_diag_path: telemetry_diag_path(&version).display().to_string(),
        canary_state: health.as_str().to_string(),
        eligibility: health.eligibility().to_string(),
        phase: evidence.state_phase,
        detail,
        state_status: evidence.state_status,
        has_remote_gate: evidence.has_remote_gate,
        has_telemetry_gate: evidence.has_telemetry_gate,
        has_e2e_gate: evidence.has_e2e_gate,
        has_telemetry_diag: evidence.has_telemetry_diag,
        release_identity_ok: evidence.release_identity_ok,
        canary_public_url: canary_public_url(&version),
    })
}

pub async fn build_release_status_report(
    db: &Db,
    query: ReleaseStatusQuery,
) -> Result<ReleaseStatusReport> {
    let recent = if let Some(sha) = &query.sha {
        let mut attempts = Vec::new();
        if let Some(project_id) = query.project_id {
            if let Some(attempt) = db
                .get_release_attempt(project_id, query.ref_name.as_deref().unwrap_or("main"), sha)
                .await?
            {
                attempts.push(attempt);
            }
        } else {
            attempts = db
                .recent_release_attempts(None, query.ref_name.as_deref(), query.limit as i64)
                .await?;
            attempts.retain(|attempt| attempt.sha == *sha);
        }
        attempts
    } else {
        db.recent_release_attempts(
            query.project_id,
            query.ref_name.as_deref(),
            query.limit as i64,
        )
        .await?
    };

    let latest = recent.first().cloned().map(view_attempt).transpose()?;
    let recent = recent
        .into_iter()
        .map(view_attempt)
        .collect::<Result<Vec<_>>>()?;
    Ok(ReleaseStatusReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id: query.project_id,
        ref_name: query.ref_name,
        sha: query.sha,
        limit: query.limit,
        total_attempts: recent.len(),
        latest,
        recent,
    })
}

pub fn summarize_release_attempt(view: &ReleaseAttemptView) -> String {
    let attempt = &view.attempt;
    let upstream = format!("upstream={}", attempt.upstream_status);
    let release_pipeline = match attempt.release_pipeline_id {
        Some(id) => format!("release_pipeline={id}"),
        None => "release_pipeline=none".to_string(),
    };
    let production_pipeline = match attempt.production_pipeline_id {
        Some(id) => format!("production_pipeline={id}"),
        None => "production_pipeline=none".to_string(),
    };
    let canary = format!("canary={}", attempt.canary_status);
    let evidence = view
        .gate_canary_e2e_path
        .rsplit('/')
        .next()
        .unwrap_or(&view.gate_canary_e2e_path);
    format!(
        "{} {} [{}] {} {} {} {} {}",
        attempt.ref_name,
        attempt.version,
        view.canary_state,
        upstream,
        release_pipeline,
        production_pipeline,
        canary,
        evidence
    )
}

pub fn summarize_release_report(report: &ReleaseStatusReport) -> String {
    if let Some(latest) = &report.latest {
        summarize_release_attempt(latest)
    } else {
        "no release attempts found".to_string()
    }
}

pub fn render_release_status_text(report: &ReleaseStatusReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;

    let _ = writeln!(out, "━━━ jeryu release status ━━━");
    let _ = writeln!(
        out,
        "  Scope:      {}",
        release_scope(&ReleaseStatusQuery {
            project_id: report.project_id,
            ref_name: report.ref_name.clone(),
            sha: report.sha.clone(),
            limit: report.limit,
        })
    );
    let _ = writeln!(out, "  Generated:  {}", report.generated_at);
    let _ = writeln!(out, "  Window:     latest {} attempt(s)", report.limit);
    let _ = writeln!(out);

    if let Some(latest) = &report.latest {
        let attempt = &latest.attempt;
        let _ = writeln!(out, "  Latest:");
        let _ = writeln!(out, "    Version:   {}", attempt.version);
        let _ = writeln!(out, "    SHA:       {}", attempt.sha);
        let _ = writeln!(
            out,
            "    Upstream:  {} (pipeline {:?})",
            attempt.upstream_status, attempt.upstream_pipeline_id
        );
        let _ = writeln!(
            out,
            "    Release:   {} (pipeline {:?})",
            attempt
                .release_pipeline_status
                .as_deref()
                .unwrap_or("(not triggered)"),
            attempt.release_pipeline_id
        );
        let _ = writeln!(
            out,
            "    Prod:      {} (pipeline {:?})",
            attempt
                .production_pipeline_status
                .as_deref()
                .unwrap_or("(not triggered)"),
            attempt.production_pipeline_id
        );
        let _ = writeln!(out, "    Canary:    {}", attempt.canary_status);
        let _ = writeln!(out, "    State:     {}", latest.canary_state);
        let _ = writeln!(out, "    Eligible:  {}", latest.eligibility);
        let _ = writeln!(
            out,
            "    Phase:     {}",
            latest.phase.as_deref().unwrap_or("(unknown)")
        );
        let _ = writeln!(
            out,
            "    StateFile: {}",
            latest.state_status.as_deref().unwrap_or("(missing)")
        );
        let _ = writeln!(
            out,
            "    Gates:     remote={} telemetry={} e2e={} telemetry_diag={} identity_ok={}",
            latest.has_remote_gate,
            latest.has_telemetry_gate,
            latest.has_e2e_gate,
            latest.has_telemetry_diag,
            latest.release_identity_ok
        );
        let _ = writeln!(
            out,
            "    URL:       {}",
            latest.canary_public_url.as_deref().unwrap_or("(pending)")
        );
        let _ = writeln!(
            out,
            "    Started:   {}",
            attempt
                .canary_started_at
                .as_deref()
                .unwrap_or("(not started)")
        );
        let _ = writeln!(
            out,
            "    Finished:  {}",
            attempt
                .canary_finished_at
                .as_deref()
                .unwrap_or("(not finished)")
        );
        let _ = writeln!(
            out,
            "    Note:      {}",
            latest.detail.as_deref().unwrap_or("(none)")
        );
        let _ = writeln!(out, "    Evidence:  {}", latest.canary_state_path);
        let _ = writeln!(out, "    Release:   {}", latest.release_dir);
        let _ = writeln!(out);
    } else {
        let _ = writeln!(out, "  Latest:     (no release attempts found)");
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "  Recent attempts:");
    if report.recent.is_empty() {
        let _ = writeln!(out, "    (none)");
    } else {
        for attempt in &report.recent {
            let a = &attempt.attempt;
            let _ = writeln!(
                out,
                "    [{}] project={} ref={} sha={} version={} upstream={} release={} prod={} canary={} phase={}",
                attempt.canary_state,
                a.project_id,
                a.ref_name,
                a.sha,
                a.version,
                a.upstream_status,
                a.release_pipeline_status
                    .as_deref()
                    .unwrap_or("not-triggered"),
                a.production_pipeline_status
                    .as_deref()
                    .unwrap_or("not-triggered"),
                a.canary_status,
                attempt.phase.as_deref().unwrap_or("unknown"),
            );
        }
    }

    out
}

pub async fn render_release_status(db: &Db, query: ReleaseStatusQuery, json: bool) -> Result<()> {
    let report = build_release_status_report(db, query).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_release_status_text(&report));
    }
    Ok(())
}

pub async fn watch_release_status(
    db: &Db,
    query: ReleaseStatusQuery,
    json: bool,
    interval_secs: u64,
) -> Result<()> {
    use std::io::{self, Write};
    use tokio::time::{Duration, sleep};

    let mut stdout = io::stdout();
    loop {
        let report = build_release_status_report(db, query.clone()).await?;
        write!(stdout, "\x1b[2J\x1b[H")?;
        if json {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&report)?)?;
        } else {
            write!(stdout, "{}", render_release_status_text(&report))?;
        }
        stdout.flush()?;

        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = sleep(Duration::from_secs(interval_secs)) => {}
        }
    }
    Ok(())
}
