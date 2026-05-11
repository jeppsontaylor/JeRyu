use super::*;

pub async fn build_progress_report(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
) -> Result<ProgressReport> {
    let root = crate::settings::release_repo_root();
    let schema = load_ci_schema(&root).await?;
    let latest_pipeline = latest_pipeline_for_ref(client, project_id, ref_name).await?;
    let winning_pipeline =
        latest_release_candidate_pipeline_for_ref(client, project_id, ref_name).await?;

    let latest_statuses =
        collect_pipeline_statuses(client, project_id, &schema.jobs, latest_pipeline.as_ref())
            .await?;
    let winning_statuses =
        collect_pipeline_statuses(client, project_id, &schema.jobs, winning_pipeline.as_ref())
            .await?;

    let winning_sha = winning_pipeline
        .as_ref()
        .map(|pipeline| pipeline.sha.clone());
    let expected_release_version = winning_sha.as_ref().map(|sha| render_release_version(sha));
    let punchlist_summary = read_punchlist_summary(&root);
    let punchlist_freshness = punchlist_freshness(
        &root,
        winning_sha.as_deref(),
        expected_release_version.as_deref(),
    );
    let use_punchlist_summary = punchlist_freshness.starts_with("current");
    let progress_statuses = if winning_statuses.is_empty() {
        &latest_statuses
    } else {
        &winning_statuses
    };
    let progress_pipeline_status = if winning_statuses.is_empty() {
        latest_pipeline
            .as_ref()
            .map(|pipeline| pipeline.status.as_str())
            .unwrap_or("pending")
    } else {
        winning_pipeline
            .as_ref()
            .map(|pipeline| pipeline.status.as_str())
            .unwrap_or("pending")
    };

    let release_critical = summary_lane_or_default(
        punchlist_summary.as_ref(),
        "release_critical_jobs",
        &schema.jobs,
        progress_statuses,
        "release-blocking",
        progress_pipeline_status,
    );
    let extended = summary_lane_or_default(
        punchlist_summary.as_ref(),
        "extended_verification",
        &schema.jobs,
        progress_statuses,
        "extended",
        progress_pipeline_status,
    );
    let research = summary_lane_or_default(
        punchlist_summary.as_ref(),
        "research_support",
        &schema.jobs,
        progress_statuses,
        "research",
        progress_pipeline_status,
    );

    let blocking_remaining = match (use_punchlist_summary, punchlist_summary.as_ref()) {
        (true, Some(summary)) => summary_job_items(summary, true, false),
        _ => collect_job_ids(&schema.jobs, |job| {
            job.lane == "release-blocking"
                && !matches!(
                    effective_progress_status(progress_statuses, &job.id, progress_pipeline_status),
                    "success" | "skipped" | "omitted" | "vti-skipped"
                )
        }),
    };
    let non_blocking_failed = match (use_punchlist_summary, punchlist_summary.as_ref()) {
        (true, Some(summary)) => summary_job_items(summary, false, true),
        _ => collect_job_ids(&schema.jobs, |job| {
            !job.release_blocking
                && matches!(
                    effective_progress_status(progress_statuses, &job.id, progress_pipeline_status),
                    "failed" | "canceled"
                )
        }),
    };
    let attempt_view = if let Some(sha) = winning_sha.as_ref() {
        build_release_status_report(
            db,
            ReleaseStatusQuery {
                project_id: Some(project_id),
                ref_name: Some(ref_name.to_string()),
                sha: Some(sha.clone()),
                limit: 1,
            },
        )
        .await?
        .latest
    } else {
        None
    };

    let mut release_execution = ReleaseExecutionProgress::default();
    if let Some(attempt) = &attempt_view {
        release_execution.attempt_exists = true;
        release_execution.remote_gate = attempt.has_remote_gate;
        release_execution.telemetry_gate = attempt.has_telemetry_gate;
        release_execution.e2e_gate = attempt.has_e2e_gate;
        release_execution.latest_attempt_sha = Some(attempt.attempt.sha.clone());
        release_execution.latest_attempt_state = Some(attempt.canary_state.clone());
        release_execution.phase = attempt.phase.clone();
        release_execution.eligibility = Some(attempt.eligibility.clone());
    }
    if let Some(summary) = &punchlist_summary
        && let Some(release_evidence) = summary.get("release_evidence")
    {
        release_execution.remote_gate = release_execution.remote_gate
            || release_evidence
                .get("remote_gate")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        release_execution.telemetry_gate = release_execution.telemetry_gate
            || release_evidence
                .get("telemetry_gate")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        release_execution.e2e_gate = release_execution.e2e_gate
            || release_evidence
                .get("e2e_gate")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
    }
    release_execution.punchlist_current = punchlist_freshness.starts_with("current");
    release_execution.percent = release_execution_percent(&release_execution);
    let current_blocker = if let Some(job) = blocking_remaining.first() {
        Some(format!("release-critical job pending: {job}"))
    } else if !release_execution.attempt_exists {
        Some("release attempt missing for winning sha".to_string())
    } else if !release_execution.remote_gate {
        Some("canary remote gate missing".to_string())
    } else if !release_execution.telemetry_gate {
        Some("canary telemetry gate missing".to_string())
    } else if !release_execution.e2e_gate {
        Some("canary e2e gate missing".to_string())
    } else if !release_execution.punchlist_current {
        Some("punchlist is outdated for winning sha".to_string())
    } else {
        None
    };

    Ok(ProgressReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id,
        ref_name: ref_name.to_string(),
        latest_pipeline_id: latest_pipeline.as_ref().map(|pipeline| pipeline.id),
        latest_pipeline_status: latest_pipeline
            .as_ref()
            .map(|pipeline| pipeline.status.clone()),
        latest_pipeline_sha: latest_pipeline
            .as_ref()
            .map(|pipeline| pipeline.sha.clone()),
        winning_pipeline_id: winning_pipeline.as_ref().map(|pipeline| pipeline.id),
        winning_sha,
        expected_release_version,
        release_critical,
        extended,
        research,
        release_execution,
        blocking_remaining,
        non_blocking_failed,
        current_blocker,
        punchlist_freshness,
    })
}

