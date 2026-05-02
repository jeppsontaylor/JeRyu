//! Owner: Interactive TUI subsystem — flow graph widget
//! Proof: `cargo nextest run -p vgit -- tui::flow`
//! Invariants: Widget rendering is pure over the supplied graph and selected node.

use super::model::{FlowColumnKind, FlowGraph};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

pub struct FlowGraphWidget<'a> {
    pub graph: &'a FlowGraph,
    pub selected_node_id: Option<i64>,
}

impl<'a> FlowGraphWidget<'a> {
    pub fn new(graph: &'a FlowGraph, selected_node_id: Option<i64>) -> Self {
        Self {
            graph,
            selected_node_id,
        }
    }
}

impl<'a> Widget for FlowGraphWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.graph.columns.is_empty() {
            buf.set_string(area.x, area.y, "Waiting for job graph...", Style::default());
            return;
        }

        let total_cols = self.graph.columns.len() as u16;
        let col_width = if total_cols > 0 {
            area.width / total_cols
        } else {
            area.width
        };

        // Headers
        for (i, col) in self.graph.columns.iter().enumerate() {
            let x = area.x + (i as u16 * col_width);
            if x < area.right() {
                buf.set_stringn(
                    x,
                    area.y,
                    &col.title,
                    col_width as usize,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                );
            }
        }

        // Separator line
        let y_sep = area.y + 1;
        if y_sep < area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y_sep)]
                    .set_symbol("─")
                    .set_style(Style::default().fg(Color::DarkGray));
            }
        }

        // Draw nodes by lane groups
        let mut max_y_drawn = y_sep + 1;

        for (i, col) in self.graph.columns.iter().enumerate() {
            let mut y_cur = y_sep + 1;
            let x_base = area.x + (i as u16 * col_width);

            for group in &col.lane_groups {
                if y_cur >= area.bottom() {
                    break;
                }

                // Print lane header if it's Tests/Security
                if col.key == FlowColumnKind::Tests || col.key == FlowColumnKind::Security {
                    buf.set_stringn(
                        x_base,
                        y_cur,
                        format!("├─ {}", group.title),
                        col_width as usize,
                        Style::default().fg(Color::Gray),
                    );
                    y_cur += 1;
                }

                // Print stacking nodes
                for &node_id in &group.node_ids {
                    if y_cur >= area.bottom() {
                        break;
                    }

                    if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id) {
                        let selected = self.selected_node_id == Some(node.id);
                        let is_stacked =
                            col.key == FlowColumnKind::Tests || col.key == FlowColumnKind::Security;
                        let prefix = if selected {
                            ">>"
                        } else if is_stacked {
                            "│ "
                        } else {
                            "  "
                        };

                        let color = match node.status.as_str() {
                            "success" => Color::Green,
                            "running" => Color::Blue,
                            "failed" => Color::Red,
                            "pending" | "created" => Color::Yellow,
                            "canceled" => Color::DarkGray,
                            _ => Color::Gray,
                        };

                        let icon = match node.status.as_str() {
                            "success" => "✓",
                            "running" => "●",
                            "failed" => "✗",
                            "pending" | "created" => "○",
                            "canceled" => "⊘",
                            _ => "◇",
                        };

                        let crit_badge = if node.is_critical_path { " [CRIT]" } else { "" };
                        let label = format!("{} {} {}{}", prefix, icon, node.label, crit_badge);
                        let mut style = Style::default().fg(color);
                        if node.is_critical_path && !selected {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        if selected {
                            style = style.add_modifier(Modifier::REVERSED);
                        }

                        buf.set_stringn(x_base, y_cur, &label, col_width as usize, style);
                        y_cur += 1;
                    }
                }
            }
            if y_cur > max_y_drawn {
                max_y_drawn = y_cur;
            }
        }
    }
}
