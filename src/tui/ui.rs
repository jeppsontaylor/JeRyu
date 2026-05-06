//! Owner: Interactive TUI subsystem — rendering logic
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Rendering redacts sensitive text and does not perform control-plane mutations directly.
//! v3: Integrated theme system, VTI badges, and contextual keybindings.
#[path = "ui_chrome.rs"]
mod ui_chrome;
#[path = "ui_panels.rs"]
mod ui_panels;
use super::app::{ActivePane, ActiveTab, App};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
};
use ui_chrome::*;
use ui_panels::*;

pub fn draw(f: &mut Frame, app: &mut App) {
    if app.maximize_logs {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Header + tabs
                Constraint::Min(10),   // Full log view
                Constraint::Length(2), // Footer
            ])
            .split(f.area());

        draw_header_tabs(f, app, chunks[0]);
        draw_logs(f, app, chunks[1]);
        draw_footer(f, app, chunks[2]);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header + tabs
            Constraint::Min(10),   // Content
            Constraint::Length(4), // Event console
            Constraint::Length(2), // Footer
        ])
        .split(f.area());

    draw_header_tabs(f, app, chunks[0]);
    draw_footer(f, app, chunks[3]);

    match app.active_tab {
        ActiveTab::Workflow => {
            let snap = crate::tui::workflow::builder::build_demo_snapshot();
            let theme = crate::tui::theme::Theme::dark();
            crate::tui::workflow::widget::draw_workflow_tab(f, chunks[1], &snap, &theme);
        }
        ActiveTab::Mission => draw_mission_tab(f, app, chunks[1]),
        ActiveTab::Release => draw_release_tab(f, app, chunks[1]),
        ActiveTab::Jobs => draw_jobs_tab(f, app, chunks[1]),
        ActiveTab::Agents => draw_agents_tab(f, app, chunks[1]),
        ActiveTab::Tests => draw_tests_tab(f, app, chunks[1]),
        ActiveTab::Pools => draw_pools_tab(f, app, chunks[1]),
        ActiveTab::Cache => draw_cache_dashboard(f, app, chunks[1]),
        ActiveTab::Evidence => draw_evidence_tab(f, app, chunks[1]),
        ActiveTab::Git => draw_git_tab(f, app, chunks[1]),
        ActiveTab::Secrets => draw_secrets_tab(f, app, chunks[1]),
    }

    draw_event_console(f, app, chunks[2]);

    if app.command_palette_open {
        draw_command_palette(f, app);
    }
    if app.help_overlay_open {
        draw_help_overlay(f, app);
    }
}
