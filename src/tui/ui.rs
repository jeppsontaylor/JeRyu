//! Owner: Interactive TUI subsystem — rendering logic
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Rendering redacts sensitive text and does not perform control-plane mutations directly.
#[path = "ui_panels.rs"]
mod ui_panels;
use super::app::{ActivePane, ActiveTab, App};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
};
use ui_panels::*;

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
            "Open evidence capsule or revisit after blocker explanation".to_string(),
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

/// Returns (outdated_age_secs, outdated_color, outdated_label) based on last_sync_at.
fn outdated_indicator(app: &App) -> (i64, Color, &'static str) {
    let age = app
        .state
        .last_sync_at
        .map(|t| chrono::Utc::now().signed_duration_since(t).num_seconds())
        .unwrap_or(0);
    if age < 5 {
        (age, Color::Green, "")
    } else if age < 30 {
        (age, Color::DarkGray, "[OUTDATED]")
    } else if age < 120 {
        (age, Color::Yellow, "[OUTDATED]")
    } else if age < 300 {
        (age, Color::LightRed, "[OUTDATED]")
    } else {
        (age, Color::Red, "!! DATA OUTDATED !!")
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
        ActiveTab::Git => draw_git_tab(f, app, chunks[1]),
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
    let (outdated_age, outdated_color, outdated_label) = outdated_indicator(app);

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

    let outdated_span = if !outdated_label.is_empty() {
        Span::styled(
            format!(" {}({}s)", outdated_label, outdated_age),
            Style::default()
                .fg(outdated_color)
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
        ("Git", ActiveTab::Git, 10),
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
        outdated_span,
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
        " f:freeze  n/N:runner  g:follow  c:cancel  r:requeue  d:remove  Enter:logs  ?:help  q:quit"
    } else {
        " ^K:palette  Tab:cycle  1-0:tab  ↑↓:move   Enter:inspect  F5:refresh  ?:help  q:quit"
    };
    let p = Paragraph::new(help)
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

// ---------------------------------------------------------------------------
// Tab 1 — Mission: action-first system cockpit
// ---------------------------------------------------------------------------
