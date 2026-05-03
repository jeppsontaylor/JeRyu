//! Owner: Interactive TUI subsystem — rendering logic
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Rendering redacts sensitive text and does not perform control-plane mutations directly.
use super::app::{ActivePane, ActiveTab, App};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
};

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

fn status_color(status: &str) -> Color {
    match status {
        "success" | "omitted" | "vti-skipped" => Color::Green,
        "running" => Color::Blue,
        "failed" => Color::Red,
        "pending" | "created" => Color::Yellow,
        "canceled" => Color::DarkGray,
        _ => Color::Gray,
    }
}

fn release_color(state: &str) -> Color {
    match state {
        "green" | "released" => Color::Green,
        "in-flight" | "canary-authorized" => Color::Cyan,
        "waiting" | "ready-for-canary" => Color::Yellow,
        "blocked" | "blocked-by-upstream" => Color::Magenta,
        "failed" => Color::Red,
        _ => Color::DarkGray,
    }
}

fn pane_border(pane: ActivePane, app: &App) -> Color {
    if app.active_pane == pane {
        Color::Cyan
    } else {
        Color::DarkGray
    }
}

fn status_badge(status: &str) -> (&'static str, Color) {
    match status {
        "success" | "passed" | "green" | "released" => ("PASS", Color::Green),
        "running" | "in-flight" | "canary-authorized" => ("RUN", Color::Cyan),
        "failed" => ("FAIL", Color::Red),
        "blocked" | "blocked-by-upstream" => ("BLOCK", Color::Magenta),
        "pending" | "created" | "waiting" | "ready-for-canary" => ("WAIT", Color::Yellow),
        "canceled" | "vti-skipped" | "omitted" => ("SKIP", Color::DarkGray),
        _ => ("INFO", Color::Gray),
    }
}

fn meter_bar(percent: u16, width: usize) -> String {
    let width = width.max(1);
    let filled = (percent.min(100) as usize * width + 50) / 100;
    format!(
        "{}{} {:>3}%",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled)),
        percent.min(100)
    )
}

fn compact_spark(values: &[i64], width: usize) -> String {
    const STEPS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if values.is_empty() || width == 0 {
        return "n/a".to_string();
    }
    let take = width.min(values.len());
    let slice = &values[values.len() - take..];
    let min = slice.iter().copied().min().unwrap_or(0);
    let max = slice.iter().copied().max().unwrap_or(min);
    if max == min {
        return STEPS[0].to_string().repeat(take);
    }
    slice
        .iter()
        .map(|value| {
            let idx = (((*value - min) as f64 / (max - min) as f64) * 7.0).round() as usize;
            STEPS[idx.min(7)]
        })
        .collect()
}

fn top_attention(app: &App) -> (String, Color, String) {
    if app.state.active_taint_count > 0 {
        return (
            format!(
                "{} active cache taint(s) can block trusted proof reuse",
                app.state.active_taint_count
            ),
            Color::Magenta,
            "Open Cache, inspect taint scope, then run clean validation".to_string(),
        );
    }
    if let Some(rel) = &app.state.release_status
        && !matches!(rel.canary_state.as_str(), "green" | "released")
    {
        return (
            format!("Release {} is {}", rel.attempt.version, rel.canary_state),
            release_color(&rel.canary_state),
            "Open Release, inspect missing gate evidence".to_string(),
        );
    }
    if let Some(job) = app
        .state
        .recent_jobs
        .iter()
        .find(|job| job.status == "failed")
    {
        return (
            format!(
                "Job #{} failed in {}",
                job.job_id,
                job.job_name.as_deref().unwrap_or("unknown job")
            ),
            Color::Red,
            "Open evidence capsule or retry after blocker explanation".to_string(),
        );
    }
    if app
        .state
        .recent_jobs
        .iter()
        .any(|job| job.status == "running")
    {
        return (
            "Validation is active on the critical path".to_string(),
            Color::Cyan,
            "Watch Flow Board and open the slowest running job".to_string(),
        );
    }
    if !app.state.gitlab_ready {
        return (
            "GitLab is not ready".to_string(),
            Color::Yellow,
            "Wait for service readiness or inspect docker status".to_string(),
        );
    }
    (
        "No blocking proof gaps detected".to_string(),
        Color::Green,
        "Start work, run VTI planning, or inspect latest release state".to_string(),
    )
}

/// Returns (stale_age_secs, stale_color, stale_label) based on last_sync_at.
fn stale_indicator(app: &App) -> (i64, Color, &'static str) {
    let age = app
        .state
        .last_sync_at
        .map(|t| chrono::Utc::now().signed_duration_since(t).num_seconds())
        .unwrap_or(0);
    if age < 5 {
        (age, Color::Green, "")
    } else if age < 30 {
        (age, Color::DarkGray, "[STALE]")
    } else if age < 120 {
        (age, Color::Yellow, "[STALE]")
    } else if age < 300 {
        (age, Color::LightRed, "[STALE]")
    } else {
        (age, Color::Red, "!! DATA STALE !!")
    }
}

// ---------------------------------------------------------------------------
// Top-level draw entry point
// ---------------------------------------------------------------------------

pub fn draw(f: &mut Frame, app: &mut App) {
    if app.maximize_logs {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Header + tabs
                Constraint::Min(10),   // Full log view
                Constraint::Length(2), // Footer
            ])
            .split(f.area());

        draw_header_tabs(f, app, chunks[0]);
        draw_logs(f, app, chunks[1]);
        draw_footer(f, app, chunks[2]);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header + tabs
            Constraint::Min(10),   // Content
            Constraint::Length(4), // Event console
            Constraint::Length(2), // Footer
        ])
        .split(f.area());

    draw_header_tabs(f, app, chunks[0]);
    draw_footer(f, app, chunks[3]);

    match app.active_tab {
        ActiveTab::Mission => draw_mission_tab(f, app, chunks[1]),
        ActiveTab::Release => draw_release_tab(f, app, chunks[1]),
        ActiveTab::Jobs => draw_jobs_tab(f, app, chunks[1]),
        ActiveTab::Agents => draw_agents_tab(f, app, chunks[1]),
        ActiveTab::Tests => draw_tests_tab(f, app, chunks[1]),
        ActiveTab::Pools => draw_pools_tab(f, app, chunks[1]),
        ActiveTab::Cache => draw_cache_dashboard(f, app, chunks[1]),
        ActiveTab::Evidence => draw_evidence_tab(f, app, chunks[1]),
        ActiveTab::Secrets => draw_secrets_tab(f, app, chunks[1]),
    }

    draw_event_console(f, app, chunks[2]);

    if app.command_palette_open {
        draw_command_palette(f, app);
    }
    if app.help_overlay_open {
        draw_help_overlay(f, app);
    }
}

// ---------------------------------------------------------------------------
// Header + Tab bar (2 rows merged into 1 widget)
// ---------------------------------------------------------------------------