fn summary_lane_or_default(
    summary: Option<&serde_json::Value>,
    key: &str,
    schema: &[CiSchemaJob],
    statuses: &HashMap<String, String>,
    lane: &str,
    pipeline_status: &str,
) -> LaneProgress {
    match summary.and_then(|summary| summary_lane_progress(summary, key)) {
        Some(progress) => progress,
        None => lane_progress(schema, statuses, lane, pipeline_status),
    }
}

pub fn render_progress_text(report: &ProgressReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;

    let _ = writeln!(out, "━━━ jeryu progress ━━━");
    let _ = writeln!(out, "  Generated:         {}", report.generated_at);
    let _ = writeln!(out, "  Ref:               {}", report.ref_name);
    let _ = writeln!(
        out,
        "  Latest pipeline:   {:?} status={} sha={}",
        report.latest_pipeline_id,
        report.latest_pipeline_status.as_deref().unwrap_or("(none)"),
        report.latest_pipeline_sha.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Winning pipeline:  {:?} sha={} version={}",
        report.winning_pipeline_id,
        report.winning_sha.as_deref().unwrap_or("(none)"),
        report
            .expected_release_version
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(out);
    write_lane_progress_summary(&mut out, report, "  ", "Release-Critical");
    let _ = writeln!(
        out,
        "  Release Execution: {:.1}% freshness={} phase={}",
        report.release_execution.percent,
        report.punchlist_freshness,
        report
            .release_execution
            .phase
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Latest attempt:    sha={} state={}",
        report
            .release_execution
            .latest_attempt_sha
            .as_deref()
            .unwrap_or("(none)"),
        report
            .release_execution
            .latest_attempt_state
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Current blocker:   {}",
        report.current_blocker.as_deref().unwrap_or("(none)")
    );
    if !report.blocking_remaining.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Blocking remaining:");
        for job in &report.blocking_remaining {
            let _ = writeln!(out, "    - {}", job);
        }
    }
    if !report.non_blocking_failed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Non-blocking failed:");
        for job in &report.non_blocking_failed {
            let _ = writeln!(out, "    - {}", job);
        }
    }
    out
}

