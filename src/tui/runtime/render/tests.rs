use super::*;
use crate::tui::{app::App, ui};
use ratatui::backend::TestBackend;

fn draw_once(app: &mut App) -> Result<()> {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| ui::draw(f, app))?;
    Ok(())
}

fn render_text(app: &mut App) -> Result<String> {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| ui::draw(f, app))?;
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer.get(x, y).symbol());
        }
        text.push('\n');
    }
    Ok(text)
}

fn job(job_id: i64, status: &str) -> crate::state::JobEvent {
    crate::state::JobEvent {
        job_id,
        project_id: 2,
        pipeline_id: Some(10),
        status: status.into(),
        job_name: Some(format!("test-job-{job_id}")),
        pool_name: Some("default".into()),
        system_id: None,
        queued_duration: None,
        received_at: "2026-04-23T19:00:00Z".into(),
    }
}

fn jankurai_finding(index: usize) -> crate::tui::jankurai::JankuraiEntry {
    crate::tui::jankurai::JankuraiEntry {
        kind: crate::tui::jankurai::JankuraiEntryKind::Finding,
        label: format!("finding-{index}"),
        severity: Some("high".into()),
        hardness: Some("hard".into()),
        path: Some(format!("src/file_{index}.rs")),
        rule: Some(format!("rule-{index}")),
        lane: Some("audit".into()),
        owner: Some("workspace".into()),
        problem: Some(format!("finding-{index} requires attention")),
        evidence: vec![format!("evidence-{index}")],
        suggested_fix: Some(format!("fix finding-{index}")),
    }
}

#[tokio::test]
async fn renders_all_primary_tabs_with_empty_state() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.jankurai.installed = true;
    for tab in [
        crate::tui::app::ActiveTab::Workflow,
        crate::tui::app::ActiveTab::Mission,
        crate::tui::app::ActiveTab::Release,
        crate::tui::app::ActiveTab::Jobs,
        crate::tui::app::ActiveTab::Agents,
        crate::tui::app::ActiveTab::Tests,
        crate::tui::app::ActiveTab::Pools,
        crate::tui::app::ActiveTab::Cache,
        crate::tui::app::ActiveTab::Evidence,
        crate::tui::app::ActiveTab::Secrets,
        crate::tui::app::ActiveTab::Git,
        crate::tui::app::ActiveTab::Jank,
    ] {
        app.active_tab = tab;
        draw_once(&mut app)?;
    }
    Ok(())
}

#[tokio::test]
async fn renders_maximized_logs_empty_state() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.active_pane = crate::tui::app::ActivePane::Jobs;
    app.maximize_logs = true;
    draw_once(&mut app)?;
    Ok(())
}

#[tokio::test]
async fn renders_flow_with_jobs_list_and_live_log() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.recent_jobs = vec![job(1, "running"), job(2, "pending")];
    app.state.live_log.text = "cargo test\nwarning: slow test\nerror: sample failure".into();
    app.state.flow.active_pipelines = vec![crate::tui::flow::PipelineFlow {
        pipeline_id: 10,
        project_id: 2,
        ref_name: "main".into(),
        sha: Some("abc123".into()),
        status: "running".into(),
        graph: crate::tui::flow::FlowGraph::default(),
        current_blocker: None,
        critical_path: vec![],
        eta: None,
        progress_pct: 64,
    }];

    app.active_tab = crate::tui::app::ActiveTab::Jobs;
    app.active_pane = crate::tui::app::ActivePane::Jobs;
    draw_once(&mut app)?;
    Ok(())
}

#[tokio::test]
async fn navigation_cycles_tabs_and_panes() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.jankurai.installed = false;
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Workflow);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Mission);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Release);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Jobs);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Agents);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Tests);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Pools);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Cache);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Evidence);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Secrets);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Git);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Workflow);

    app.state.jankurai.installed = true;
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Mission);
    app.active_tab = crate::tui::app::ActiveTab::Git;
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Jank);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Workflow);

    assert_eq!(app.active_pane, crate::tui::app::ActivePane::Jobs);
    app.cycle_pane_next();
    assert_eq!(app.active_pane, crate::tui::app::ActivePane::Jobs);
    Ok(())
}

#[tokio::test]
async fn jank_tab_is_hidden_when_unavailable() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.jankurai.installed = false;
    let text = render_text(&mut app)?;
    assert!(!text.contains("Jank"), "jank tab should stay hidden");
    Ok(())
}

