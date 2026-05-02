//! Owner: Interactive TUI subsystem — flow inspector pane
//! Proof: `cargo nextest run -p jeryu -- tui::flow`
//! Invariants: Inspector output is read-only and redacts sensitive trace material.

use super::model::FlowNode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn draw_inspector(
    f: &mut Frame,
    area: Rect,
    node: Option<&FlowNode>,
    trace_tail: Option<&str>,
) {
    if let Some(n) = node {
        let title = format!(" JOB {} ", n.label);

        let color = match n.status.as_str() {
            "success" => Color::Green,
            "running" => Color::Blue,
            "failed" => Color::Red,
            "pending" | "created" => Color::Yellow,
            "canceled" => Color::DarkGray,
            _ => Color::Gray,
        };

        let eta_str = if let Some(ref e) = n.eta {
            format!("{}s", e.remaining_secs)
        } else {
            "N/A".to_string()
        };

        let body = format!(
            "Status: {}\nProgress: {}%\nETA: {}\nPhase: {:?} Lane: {:?}\nRequired: {}\nCritical Path: {}\n\nTrace tail:\n{}",
            n.status,
            n.progress_pct,
            eta_str,
            n.column,
            n.lane,
            n.is_required,
            n.is_critical_path,
            trace_tail.unwrap_or("Waiting for logs...")
        );

        let p = Paragraph::new(body)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(color)),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(p, area);
    } else {
        let p =
            Paragraph::new("No job selected or graph is empty.\nUse arrow keys to navigate flow.")
                .block(
                    Block::default()
                        .title(" [ Inspector ] ")
                        .borders(Borders::ALL),
                );
        f.render_widget(p, area);
    }
}
