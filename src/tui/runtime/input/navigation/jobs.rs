use crate::tui::app::App;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

pub(crate) async fn handle(app: &mut App, key: KeyEvent) -> Result<Option<bool>> {
    match key.code {
        KeyCode::Enter if app.active_pane == crate::tui::app::ActivePane::Jobs => {
            app.open_selected_job_log();
            Ok(Some(false))
        }
        KeyCode::Char('f') if app.active_tab == crate::tui::app::ActiveTab::Jobs => {
            app.feed_toggle_pin();
            Ok(Some(false))
        }
        KeyCode::Char('n') if app.active_tab == crate::tui::app::ActiveTab::Jobs => {
            app.feed_next();
            Ok(Some(false))
        }
        KeyCode::Char('N') if app.active_tab == crate::tui::app::ActiveTab::Jobs => {
            app.feed_prev();
            Ok(Some(false))
        }
        KeyCode::Char('g')
            if app.active_tab == crate::tui::app::ActiveTab::Jobs && !app.maximize_logs =>
        {
            app.feed_follow_toggle();
            Ok(Some(false))
        }
        KeyCode::Char('c') if app.active_tab == crate::tui::app::ActiveTab::Jobs => {
            app.cancel_selected_job().await?;
            Ok(Some(false))
        }
        KeyCode::Char('d') if app.active_tab == crate::tui::app::ActiveTab::Jobs => {
            app.remove_selected_item().await?;
            Ok(Some(false))
        }
        KeyCode::Char('r') if app.active_tab == crate::tui::app::ActiveTab::Jobs => {
            app.requeue_selected_job().await?;
            Ok(Some(false))
        }
        _ => Ok(None),
    }
}
