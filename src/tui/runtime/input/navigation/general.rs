use crate::tui::app::App;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) async fn handle(app: &mut App, key: KeyEvent) -> Result<Option<bool>> {
    match key.code {
        KeyCode::Char('k') if key.modifiers == KeyModifiers::CONTROL => {
            app.command_palette_open = true;
            app.command_palette_query.clear();
            app.selected_palette_index = 0;
            Ok(Some(false))
        }
        KeyCode::Char('q') => Ok(Some(true)),
        KeyCode::Esc => {
            if app.maximize_logs {
                app.close_log_view();
                Ok(Some(false))
            } else {
                Ok(Some(true))
            }
        }
        KeyCode::Char('?') => {
            app.help_overlay_open = !app.help_overlay_open;
            Ok(Some(false))
        }
        KeyCode::F(5) => {
            app.force_refresh().await;
            Ok(Some(false))
        }
        KeyCode::Char('p') => {
            app.toggle_pool_paused().await?;
            Ok(Some(false))
        }
        KeyCode::Tab => {
            app.cycle_tab_next();
            Ok(Some(false))
        }
        KeyCode::Right => {
            app.cycle_pane_next();
            Ok(Some(false))
        }
        KeyCode::Left => {
            app.cycle_pane_prev();
            Ok(Some(false))
        }
        KeyCode::Up => {
            if app.maximize_logs {
                app.scroll_logs_up(1);
            } else {
                app.up();
            }
            Ok(Some(false))
        }
        KeyCode::Down => {
            if app.maximize_logs {
                app.scroll_logs_down(1);
            } else {
                app.down();
            }
            Ok(Some(false))
        }
        KeyCode::PageUp if app.maximize_logs => {
            app.scroll_logs_up(20);
            Ok(Some(false))
        }
        KeyCode::PageDown | KeyCode::Char(' ') if app.maximize_logs => {
            app.scroll_logs_down(20);
            Ok(Some(false))
        }
        KeyCode::Char('G') | KeyCode::End if app.maximize_logs => {
            app.follow_logs();
            Ok(Some(false))
        }
        KeyCode::Home if app.maximize_logs => {
            app.jump_logs_top();
            Ok(Some(false))
        }
        _ => Ok(None),
    }
}
