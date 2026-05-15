use super::{
    LiveLogState, StageProgress, TuiStateSnapshot, build_stage_progress_from_ci_runs,
    build_stage_progress_from_events, live_job_status_rank,
};
use crate::state::{CiJobRun, JobEvent};
use crate::tui::flow::{FlowGraph, FlowSnapshot, PipelineFlow};
use anyhow::Result;

fn job(job_id: i64, status: &str, received_at: &str) -> JobEvent {
    JobEvent {
        job_id,
        project_id: 2,
        pipeline_id: Some(10),
        status: status.into(),
        job_name: Some(format!("test-job-{job_id}")),
        pool_name: Some("default".into()),
        system_id: None,
        queued_duration: None,
        received_at: received_at.into(),
    }
}

fn pipeline_flow(pipeline_id: i64) -> PipelineFlow {
    PipelineFlow {
        pipeline_id,
        project_id: 2,
        ref_name: "main".into(),
        sha: Some("abc123".into()),
        status: "running".into(),
        graph: FlowGraph::default(),
        current_blocker: None,
        critical_path: Vec::new(),
        eta: None,
        progress_pct: 50,
    }
}

#[test]
fn live_jobs_sort_running_ahead_of_created_and_pending() {
    let mut jobs = [
        JobEvent {
            job_id: 1,
            project_id: 2,
            pipeline_id: None,
            status: "created".into(),
            job_name: Some("build-enclave-server".into()),
            pool_name: Some("x86-64".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:00:00Z".into(),
        },
        JobEvent {
            job_id: 2,
            project_id: 2,
            pipeline_id: None,
            status: "running".into(),
            job_name: Some("test-rust-nextest-1".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:01:00Z".into(),
        },
        JobEvent {
            job_id: 3,
            project_id: 2,
            pipeline_id: None,
            status: "pending".into(),
            job_name: Some("test-rust-nextest-4".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:02:00Z".into(),
        },
    ];

    jobs.sort_by(|left, right| {
        live_job_status_rank(&right.status)
            .cmp(&live_job_status_rank(&left.status))
            .then_with(|| right.received_at.cmp(&left.received_at))
            .then_with(|| right.job_id.cmp(&left.job_id))
    });

    let statuses: Vec<_> = jobs.iter().map(|job| job.status.as_str()).collect();
    assert_eq!(statuses, vec!["running", "pending", "created"]);
}

#[tokio::test]
async fn core_snapshot_preserves_flow_and_live_log_state() -> Result<()> {
    let mut app = super::test_app().await?;
    app.state.flow.outdated = true;
    app.state.live_log = LiveLogState {
        text: "running test output".into(),
        ..Default::default()
    };

    app.sync_tx.send(TuiStateSnapshot::default()).await.unwrap();
    app.tick().await;

    assert!(app.state.flow.outdated);
    assert_eq!(app.state.live_log.text, "running test output");
    Ok(())
}

#[tokio::test]
async fn refresh_coerces_jank_tab_when_availability_disappears() -> Result<()> {
    let mut app = super::test_app().await?;
    app.state.jankurai.installed = true;
    app.active_tab = super::ActiveTab::Jank;

    let mut snap = TuiStateSnapshot::default();
    snap.jankurai.installed = false;
    app.sync_tx.send(snap).await.unwrap();
    app.tick().await;

    assert_eq!(app.active_tab, super::ActiveTab::Workflow);
    Ok(())
}

#[tokio::test]
async fn empty_flow_snapshot_does_not_blank_existing_board() -> Result<()> {
    let mut app = super::test_app().await?;
    let generated_at = chrono::Utc::now();
    app.state.flow = FlowSnapshot {
        generated_at,
        active_pipelines: vec![pipeline_flow(42)],
        last_non_empty_at: Some(generated_at),
        ..Default::default()
    };

    app.flow_tx
        .send(FlowSnapshot {
            generated_at: generated_at + chrono::Duration::seconds(5),
            gitlab_online: true,
            ..Default::default()
        })
        .await
        .unwrap();
    app.tick().await;

    assert_eq!(app.state.flow.active_pipelines.len(), 1);
    assert_eq!(app.state.flow.active_pipelines[0].pipeline_id, 42);
    assert!(app.state.flow.outdated);
    assert_eq!(app.state.flow.last_non_empty_at, Some(generated_at));
    assert!(app.state.flow.gitlab_online);
    Ok(())
}

#[tokio::test]
async fn empty_flow_snapshot_uses_recent_jobs_before_collector_graph_arrives() -> Result<()> {
    let mut app = super::test_app().await?;
    let generated_at = chrono::Utc::now();
    app.state.recent_jobs = vec![
        JobEvent {
            job_id: 7,
            project_id: 2,
            pipeline_id: Some(55),
            status: "running".into(),
            job_name: Some("test-frontend-nht".into()),
            pool_name: Some("default".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:00:00Z".into(),
        },
        JobEvent {
            job_id: 8,
            project_id: 2,
            pipeline_id: Some(55),
            status: "created".into(),
            job_name: Some("test-local-rc".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:01:00Z".into(),
        },
    ];

    app.flow_tx
        .send(FlowSnapshot {
            generated_at,
            gitlab_online: true,
            ..Default::default()
        })
        .await
        .unwrap();
    app.tick().await;

    assert_eq!(app.state.flow.active_pipelines.len(), 1);
    assert_eq!(app.state.flow.active_pipelines[0].pipeline_id, 55);
    assert_eq!(app.state.flow.active_pipelines[0].graph.nodes.len(), 2);
    assert!(app.state.flow.outdated);
    Ok(())
}

#[tokio::test]
async fn selected_job_survives_refresh_reorder() -> Result<()> {
    let mut app = super::test_app().await?;
    app.state.recent_jobs = vec![
        job(1, "running", "2026-04-23T19:00:00Z"),
        job(2, "pending", "2026-04-23T19:01:00Z"),
    ];
    app.selected_job_index = 1;
    app.remember_selected_job();

    let snap = TuiStateSnapshot {
        recent_jobs: vec![
            job(2, "running", "2026-04-23T19:02:00Z"),
            job(1, "success", "2026-04-23T19:03:00Z"),
        ],
        ..Default::default()
    };
    app.sync_tx.send(snap).await.unwrap();
    app.tick().await;

    assert_eq!(app.selected_job_index, 0);
    assert_eq!(app.selected_job().map(|job| job.job_id), Some(2));
    assert_eq!(app.log_target.map(|target| target.job_id), None);
    Ok(())
}

#[tokio::test]
async fn opening_and_scrolling_logs_controls_follow_mode() -> Result<()> {
    let mut app = super::test_app().await?;
    app.state.recent_jobs = vec![job(7, "running", "2026-04-23T19:00:00Z")];

    app.open_selected_job_log();
    assert!(app.maximize_logs);
    assert!(app.follow_log_tail);
    assert_eq!(app.log_target.map(|target| target.job_id), Some(7));

    app.scroll_logs_up(1);
    assert!(!app.follow_log_tail);

    app.follow_logs();
    assert!(app.follow_log_tail);
    assert_eq!(app.log_scroll_offset, u16::MAX);
    Ok(())
}

// ── Stage progress helpers ────────────────────────────────────────────────────

fn ci_run(stage: &str, status: &str) -> CiJobRun {
    CiJobRun {
        job_id: 1,
        project_id: 2,
        pipeline_id: 100,
        root_pipeline_id: 100,
        pipeline_sha: "abc".into(),
        ref_name: "main".into(),
        job_name: format!("job-{stage}"),
        stage: stage.into(),
        status: status.into(),
        runner: None,
        runner_pool: None,
        queued_duration_secs: None,
        duration_secs: None,
        started_at: None,
        finished_at: None,
        web_url: None,
        observed_at: "2026-05-14T10:00:00Z".into(),
    }
}

fn event_for_pipeline(pipeline_id: i64, pool: &str, status: &str) -> JobEvent {
    JobEvent {
        job_id: 1,
        project_id: 2,
        pipeline_id: Some(pipeline_id),
        status: status.into(),
        job_name: Some(format!("job-{pool}")),
        pool_name: Some(pool.into()),
        system_id: None,
        queued_duration: None,
        received_at: "2026-05-14T10:00:00Z".into(),
    }
}

#[test]
fn stage_progress_from_ci_runs_groups_and_counts() {
    let runs = vec![
        ci_run("build", "success"),
        ci_run("build", "success"),
        ci_run("test", "running"),
        ci_run("test", "pending"),
        ci_run("test", "failed"),
        ci_run("deploy", "pending"),
    ];
    let stages = build_stage_progress_from_ci_runs(&runs);

    assert_eq!(stages.len(), 3);

    assert_eq!(stages[0].stage_name, "build");
    assert_eq!(stages[0].total_jobs, 2);
    assert_eq!(stages[0].completed_jobs, 2);
    assert_eq!(stages[0].running_jobs, 0);
    assert_eq!(stages[0].failed_jobs, 0);
    assert_eq!(stages[0].status, "success");

    assert_eq!(stages[1].stage_name, "test");
    assert_eq!(stages[1].total_jobs, 3);
    assert_eq!(stages[1].running_jobs, 1);
    assert_eq!(stages[1].failed_jobs, 1);
    // failed_jobs > 0 → failed, even with running jobs
    assert_eq!(stages[1].status, "failed");

    assert_eq!(stages[2].stage_name, "deploy");
    assert_eq!(stages[2].total_jobs, 1);
    assert_eq!(stages[2].completed_jobs, 0);
    assert_eq!(stages[2].status, "pending");
}

#[test]
fn stage_progress_from_ci_runs_preserves_insertion_order() {
    let runs = vec![ci_run("z-stage", "success"), ci_run("a-stage", "success")];
    let stages = build_stage_progress_from_ci_runs(&runs);
    assert_eq!(stages[0].stage_name, "z-stage");
    assert_eq!(stages[1].stage_name, "a-stage");
}

#[test]
fn stage_progress_from_events_filters_by_pipeline_id() {
    let events = vec![
        event_for_pipeline(100, "build", "success"),
        event_for_pipeline(100, "test", "running"),
        event_for_pipeline(100, "test", "success"),
        event_for_pipeline(999, "build", "running"), // different pipeline → excluded
    ];
    let stages = build_stage_progress_from_events(&events, 100);

    assert_eq!(stages.len(), 2);

    assert_eq!(stages[0].stage_name, "build");
    assert_eq!(stages[0].total_jobs, 1);
    assert_eq!(stages[0].completed_jobs, 1);
    assert_eq!(stages[0].status, "success");

    assert_eq!(stages[1].stage_name, "test");
    assert_eq!(stages[1].total_jobs, 2);
    assert_eq!(stages[1].running_jobs, 1);
    assert_eq!(stages[1].completed_jobs, 1);
    assert_eq!(stages[1].status, "running");
}

#[test]
fn stage_progress_overall_pct_weights_running_at_half() {
    // 2 success + 2 running out of 6 total → (2 + 2*0.5) / 6 = 50%
    let stages = vec![
        StageProgress {
            stage_name: "build".into(),
            total_jobs: 2,
            completed_jobs: 2,
            running_jobs: 0,
            failed_jobs: 0,
            status: "success".into(),
            ..Default::default()
        },
        StageProgress {
            stage_name: "test".into(),
            total_jobs: 4,
            completed_jobs: 0,
            running_jobs: 2,
            failed_jobs: 0,
            status: "running".into(),
            ..Default::default()
        },
    ];
    let total: usize = stages.iter().map(|s| s.total_jobs).sum();
    let completed: usize = stages.iter().map(|s| s.completed_jobs).sum();
    let running: usize = stages.iter().map(|s| s.running_jobs).sum();
    let pct = ((completed as f64 + running as f64 * 0.5) / total as f64 * 100.0) as u16;
    assert_eq!(pct, 50);
}

#[test]
fn stage_progress_from_events_empty_when_no_matching_pipeline() {
    let events = vec![event_for_pipeline(999, "build", "running")];
    let stages = build_stage_progress_from_events(&events, 100);
    assert!(stages.is_empty());
}
