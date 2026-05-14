use super::*;
use crate::tui::app::ReleaseSubPane;

pub(crate) fn draw_release_tab(f: &mut Frame, app: &App, area: Rect) {
    // Top strip: sub-pane selector (1/2/3 or h/l to cycle).
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(8)])
        .split(area);

    draw_release_subpane_tabs(f, app, split[0]);

    match app.release_subpane {
        ReleaseSubPane::Pipeline => draw_release_pipeline_pane(f, app, split[1]),
        ReleaseSubPane::Evidence => draw_release_evidence_pane(f, app, split[1]),
        ReleaseSubPane::Rollback => draw_release_rollback_pane(f, app, split[1]),
    }
}

fn draw_release_subpane_tabs(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::styled(
        " release ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )];
    for (i, pane) in [
        ReleaseSubPane::Pipeline,
        ReleaseSubPane::Evidence,
        ReleaseSubPane::Rollback,
    ]
    .iter()
    .enumerate()
    {
        let style = if *pane == app.release_subpane {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(Color::Gray)
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" [{}] {} ", i + 1, pane.label()),
            style,
        ));
    }
    spans.push(Span::raw("   "));
    spans.push(Span::styled(
        "(1/2/3 or h/l to cycle)",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default()),
        area,
    );
}

fn draw_release_pipeline_pane(f: &mut Frame, app: &App, area: Rect) {
    let snap = &app.state.release_stages;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    let stages: [(&str, &Vec<crate::tui::app::ReleaseStageCard>, Color); 5] = [
        ("Plan", &snap.plan, Color::Blue),
        ("Build", &snap.build, Color::Cyan),
        ("Proof", &snap.proof, Color::Yellow),
        ("Canary", &snap.canary, Color::Magenta),
        ("Stable", &snap.stable, Color::Green),
    ];

    for (i, (name, cards, color)) in stages.iter().enumerate() {
        let title = format!(" {} [{}] ", name, cards.len());
        let items: Vec<ListItem> = cards
            .iter()
            .map(|c| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", c.label), Style::default().fg(*color)),
                    Span::styled(format!("{} ", c.agent_id), Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("({}) ", c.age),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();
        let list = List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(*color)),
        );
        f.render_widget(list, cols[i]);
    }
}

fn draw_release_rollback_pane(f: &mut Frame, app: &App, area: Rect) {
    let _ = app;
    let ladder = crate::release::default_ladder();
    let items: Vec<ListItem> = ladder
        .iter()
        .map(|s| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" [{}] ", s.n),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<13} ", s.kind), Style::default().fg(Color::Cyan)),
                Span::raw(s.description.clone()),
            ]))
        })
        .collect();
    let list = List::new(items).block(
        Block::default()
            .title(" [ Rollback ladder ] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red)),
    );
    f.render_widget(list, area);
}

fn draw_release_evidence_pane(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(34)])
        .split(area);

    // Center: Release Gate Matrix (original Release tab content)
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

pub(crate) fn draw_release_inspector(f: &mut Frame, app: &App, area: Rect) {
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
