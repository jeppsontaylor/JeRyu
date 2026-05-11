use super::*;

pub(crate) fn draw_pipeline_progress(f: &mut Frame, app: &App, area: Rect) {
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
    let eta_label = match progress.eta_remaining_secs {
        Some(secs) if secs >= 3600 => format!(
            "ETA ~{}h{}m ({})",
            secs / 3600,
            (secs % 3600) / 60,
            progress.eta_confidence
        ),
        Some(secs) if secs >= 60 => format!(
            "ETA ~{}m{}s ({})",
            secs / 60,
            secs % 60,
            progress.eta_confidence
        ),
        Some(secs) => format!("ETA ~{}s ({})", secs, progress.eta_confidence),
        None => "ETA unknown".into(),
    };

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

pub(crate) fn draw_job_matrix(f: &mut Frame, app: &App, area: Rect) {
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
pub(crate) fn draw_pipeline_nav(f: &mut Frame, app: &App, area: Rect) {
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

pub(crate) fn draw_job_inspector_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Inspector ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pane_border(ActivePane::Jobs, app.active_pane)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(job) = app.selected_job() else {
        f.render_widget(
            Paragraph::new("\n  Choose a job with ↑↓").style(Style::default().fg(Color::DarkGray)),
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
            "  [d] Remove event",
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
            "  [r] Retry  [d] Remove",
            Style::default().fg(Color::White),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

// ---------------------------------------------------------------------------
// Tab 4 — Agents: mission/session cockpit
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_more.rs"]
mod ui_panels_body_more;
pub(crate) use ui_panels_body_more::*;
