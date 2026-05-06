use super::*;

// ---------------------------------------------------------------------------
// Tab 7 — Cache (existing dashboard, preserved)
// ---------------------------------------------------------------------------

pub(crate) fn draw_cache_dashboard(f: &mut Frame, app: &App, area: Rect) {
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

pub(crate) fn draw_evidence_tab(f: &mut Frame, app: &App, area: Rect) {
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
            if let Some(requeue_id) = cap.requeued_from_job_id {
                lines.push(Line::from(vec![
                    Span::styled("requeue_of: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("job#{}", requeue_id),
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

pub(crate) fn draw_audit_ledger(f: &mut Frame, app: &App, area: Rect) {
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
            let job_str = match ev.job_id {
                Some(id) => format!("job#{} ", id),
                None => String::new(),
            };
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

pub(crate) fn draw_secrets_tab(f: &mut Frame, app: &App, area: Rect) {
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
// Tab 10 — Git
// ---------------------------------------------------------------------------

pub(crate) fn draw_git_tab(f: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<ListItem> = app
        .state
        .recent_git_events
        .iter()
        .map(|event| {
            let ts = event.created_at.get(..16).unwrap_or(&event.created_at);
            let status = if event.exit_code == 0 {
                "success"
            } else {
                "failed"
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:<12} ", event.command_class),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:<5} ", status),
                    Style::default().fg(status_color(status)),
                ),
                Span::styled(
                    format!("{:<7} ", event.mirror_status),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(
                    event.argv_redacted.clone(),
                    Style::default().fg(Color::White),
                ),
            ]))
        })
        .collect();

    let body = if rows.is_empty() {
        List::new(vec![ListItem::new("  No git commands recorded yet.")])
    } else {
        List::new(rows)
    };

    f.render_widget(
        body.block(
            Block::default()
                .title(format!(
                    " [ Git Command Ledger ({}) ] ",
                    app.state.recent_git_events.len()
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}
