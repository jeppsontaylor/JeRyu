use super::*;

pub(crate) fn draw_jank_tab(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Min(12),
        ])
        .split(area);
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(rows[0]);
    let middle_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(rows[1]);
    let bottom_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(rows[2]);

    let scan = app.state.jankurai.last_scan.as_ref();
    let history = &app.state.jankurai.history;
    let score_text = scan_text(scan, |scan| scan.score.to_string(), "n/a");
    let raw_score_text = scan_text(scan, |scan| scan.raw_score.to_string(), "n/a");
    let minimum_score_text = scan_text(scan, |scan| scan.minimum_score.to_string(), "n/a");
    let decision_text = scan_text(scan, |scan| scan.decision.clone(), "n/a");
    let score_status_text = scan_text(scan, |scan| scan.score_status.clone(), "n/a");
    let generated_at_text = match scan {
        Some(scan) => match &scan.generated_at {
            Some(generated_at) => format_timestamp(generated_at),
            None => "n/a".into(),
        },
        None => "n/a".into(),
    };
    let finding_count_text = scan_text(scan, |scan| scan.finding_count.to_string(), "0");
    let hard_findings_text = scan_text(scan, |scan| scan.hard_findings.to_string(), "0");
    let soft_findings_text = scan_text(scan, |scan| scan.soft_findings.to_string(), "0");
    let cap_count_text = scan_text(scan, |scan| scan.caps_applied.len().to_string(), "0");

    let summary_lines = vec![
        Line::from(vec![
            Span::styled("score:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                score_text,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   raw: ", Style::default().fg(Color::DarkGray)),
            Span::styled(raw_score_text, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("min:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(minimum_score_text, Style::default().fg(Color::Yellow)),
            Span::styled("   decision: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                decision_text,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("score gate: ", Style::default().fg(Color::DarkGray)),
            Span::styled(score_status_text, Style::default().fg(Color::White)),
            Span::styled("   at: ", Style::default().fg(Color::DarkGray)),
            Span::styled(generated_at_text, Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("findings: ", Style::default().fg(Color::DarkGray)),
            Span::styled(finding_count_text, Style::default().fg(Color::White)),
            Span::styled(" total / ", Style::default().fg(Color::DarkGray)),
            Span::styled(hard_findings_text, Style::default().fg(Color::Red)),
            Span::styled(" hard / ", Style::default().fg(Color::DarkGray)),
            Span::styled(soft_findings_text, Style::default().fg(Color::Yellow)),
            Span::styled(" soft", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("caps:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(cap_count_text, Style::default().fg(Color::Magenta)),
            Span::styled("   history points: ", Style::default().fg(Color::DarkGray)),
            Span::styled(history.len().to_string(), Style::default().fg(Color::White)),
        ]),
    ];

    let summary_block = Block::default()
        .title(" [ Jankurai Summary ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let summary_inner = summary_block.inner(top_cols[0]);
    f.render_widget(summary_block, top_cols[0]);
    f.render_widget(Paragraph::new(summary_lines), summary_inner);

    let status_block = Block::default()
        .title(" [ Jankurai Status ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.state.jankurai.error.is_some() {
            Color::Red
        } else {
            Color::DarkGray
        }));
    let status_inner = status_block.inner(top_cols[1]);
    f.render_widget(status_block, top_cols[1]);
    if let Some(error) = &app.state.jankurai.error {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Parse / load error",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    short_text(error, status_inner.width.saturating_sub(2) as usize),
                    Style::default().fg(Color::White),
                )),
            ])
            .wrap(Wrap { trim: false }),
            status_inner,
        );
    } else {
        let installed = if app.jankurai_available() {
            "installed"
        } else {
            "not installed"
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Jankurai",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("PATH: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(installed, Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("points: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(history.len().to_string(), Style::default().fg(Color::Green)),
                ]),
            ])
            .wrap(Wrap { trim: false }),
            status_inner,
        );
    }

    let chart_block = Block::default()
        .title(" [ Score History ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let chart_inner = chart_block.inner(middle_cols[0]);
    f.render_widget(chart_block, middle_cols[0]);
    if history.is_empty() {
        f.render_widget(
            Paragraph::new("  No Jankurai history found")
                .style(Style::default().fg(Color::DarkGray)),
            chart_inner,
        );
    } else if chart_inner.width < 40 || chart_inner.height < 6 {
        let scores: Vec<i64> = history.iter().map(|point| point.score).collect();
        let spark = crate::tui::widgets::sparkline::spark_i64(
            &scores,
            chart_inner.width.saturating_sub(4) as usize,
            Color::Cyan,
        );
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("score: ", Style::default().fg(Color::DarkGray)),
                    spark,
                ]),
                Line::from(vec![
                    Span::styled("range: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(
                            "{} -> {}",
                            scores.iter().min().copied().unwrap_or(0),
                            scores.iter().max().copied().unwrap_or(0)
                        ),
                        Style::default().fg(Color::White),
                    ),
                ]),
            ]),
            chart_inner,
        );
    } else {
        let data: Vec<(f64, f64)> = history
            .iter()
            .enumerate()
            .map(|(index, point)| (index as f64, point.score as f64))
            .collect();
        let labels = chart_labels(history);
        let chart = Chart::new(vec![
            Dataset::default()
                .name("score")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Cyan))
                .data(&data),
        ])
        .block(Block::default())
        .x_axis(
            Axis::default()
                .title("time")
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, (data.len().saturating_sub(1)).max(1) as f64])
                .labels(labels.0),
        )
        .y_axis(
            Axis::default()
                .title("score")
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, 100.0])
                .labels(labels.1),
        );
        f.render_widget(chart, chart_inner);
    }

    let breakdown_block = Block::default()
        .title(" [ Last Scan Dimensions ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let breakdown_inner = breakdown_block.inner(middle_cols[1]);
    f.render_widget(breakdown_block, middle_cols[1]);
    if app.state.jankurai.dimensions.is_empty() {
        f.render_widget(
            Paragraph::new("  No dimension breakdown available")
                .style(Style::default().fg(Color::DarkGray)),
            breakdown_inner,
        );
    } else {
        let lines = app
            .state
            .jankurai
            .dimensions
            .iter()
            .map(|dimension| {
                let notes = if dimension.notes.is_empty() {
                    String::new()
                } else {
                    format!(" notes: {}", short_text(&dimension.notes.join("; "), 40))
                };
                Line::from(vec![
                    Span::styled(
                        format!("{:>3} ", dimension.score),
                        Style::default().fg(if dimension.score >= 90 {
                            Color::Green
                        } else if dimension.score >= 75 {
                            Color::Yellow
                        } else {
                            Color::Red
                        }),
                    ),
                    Span::styled(
                        format!("w{:>2} ", dimension.weight),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(
                        short_text(
                            &dimension.name,
                            breakdown_inner.width.saturating_sub(16) as usize,
                        ),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(notes, Style::default().fg(Color::DarkGray)),
                ])
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            breakdown_inner,
        );
    }

    let issues_block = Block::default()
        .title(" [ Caps / Findings ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let issues_inner = issues_block.inner(bottom_cols[0]);
    f.render_widget(issues_block, bottom_cols[0]);

    let (visible_start, visible_end) = visible_entry_window(
        app.state.jankurai.entries.len(),
        app.selected_jankurai_index,
        issues_inner.height as usize,
    );
    let items: Vec<ListItem> = app
        .state
        .jankurai
        .entries
        .iter()
        .enumerate()
        .skip(visible_start)
        .take(visible_end.saturating_sub(visible_start))
        .enumerate()
        .map(|(visible_index, (entry_index, entry))| {
            let index = visible_start + visible_index;
            debug_assert_eq!(index, entry_index);
            let selected = index == app.selected_jankurai_index;
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            let (badge, badge_color) = match entry.kind {
                crate::tui::jankurai::JankuraiEntryKind::Cap => ("CAP", Color::Magenta),
                crate::tui::jankurai::JankuraiEntryKind::Finding => match entry.severity.as_deref()
                {
                    Some("high") => ("HIGH", Color::Red),
                    Some("medium") => ("MED", Color::Yellow),
                    Some("low") => ("LOW", Color::Green),
                    _ => ("INFO", Color::Gray),
                },
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {:<5} ", badge),
                    Style::default()
                        .fg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "{:<18} ",
                        short_text(entry.path.as_deref().unwrap_or(""), 18)
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    short_text(
                        entry.problem.as_deref().unwrap_or(&entry.label),
                        issues_inner.width.saturating_sub(32) as usize,
                    ),
                    Style::default().fg(Color::White),
                ),
            ]);
            ListItem::new(line).style(style)
        })
        .collect();

    if items.is_empty() {
        f.render_widget(
            Paragraph::new("  No caps or findings recorded.")
                .style(Style::default().fg(Color::DarkGray)),
            issues_inner,
        );
    } else {
        f.render_widget(List::new(items), issues_inner);
    }

    let detail_block = Block::default()
        .title(" [ Entry Detail ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let detail_inner = detail_block.inner(bottom_cols[1]);
    f.render_widget(detail_block, bottom_cols[1]);

    if let Some(entry) = app.selected_jankurai_entry() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("kind:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    match entry.kind {
                        crate::tui::jankurai::JankuraiEntryKind::Cap => "cap",
                        crate::tui::jankurai::JankuraiEntryKind::Finding => "finding",
                    },
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("rule:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    entry.rule.as_deref().unwrap_or("n/a"),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("path:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    entry.path.as_deref().unwrap_or("n/a"),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("lane:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    entry.lane.as_deref().unwrap_or("n/a"),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("owner:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    entry.owner.as_deref().unwrap_or("n/a"),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("severity:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    entry.severity.as_deref().unwrap_or("n/a"),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("hardness:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    entry.hardness.as_deref().unwrap_or("n/a"),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(Span::styled(
                "────────────────────────",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(vec![
                Span::styled("problem: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    short_text(
                        entry.problem.as_deref().unwrap_or("n/a"),
                        detail_inner.width.saturating_sub(11) as usize,
                    ),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("fix:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    short_text(
                        entry.suggested_fix.as_deref().unwrap_or("n/a"),
                        detail_inner.width.saturating_sub(11) as usize,
                    ),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
        ];

        if !entry.evidence.is_empty() {
            lines.push(Line::from(Span::styled(
                "evidence:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            for item in &entry.evidence {
                lines.push(Line::from(Span::styled(
                    format!(
                        "  - {}",
                        short_text(item, detail_inner.width.saturating_sub(6) as usize)
                    ),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            detail_inner,
        );
    } else {
        f.render_widget(
            Paragraph::new("  No Jankurai entry selected.")
                .style(Style::default().fg(Color::DarkGray)),
            detail_inner,
        );
    }
}

fn visible_entry_window(
    entry_count: usize,
    selected_index: usize,
    row_count: usize,
) -> (usize, usize) {
    if entry_count == 0 || row_count == 0 {
        return (0, 0);
    }
    let visible_count = row_count.min(entry_count);
    let selected = selected_index.min(entry_count - 1);
    let mut start = selected.saturating_sub(visible_count / 2);
    if start + visible_count > entry_count {
        start = entry_count - visible_count;
    }
    (start, start + visible_count)
}

fn scan_text(
    scan: Option<&crate::tui::jankurai::JankuraiScan>,
    value: impl FnOnce(&crate::tui::jankurai::JankuraiScan) -> String,
    absent: &'static str,
) -> String {
    match scan {
        Some(scan) => value(scan),
        None => absent.into(),
    }
}

fn chart_labels(
    history: &[crate::tui::jankurai::JankuraiHistoryPoint],
) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let start = match history.first() {
        Some(point) => format_timestamp(&point.generated_at),
        None => "start".into(),
    };
    let end = match history.last() {
        Some(point) => format_timestamp(&point.generated_at),
        None => "end".into(),
    };
    (
        vec![
            Span::styled(start, Style::default().fg(Color::DarkGray)),
            Span::styled(end, Style::default().fg(Color::DarkGray)),
        ],
        vec![
            Span::styled("0", Style::default().fg(Color::DarkGray)),
            Span::styled("50", Style::default().fg(Color::DarkGray)),
            Span::styled("100", Style::default().fg(Color::DarkGray)),
        ],
    )
}

fn format_timestamp(value: &chrono::DateTime<chrono::Utc>) -> String {
    value.format("%Y-%m-%d %H:%M").to_string()
}