#[tokio::test]
async fn help_and_palette_hide_jank_when_unavailable() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.jankurai.installed = false;
    app.help_overlay_open = true;
    let help_text = render_text(&mut app)?;
    assert!(
        !help_text.contains("Jank"),
        "help overlay should hide jank shortcut when unavailable"
    );

    app.help_overlay_open = false;
    app.command_palette_open = true;
    app.command_palette_query = "jank".into();
    let palette_text = render_text(&mut app)?;
    assert!(
        !palette_text.contains("Jank"),
        "command palette should hide jank action when unavailable"
    );
    Ok(())
}

#[tokio::test]
async fn jank_tab_renders_with_available_snapshot() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.jankurai.installed = true;
    app.state.jankurai.history = vec![
        crate::tui::jankurai::JankuraiHistoryPoint {
            generated_at: chrono::Utc::now(),
            score: 81,
            raw_score: Some(82),
            decision: Some("advisory".into()),
        },
        crate::tui::jankurai::JankuraiHistoryPoint {
            generated_at: chrono::Utc::now(),
            score: 92,
            raw_score: Some(92),
            decision: Some("advisory".into()),
        },
    ];
    app.state.jankurai.dimensions = vec![crate::tui::jankurai::JankuraiDimension {
        name: "Ownership and navigation surface".into(),
        weight: 13,
        score: 100,
        weighted_points: 13.0,
        evidence: vec!["root AGENTS.md present".into()],
        notes: vec!["stable".into()],
    }];
    app.state.jankurai.last_scan = Some(crate::tui::jankurai::JankuraiScan {
        generated_at: Some(chrono::Utc::now()),
        score: 92,
        raw_score: 92,
        minimum_score: 85,
        decision: "block".into(),
        score_status: "advisory".into(),
        finding_count: 2,
        hard_findings: 1,
        soft_findings: 1,
        caps_applied: vec!["fallback-soup-in-product-code".into()],
    });
    app.state.jankurai.entries = vec![
        crate::tui::jankurai::JankuraiEntry {
            kind: crate::tui::jankurai::JankuraiEntryKind::Cap,
            label: "fallback-soup-in-product-code".into(),
            severity: Some("cap".into()),
            hardness: Some("n/a".into()),
            path: Some("agent/repo-score.json".into()),
            rule: Some("fallback-soup-in-product-code".into()),
            lane: Some("audit".into()),
            owner: Some("agent".into()),
            problem: Some("cap applied: fallback-soup-in-product-code".into()),
            evidence: vec!["applied cap recorded in repo score".into()],
            suggested_fix: Some("review the blocking audit rule and rerun the score lane".into()),
        },
        crate::tui::jankurai::JankuraiEntry {
            kind: crate::tui::jankurai::JankuraiEntryKind::Finding,
            label: "HLT-002".into(),
            severity: Some("high".into()),
            hardness: Some("hard".into()),
            path: Some("agent/generated-zones.toml".into()),
            rule: Some("HLT-002-GENERATED-MUTATION".into()),
            lane: Some("contract".into()),
            owner: Some("agent".into()),
            problem: Some("generated zone file `agent/repo-score.json` is missing".into()),
            evidence: vec!["generated zone integrity violation".into()],
            suggested_fix: Some("regenerate the score artifacts".into()),
        },
    ];
    app.selected_jankurai_index = 1;
    app.active_tab = crate::tui::app::ActiveTab::Jank;

    let text = render_text(&mut app)?;
    assert!(text.contains("Jankurai Summary"));
    assert!(text.contains("generated zone file"));
    assert!(text.contains("CAP"));
    Ok(())
}

#[tokio::test]
async fn jank_tab_windows_findings_around_selected_entry() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.jankurai.installed = true;
    app.state.jankurai.entries = (0..32).map(jankurai_finding).collect();
    app.selected_jankurai_index = 30;
    app.active_tab = crate::tui::app::ActiveTab::Jank;

    let text = render_text(&mut app)?;
    assert!(
        text.contains("finding-30 requires attention"),
        "selected finding should remain visible"
    );
    assert!(
        !text.contains("finding-0 requires attention"),
        "issue list should be windowed near the selected entry"
    );
    Ok(())
}

#[tokio::test]
async fn jank_tab_renders_single_history_point() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.jankurai.installed = true;
    app.state.jankurai.history = vec![crate::tui::jankurai::JankuraiHistoryPoint {
        generated_at: chrono::Utc::now(),
        score: 88,
        raw_score: Some(89),
        decision: Some("advisory".into()),
    }];
    app.active_tab = crate::tui::app::ActiveTab::Jank;

    let text = render_text(&mut app)?;
    assert!(text.contains("Score History"));
    assert!(text.contains("Jankurai Summary"));
    Ok(())
}
