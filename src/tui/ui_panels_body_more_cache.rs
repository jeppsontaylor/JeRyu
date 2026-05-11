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
