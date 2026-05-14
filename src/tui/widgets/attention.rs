//! Owner: Interactive TUI subsystem — attention queue widget
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::attention`
//! Invariants: Attention items are rendered in severity order; never mutates state.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::api::entity::Severity;
use crate::api::read_model::AttentionItem;
use crate::tui::theme::Theme;

/// Render the left-rail attention queue from ranked attention items.
pub fn render_attention_rail(
    f: &mut Frame,
    area: Rect,
    items: &[AttentionItem],
    selected: Option<usize>,
    theme: &Theme,
) {
    let block = Block::default()
        .title(" [ Attention ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if items.is_empty() {
            theme.ok
        } else {
            severity_color(
                items.first().map(|i| i.severity).unwrap_or(Severity::Info),
                theme,
            )
        }));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if items.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " No blockers",
                Style::default().fg(theme.ok),
            ))),
            inner,
        );
        return;
    }

    let max_items = inner.height as usize;
    let lines: Vec<Line> = items
        .iter()
        .take(max_items)
        .enumerate()
        .map(|(idx, item)| {
            let sev_color = severity_color(item.severity, theme);
            let is_selected = selected == Some(idx);

            let mut spans = vec![
                Span::styled(
                    format!(" {} ", item.severity.label()),
                    Style::default()
                        .fg(theme.text_inverse)
                        .bg(sev_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ];

            let max_title = inner.width.saturating_sub(8) as usize;
            let title = if item.title.len() > max_title && max_title > 3 {
                format!("{}...", &item.title[..max_title - 3])
            } else {
                item.title.clone()
            };

            if is_selected {
                spans.push(Span::styled(
                    title,
                    Style::default()
                        .fg(sev_color)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
            } else {
                spans.push(Span::styled(
                    title,
                    Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
                ));
            }

            Line::from(spans)
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn severity_color(severity: Severity, theme: &Theme) -> ratatui::style::Color {
    match severity {
        Severity::Critical => theme.fail,
        Severity::Error => theme.warning,
        Severity::Warning => theme.waiting,
        Severity::Info => theme.text_muted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_color_maps_correctly() {
        let t = Theme::dark();
        assert_eq!(severity_color(Severity::Critical, &t), t.fail);
        assert_eq!(severity_color(Severity::Warning, &t), t.waiting);
    }
}