pub(crate) fn pipeline_lane_progress(
    schema: &[CiSchemaJob],
    statuses: &HashMap<String, AggregatedPipelineJob>,
    lane: &str,
    pipeline_status: &str,
) -> LaneProgress {
    let mut total = 0usize;
    let mut passed = 0usize;
    for job in schema.iter().filter(|job| job.lane == lane) {
        let status = effective_job_status(statuses.get(&job.id), pipeline_status);
        if matches!(status, "omitted" | "skipped" | "vti-skipped") {
            continue;
        }
        total += 1;
        if status == "success" {
            passed += 1;
        }
    }
    let percent = lane_progress_percent(total, passed);
    LaneProgress {
        passed,
        total,
        percent,
    }
}

pub(crate) fn lane_progress_percent(total: usize, passed: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (passed as f64 / total as f64) * 100.0
    }
}

pub(crate) fn collect_job_ids<F>(jobs: &[CiSchemaJob], predicate: F) -> Vec<String>
where
    F: Fn(&CiSchemaJob) -> bool,
{
    jobs.iter()
        .filter(|job| predicate(job))
        .map(|job| job.id.clone())
        .collect::<Vec<_>>()
}

pub(crate) trait LaneProgressSummaryView {
    fn release_critical_progress(&self) -> &LaneProgress;
    fn extended_progress(&self) -> &LaneProgress;
    fn research_progress(&self) -> &LaneProgress;
}

macro_rules! impl_lane_progress_summary_view {
    ($ty:ty) => {
        impl LaneProgressSummaryView for $ty {
            fn release_critical_progress(&self) -> &LaneProgress {
                &self.release_critical
            }

            fn extended_progress(&self) -> &LaneProgress {
                &self.extended
            }

            fn research_progress(&self) -> &LaneProgress {
                &self.research
            }
        }
    };
}

impl_lane_progress_summary_view!(ProgressReport);
impl_lane_progress_summary_view!(PipelineExplainReport);

pub(crate) fn write_lane_progress_summary<T: LaneProgressSummaryView>(
    out: &mut String,
    report: &T,
    indent: &str,
    release_critical_label: &str,
) {
    use std::fmt::Write as _;

    let _ = writeln!(
        out,
        "{indent}{release_critical_label}: {}/{} ({:.1}%)",
        report.release_critical_progress().passed,
        report.release_critical_progress().total,
        report.release_critical_progress().percent
    );
    let _ = writeln!(
        out,
        "{indent}Extended:         {}/{} ({:.1}%)",
        report.extended_progress().passed,
        report.extended_progress().total,
        report.extended_progress().percent
    );
    let _ = writeln!(
        out,
        "{indent}Research:         {}/{} ({:.1}%)",
        report.research_progress().passed,
        report.research_progress().total,
        report.research_progress().percent
    );
}

pub(crate) fn effective_job_status<'a>(
    state: Option<&'a AggregatedPipelineJob>,
    pipeline_status: &str,
) -> &'a str {
    match state {
        Some(state) => state.status.as_str(),
        None => match pipeline_status {
            "success" | "failed" | "canceled" | "skipped" => "omitted",
            _ => "pending",
        },
    }
}

pub(crate) fn pipeline_item(
    job: &CiSchemaJob,
    state: Option<&AggregatedPipelineJob>,
    effective_status: &str,
) -> PipelineExplainItem {
    PipelineExplainItem {
        id: job.id.clone(),
        status: display_job_status(effective_status).to_string(),
        stage: state.and_then(|s| s.stage.clone()),
        runner_pool: job.runner_pool.clone(),
        kind: job.kind.clone(),
        component: job.component.clone(),
        evidence_driven: job.evidence_driven,
        estimated_cost: job.estimated_cost.clone(),
        evidence_outputs: job.evidence_outputs.clone(),
        depends_on: job.depends_on.clone(),
    }
}

pub(crate) fn display_job_status(status: &str) -> &str {
    match status {
        "omitted" => "vti-skipped",
        other => other,
    }
}

pub(crate) fn write_pipeline_item_section(
    out: &mut String,
    heading: &str,
    items: &[PipelineExplainItem],
) {
    if items.is_empty() {
        return;
    }

    use std::fmt::Write as _;

    let _ = writeln!(out);
    let _ = writeln!(out, "  {heading}:");
    for item in items {
        let _ = writeln!(
            out,
            "    - {} [{} / {} / {}]",
            item.id,
            item.runner_pool,
            item.kind,
            display_job_status(&item.status)
        );
    }
}
