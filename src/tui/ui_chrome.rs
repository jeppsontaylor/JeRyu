//! Chrome rendering only.
//!
//! This module may read already-loaded TUI state to render headers, tabs,
//! events, and key hints. It must not import durable adapter modules.

use crate::tui::app::ActivePane;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub(crate) struct ChromeTab {
    pub(crate) key: &'static str,
    pub(crate) name: &'static str,
    pub(crate) active: bool,
}

pub(crate) struct ChromeRelease {
    pub(crate) short_sha: String,
    pub(crate) state_label: String,
}

pub(crate) struct ChromeHeaderState {
    pub(crate) active_containers: usize,
    pub(crate) active_runner_groups: usize,
    pub(crate) total_runner_groups: usize,
    pub(crate) agent_count: usize,
    pub(crate) cache_hit_ratio: f64,
    pub(crate) active_taint_count: i64,
    pub(crate) gitlab_ready: bool,
    pub(crate) release: Option<ChromeRelease>,
    pub(crate) last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) tabs: Vec<ChromeTab>,
}

pub(crate) struct ChromeEvent {
    pub(crate) ts: String,
    pub(crate) badge: &'static str,
    pub(crate) color: Color,
    pub(crate) name: String,
}

pub(crate) struct ChromeEventState {
    pub(crate) entries: Vec<ChromeEvent>,
    pub(crate) ticker_offset: usize,
}

pub(crate) struct AttentionState {
    pub(crate) active_taint_count: i64,
    pub(crate) release: Option<AttentionRelease>,
    pub(crate) failed_job: Option<AttentionJob>,
    pub(crate) has_running_job: bool,
    pub(crate) gitlab_ready: bool,
}

pub(crate) struct AttentionRelease {
    pub(crate) version: String,
    pub(crate) state_label: String,
}

pub(crate) struct AttentionJob {
    pub(crate) id: i64,
    pub(crate) name: String,
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

pub(crate) fn status_color(status: &str) -> Color {
    match status {
        "success" | "omitted" | "vti-skipped" => Color::Green,
        "running" => Color::Blue,
        "failed" => Color::Red,
        "pending" | "created" => Color::Yellow,
        "canceled" => Color::DarkGray,
        _ => Color::Gray,
    }
}

pub(crate) fn release_color(state: &str) -> Color {
    match state {
        "green" | "released" => Color::Green,
        "in-flight" | "canary-authorized" => Color::Cyan,
        "waiting" | "ready-for-canary" => Color::Yellow,
        "blocked" | "blocked-by-upstream" => Color::Magenta,
        "failed" => Color::Red,
        _ => Color::DarkGray,
    }
}

pub(crate) fn pane_border(pane: ActivePane, active_pane: ActivePane) -> Color {
    if active_pane == pane {
        Color::Cyan
    } else {
        Color::DarkGray
    }
}

pub(crate) fn status_badge(status: &str) -> (&'static str, Color) {
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

pub(crate) fn meter_bar(percent: u16, width: usize) -> String {
    let width = width.max(1);
    let filled = (percent.min(100) as usize * width + 50) / 100;
    format!(
        "{}{} {:>3}%",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled)),
        percent.min(100)
    )
}

