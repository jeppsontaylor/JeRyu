//! Owner: Interactive TUI subsystem - runtime navigation routing
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Navigation stays split into focused key families with bounded side effects.

mod general;
mod jobs;
mod tabs;

use crate::tui::app::App;
use anyhow::Result;
use crossterm::event::KeyEvent;

pub(crate) async fn handle_navigation_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    if let Some(done) = general::handle(app, key).await? {
        return Ok(done);
    }
    if let Some(done) = jobs::handle(app, key).await? {
        return Ok(done);
    }
    if let Some(done) = tabs::handle(app, key).await? {
        return Ok(done);
    }
    Ok(false)
}
