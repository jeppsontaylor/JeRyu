//! Owner: Interactive TUI subsystem — workflow DAG renderer
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::widget`
//! Invariants: Widget is pure rendering; it never mutates workflow state.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::model::*;
use crate::tui::theme::Theme;

/// Draw the full workflow tab: summary banner + phase rows.
pub fn draw_workflow_tab(f: &mut Frame, area: Rect, snapshot: &WorkflowSnapshot, theme: &Theme) {
    if snapshot.phases.is_empty() {
        draw_empty_state(f, area, snapshot, theme);
        return;
    }

    // Layout: summary banner (3 lines) + scrollable phases.
    let phase_count = snapshot.phases.len();
    let mut constraints = vec![Constraint::Length(4)]; // Banner
    for _ in 0..phase_count {
        constraints.push(Constraint::Length(6)); // Each phase row
    }
    constraints.push(Constraint::Min(1)); // Remaining space

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    draw_summary_banner(f, rows[0], snapshot, theme);

    for (i, phase) in snapshot.phases.iter().enumerate() {
        let phase_area = rows[i + 1];
        draw_phase_row(f, phase_area, phase, snapshot, theme);
    }
}

fn draw_empty_state(f: &mut Frame, area: Rect, _snapshot: &WorkflowSnapshot, theme: &Theme) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  No active workflow",
            theme.bold(theme.text_muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Waiting for a VTI plan or active pipeline.",
            theme.muted(),
        )),
        Line::from(Span::styled(
            "  Run `jeryu test select` or push a commit to generate a workflow.",
            theme.muted(),
        )),
    ];

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ 0:Workflow ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_subtle)),
        ),
        area,
    );
}

fn draw_summary_banner(f: &mut Frame, area: Rect, snap: &WorkflowSnapshot, theme: &Theme) {
    let s = &snap.summary;
    let overall_color = if s.error > 0 {
        theme.fail
    } else if s.running > 0 {
        theme.running
    } else if s.blocked > 0 {
        theme.blocked
    } else if s.total == s.passed + s.cached + s.skipped {
        theme.ok
    } else {
        theme.waiting
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  Workflow: {} ", snap.title),
                Style::default()
                    .fg(theme.text_inverse)
                    .bg(overall_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  mode:{} ", snap.mode), theme.secondary()),
            Span::styled(
                format!("conf:{:.0}% ", snap.confidence * 100.0),
                theme.bold(theme.ok),
            ),
            Span::styled(
                format!("progress:{:.0}%", s.overall_pct),
                theme.bold(overall_color),
            ),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            status_count("✓", s.passed, theme.ok, theme),
            Span::raw("  "),
            status_count("●", s.running, theme.running, theme),
            Span::raw("  "),
            status_count("○", s.waiting, theme.waiting, theme),
            Span::raw("  "),
            status_count("✗", s.error, theme.fail, theme),
            Span::raw("  "),
            status_count("⊘", s.skipped, theme.skipped, theme),
            Span::raw("  "),
            status_count("◈", s.cached, theme.vti_fire, theme),
        ]),
    ];

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ 0:Workflow ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(overall_color)),
        ),
        area,
    );
}

fn status_count<'a>(glyph: &str, count: u32, color: Color, theme: &Theme) -> Span<'a> {
    Span::styled(
        format!("{} {}", glyph, count),
        if count > 0 {
            theme.bold(color)
        } else {
            theme.muted()
        },
    )
}

fn draw_phase_row(
    f: &mut Frame,
    area: Rect,
    phase: &WorkflowPhase,
    snap: &WorkflowSnapshot,
    theme: &Theme,
) {
    let node_count = phase.node_ids.len().max(1);
    let pct = (100 / node_count.max(1)) as u16;
    let constraints: Vec<Constraint> = (0..node_count)
        .map(|_| Constraint::Percentage(pct))
        .collect();

    let block = Block::default()
        .title(format!(" {} ", phase.title))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme.border_subtle));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(inner);

    for (i, node_id) in phase.node_ids.iter().enumerate() {
        if let Some(col) = cols.get(i)
            && let Some(node) = snap.node(node_id)
        {
            draw_node_card(f, *col, node, snap, theme);
        }
    }
}

fn draw_node_card(
    f: &mut Frame,
    area: Rect,
    node: &WorkflowNode,
    snap: &WorkflowSnapshot,
    theme: &Theme,
) {
    let is_selected = snap.selected_node_id.as_deref() == Some(&node.id);
    let status_color = node_color(node.status, theme);

    let border_style = if is_selected {
        Style::default()
            .fg(theme.border_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(status_color)
    };

    let vti_badge = match node.vti_status.as_ref() {
        Some(v) => v.badge(),
        None => "",
    };
    let cache_badge = match node.cache_verdict.as_ref() {
        Some(c) => c.badge(),
        None => "",
    };

    let title = format!(
        " {} {} {}{}",
        node.status.glyph(),
        node.status.label(),
        crate::tui::widgets::truncate_label(&node.label, area.width.saturating_sub(18) as usize),
        if node.critical_path { " [CRIT]" } else { "" },
    );

    let mut lines = Vec::new();

    // Command line
    if let Some(cmd) = &node.command {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                crate::tui::widgets::truncate_label(cmd, area.width.saturating_sub(4) as usize)
            ),
            theme.muted(),
        )));
    }

    // Badges + progress
    let mut badge_spans = vec![Span::styled("  ", Style::default())];
    if !vti_badge.is_empty() {
        badge_spans.push(Span::styled(
            format!("{} ", vti_badge),
            theme.bold(theme.vti_fire),
        ));
    }
    if !cache_badge.is_empty() {
        badge_spans.push(Span::styled(
            format!("{} ", cache_badge),
            theme.bold(theme.ok),
        ));
    }
    if let Some(pct) = node.progress_pct {
        badge_spans.push(Span::styled(format!("{}%", pct), theme.bold(status_color)));
    }
    if let Some(eta) = node.eta_secs {
        badge_spans.push(Span::styled(format!(" eta:{}s", eta), theme.muted()));
    }
    if badge_spans.len() > 1 {
        lines.push(Line::from(badge_spans));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        ),
        area,
    );
}

fn node_color(status: WorkflowStatus, theme: &Theme) -> Color {
    match status {
        WorkflowStatus::Ran => theme.ok,
        WorkflowStatus::Running => theme.running,
        WorkflowStatus::Error => theme.fail,
        WorkflowStatus::Waiting => theme.waiting,
        WorkflowStatus::Skipped => theme.skipped,
        WorkflowStatus::Cached => theme.vti_fire,
        WorkflowStatus::Blocked => theme.blocked,
        WorkflowStatus::Unknown => theme.text_muted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::workflow::builder::build_demo_snapshot;

    #[test]
    fn node_color_maps_all_statuses() {
        let theme = Theme::dark();
        // Ensure every status maps without panic.
        for s in &[
            WorkflowStatus::Waiting,
            WorkflowStatus::Running,
            WorkflowStatus::Ran,
            WorkflowStatus::Error,
            WorkflowStatus::Skipped,
            WorkflowStatus::Cached,
            WorkflowStatus::Blocked,
            WorkflowStatus::Unknown,
        ] {
            let _ = node_color(*s, &theme);
        }
    }

    #[test]
    fn demo_snapshot_has_phases() {
        let snap = build_demo_snapshot();
        assert!(!snap.phases.is_empty());
        assert!(!snap.nodes.is_empty());
    }
}