pub(crate) fn compact_spark(values: &[i64], width: usize) -> String {
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

pub(crate) fn top_attention(attention: &AttentionState) -> (String, Color, String) {
    if attention.active_taint_count > 0 {
        return (
            format!(
                "{} active cache taint(s) can block trusted proof reuse",
                attention.active_taint_count
            ),
            Color::Magenta,
            "Open Cache, inspect taint scope, then run clean validation".to_string(),
        );
    }
    if let Some(rel) = &attention.release
        && !matches!(rel.state_label.as_str(), "green" | "released")
    {
        return (
            format!("Release {} is {}", rel.version, rel.state_label),
            release_color(&rel.state_label),
            "Open Release, inspect missing gate evidence".to_string(),
        );
    }
    if let Some(job) = &attention.failed_job {
        return (
            format!("Job #{} failed in {}", job.id, job.name),
            Color::Red,
            "Open evidence capsule or revisit after blocker explanation".to_string(),
        );
    }
    if attention.has_running_job {
        return (
            "Validation is active on the critical path".to_string(),
            Color::Cyan,
            "Watch Flow Board and open the slowest running job".to_string(),
        );
    }
    if !attention.gitlab_ready {
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
pub(crate) fn outdated_indicator(
    last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
) -> (i64, Color, &'static str) {
    let age = last_sync_at
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
// Header + Tab bar (2 rows merged into 1 widget)
// ---------------------------------------------------------------------------

pub(crate) fn draw_header_tabs(f: &mut Frame, header: &ChromeHeaderState, area: Rect) {
    let (outdated_age, outdated_color, outdated_label) = outdated_indicator(header.last_sync_at);

    let gitlab_span = if header.gitlab_ready {
        Span::styled("GitLab:OK", Style::default().fg(Color::Green))
    } else {
        Span::styled("GitLab:BOOT", Style::default().fg(Color::Yellow))
    };

    let release_span = if let Some(ref rel) = header.release {
        Span::styled(
            format!(" rel:{} {}", rel.short_sha, rel.state_label),
            Style::default()
                .fg(release_color(&rel.state_label))
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

    let top_spans: Vec<Span> = vec![
        Span::styled(
            " jeryu ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        gitlab_span,
        Span::styled(
            format!(" ctrs:{}", header.active_containers),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            format!(
                " runners:{}/{}",
                header.active_runner_groups, header.total_runner_groups
            ),
            Style::default().fg(
                if header.active_runner_groups == header.total_runner_groups {
                    Color::Green
                } else {
                    Color::Yellow
                },
            ),
        ),
        release_span,
        // v3 — Agent count badge
        Span::styled(
            format!(" agents:{}", header.agent_count),
            Style::default().fg(if header.agent_count == 0 {
                Color::DarkGray
            } else {
                Color::Rgb(102, 255, 255)
            }),
        ),
        // v3 — Cache hit ratio
        Span::styled(
            format!(" cache:{:.0}%", header.cache_hit_ratio * 100.0),
            Style::default().fg(if header.cache_hit_ratio > 0.8 {
                Color::Green
            } else if header.cache_hit_ratio > 0.5 {
                Color::Yellow
            } else {
                Color::Red
            }),
        ),
        // v3 — Taint indicator
        if header.active_taint_count > 0 {
            Span::styled(
                format!(" taint:{}", header.active_taint_count),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        },
        outdated_span,
    ];

    let mut tab_spans: Vec<Span> = vec![];
    for tab in &header.tabs {
        if tab.active {
            tab_spans.push(Span::styled(
                format!("[{}:{}]", tab.key, tab.name),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(
                format!(" {}:{} ", tab.key, tab.name),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    let p = Paragraph::new(vec![Line::from(top_spans), Line::from(tab_spans)])
        .block(Block::default().borders(Borders::BOTTOM))
        .style(Style::default().fg(Color::White));
    f.render_widget(p, area);
}

// ---------------------------------------------------------------------------
// Event console (bottom strip above footer)
// ---------------------------------------------------------------------------

pub(crate) fn draw_event_console(f: &mut Frame, events: &ChromeEventState, area: Rect) {
    let block = Block::default()
        .title(" Events ── Ctrl-K: command palette  /: search  ?: help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build ticker line from recent events (scrolling right-to-left)
    let mut ticker_spans: Vec<Span> = Vec::new();
    let _now = chrono::Utc::now();

    if events.entries.is_empty() {
        let p = Paragraph::new(Span::styled(
            "  No events yet. Events appear here as jobs run.",
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(p, inner);
        return;
    }

    // Build a single scrolling line
    for event in &events.entries {
        ticker_spans.push(Span::styled(
            format!(" {} ", event.ts),
            Style::default().fg(Color::DarkGray),
        ));
        ticker_spans.push(Span::styled(
            format!("[{}]", event.badge),
            Style::default()
                .fg(event.color)
                .add_modifier(Modifier::BOLD),
        ));
        ticker_spans.push(Span::styled(
            format!(" {}  │", event.name),
            Style::default().fg(Color::White),
        ));
    }

    // Scroll offset drives the horizontal shift
    let offset = (events.ticker_offset % (events.entries.len() * 30 + 1)) as u16;

    let p = Paragraph::new(Line::from(ticker_spans)).scroll((0, offset));
    f.render_widget(p, inner);
}

// ---------------------------------------------------------------------------
// Footer / key hints
// ---------------------------------------------------------------------------

pub(crate) fn draw_footer(f: &mut Frame, help: &str, area: Rect) {
    let p = Paragraph::new(help)
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}
