//! Owner: Interactive TUI subsystem — agent fleet cockpit widget
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::agent_fleet`
//! Invariants: Agent fleet view renders from AgentSession; no control-plane mutation.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::api::agent_session::{AgentBudget, AgentSession, AgentState, PatchAttempt, PatchStatus, TrustTier};
use crate::tui::theme::Theme;

/// Render the full agent fleet view — replaces pipeline-centric agent rendering.
pub fn render_agent_fleet(
    f: &mut Frame,
    area: Rect,
    sessions: &[AgentSession],
    selected: usize,
    theme: &Theme,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),  // Agent list
            Constraint::Percentage(40),  // Agent detail cockpit
            Constraint::Percentage(26),  // Grants + actions
        ])
        .split(area);

    // ── Agent List ──────────────────────────────────────────────────
    let items: Vec<ListItem> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let is_selected = i == selected;
            let prefix = if is_selected { ">>" } else { "  " };
            let state_color = state_theme_color(s.state, theme);

            let line = Line::from(vec![
                Span::styled(
                    format!("{} {} ", prefix, s.state.glyph()),
                    Style::default()
                        .fg(state_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<5} ", s.state.label()),
                    Style::default().fg(state_color),
                ),
                Span::styled(
                    super::truncate_label(&s.objective, cols[0].width.saturating_sub(16) as usize),
                    if is_selected { theme.primary() } else { theme.secondary() },
                ),
            ]);

            let style = if is_selected {
                Style::default().bg(Color::Rgb(45, 45, 55))
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let count_active = sessions.iter().filter(|s| s.state.is_active()).count();
    f.render_widget(
        List::new(items).block(
            Block::default()
                .title(format!(" [ Agent Fleet ({}/{}) ] ", count_active, sessions.len()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.agent)),
        ),
        cols[0],
    );

    // ── Agent Detail Cockpit ────────────────────────────────────────
    let detail_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // Detail card
            Constraint::Min(6),    // Patch race board
        ])
        .split(cols[1]);

    if let Some(session) = sessions.get(selected) {
        render_session_detail(f, detail_rows[0], session, theme);
        render_patch_board(f, detail_rows[1], session, theme);
    } else {
        f.render_widget(
            Paragraph::new("  No agent sessions yet.\n  Branches starting with agent/ appear here.")
                .block(
                    Block::default()
                        .title(" [ Agent Cockpit ] ")
                        .borders(Borders::ALL),
                )
                .style(theme.muted()),
            detail_rows[0],
        );
    }

    // ── Grants + Actions ────────────────────────────────────────────
    if let Some(session) = sessions.get(selected) {
        render_agent_grants(f, cols[2], session, theme);
    } else {
        f.render_widget(
            Paragraph::new("  —")
                .block(
                    Block::default()
                        .title(" [ Grants ] ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border_subtle)),
                ),
            cols[2],
        );
    }
}

fn render_session_detail(f: &mut Frame, area: Rect, s: &AgentSession, theme: &Theme) {
    let state_color = state_theme_color(s.state, theme);

    let budget_bar = |used: f64, limit: f64| -> String {
        if limit <= 0.0 { return "n/a".to_string(); }
        let pct = ((used / limit) * 100.0).min(100.0);
        let filled = (pct as usize * 10 / 100).min(10);
        format!("{}{}  {:.0}%", "█".repeat(filled), "░".repeat(10 - filled), pct)
    };

    let time_bar = budget_bar(s.budget.time_used_secs as f64, s.budget.time_limit_secs as f64);
    let ci_bar = budget_bar(s.budget.ci_minutes_used as f64, s.budget.ci_minutes_limit as f64);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("  State:  ", theme.muted()),
            Span::styled(
                format!("{} {}", s.state.glyph(), s.state.label()),
                theme.bold(state_color),
            ),
            Span::styled("  Trust: ", theme.muted()),
            Span::styled(
                s.trust_tier.label(),
                Style::default().fg(trust_color(s.trust_tier, theme)),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Goal:   ", theme.muted()),
            Span::styled(
                super::truncate_label(&s.objective, area.width.saturating_sub(14) as usize),
                theme.primary(),
            ),
        ]),
    ];

    if let Some(ref intent) = s.current_intent {
        lines.push(Line::from(vec![
            Span::styled("  Intent: ", theme.muted()),
            Span::styled(intent.clone(), Style::default().fg(theme.running)),
        ]));
    }
    if let Some(ref step) = s.current_step {
        lines.push(Line::from(vec![
            Span::styled("  Step:   ", theme.muted()),
            Span::styled(step.clone(), theme.secondary()),
        ]));
    }

    // Budget bars
    lines.push(Line::from(vec![
        Span::styled("  Time:   ", theme.muted()),
        Span::styled(
            time_bar,
            Style::default().fg(if s.budget.time_pct() > 80.0 { theme.fail } else { theme.ok }),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  CI:     ", theme.muted()),
        Span::styled(
            ci_bar,
            Style::default().fg(if s.budget.is_exhausted() { theme.fail } else { theme.ok }),
        ),
    ]));

    if let Some(conf) = s.confidence {
        lines.push(Line::from(vec![
            Span::styled("  Conf:   ", theme.muted()),
            Span::styled(
                format!("{:.0}%", conf * 100.0),
                Style::default().fg(if conf > 0.7 { theme.ok } else { theme.waiting }),
            ),
        ]));
    }

    if let Some(ref branch) = s.branch {
        lines.push(Line::from(vec![
            Span::styled("  Branch: ", theme.muted()),
            Span::styled(branch.clone(), theme.secondary()),
        ]));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(format!(" [ {} ] ", s.id))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(state_color)),
        ),
        area,
    );
}

fn render_patch_board(f: &mut Frame, area: Rect, s: &AgentSession, theme: &Theme) {
    let block = Block::default()
        .title(format!(" [ Patch Race ({}) ] ", s.patch_attempts.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if s.patch_attempts.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  No patches yet", theme.muted())),
            inner,
        );
        return;
    }

    let max = inner.height as usize;
    let lines: Vec<Line> = s.patch_attempts
        .iter()
        .take(max)
        .map(|p| {
            let status_color = patch_color(p.status, theme);
            let score_str = match p.score {
                Some(score) => format!(" score:{}", score),
                None => String::new(),
            };
            Line::from(vec![
                Span::styled(
                    format!(" {} ", p.status.glyph()),
                    theme.bold(status_color),
                ),
                Span::styled(
                    super::truncate_label(&p.label, inner.width.saturating_sub(30) as usize),
                    theme.primary(),
                ),
                Span::styled(
                    format!(" {} {}{}", p.diff_stat, p.risk.label(), score_str),
                    theme.muted(),
                ),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_agent_grants(f: &mut Frame, area: Rect, s: &AgentSession, theme: &Theme) {
    let mut lines = vec![
        Line::from(Span::styled(
            "  Authority",
            theme.bold(theme.text_secondary),
        )),
        Line::from(vec![
            Span::styled("  Trust: ", theme.muted()),
            Span::styled(
                s.trust_tier.label(),
                Style::default().fg(trust_color(s.trust_tier, theme)),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Active Grants",
            theme.bold(theme.text_secondary),
        )),
    ];

    if s.grants.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.muted())));
    } else {
        for g in s.grants.iter().take(4) {
            let remaining = g.expires_at.signed_duration_since(chrono::Utc::now()).num_seconds().max(0);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", g.action_id),
                    Style::default().fg(theme.waiting),
                ),
                Span::styled(
                    format!("{}s left", remaining),
                    theme.muted(),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Blockers",
        theme.bold(theme.text_secondary),
    )));

    if s.blockers.is_empty() {
        lines.push(Line::from(Span::styled("  clear", Style::default().fg(theme.ok))));
    } else {
        for b in s.blockers.iter().take(3) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", b.severity.label()),
                    theme.bold(match b.severity {
                        crate::api::entity::Severity::Critical => theme.fail,
                        crate::api::entity::Severity::Error => theme.warning,
                        _ => theme.waiting,
                    }),
                ),
                Span::styled(
                    super::truncate_label(&b.summary, area.width.saturating_sub(10) as usize),
                    theme.primary(),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Actions",
        theme.bold(theme.text_secondary),
    )));
    lines.push(Line::from(Span::styled("  ^K explain blockers", theme.primary())));
    lines.push(Line::from(Span::styled("  ^K fetch capsule", theme.primary())));

    if let Some(ref next) = s.next_action {
        lines.push(Line::from(vec![
            Span::styled("  → ", Style::default().fg(theme.running)),
            Span::styled(&next.label, theme.bold(theme.running)),
        ]));
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" [ Grants / Blockers ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.blocked)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn state_theme_color(state: AgentState, theme: &Theme) -> Color {
    match state {
        AgentState::Spawning => theme.waiting,
        AgentState::Diagnosing => theme.running,
        AgentState::Patching => theme.agent,
        AgentState::Validating => theme.running,
        AgentState::Racing => theme.vti_fire,
        AgentState::Blocked | AgentState::Failed => theme.fail,
        AgentState::WaitingApproval => theme.warning,
        AgentState::Completed => theme.ok,
        AgentState::Paused => theme.skipped,
    }
}

fn trust_color(tier: TrustTier, theme: &Theme) -> Color {
    match tier {
        TrustTier::Untrusted => theme.fail,
        TrustTier::Standard => theme.waiting,
        TrustTier::Trusted => theme.ok,
        TrustTier::Elevated => theme.blocked,
    }
}

fn patch_color(status: PatchStatus, theme: &Theme) -> Color {
    match status {
        PatchStatus::Proposed => theme.waiting,
        PatchStatus::Testing => theme.running,
        PatchStatus::Green => theme.ok,
        PatchStatus::Failed => theme.fail,
        PatchStatus::Winner => theme.vti_fire,
        PatchStatus::Archived => theme.skipped,
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_colors_are_themed() {
        let t = Theme::dark();
        assert_eq!(state_theme_color(AgentState::Completed, &t), t.ok);
        assert_eq!(state_theme_color(AgentState::Failed, &t), t.fail);
        assert_eq!(state_theme_color(AgentState::Racing, &t), t.vti_fire);
    }

    #[test]
    fn trust_colors_are_themed() {
        let t = Theme::dark();
        assert_eq!(trust_color(TrustTier::Trusted, &t), t.ok);
        assert_eq!(trust_color(TrustTier::Untrusted, &t), t.fail);
    }
}
