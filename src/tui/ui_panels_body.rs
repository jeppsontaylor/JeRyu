use super::*;

pub(crate) fn draw_release_tab(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(34)])
        .split(area);

    // Left: gate matrix stacked above pipeline progress
    let gate_h = if app.state.release_status.is_some() {
        12u16
    } else {
        4u16
    };
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(gate_h.min(area.height.saturating_sub(7))),
            Constraint::Min(5),
        ])
        .split(cols[0]);

    draw_release_gates(f, app, left_rows[0]);
    draw_pipeline_progress(f, app, left_rows[1]);

    // Right: per-job list for active pipeline
    draw_release_job_list(f, app, cols[1]);
}

fn draw_release_gates(f: &mut Frame, app: &App, area: Rect) {
    let border_color = app
        .state
        .release_status
        .as_ref()
        .map(|r| release_color(&r.canary_state))
        .unwrap_or(Color::DarkGray);

    let gate_block = Block::default()
        .title(" [ Release Gate Matrix ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let gate_inner = gate_block.inner(area);
    f.render_widget(gate_block, area);

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

        let sep_width = gate_inner.width.saturating_sub(4) as usize;
        let header = Line::from(Span::styled(
            format!(
                "  RELEASE: {}  Phase: {}  ",
                attempt.version, rel.canary_state
            ),
            Style::default()
                .fg(release_color(&rel.canary_state))
                .add_modifier(Modifier::BOLD),
        ));
        let sep = Line::from(Span::styled(
            format!("  {:-<sep_width$}", ""),
            Style::default().fg(Color::DarkGray),
        ));
        let col_header = Line::from(Span::styled(
            format!("  {:<28} {:<7} {:<16} Detail", "Gate", "Status", "State"),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ));

        let mut lines = vec![header, sep.clone(), col_header, sep.clone()];
        for (gate, badge, state, detail) in &gate_rows {
            let badge_color = match *badge {
                "[OK]" => Color::Green,
                "[RUN]" => Color::Cyan,
                "[FAIL]" => Color::Red,
                _ => Color::Yellow,
            };
            lines.push(Line::from(vec![
                Span::raw(format!("  {:<28} ", gate)),
                Span::styled(
                    format!("{:<7} ", badge),
                    Style::default()
                        .fg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<16} ", state), Style::default().fg(Color::White)),
                Span::styled(short_text(detail, 20), Style::default().fg(Color::DarkGray)),
            ]));
        }
        f.render_widget(Paragraph::new(lines), gate_inner);
    } else {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "  No active release attempt.",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    "  Waiting for green main pipeline.",
                    Style::default().fg(Color::DarkGray),
                )),
            ]),
            gate_inner,
        );
    }
}

pub(crate) fn draw_release_job_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Jobs ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let active_pid = app
        .state
        .pipeline_progress_view
        .as_ref()
        .map(|p| p.pipeline_id);
    let tick = app.tick_count;
    let max_name = inner.width.saturating_sub(5) as usize;

    let mut lines: Vec<Line> = Vec::new();

    let jobs: Vec<&crate::state::JobEvent> = app
        .state
        .recent_jobs
        .iter()
        .filter(|j| active_pid.map_or(true, |pid| j.pipeline_id == Some(pid)))
        .take(inner.height as usize)
        .collect();

    for job in &jobs {
        let (dot, color) = match job.status.as_str() {
            "success" => ("●", Color::Green),
            "running" => {
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
            "pending" | "created" => ("○", Color::Yellow),
            "canceled" => ("○", Color::DarkGray),
            _ => ("○", Color::Gray),
        };
        let name = short_text(job.job_name.as_deref().unwrap_or("?"), max_name);
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(format!("{dot} "), Style::default().fg(color)),
            Span::styled(name, Style::default().fg(Color::White)),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No jobs tracked",
            Style::default().fg(Color::DarkGray),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ---------------------------------------------------------------------------
// Tab 3 — Jobs: flow board + jobs list + log preview
// ---------------------------------------------------------------------------

pub(crate) fn draw_jobs_tab(f: &mut Frame, app: &mut App, area: Rect) {
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

#[path = "ui_panels_body_runtime_extra.rs"]
mod ui_panels_body_runtime_extra;
pub(crate) use ui_panels_body_runtime_extra::*;