fn draw_header_tabs(f: &mut Frame, app: &mut App, area: Rect) {
    let (stale_age, stale_color, stale_label) = stale_indicator(app);

    let gitlab_span = if app.state.gitlab_ready {
        Span::styled("GitLab:OK", Style::default().fg(Color::Green))
    } else {
        Span::styled("GitLab:BOOT", Style::default().fg(Color::Yellow))
    };

    let pools_total = app.state.pools.len();
    let pools_active = app.state.pools.iter().filter(|p| !p.paused).count();

    let release_span = if let Some(ref rel) = app.state.release_status {
        let short_sha = rel.attempt.sha.get(..8).unwrap_or(rel.attempt.sha.as_str());
        Span::styled(
            format!(" rel:{} {}", short_sha, rel.canary_state),
            Style::default()
                .fg(release_color(&rel.canary_state))
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(" rel:none", Style::default().fg(Color::DarkGray))
    };

    let stale_span = if !stale_label.is_empty() {
        Span::styled(
            format!(" {}({}s)", stale_label, stale_age),
            Style::default()
                .fg(stale_color)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let tab_defs: &[(&str, ActiveTab, u8)] = &[
        ("Mission", ActiveTab::Mission, 1),
        ("Release", ActiveTab::Release, 2),
        ("Jobs", ActiveTab::Jobs, 3),
        ("Agents", ActiveTab::Agents, 4),
        ("Tests", ActiveTab::Tests, 5),
        ("Pools", ActiveTab::Pools, 6),
        ("Cache", ActiveTab::Cache, 7),
        ("Evidence", ActiveTab::Evidence, 8),
        ("Secrets", ActiveTab::Secrets, 9),
    ];

    let mut spans: Vec<Span> = vec![
        Span::styled(
            " jeryu ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        gitlab_span,
        Span::styled(
            format!(" ctrs:{}", app.state.active_containers),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            format!(" pools:{}/{}", pools_active, pools_total),
            Style::default().fg(if pools_active == pools_total {
                Color::Green
            } else {
                Color::Yellow
            }),
        ),
        release_span,
        stale_span,
        Span::raw("  "),
    ];

    for (name, tab, n) in tab_defs {
        if app.active_tab == *tab {
            spans.push(Span::styled(
                format!("[{}:{}]", n, name),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {}:{} ", n, name),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    let p = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::BOTTOM))
        .style(Style::default().fg(Color::White));
    f.render_widget(p, area);
}

// ---------------------------------------------------------------------------
// Event console (bottom strip above footer)
// ---------------------------------------------------------------------------

fn draw_event_console(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Events ── Ctrl-K: command palette  /: search  ?: help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build ticker line from recent events (scrolling right-to-left)
    let mut ticker_spans: Vec<Span> = Vec::new();
    let _now = chrono::Utc::now();

    // Collect event entries
    let events: Vec<(&str, &str, Color, &str)> = app
        .state
        .recent_jobs
        .iter()
        .take(20)
        .map(|job| {
            let ts = job.received_at.get(11..19).unwrap_or("--:--:--");
            let (badge, color) = status_badge(&job.status);
            let name = job.job_name.as_deref().unwrap_or("job");
            (ts, badge, color, name)
        })
        .collect();

    if events.is_empty() {
        let p = Paragraph::new(Span::styled(
            "  No events yet. Events appear here as jobs run.",
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(p, inner);
        return;
    }

    // Build a single scrolling line
    for (ts, badge, color, name) in &events {
        ticker_spans.push(Span::styled(
            format!(" {ts} "),
            Style::default().fg(Color::DarkGray),
        ));
        ticker_spans.push(Span::styled(
            format!("[{badge}]"),
            Style::default().fg(*color).add_modifier(Modifier::BOLD),
        ));
        ticker_spans.push(Span::styled(
            format!(" {name}  │"),
            Style::default().fg(Color::White),
        ));
    }

    // Scroll offset drives the horizontal shift
    let offset = (app.state.event_ticker_offset % (events.len() * 30 + 1)) as u16;

    let p = Paragraph::new(Line::from(ticker_spans)).scroll((0, offset));
    f.render_widget(p, inner);
}

// ---------------------------------------------------------------------------
// Footer / key hints
// ---------------------------------------------------------------------------

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let help = if app.maximize_logs {
        " Esc:minimize  ↑↓:scroll  PgUp/Dn:jump  Home:top  G/End:bottom  q:quit"
    } else if app.active_tab == ActiveTab::Jobs {
        " f:freeze  n/N:runner  g:follow  c:cancel  r:retry  d:del  Enter:logs  ?:help  q:quit"
    } else {
        " ^K:palette  Tab:cycle  1-9:tab  ↑↓:select  Enter:inspect  F5:refresh  ?:help  q:quit"
    };
    let p = Paragraph::new(help)
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

// ---------------------------------------------------------------------------
// Tab 1 — Mission: action-first system cockpit
// ---------------------------------------------------------------------------

fn draw_mission_tab(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Min(10),
        ])
        .split(area);
    let headline_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(44), Constraint::Length(42)])
        .split(rows[0]);
    let metric_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(rows[1]);
    let body_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(44),
            Constraint::Percentage(34),
            Constraint::Percentage(22),
        ])
        .split(rows[2]);

    let pool_active = app.state.pools.iter().filter(|p| !p.paused).count();
    let pool_total = app.state.pools.len();
    let running_jobs = app
        .state
        .recent_jobs
        .iter()
        .filter(|j| j.status == "running")
        .count();
    let failed_jobs = app
        .state
        .recent_jobs
        .iter()
        .filter(|j| j.status == "failed")
        .count();
    let blocked_work = failed_jobs
        + usize::from(app.state.active_taint_count > 0)
        + usize::from(
            app.state
                .release_status
                .as_ref()
                .is_some_and(|rel| !matches!(rel.canary_state.as_str(), "green" | "released")),
        );
    let release_ready = app
        .state
        .release_status
        .as_ref()
        .is_some_and(|rel| matches!(rel.canary_state.as_str(), "green" | "released"));
    let release_progress = app
        .state
        .release_status
        .as_ref()
        .map(|rel| match rel.canary_state.as_str() {
            "released" => 100,
            "green" => 92,
            "in-flight" | "canary-authorized" => 70,
            "ready-for-canary" => 55,
            "waiting" => 35,
            "blocked" | "blocked-by-upstream" => 25,
            "failed" => 10,
            _ => 20,
        })
        .unwrap_or(0);
    let cache_trust = if app.state.active_taint_count == 0 {
        100
    } else {
        35
    };
    let autonomy_score = 100u16
        .saturating_sub((blocked_work as u16).saturating_mul(18))
        .saturating_sub(if !app.state.gitlab_ready { 22 } else { 0 })
        .saturating_sub(if app.state.proxy_healthy { 0 } else { 8 })
        .min(100);
    let (headline, headline_color, next_action) = top_attention(app);
    let (_, stale_color, stale_label) = stale_indicator(app);

    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "  TOP SIGNAL  ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(headline_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", short_text(&headline, 84)),
                    Style::default()
                        .fg(headline_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Next action: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    short_text(&next_action, 92),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Freshness: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    if stale_label.is_empty() {
                        "fresh"
                    } else {
                        stale_label
                    },
                    Style::default().fg(stale_color),
                ),
                Span::styled("   Command: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "^K actions  Enter inspect  3 flow  4 agents  8 evidence",
                    Style::default().fg(Color::Cyan),
                ),
            ]),
        ])
        .block(
            Block::default()
                .title(" [ Mission Control ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(headline_color)),
        ),
        headline_cols[0],
    );

    f.render_widget(
        Paragraph::new(vec![
            readiness_line(
                "GitLab",
                if app.state.gitlab_ready {
                    "PASS online"
                } else {
                    "WAIT booting"
                },
                if app.state.gitlab_ready {
                    Color::Green
                } else {
                    Color::Yellow
                },
            ),
            readiness_line(
                "Runners",
                &format!("{pool_active}/{pool_total} active"),
                if pool_active == pool_total {
                    Color::Green
                } else {
                    Color::Yellow
                },
            ),
            readiness_line(
                "Gateway",
                &format!(
                    "proxy:{} registry:{}",
                    if app.state.proxy_healthy {
                        "PASS"
                    } else {
                        "FAIL"
                    },
                    if app.state.registry_healthy {
                        "PASS"
                    } else {
                        "FAIL"
                    }
                ),
                if app.state.proxy_healthy && app.state.registry_healthy {
                    Color::Green
                } else {
                    Color::Red
                },
            ),
            readiness_line(
                "Containers",
                &app.state.active_containers.to_string(),
                Color::Cyan,
            ),
        ])
        .block(
            Block::default()
                .title(" [ Readiness ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        headline_cols[1],
    );

    draw_metric_tile(
        f,
        metric_cols[0],
        "Autonomy",
        &format!("{}%", autonomy_score),
        &meter_bar(autonomy_score, 12),
        if autonomy_score >= 80 {
            Color::Green
        } else if autonomy_score >= 55 {
            Color::Yellow
        } else {
            Color::Red
        },
    );
    draw_metric_tile(
        f,
        metric_cols[1],
        "Active Work",
        &format!("{} jobs", app.state.recent_jobs.len()),
        &format!("{running_jobs} running / {failed_jobs} failed"),
        if failed_jobs > 0 {
            Color::Red
        } else if running_jobs > 0 {
            Color::Cyan
        } else {
            Color::Green
        },
    );
    draw_metric_tile(
        f,
        metric_cols[2],
        "Release",
        if release_ready { "ready" } else { "proofing" },
        &meter_bar(release_progress, 12),
        if release_ready {
            Color::Green
        } else {
            Color::Yellow
        },
    );
    draw_metric_tile(
        f,
        metric_cols[3],
        "Cache Trust",
        &format!("{} taints", app.state.active_taint_count),
        &meter_bar(cache_trust, 12),
        if app.state.active_taint_count > 0 {
            Color::Magenta
        } else {
            Color::Green
        },
    );
    // TUI v2 — Live Runners metric tile
    let feed_count = app.state.runner_feeds.len();
    let feed_running = app
        .state
        .runner_feeds
        .iter()
        .filter(|f| f.status == "running")
        .count();
    let feed_failed = app
        .state
        .runner_feeds
        .iter()
        .filter(|f| f.status == "failed")
        .count();
    draw_metric_tile(
        f,
        metric_cols[4],
        "Live Runners",
        &format!("{} active", feed_count),
        &format!("{feed_running}▶ {feed_failed}✕"),
        if feed_failed > 0 {
            Color::Red
        } else if feed_running > 0 {
            Color::Cyan
        } else {
            Color::DarkGray
        },
    );

    draw_attention_queue(f, app, body_cols[0]);
    draw_proof_lanes(f, app, body_cols[1]);
    draw_action_stack(f, app, body_cols[2]);
}

fn readiness_line(label: &str, value: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {label:<11}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(value.to_string(), Style::default().fg(color)),
    ])
}

fn draw_metric_tile(
    f: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    detail: &str,
    color: Color,
) {
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!("  {value}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("  {detail}"),
                Style::default().fg(Color::White),
            )),
        ])
        .block(
            Block::default()
                .title(format!(" [ {title} ] "))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color)),
        ),
        area,
    );
}

fn draw_attention_queue(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    for job in app
        .state
        .recent_jobs
        .iter()
        .filter(|job| job.status == "failed")
        .take(4)
    {
        lines.push(attention_line(
            "P0",
            Color::Red,
            &format!("Job #{} failed", job.job_id),
            job.job_name.as_deref().unwrap_or("open logs/evidence"),
        ));
    }
    if app.state.active_taint_count > 0 {
        lines.push(attention_line(
            "P0",
            Color::Magenta,
            "Cache taint active",
            "trusted proof reuse blocked",
        ));
    }
    if let Some(rel) = &app.state.release_status
        && !matches!(rel.canary_state.as_str(), "green" | "released")
    {
        lines.push(attention_line(
            "P1",
            release_color(&rel.canary_state),
            &format!("Release {}", rel.canary_state),
            &rel.eligibility,
        ));
    }
    for job in app
        .state
        .recent_jobs
        .iter()
        .filter(|job| job.status == "running")
        .take(3)
    {
        lines.push(attention_line(
            "P2",
            Color::Cyan,
            &format!("Job #{} running", job.job_id),
            job.job_name.as_deref().unwrap_or("validation"),
        ));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No urgent blockers. Start with VTI planning or inspect latest release.",
            Style::default().fg(Color::Green),
        )));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ Attention Queue ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

fn attention_line(priority: &str, color: Color, title: &str, detail: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {priority:<3} "),
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<28}", short_text(title, 28)),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(short_text(detail, 44), Style::default().fg(Color::White)),
    ])
}

