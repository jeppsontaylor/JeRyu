//! Owner: Interactive TUI subsystem — rendering logic
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Rendering redacts sensitive text and does not perform control-plane mutations directly.
//! v3: Integrated theme system, VTI badges, and contextual keybindings.
#[path = "ui_chrome.rs"]
pub(crate) mod ui_chrome;
#[path = "ui_panels.rs"]
mod ui_panels;
use super::app::{ActivePane, App};
pub(crate) use super::app::ActiveTab;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    symbols::Marker,
    widgets::{Axis, Block, Borders, Chart, Clear, Dataset, Gauge, GraphType, List, ListItem, Paragraph, Wrap},
};
use ui_chrome::*;
use ui_panels::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderTab {
    Workflow,
    Mission,
    Release,
    Jobs,
    Agents,
    Tests,
    RunnerGroups,
    Cache,
    Evidence,
    Git,
    Secrets,
    Jank,
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let header = app.chrome_header_state();
    let footer = app.footer_help();
    if app.maximize_logs {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header + tabs
                Constraint::Min(10),   // Full log view
                Constraint::Length(2), // Footer
            ])
            .split(f.area());

        draw_header_tabs(f, &header, chunks[0]);
        draw_logs(f, app, chunks[1]);
        draw_footer(f, footer, chunks[2]);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header + tabs
            Constraint::Min(10),   // Content
            Constraint::Length(4), // Event console
            Constraint::Length(2), // Footer
        ])
        .split(f.area());

    draw_header_tabs(f, &header, chunks[0]);
    draw_footer(f, footer, chunks[3]);

    match app.render_tab() {
        RenderTab::Workflow => {
            let mut snap = crate::tui::workflow::builder::build_demo_snapshot();
            snap.selected_node_id = app.workflow_nav.selected_node_id(&snap).map(|s| s.to_string());
            let theme = crate::tui::theme::Theme::dark();
            crate::tui::workflow::widget::draw_workflow_tab(f, chunks[1], &snap, &theme);
        }
        RenderTab::Mission => draw_mission_tab(f, app, chunks[1]),
        RenderTab::Release => draw_release_tab(f, app, chunks[1]),
        RenderTab::Jobs => draw_jobs_tab(f, app, chunks[1]),
        RenderTab::Agents => draw_agents_tab(f, app, chunks[1]),
        RenderTab::Tests => draw_tests_tab(f, app, chunks[1]),
        RenderTab::RunnerGroups => draw_runner_groups_tab(f, app, chunks[1]),
        RenderTab::Cache => draw_cache_dashboard(f, app, chunks[1]),
        RenderTab::Evidence => draw_evidence_tab(f, app, chunks[1]),
        RenderTab::Git => draw_git_tab(f, app, chunks[1]),
        RenderTab::Secrets => draw_secrets_tab(f, app, chunks[1]),
        RenderTab::Jank => draw_jank_tab(f, app, chunks[1]),
    }

    let events = app.chrome_event_state();
    draw_event_console(f, &events, chunks[2]);

    if app.command_palette_open {
        draw_command_palette(f, app);
    }
    if app.help_overlay_open {
        draw_help_overlay(f, app);
    }
}