fn draw_proof_lanes(f: &mut Frame, app: &App, area: Rect) {
    let release_state = app
        .state
        .release_status
        .as_ref()
        .map(|rel| rel.canary_state.as_str())
        .unwrap_or("none");
    let lanes = vec![
        (
            "Capability grants",
            if app
                .state
                .recent_audit_events
                .iter()
                .any(|ev| ev.event_type.contains("capability"))
            {
                "observed"
            } else {
                "quiet"
            },
        ),
        (
            "VTI receipts",
            if app
                .state
                .recent_audit_events
                .iter()
                .any(|ev| ev.event_type.contains("vti"))
            {
                "observed"
            } else {
                "needed"
            },
        ),
        (
            "Merge proof",
            if failed_or_tainted(app) {
                "blocked"
            } else {
                "dry-run"
            },
        ),
        ("Release gate", release_state),
        ("Sandbox", "strict fails closed"),
        (
            "Evidence ledger",
            if app.state.recent_evidence.is_empty() {
                "empty"
            } else {
                "capsules"
            },
        ),
    ];
    let lines: Vec<Line> = lanes
        .into_iter()
        .map(|(lane, state)| {
            let (badge, color) = status_badge(state);
            Line::from(vec![
                Span::styled(
                    format!(" {badge:<5} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{lane:<18}"), Style::default().fg(Color::White)),
                Span::styled(state.to_string(), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ Proof Stack ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_action_stack(f: &mut Frame, app: &App, area: Rect) {
    let jobs_by_state = [
        app.state
            .recent_jobs
            .iter()
            .filter(|j| j.status == "running")
            .count() as i64,
        app.state
            .recent_jobs
            .iter()
            .filter(|j| j.status == "pending" || j.status == "created")
            .count() as i64,
        app.state
            .recent_jobs
            .iter()
            .filter(|j| j.status == "success")
            .count() as i64,
        app.state
            .recent_jobs
            .iter()
            .filter(|j| j.status == "failed")
            .count() as i64,
    ];
    let lines = vec![
        Line::from(vec![
            Span::styled("  CI shape   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                compact_spark(&jobs_by_state, 8),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Agents     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                app.state.agent_pipelines.len().to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Evidence   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                app.state.recent_evidence.len().to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Recommended",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ^K explain blockers",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  3 open flow board",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  4 inspect agents",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  8 open evidence",
            Style::default().fg(Color::White),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ Next Actions ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

fn failed_or_tainted(app: &App) -> bool {
    app.state.active_taint_count > 0
        || app
            .state
            .recent_jobs
            .iter()
            .any(|job| job.status == "failed")
}

// ---------------------------------------------------------------------------
// Tab 2 — Release: full gate matrix
// ---------------------------------------------------------------------------

fn draw_release_tab(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(34)])
        .split(area);

    // Center: Release Gate Matrix
    let gate_block = Block::default()
        .title(" [ Release Gate Matrix ] ")
        .borders(Borders::ALL)
        .border_style(
            Style::default().fg(app
                .state
                .release_status
                .as_ref()
                .map(|r| release_color(&r.canary_state))
                .unwrap_or(Color::DarkGray)),
        );
    let gate_inner = gate_block.inner(cols[0]);
    f.render_widget(gate_block, cols[0]);

    if let Some(ref rel) = app.state.release_status {
        let attempt = &rel.attempt;

        let gate_rows: Vec<(&str, &str, &str, &str)> = vec![
            (
                "Main branch green",
                if attempt.upstream_status == "success" {
                    "[OK]"
                } else {
                    "[WAIT]"
                },
                attempt.upstream_status.as_str(),
                "",
            ),
            (
                "Release identity",
                if rel.release_identity_ok {
                    "[OK]"
                } else {
                    "[WAIT]"
                },
                if rel.release_identity_ok {
                    "verified"
                } else {
                    "pending"
                },
                "",
            ),
            (
                "Release pipeline",
                match attempt.release_pipeline_status.as_deref() {
                    Some("success") => "[OK]",
                    Some("running") => "[RUN]",
                    Some("failed") => "[FAIL]",
                    _ => "[WAIT]",
                },
                attempt
                    .release_pipeline_status
                    .as_deref()
                    .unwrap_or("not-started"),
                &rel.canary_state_path,
            ),
            (
                "Canary health",
                match rel.canary_state.as_str() {
                    "released" => "[OK]",
                    "in-flight" | "canary-authorized" => "[RUN]",
                    "failed" => "[FAIL]",
                    _ => "[WAIT]",
                },
                rel.canary_state.as_str(),
                rel.canary_public_url.as_deref().unwrap_or(""),
            ),
            (
                "E2E gate",
                match attempt.production_pipeline_status.as_deref() {
                    Some("success") => "[OK]",
                    Some("running") => "[RUN]",
                    Some("failed") => "[FAIL]",
                    _ => "[WAIT]",
                },
                "waiting on canary",
                &rel.gate_canary_e2e_path,
            ),
            (
                "Prod promotion",
                match attempt.production_pipeline_status.as_deref() {
                    Some("success") => "[OK]",
                    Some("running") => "[RUN]",
                    _ => "[WAIT]",
                },
                attempt
                    .production_pipeline_status
                    .as_deref()
                    .unwrap_or("not-triggered"),
                "",
            ),
        ];

        let header = Line::from(vec![Span::styled(
            format!(
                "  RELEASE: {}  Phase: {}  ",
                attempt.version, rel.canary_state
            ),
            Style::default()
                .fg(release_color(&rel.canary_state))
                .add_modifier(Modifier::BOLD),
        )]);

        let sep = Line::from(Span::styled(
            format!(
                "  {:-<width$}",
                "",
                width = gate_inner.width.saturating_sub(4) as usize
            ),
            Style::default().fg(Color::DarkGray),
        ));

        let col_header = Line::from(vec![Span::styled(
            format!("  {:<28} {:<7} {:<16} Detail", "Gate", "Status", "State"),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )]);

        let mut lines = vec![header, sep.clone(), col_header, sep.clone()];
        for (gate, badge, state, detail) in &gate_rows {
            let badge_color = match *badge {
                "[OK]" => Color::Green,
                "[RUN]" => Color::Blue,
                "[FAIL]" => Color::Red,
                _ => Color::Yellow,
            };
            let short_detail = short_text(detail, 20);
            lines.push(Line::from(vec![
                Span::raw(format!("  {:<28} ", gate)),
                Span::styled(
                    format!("{:<7} ", badge),
                    Style::default()
                        .fg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<16} ", state), Style::default().fg(Color::White)),
                Span::styled(short_detail, Style::default().fg(Color::DarkGray)),
            ]));
        }

        f.render_widget(Paragraph::new(lines), gate_inner);
    } else {
        f.render_widget(
            Paragraph::new("\n  No release in progress.\n  Waiting for first green main pipeline.")
                .style(Style::default().fg(Color::DarkGray)),
            gate_inner,
        );
    }

    // Right: Inspector shows release note / progress report
    draw_release_inspector(f, app, cols[1]);
}

fn draw_release_inspector(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Inspector ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let content = if let Some(ref rel) = app.state.release_status {
        let attempt = &rel.attempt;
        format!(
            "sha: {}\nref: {}\n\ncanary_url:\n{}\n\nnote:\n{}\n\neligibility:\n{}",
            attempt.sha.get(..12).unwrap_or(&attempt.sha),
            attempt.ref_name,
            rel.canary_public_url.as_deref().unwrap_or("n/a"),
            attempt.canary_note.as_deref().unwrap_or("(none)"),
            rel.eligibility,
        )
    } else {
        "No release attempt.\n\nActions available:\n  n/a".to_string()
    };

    f.render_widget(
        Paragraph::new(content)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

// ---------------------------------------------------------------------------
// Tab 3 — Jobs: flow board + jobs list + log preview
// ---------------------------------------------------------------------------

fn draw_jobs_tab(f: &mut Frame, app: &mut App, area: Rect) {
    // TUI v2 — Split layout: Live Feed (60%) | Progress+Matrix (40%)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Left: Live Runner Feed
            Constraint::Percentage(40), // Right: Progress + Matrix + Inspector
        ])
        .split(area);

    // Left column: Live Runner Feed
    draw_live_runner_feed(f, app, cols[0]);

    // Right column: Pipeline Progress on top, Job Matrix below, Inspector at bottom
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Pipeline progress
            Constraint::Min(8),     // Job matrix
            Constraint::Length(10), // Inspector
        ])
        .split(cols[1]);

    draw_pipeline_progress(f, app, right_rows[0]);
    draw_job_matrix(f, app, right_rows[1]);
    draw_job_inspector_panel(f, app, right_rows[2]);
}

// ---------------------------------------------------------------------------
// TUI v2 — Live Runner Feed
// ---------------------------------------------------------------------------

fn feed_line_color(line: &str) -> Color {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error")
        || lower.contains("failed")
        || lower.contains("fatal")
        || lower.contains("panicked")
    {
        Color::Red
    } else if lower.contains("warning") || lower.contains("warn") {
        Color::Yellow
    } else if lower.contains("compiling")
        || lower.contains("running")
        || lower.contains("downloading")
        || lower.contains("fetching")
    {
        Color::Cyan
    } else if lower.contains("finished")
        || lower.contains("test result: ok")
        || lower.contains("passed")
        || lower.contains("... ok")
    {
        Color::Green
    } else if line.starts_with('[') && line.len() > 10 {
        // Timestamp prefix — dim it
        Color::DarkGray
    } else {
        Color::White
    }
}

fn format_elapsed(secs: f64) -> String {
    let total = secs as u64;
    if total >= 3600 {
        format!("{}h{}m{}s", total / 3600, (total % 3600) / 60, total % 60)
    } else if total >= 60 {
        format!("{}m{}s", total / 60, total % 60)
    } else {
        format!("{}s", total)
    }
}

fn draw_live_runner_feed(f: &mut Frame, app: &App, area: Rect) {
    let feeds = &app.state.runner_feeds;
    let active_idx = app.state.active_feed_index;
    let is_cycling = app.feed_pinned.is_none();

    let cycle_label = if is_cycling {
        "⟳ cycling 5s"
    } else {
        "⏸ pinned"
    };
    let runner_label = if feeds.is_empty() {
        "no runners".to_string()
    } else {
        format!("runner {}/{}", active_idx + 1, feeds.len())
    };

    let block = Block::default()
        .title(format!(
            " Live Runner Feed ── {} ── {} ",
            cycle_label, runner_label
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if feeds.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No active runners. Waiting for CI jobs...",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Tip: Start a pipeline to see live logs here.",
                    Style::default().fg(Color::DarkGray),
                )),
            ]),
            inner,
        );
        return;
    }

    let feed = &feeds[active_idx.min(feeds.len().saturating_sub(1))];

    // Split into header (2 lines) + logs area + indicator strip (1 line)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Runner header
            Constraint::Min(4),    // Log content
            Constraint::Length(1), // Runner indicator strip
        ])
        .split(inner);

    // Runner header
    let feed_color = status_color(&feed.status);
    let header_spans = vec![
        Span::styled(
            format!(" {} ", &feed.runner_name),
            Style::default()
                .fg(Color::Black)
                .bg(feed_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" │ {} ", &feed.job_name),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("│ ⏱ {} ", format_elapsed(feed.elapsed_secs)),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            format!("│ job #{}", feed.job_id),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    f.render_widget(Paragraph::new(Line::from(header_spans)), rows[0]);

    // Log content with color coding
    let log_lines: Vec<Line> = feed
        .log_tail
        .lines()
        .map(|line| {
            let color = feed_line_color(line);
            let style = if color == Color::Red || color == Color::Green {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect();

    let total_lines = log_lines.len() as u16;
    let visible_height = rows[1].height;
    let scroll_offset = if app.feed_follow_tail {
        total_lines.saturating_sub(visible_height)
    } else {
        app.feed_scroll_offset
            .min(total_lines.saturating_sub(visible_height))
    };

    f.render_widget(
        Paragraph::new(log_lines).scroll((scroll_offset, 0)),
        rows[1],
    );

    // Runner indicator strip
    let mut indicator_spans: Vec<Span> = vec![Span::raw(" ")];
    for (i, f_entry) in feeds.iter().enumerate() {
        let is_active = i == active_idx;
        let dot_color = status_color(&f_entry.status);
        let dot = if f_entry.status == "running" || f_entry.status == "pending" {
            "●"
        } else if f_entry.status == "failed" {
            "✕"
        } else {
            "○"
        };
        let name = short_text(&f_entry.runner_name, 12);
        if is_active {
            indicator_spans.push(Span::styled(
                format!("{dot} {name} "),
                Style::default()
                    .fg(dot_color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        } else {
            indicator_spans.push(Span::styled(
                format!("{dot} {name} "),
                Style::default().fg(dot_color),
            ));
        }
        indicator_spans.push(Span::styled(" ", Style::default()));
    }
    f.render_widget(Paragraph::new(Line::from(indicator_spans)), rows[2]);
}

// ---------------------------------------------------------------------------
// TUI v2 — Pipeline Progress
// ---------------------------------------------------------------------------

fn draw_pipeline_progress(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Pipeline Progress ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(ref progress) = app.state.pipeline_progress_view else {
        f.render_widget(Paragraph::new("  No active pipeline"), inner);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    // Pipeline header
    lines.push(Line::from(vec![
        Span::styled(
            format!("  #{} ", progress.pipeline_id),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}@{}", progress.ref_name, progress.sha_short),
            Style::default().fg(Color::White),
        ),
    ]));

    // Stage rows
    let tick = app.tick_count;
    for stage in &progress.stages {
        let (icon, icon_color) = match stage.status.as_str() {
            "success" => ("●", Color::Green),
            "running" => {
                // Animated indicator
                if tick % 4 < 2 {
                    ("◉", Color::Cyan)
                } else {
                    ("◎", Color::Cyan)
                }
            }
            "failed" => {
                if tick % 4 < 2 {
                    ("✕", Color::Red)
                } else {
                    ("✕", Color::LightRed)
                }
            }
            _ => ("○", Color::Yellow),
        };

        let bar_width = 16usize;
        let fill = if stage.total_jobs > 0 {
            (stage.completed_jobs * bar_width + stage.total_jobs / 2) / stage.total_jobs
        } else {
            0
        };
        let running_fill = if stage.total_jobs > 0 && stage.running_jobs > 0 {
            1.max((stage.running_jobs * bar_width) / stage.total_jobs)
        } else {
            0
        };
        let bar = format!(
            "{}{}{}",
            "█".repeat(fill),
            "▓".repeat(running_fill.min(bar_width - fill)),
            "░".repeat(bar_width.saturating_sub(fill + running_fill)),
        );

        let count_label = format!(
            "{}/{}",
            stage.completed_jobs + stage.running_jobs,
            stage.total_jobs
        );

        lines.push(Line::from(vec![
            Span::styled(format!("  {icon} "), Style::default().fg(icon_color)),
            Span::styled(
                format!("{:<12}", short_text(&stage.stage_name, 12)),
                Style::default().fg(Color::White),
            ),
            Span::styled(bar, Style::default().fg(icon_color)),
            Span::styled(
                format!(" {:<5}", count_label),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    // Overall progress bar
    lines.push(Line::from(""));
    let overall_bar = meter_bar(progress.overall_pct, 20);
    let eta_label = progress
        .eta_remaining_secs
        .map(|secs| {
            if secs >= 3600 {
                format!(
                    "ETA ~{}h{}m ({})",
                    secs / 3600,
                    (secs % 3600) / 60,
                    progress.eta_confidence
                )
            } else if secs >= 60 {
                format!(
                    "ETA ~{}m{}s ({})",
                    secs / 60,
                    secs % 60,
                    progress.eta_confidence
                )
            } else {
                format!("ETA ~{}s ({})", secs, progress.eta_confidence)
            }
        })
        .unwrap_or_else(|| "ETA unknown".into());

    lines.push(Line::from(vec![
        Span::styled(format!("  {overall_bar}"), Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("  {eta_label}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

// ---------------------------------------------------------------------------
// TUI v2 — Job Matrix
// ---------------------------------------------------------------------------

fn draw_job_matrix(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Job Matrix ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Group jobs by stage/pool
    let mut groups: Vec<(&str, Vec<&crate::state::JobEvent>)> = Vec::new();
    let mut current_stage: Option<&str> = None;

    for job in &app.state.recent_jobs {
        let stage = job.pool_name.as_deref().unwrap_or("default");
        if current_stage != Some(stage) {
            groups.push((stage, vec![job]));
            current_stage = Some(stage);
        } else if let Some(last) = groups.last_mut() {
            last.1.push(job);
        }
    }

    let tick = app.tick_count;
    let mut lines: Vec<Line> = Vec::new();
    for (stage_name, jobs) in groups.iter().take(inner.height as usize) {
        let mut spans: Vec<Span> = vec![Span::styled(
            format!("  {:<14}", short_text(stage_name, 14)),
            Style::default().fg(Color::DarkGray),
        )];
        for job in jobs.iter().take(20) {
            let (dot, color) = match job.status.as_str() {
                "success" => ("●", Color::Green),
                "running" => {
                    if tick % 4 < 2 {
                        ("●", Color::Cyan)
                    } else {
                        ("◌", Color::Cyan)
                    }
                }
                "failed" => {
                    if tick % 6 < 3 {
                        ("●", Color::Red)
                    } else {
                        ("●", Color::LightRed)
                    }
                }
                "pending" | "created" => ("○", Color::Yellow),
                "canceled" => ("○", Color::DarkGray),
                _ => ("○", Color::Gray),
            };
            spans.push(Span::styled(format!("{dot} "), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No jobs tracked",
            Style::default().fg(Color::DarkGray),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

#[allow(dead_code)]
fn draw_pipeline_nav(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .state
        .pipelines
        .iter()
        .enumerate()
        .map(|(i, pm)| {
            let selected = i == app.selected_pipeline_index;
            let color = status_color(&pm.pipeline.status);
            let prefix = if selected { ">" } else { " " };
            let short_ref = short_text(&pm.pipeline.ref_name, 14);
            let line = Line::from(vec![
                Span::styled(
                    format!("{} #{:<6} ", prefix, pm.pipeline.pipeline_id),
                    Style::default().fg(if selected {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(short_ref, Style::default().fg(color)),
            ]);
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Pipelines ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(list, area);
}

fn draw_job_inspector_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Inspector ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pane_border(ActivePane::Jobs, app)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(job) = app.selected_job() else {
        f.render_widget(
            Paragraph::new("\n  Select a job with ↑↓").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    };

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("Job  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("#{}", job.job_id),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Name ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                job.job_name.as_deref().unwrap_or("?"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Status ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &job.status,
                Style::default()
                    .fg(status_color(&job.status))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Pool ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                job.pool_name.as_deref().unwrap_or("-"),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(Span::styled(
            "─────────────────",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    if let Some(ref cap) = app.state.inspector_capsule {
        lines.push(Line::from(Span::styled(
            "Evidence:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled("  exit:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({})", cap.exit_code, cap.failure_kind),
                Style::default().fg(Color::Red),
            ),
        ]));
        // Show first 3 lines of log snippet
        for snippet_line in cap.log_snippet.lines().take(3) {
            lines.push(Line::from(Span::styled(
                format!("  {}", short_text(snippet_line, 28)),
                Style::default().fg(Color::Yellow),
            )));
        }
        lines.push(Line::from(Span::styled(
            "─────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "Actions:",
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::from(Span::styled(
            "  [r] Retry job",
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            "  [d] Delete event",
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            "─────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "Blocked:",
            Style::default().fg(Color::DarkGray),
        )));
        if job.status != "success" {
            lines.push(Line::from(Span::styled(
                "  Promote — not green",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else if job.status == "failed" {
        lines.push(Line::from(Span::styled(
            "  No capsule found",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  (evidence not stored yet)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  No evidence",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  Actions:",
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::from(Span::styled(
            "  [r] Retry  [d] Delete",
            Style::default().fg(Color::White),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

// ---------------------------------------------------------------------------
// Tab 4 — Agents: mission/session cockpit
// ---------------------------------------------------------------------------

fn draw_agents_tab(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(36),
            Constraint::Percentage(39),
            Constraint::Percentage(25),
        ])
        .split(area);

    let items: Vec<ListItem> = app
        .state
        .agent_pipelines
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let selected = i == app.selected_job_index;
            let prefix = if selected { ">>" } else { "  " };
            let short_sha = p.sha.get(..8).unwrap_or(&p.sha);
            let ts = p.updated_at.get(..16).unwrap_or(&p.updated_at);
            let (badge, color) = status_badge(&p.status);
            let phase = agent_phase_for_status(&p.status);
            let line = Line::from(vec![
                Span::styled(
                    format!("{prefix} {badge:<5} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<22} ", short_text(&p.ref_name, 22)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:<9} {} {}", phase, short_sha, ts),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(
                " [ Agent Sessions ({}) ] ",
                app.state.agent_pipelines.len()
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(list, cols[0]);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(13), Constraint::Min(8)])
        .split(cols[1]);

    let detail_block = Block::default()
        .title(" [ Agent Cockpit ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let detail_inner = detail_block.inner(rows[0]);
    f.render_widget(detail_block, rows[0]);

    if let Some(p) = app.state.agent_pipelines.get(app.selected_job_index) {
        let phase = agent_phase_for_status(&p.status);
        let progress = match p.status.as_str() {
            "success" => 100,
            "failed" => 100,
            "running" => 68,
            "pending" | "created" => 20,
            _ => 42,
        };
        let next_action = match p.status.as_str() {
            "failed" => "open evidence capsule or spawn repair",
            "running" => "watch pipeline logs and VTI receipt",
            "success" => "request merge proof dry-run",
            _ => "wait for runner assignment",
        };
        let lines = vec![
            Line::from(vec![
                Span::styled("Goal:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    short_text(&p.ref_name, 46),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Phase:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    phase,
                    Style::default()
                        .fg(status_color(&p.status))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  Progress ", Style::default().fg(Color::DarkGray)),
                Span::styled(meter_bar(progress, 10), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Branch:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&p.ref_name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("SHA:      ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    p.sha.get(..12).unwrap_or(&p.sha),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::styled("Status:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&p.status, Style::default().fg(status_color(&p.status))),
            ]),
            Line::from(vec![
                Span::styled("Pipeline: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("#{} (project #{})", p.pipeline_id, p.project_id),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("Updated:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(&p.updated_at, Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("Next:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(next_action, Style::default().fg(Color::Yellow)),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), detail_inner);
    } else {
        f.render_widget(
            Paragraph::new(
                "\n  No agent sessions yet.\n  Branch names starting with agent/ appear here.",
            )
            .style(Style::default().fg(Color::DarkGray)),
            detail_inner,
        );
    }

    let cap_block = Block::default()
        .title(" [ Agent Timeline ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let cap_inner = cap_block.inner(rows[1]);
    f.render_widget(cap_block, rows[1]);

    let cap_items: Vec<Line> = app
        .state
        .recent_audit_events
        .iter()
        .filter(|ev| {
            ev.event_type.contains("capability")
                || ev.event_type.contains("agent")
                || ev.event_type.contains("propose")
                || ev.event_type.contains("merge")
                || ev.event_type.contains("patch")
        })
        .take(cap_inner.height as usize)
        .map(|ev| {
            let ts = ev.timestamp.get(..16).unwrap_or(&ev.timestamp);
            let (badge, color) = if ev.event_type.contains("grant") {
                ("GRANT", Color::Yellow)
            } else if ev.event_type.contains("merge") {
                ("MERGE", Color::Magenta)
            } else if ev.event_type.contains("capability") {
                ("CAP", Color::Cyan)
            } else {
                ("STEP", Color::Green)
            };
            Line::from(vec![
                Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{badge:<6} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<24} ", short_text(&ev.event_type, 24)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("actor:{}", short_text(&ev.actor, 16)),
                    Style::default().fg(Color::White),
                ),
            ])
        })
        .collect();

    if cap_items.is_empty() {
        f.render_widget(
            Paragraph::new("  No agent/capability timeline events recorded.")
                .style(Style::default().fg(Color::DarkGray)),
            cap_inner,
        );
    } else {
        f.render_widget(Paragraph::new(cap_items), cap_inner);
    }

    draw_agent_actions(f, app, cols[2]);
}

fn agent_phase_for_status(status: &str) -> &'static str {
    match status {
        "success" => "review",
        "failed" => "blocked",
        "running" => "testing",
        "pending" | "created" => "queued",
        "canceled" => "stopped",
        _ => "working",
    }
}

fn draw_agent_actions(f: &mut Frame, app: &App, area: Rect) {
    let selected_status = app
        .state
        .agent_pipelines
        .get(app.selected_job_index)
        .map(|p| p.status.as_str())
        .unwrap_or("idle");
    let lines = vec![
        Line::from(Span::styled(
            "  Authority",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  write branch ", Style::default().fg(Color::DarkGray)),
            Span::styled("grant required", Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("  merge        ", Style::default().fg(Color::DarkGray)),
            Span::styled("proof required", Style::default().fg(Color::Magenta)),
        ]),
        Line::from(vec![
            Span::styled("  sandbox      ", Style::default().fg(Color::DarkGray)),
            Span::styled("strict fail-closed", Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Available actions",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ^K explain blockers",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  ^K fetch capsule",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            if selected_status == "success" {
                "  ^K request merge proof"
            } else {
                "  ^K run validation"
            },
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  ^K revoke grant",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" [ Actions / Grants ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

// ---------------------------------------------------------------------------
// Tab 5 — Tests (existing)
// ---------------------------------------------------------------------------

fn draw_tests_tab(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let (bottlenecks, label) = match app.test_view_mode {
        crate::tui::app::TestViewMode::Average => (&app.state.test_bottlenecks_avg, "Average"),
        crate::tui::app::TestViewMode::Latest => (&app.state.test_bottlenecks_latest, "Latest"),
    };

    let items: Vec<ListItem> = bottlenecks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let color = if i == app.selected_test_index {
                Color::Black
            } else if match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => b.avg_duration_ms > 300_000.0,
                crate::tui::app::TestViewMode::Latest => b.latest_duration_ms > 300_000,
            } {
                Color::Red
            } else if match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => b.avg_duration_ms > 60_000.0,
                crate::tui::app::TestViewMode::Latest => b.latest_duration_ms > 60_000,
            } {
                Color::Yellow
            } else {
                Color::Green
            };

            let bg = if i == app.selected_test_index {
                Color::Cyan
            } else {
                Color::Reset
            };

            let dur_text = match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => {
                    format!("{:.1}s", b.avg_duration_ms / 1000.0)
                }
                crate::tui::app::TestViewMode::Latest => {
                    format!("{:.1}s", b.latest_duration_ms as f64 / 1000.0)
                }
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<10} ", dur_text),
                    Style::default().fg(color).bg(bg),
                ),
                Span::styled(
                    format!("({:02}x) ", b.count),
                    Style::default().fg(Color::DarkGray).bg(bg),
                ),
                Span::styled(
                    b.test_name.clone(),
                    Style::default()
                        .fg(if i == app.selected_test_index {
                            Color::Black
                        } else {
                            Color::White
                        })
                        .bg(bg),
                ),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" [ Bottlenecks ({}) - 'v' to toggle ] ", label)),
    );
    f.render_widget(list, chunks[0]);

    let history_block = Block::default()
        .borders(Borders::ALL)
        .title(" [ History Drill-Down - Enter to load ] ");

    if let Some(hist) = &app.selected_test_history {
        let h_items: Vec<ListItem> = hist
            .iter()
            .map(|h| {
                let color = match h.status.as_str() {
                    "success" | "passed" => Color::Green,
                    "failed" => Color::Red,
                    _ => Color::Yellow,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(
                            "{:<15} ",
                            h.created_at.split('T').next().unwrap_or(&h.created_at)
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(format!("{:<8} ", h.status), Style::default().fg(color)),
                    Span::styled(
                        format!("{:.1}s", h.duration_ms as f64 / 1000.0),
                        Style::default().fg(Color::White),
                    ),
                ]))
            })
            .collect();
        f.render_widget(List::new(h_items).block(history_block), chunks[1]);
    } else {
        f.render_widget(
            Paragraph::new("\n  Select a test and press [Enter] to view execution history.")
                .block(history_block)
                .style(Style::default().fg(Color::DarkGray)),
            chunks[1],
        );
    }
}

// ---------------------------------------------------------------------------
// Tab 6 — Pools
// ---------------------------------------------------------------------------

fn draw_pools_tab(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Left: pools list
    let active = app.active_pane == ActivePane::Pools;
    let items: Vec<ListItem> = app
        .state
        .pools
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let selected = active && i == app.selected_pool_index;
            let prefix = if selected { "> " } else { "  " };
            let state_badge = if p.paused {
                Span::styled("[PAUSED]", Style::default().fg(Color::Yellow))
            } else {
                Span::styled("[ACTIVE]", Style::default().fg(Color::Green))
            };
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                state_badge,
                Span::raw(format!(" {}", p.name)),
            ]);
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(" [ Runner Pools ({}) ] ", app.state.pools.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pane_border(ActivePane::Pools, app))),
    );
    f.render_widget(list, cols[0]);

    // Right: pool detail
    let detail = if let Some(pool) = app.state.pools.get(app.selected_pool_index) {
        format!(
            "\n  Name:      {}\n  Status:    {}\n  Min Warm:  {}\n  Max:       {}\n\n  [p] Toggle pause/resume",
            pool.name,
            if pool.paused { "[PAUSED]" } else { "[ACTIVE]" },
            pool.min_warm,
            pool.max_managers,
        )
    } else {
        "\n  No pool selected.".to_string()
    };

    f.render_widget(
        Paragraph::new(detail)
            .block(
                Block::default()
                    .title(" [ Pool Detail ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false }),
        cols[1],
    );
}

// ---------------------------------------------------------------------------
// Tab 7 — Cache (existing dashboard, preserved)
// ---------------------------------------------------------------------------

fn draw_cache_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let objects_str = format!(
        "\n  Total Cached Objects: {}\n  Hot Cache Bandwidth:  {} MB\n  Exact Hits:  {} / {} ({:.1}%)\n  Misses:      {}\n\n  CAS Disk:    {} MiB\n  Crate Cache: {} MiB",
        app.state.cache_objects_count,
        app.state.hot_cache_usage_bytes / 1024 / 1024,
        app.state.cache_hits,
        app.state.total_requests,
        app.state.hit_ratio,
        app.state.miss_count,
        app.state.cas_disk_bytes / 1024 / 1024,
        app.state.crate_cache_disk_bytes / 1024 / 1024
    );
    f.render_widget(
        Paragraph::new(objects_str).block(
            Block::default()
                .title(" [ Storage Overview ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray)),
        ),
        top_chunks[0],
    );

    let proxy_str = if app.state.proxy_healthy {
        "ONLINE"
    } else {
        "OFFLINE"
    };
    let reg_str = if app.state.registry_healthy {
        "ONLINE"
    } else {
        "OFFLINE"
    };
    let services_str = format!(
        "\n  Singleflight Gateway: {}\n  OCI Mirror:           {}\n  CA Certs Injected:    {}",
        proxy_str, reg_str, app.state.ca_mounted
    );
    f.render_widget(
        Paragraph::new(services_str).block(
            Block::default()
                .title(" [ Gateway Health ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray)),
        ),
        top_chunks[1],
    );

    let sf_str = format!(
        "\n  Coalesced Fetches: {}\n  Est. Bandwidth Saved: ~{} MB\n\n  Eliminating redundant crate downloads\n  across parallel runners automatically.",
        app.state.singleflight_requests,
        app.state.singleflight_requests * 5
    );
    f.render_widget(
        Paragraph::new(sf_str).block(
            Block::default()
                .title(" [ Singleflight Analytics ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        ),
        bottom_chunks[0],
    );

    let taint_str = format!(
        "\n  Active Taint Rules:        {}\n  Detonation Lane Breaches:  {}\n  Cold Execution Downgrades: {}\n\n  {}",
        app.state.active_taint_count,
        app.state.detonation_breaches,
        app.state.cold_execution_downgrades,
        if app.state.active_taint_count == 0 && app.state.detonation_breaches == 0 {
            "System executing hermetically."
        } else {
            "[RISK] Taint rules active — cache quarantined."
        }
    );
    f.render_widget(
        Paragraph::new(taint_str).block(
            Block::default()
                .title(" [ Trust & Taint Boundaries ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if app.state.active_taint_count > 0 {
                    Color::Magenta
                } else {
                    Color::LightRed
                })),
        ),
        bottom_chunks[1],
    );
}

// ---------------------------------------------------------------------------
// Tab 8 — Evidence: failure capsule viewer
// ---------------------------------------------------------------------------

fn draw_evidence_tab(f: &mut Frame, app: &App, area: Rect) {
    use crate::tui::app::EvidenceViewMode;
    if app.evidence_view_mode == EvidenceViewMode::AuditLedger {
        draw_audit_ledger(f, app, area);
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // Left: evidence record list
    let items: Vec<ListItem> = app
        .state
        .recent_evidence
        .iter()
        .enumerate()
        .map(|(i, rec)| {
            let selected = i == app.selected_evidence_index;
            let prefix = if selected { "> " } else { "  " };
            let ts = rec.created_at.get(..16).unwrap_or(&rec.created_at);
            let kind_color = match rec.failure_kind.as_str() {
                "compile_failure" => Color::Red,
                "test_failure" => Color::LightRed,
                "timeout" => Color::Yellow,
                "network" => Color::Cyan,
                "quarantined" => Color::Magenta,
                _ => Color::Gray,
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{}{} ", prefix, ts),
                    Style::default().fg(if selected {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(
                    format!("job#{:<6} ", rec.job_id),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    short_text(&rec.failure_kind, 14),
                    Style::default().fg(kind_color),
                ),
            ]);
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(
                " [ Evidence Capsules ({}) — 'a': audit ledger ] ",
                app.state.recent_evidence.len()
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(list, cols[0]);

    // Right: capsule detail
    let detail_block = Block::default()
        .title(" [ Capsule Detail ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let detail_inner = detail_block.inner(cols[1]);
    f.render_widget(detail_block, cols[1]);

    if let Some(rec) = app.state.recent_evidence.get(app.selected_evidence_index) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("job:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("#{}", rec.job_id), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("ref:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(&rec.ref_name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("sha:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    rec.commit_sha.get(..12).unwrap_or(&rec.commit_sha),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::styled("stage:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&rec.stage, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("exit:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", rec.exit_code),
                    Style::default().fg(Color::Red),
                ),
            ]),
            Line::from(vec![
                Span::styled("kind:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(&rec.failure_kind, Style::default().fg(Color::LightRed)),
            ]),
            Line::from(Span::styled(
                "─────────────────",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        // Show causal link if available
        if let Ok(cap) = serde_json::from_str::<crate::capsule::FailureCapsule>(&rec.payload) {
            if let Some(sup) = &cap.superseded_by_sha {
                let sup_short = sup.get(..12).unwrap_or(sup).to_string();
                lines.push(Line::from(vec![
                    Span::styled("superseded: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(sup_short, Style::default().fg(Color::Yellow)),
                ]));
            }
            if let Some(retry_id) = cap.retried_from_job_id {
                lines.push(Line::from(vec![
                    Span::styled("retry_of:   ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("job#{}", retry_id),
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
            }
            lines.push(Line::from(Span::styled(
                "Log snippet:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            let snippet_width = (detail_inner.width as usize).saturating_sub(4);
            for snippet_line in cap.log_snippet.lines().take(6) {
                lines.push(Line::from(Span::styled(
                    format!("  {}", short_text(snippet_line, snippet_width)),
                    Style::default().fg(Color::White),
                )));
            }
        }

        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            detail_inner,
        );
    } else {
        f.render_widget(
            Paragraph::new("\n  No evidence records.\n  Capsules appear here when jobs fail.")
                .style(Style::default().fg(Color::DarkGray)),
            detail_inner,
        );
    }
}

// ---------------------------------------------------------------------------
// Audit ledger view (Evidence tab alternate mode)
// ---------------------------------------------------------------------------

fn draw_audit_ledger(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Audit Ledger — 'a': capsule view ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let tag_color = |ev_type: &str| -> Color {
        if ev_type.contains("cache") {
            Color::Blue
        } else if ev_type.contains("release") {
            Color::Magenta
        } else if ev_type.contains("secret") {
            Color::Yellow
        } else if ev_type.contains("agent") || ev_type.contains("capability") {
            Color::Cyan
        } else if ev_type.contains("job") {
            Color::Green
        } else {
            Color::Gray
        }
    };

    let items: Vec<Line> = app
        .state
        .recent_audit_events
        .iter()
        .take(inner.height as usize)
        .map(|ev| {
            let ts = ev.timestamp.get(..16).unwrap_or(&ev.timestamp);
            let tag = if ev.event_type.contains("cache") {
                "[CACHE]  "
            } else if ev.event_type.contains("release") {
                "[RELEASE]"
            } else if ev.event_type.contains("secret") {
                "[SECRET] "
            } else if ev.event_type.contains("agent") {
                "[AGENT]  "
            } else if ev.event_type.contains("job") {
                "[JOB]    "
            } else {
                "[EVENT]  "
            };
            let job_str = ev
                .job_id
                .map(|id| format!("job#{} ", id))
                .unwrap_or_default();
            Line::from(vec![
                Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} ", tag),
                    Style::default().fg(tag_color(&ev.event_type)),
                ),
                Span::styled(
                    format!("{:<20} ", short_text(&ev.event_type, 20)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{}{}", job_str, short_text(&ev.actor, 14)),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();

    if items.is_empty() {
        f.render_widget(
            Paragraph::new("\n  No audit events recorded yet.")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
    } else {
        f.render_widget(Paragraph::new(items), inner);
    }
}

// ---------------------------------------------------------------------------
// Tab 9 — Secrets
// ---------------------------------------------------------------------------

fn draw_secrets_tab(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let items: Vec<ListItem> = app
        .state
        .secret_audit_events
        .iter()
        .map(|ev| {
            let ts = ev.created_at.get(..16).unwrap_or(&ev.created_at);
            let status_color = match ev.status.as_str() {
                "ok" | "success" => Color::Green,
                "error" | "failed" => Color::Red,
                _ => Color::Yellow,
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:<8} ", ev.action),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:<8} ", ev.status),
                    Style::default().fg(status_color),
                ),
                Span::styled(&ev.repo_name, Style::default().fg(Color::White)),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(
                " [ Secret Audit Events ({}) ] ",
                app.state.secret_audit_events.len()
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(list, cols[0]);

    f.render_widget(
        Paragraph::new("\n  Vault integration active.\n\n  Events appear here as secrets\n  are rotated, fetched, or revoked.\n\n  [RISK] = Security event requiring review.")
            .block(
                Block::default()
                    .title(" [ Vault Status ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        cols[1],
    );
}

// ---------------------------------------------------------------------------
// Shared renderers (preserved from previous version)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn draw_release_banner(f: &mut Frame, app: &App, area: Rect) {
    let body = if let Some(ref release) = app.state.release_status {
        let attempt = &release.attempt;
        format!(
            " Version: {}  State: {} ({})  Upstream: {}  Prod: {} {:?}  Note: {}",
            attempt.version,
            release.canary_state,
            release.eligibility,
            attempt.upstream_status,
            attempt
                .production_pipeline_status
                .as_deref()
                .unwrap_or("not-triggered"),
            attempt.production_pipeline_id,
            attempt.canary_note.as_deref().unwrap_or("(none)")
        )
    } else {
        " No release attempts yet.  Waiting for the first green main pipeline.".to_string()
    };

    let color = if let Some(ref release) = app.state.release_status {
        release_color(&release.canary_state)
    } else {
        Color::DarkGray
    };

    let panel = Paragraph::new(body)
        .block(
            Block::default()
                .title(" [ Release Watch ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(panel, area);
}

#[allow(dead_code)]
fn draw_flow_board(f: &mut Frame, app: &App, area: Rect) {
    let (stale_age, stale_color, _stale_label) = stale_indicator(app);
    let flow_stale = app.state.flow.stale;
    let title = if flow_stale {
        if let Some(last) = app.state.flow.last_non_empty_at {
            let age = chrono::Utc::now()
                .signed_duration_since(last)
                .num_seconds()
                .max(0);
            format!(" FLOW BOARD [stale {}s] ", age)
        } else {
            format!(" FLOW BOARD [stale {}s] ", stale_age)
        }
    } else {
        " FLOW BOARD ".to_string()
    };
    let border_color = if flow_stale {
        stale_color
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if let Some(pipe_flow) = app.state.flow.active_pipelines.first() {
        let selected_job_id = app.selected_job().map(|job| job.job_id);
        let selected_id = selected_job_id.and_then(|job_id| {
            pipe_flow
                .graph
                .nodes
                .iter()
                .find(|node| node.job_id == Some(job_id))
                .map(|node| node.id)
        });
        let widget = crate::tui::flow::FlowGraphWidget::new(&pipe_flow.graph, selected_id);
        f.render_widget(widget, inner_area);
    } else {
        let msg = if let Some(last) = app.state.flow.last_non_empty_at {
            let age = chrono::Utc::now()
                .signed_duration_since(last)
                .num_seconds()
                .max(0);
            format!("No active pipelines  (last seen {}s ago)", age)
        } else {
            "Waiting for active pipelines...".to_string()
        };
        let p = Paragraph::new(msg).block(Block::default());
        f.render_widget(p, inner_area);
    }
}

#[allow(dead_code)]
fn draw_jobs(f: &mut Frame, app: &App, area: Rect) {
    let active = app.active_pane == ActivePane::Jobs;
    let now = chrono::Utc::now();
    let items: Vec<ListItem> = app
        .state
        .recent_jobs
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let selected = active && i == app.selected_job_index;
            let color = status_color(&j.status);
            let icon = match j.status.as_str() {
                "success" => "OK",
                "running" => "RUN",
                "failed" => "FAIL",
                "pending" | "created" => "WAIT",
                "canceled" => "STOP",
                _ => "JOB",
            };

            let prefix = if selected { "> " } else { "  " };
            let name = j.job_name.as_deref().unwrap_or("unknown_job");

            let (pct, pct_color) = match j.status.as_str() {
                "success" => (100u16, Color::Green),
                "failed" | "canceled" => {
                    let run_secs = j.queued_duration.unwrap_or(0.0) as u64;
                    let p = if run_secs > 0 {
                        ((run_secs as f64 / 120.0) * 100.0).min(99.0) as u16
                    } else {
                        0
                    };
                    (
                        p,
                        if j.status == "failed" {
                            Color::Red
                        } else {
                            Color::DarkGray
                        },
                    )
                }
                "running" => {
                    let elapsed =
                        if let Ok(st) = chrono::DateTime::parse_from_rfc3339(&j.received_at) {
                            now.signed_duration_since(st).num_seconds()
                        } else {
                            0
                        };
                    let p = ((elapsed as f64 / 120.0) * 100.0).min(99.0) as u16;
                    (p, Color::Cyan)
                }
                _ => (0, Color::DarkGray),
            };

            let elapsed = chrono::DateTime::parse_from_rfc3339(&j.received_at)
                .ok()
                .map(|st| {
                    chrono::Utc::now()
                        .signed_duration_since(st)
                        .num_seconds()
                        .max(0)
                })
                .unwrap_or_default();
            let pipeline = j
                .pipeline_id
                .map(|id| format!("#{}", id))
                .unwrap_or_else(|| "#?".to_string());

            let content = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{:<4} ", icon),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:>3}% ", pct), Style::default().fg(pct_color)),
                Span::styled(
                    format!("{:<6} ", format_duration(elapsed)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<6} ", pipeline),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(name.to_string(), Style::default().fg(Color::White)),
            ]);

            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(
                " [{}] Live Jobs ({}) ",
                if active { "*" } else { " " },
                app.state.recent_jobs.len()
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pane_border(ActivePane::Jobs, app))),
    );
    f.render_widget(list, area);
}

fn draw_logs(f: &mut Frame, app: &mut App, area: Rect) {
    let job_name = if let Some(j) = app.selected_job() {
        j.job_name
            .clone()
            .unwrap_or_else(|| format!("Job #{}", j.job_id))
    } else {
        "None".to_string()
    };
    let log_state = &app.state.live_log;
    let title_state = if let Some(error) = &log_state.error {
        format!("stale: {}", short_text(error, 48))
    } else if log_state.stale {
        "stale".to_string()
    } else if log_state.target.is_some() {
        "live".to_string()
    } else {
        "idle".to_string()
    };
    let follow_state = if app.follow_log_tail {
        "follow"
    } else {
        "manual"
    };

    let outer_block = Block::default()
        .title(format!(
            " Log: {} [{} | {}] ",
            job_name, title_state, follow_state
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.maximize_logs {
            Color::Cyan
        } else if log_state.stale || log_state.error.is_some() {
            Color::Yellow
        } else {
            pane_border(ActivePane::Jobs, app)
        }));

    let inner_area = outer_block.inner(area);
    f.render_widget(outer_block, area);

    let (gauge_area, log_area) = if app.state.recent_jobs.is_empty() {
        (None, inner_area)
    } else {
        let rc = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(2)])
            .split(inner_area);
        (Some(rc[0]), rc[1])
    };

    if let Some(j) = app.selected_job()
        && let Some(g_area) = gauge_area
    {
        let mut elapsed = 0;
        if let Ok(st) = chrono::DateTime::parse_from_rfc3339(&j.received_at) {
            elapsed = chrono::Utc::now().signed_duration_since(st).num_seconds();
        }
        let pct = match j.status.as_str() {
            "success" | "failed" | "canceled" => 100,
            _ => {
                let mut p = (elapsed as f64 / 120.0 * 100.0) as u16;
                if p > 99 {
                    p = 99;
                }
                p
            }
        };
        let eta_str = if pct == 100 {
            "Done".to_string()
        } else {
            let eta = 120 - elapsed;
            if eta < 0 {
                "Finishing...".to_string()
            } else {
                format!("{}s", eta)
            }
        };

        let color = match j.status.as_str() {
            "failed" => Color::Red,
            "success" => Color::Green,
            _ => Color::Cyan,
        };

        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
            .percent(pct)
            .label(format!("{}% ({})", pct, eta_str));
        f.render_widget(gauge, g_area);
    }

    let parsed_text = if !log_state.text.is_empty() {
        render_log_text(&log_state.text)
    } else if app.active_pane == ActivePane::Jobs {
        Text::raw("Select a running, failed, or recent job. Fetching...")
    } else {
        Text::raw("Focus Jobs pane to tail logs...")
    };

    let total_lines = parsed_text.lines.len() as u16;
    let view_height = log_area.height;
    let max_scroll = total_lines.saturating_sub(view_height);
    if app.follow_log_tail || app.log_scroll_offset > max_scroll {
        app.log_scroll_offset = max_scroll;
    }

    let p = Paragraph::new(parsed_text)
        .wrap(Wrap { trim: false })
        .scroll((app.log_scroll_offset, 0));
    f.render_widget(p, log_area);
}

// ---------------------------------------------------------------------------
// Log text rendering helpers
// ---------------------------------------------------------------------------

fn redact_log_line(line: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let mut result = Cow::Borrowed(line);
    let patterns: &[(&str, &str)] = &[("glpat-", "glpat-[REDACTED]"), ("hvs.", "hvs.[REDACTED]")];
    for (prefix, replacement) in patterns {
        if let Some(start) = result.find(prefix) {
            let s = result.into_owned();
            let end = s[start + prefix.len()..]
                .find(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                .map(|i| start + prefix.len() + i)
                .unwrap_or(s.len());
            result = Cow::Owned(format!("{}{}{}", &s[..start], replacement, &s[end..]));
        }
    }
    // Redact URL credentials: ://user:token@
    if result.contains("://") && result.contains('@') {
        let s = result.into_owned();
        let redacted = regex_redact_url_creds(&s);
        result = Cow::Owned(redacted);
    }
    result
}

fn regex_redact_url_creds(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("://") {
        let after = &rest[pos + 3..];
        out.push_str(&rest[..pos + 3]);
        if let Some(at_pos) = after.find('@') {
            if let Some(colon_pos) = after[..at_pos].find(':') {
                out.push_str(&after[..colon_pos + 1]);
                out.push_str("[REDACTED]");
                rest = &after[at_pos..];
            } else {
                out.push_str(&after[..at_pos]);
                rest = &after[at_pos..];
            }
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn render_log_text(log: &str) -> Text<'static> {
    if log.contains('\x1b') {
        use ansi_to_tui::IntoText;
        if let Ok(text) = log.into_text() {
            let redacted_lines: Vec<Line<'static>> = text
                .lines
                .into_iter()
                .map(|line| {
                    let raw: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    let redacted = redact_log_line(&raw);
                    if redacted.as_ref() != raw.as_str() {
                        Line::from(Span::raw(redacted.into_owned()))
                    } else {
                        Line::from(
                            line.spans
                                .into_iter()
                                .map(|s| Span::styled(s.content.into_owned(), s.style))
                                .collect::<Vec<_>>(),
                        )
                    }
                })
                .collect();
            return Text::from(redacted_lines);
        }
    }
    highlight_plain_log(log)
}

fn highlight_plain_log(log: &str) -> Text<'static> {
    let lines = log
        .lines()
        .map(|line| {
            let line = redact_log_line(line).into_owned();
            let line = line.as_str();
            let lower = line.to_lowercase();
            let style = if lower.contains("error")
                || lower.contains("failed")
                || lower.contains("panic")
                || lower.contains("fatal")
            {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else if lower.contains("warning") || lower.contains("warn") {
                Style::default().fg(Color::Yellow)
            } else if lower.contains("success")
                || lower.contains("passed")
                || lower.ends_with(" ok")
                || lower.contains(" finished ")
            {
                Style::default().fg(Color::Green)
            } else if lower.starts_with('$')
                || lower.starts_with('+')
                || lower.contains("cargo ")
                || lower.contains("docker ")
            {
                Style::default().fg(Color::Cyan)
            } else if lower.starts_with('[') || lower.contains("t00:") {
                Style::default().fg(Color::Gray)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect::<Vec<_>>();
    Text::from(lines)
}

// ---------------------------------------------------------------------------
// String utilities
// ---------------------------------------------------------------------------

fn short_text(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let text = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{}…", text)
    } else {
        text
    }
}

#[allow(dead_code)]
fn format_duration(secs: i64) -> String {
    if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

// ---------------------------------------------------------------------------
// Command Palette overlay (Ctrl-K)
// ---------------------------------------------------------------------------

fn draw_command_palette(f: &mut Frame, app: &App) {
    use crate::tui::action_registry;

    let screen = f.area();
    let modal_w = (screen.width as f32 * 0.60) as u16;
    let modal_h = (screen.height as f32 * 0.60) as u16;
    let modal_x = (screen.width.saturating_sub(modal_w)) / 2;
    let modal_y = (screen.height.saturating_sub(modal_h)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_w, modal_h);

    // Clear the area
    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" Command Palette — type to filter, ↑↓ navigate, Enter execute, Esc close ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    // Split: input line at top, action list + preview below
    let splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(2)])
        .split(inner);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(splits[1]);

    // Input row
    let input_line = Line::from(vec![
        Span::styled(
            "> ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}_", app.command_palette_query),
            Style::default().fg(Color::White),
        ),
    ]);
    let input_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));
    let input_inner = input_block.inner(splits[0]);
    f.render_widget(input_block, splits[0]);
    f.render_widget(Paragraph::new(input_line), input_inner);

    // Filtered action list
    let matches: Vec<&action_registry::ActionEntry> =
        action_registry::filtered(&app.command_palette_query).collect();

    if matches.is_empty() {
        f.render_widget(
            Paragraph::new("  No matching actions.").style(Style::default().fg(Color::DarkGray)),
            body[0],
        );
        return;
    }

    let items: Vec<ListItem> = matches
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = i == app.selected_palette_index;
            let bg = if selected {
                Color::DarkGray
            } else {
                Color::Reset
            };
            let risk_color = entry.risk_tier.color();
            let key_hint = entry
                .key_hint
                .map(|k| format!(" [{k}]"))
                .unwrap_or_default();
            let line = Line::from(vec![
                Span::styled(
                    format!(" {:<28}", entry.label),
                    Style::default().fg(Color::White).bg(bg),
                ),
                Span::styled(
                    format!("{:<6}", entry.risk_tier.label()),
                    Style::default().fg(risk_color).bg(bg),
                ),
                Span::styled(
                    format!("{:<6}", key_hint),
                    Style::default().fg(Color::DarkGray).bg(bg),
                ),
                Span::styled(
                    format!(
                        "  {}",
                        short_text(
                            entry.description,
                            (body[0].width as usize).saturating_sub(46)
                        )
                    ),
                    Style::default().fg(Color::DarkGray).bg(bg),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default())
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_widget(list, body[0]);

    // Column header
    let header = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {:<28}{:<6}{:<6}  Description", "Action", "Risk", "Key"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )]));
    // Render header over the top of the action list.
    if body[0].height > 2 {
        let header_area = Rect::new(body[0].x, body[0].y, body[0].width, 1);
        f.render_widget(header, header_area);
    }

    let selected = matches
        .get(app.selected_palette_index)
        .copied()
        .unwrap_or(matches[0]);
    draw_action_preview(f, app, selected, body[1]);
}

fn draw_action_preview(
    f: &mut Frame,
    app: &App,
    entry: &crate::tui::action_registry::ActionEntry,
    area: Rect,
) {
    let enabled_reason = action_enabled_reason(app, entry.id);
    let enabled = enabled_reason.is_none();
    let risk = entry.risk_tier.label();
    let risk_color = entry.risk_tier.color();
    let side_effect = entry.side_effect_class().label();
    let grant = entry.required_grant().label();
    let lines = vec![
        Line::from(Span::styled(
            entry.label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Risk:        ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                risk,
                Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Side effect: ", Style::default().fg(Color::DarkGray)),
            Span::styled(side_effect, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Grant:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                grant,
                Style::default().fg(if grant == "none" {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Dry run:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if entry.dry_run {
                    "available"
                } else {
                    "not declared"
                },
                Style::default().fg(if entry.dry_run {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Status:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if enabled { "enabled" } else { "disabled" },
                Style::default()
                    .fg(if enabled { Color::Green } else { Color::Red })
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "What will happen",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            short_text(entry.description, area.width.saturating_sub(4) as usize),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            enabled_reason.unwrap_or_else(|| {
                "Ready. Press Enter to execute or preview via the matching CLI/API surface."
                    .to_string()
            }),
            Style::default().fg(if enabled { Color::Green } else { Color::Yellow }),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" [ Preview / Blast Radius ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(risk_color)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn action_enabled_reason(app: &App, action_id: &str) -> Option<String> {
    match action_id {
        "retry_job" => {
            let Some(job) = app.selected_job() else {
                return Some("Select a failed or canceled job first.".to_string());
            };
            if matches!(job.status.as_str(), "failed" | "canceled") {
                None
            } else {
                Some(format!("Selected job status is '{}', not failed/canceled.", job.status))
            }
        }
        "delete_record" | "open_logs" => app
            .selected_job()
            .map(|_| None)
            .unwrap_or_else(|| Some("Select a job first.".to_string())),
        "pause_pool" => app
            .state
            .pools
            .get(app.selected_pool_index)
            .map(|_| None)
            .unwrap_or_else(|| Some("Select a runner pool first.".to_string())),
        "request_merge" => Some("Merge proof must be requested through the evidence-bound API; green UI state is intentionally not inferred.".to_string()),
        "propose_patch" | "race_patches" | "run_tests" => Some(
            "Requires a scoped capability grant and request envelope before side effects."
                .to_string(),
        ),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// TUI v2 — Help overlay
// ---------------------------------------------------------------------------

fn draw_help_overlay(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup_w = 60u16.min(area.width.saturating_sub(4));
    let popup_h = 22u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup = Rect::new(x, y, popup_w, popup_h);

    f.render_widget(Clear, popup);

    let tab_name = match app.active_tab {
        ActiveTab::Mission => "Mission",
        ActiveTab::Release => "Release",
        ActiveTab::Jobs => "Jobs",
        ActiveTab::Agents => "Agents",
        ActiveTab::Tests => "Tests",
        ActiveTab::Pools => "Pools",
        ActiveTab::Cache => "Cache",
        ActiveTab::Evidence => "Evidence",
        ActiveTab::Secrets => "Secrets",
    };

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!(" Keyboard Shortcuts — {tab_name} Tab"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        help_row("1-9", "Switch to tab N"),
        help_row("Tab", "Cycle to next tab"),
        help_row("Ctrl-K", "Open command palette"),
        help_row("?", "Toggle this help overlay"),
        help_row("F5", "Force refresh all data"),
        help_row("q / Esc", "Quit TUI"),
        Line::from(""),
    ];

    // Tab-specific bindings
    match app.active_tab {
        ActiveTab::Jobs => {
            lines.push(Line::from(Span::styled(
                " ── Runner Feed ──",
                Style::default().fg(Color::Cyan),
            )));
            lines.push(help_row("f", "Freeze/unfreeze auto-cycle"));
            lines.push(help_row("n", "Next runner"));
            lines.push(help_row("N", "Previous runner"));
            lines.push(help_row("g", "Toggle follow-tail mode"));
            lines.push(help_row("Enter", "Open full-screen log view"));
            lines.push(help_row("c", "Cancel selected job"));
            lines.push(help_row("r", "Retry failed job"));
            lines.push(help_row("d", "Delete job record"));
        }
        ActiveTab::Tests => {
            lines.push(help_row("v / t", "Toggle average/latest view"));
            lines.push(help_row("Enter", "Show test history"));
            lines.push(help_row("↑↓", "Select test"));
        }
        ActiveTab::Pools => {
            lines.push(help_row("p", "Pause/resume selected pool"));
        }
        ActiveTab::Evidence => {
            lines.push(help_row("a", "Toggle capsules/audit ledger"));
        }
        _ => {
            lines.push(help_row("↑↓", "Navigate items"));
            lines.push(help_row("Enter", "Inspect selected item"));
        }
    }

    let block = Block::default()
        .title(" [ Help ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn help_row(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<12}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc.to_string(), Style::default().fg(Color::White)),
    ])
}
