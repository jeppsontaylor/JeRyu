use crate::tui::app::App;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

pub(crate) async fn handle(app: &mut App, key: KeyEvent) -> Result<Option<bool>> {
    match key.code {
        KeyCode::Enter if app.active_tab == crate::tui::app::ActiveTab::Tests => {
            app.fetch_selected_test_history().await;
            Ok(Some(false))
        }
        KeyCode::Char('a') if app.active_tab == crate::tui::app::ActiveTab::Evidence => {
            app.evidence_view_mode = match app.evidence_view_mode {
                crate::tui::app::EvidenceViewMode::Capsules => {
                    crate::tui::app::EvidenceViewMode::AuditLedger
                }
                crate::tui::app::EvidenceViewMode::AuditLedger => {
                    crate::tui::app::EvidenceViewMode::Capsules
                }
            };
            Ok(Some(false))
        }
        KeyCode::Char('v') | KeyCode::Char('t')
            if app.active_tab == crate::tui::app::ActiveTab::Tests =>
        {
            app.toggle_test_view_mode();
            Ok(Some(false))
        }
        KeyCode::Char('j') if app.jankurai_available() => {
            app.active_tab = crate::tui::app::ActiveTab::Jank;
            Ok(Some(false))
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            let index = c.to_digit(10).unwrap() as u8;
            set_tab(app, index)
        }
        _ => Ok(None),
    }
}

fn set_tab(app: &mut App, index: u8) -> Result<Option<bool>> {
    if let Some(tab) = crate::tui::app::ActiveTab::from_number(index) {
        app.active_tab = tab;
    }
    Ok(Some(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[tokio::test]
    async fn j_key_opens_jank_when_available() {
        let mut app = crate::tui::app::test_app().await.expect("test app");
        app.state.jankurai.installed = true;

        let handled = handle(
            &mut app,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        )
        .await
        .expect("navigation");

        assert_eq!(handled, Some(false));
        assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Jank);
    }

    #[tokio::test]
    async fn j_key_is_ignored_when_unavailable() {
        let mut app = crate::tui::app::test_app().await.expect("test app");
        app.state.jankurai.installed = false;

        let handled = handle(
            &mut app,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        )
        .await
        .expect("navigation");

        assert_eq!(handled, None);
        assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Workflow);
    }

    #[tokio::test]
    async fn numeric_tabs_do_not_open_jank() {
        let mut app = crate::tui::app::test_app().await.expect("test app");
        app.state.jankurai.installed = true;

        let handled = handle(
            &mut app,
            KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE),
        )
        .await
        .expect("navigation");

        assert_eq!(handled, Some(false));
        assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Workflow);
        assert!(crate::tui::app::ActiveTab::from_number(10).is_none());
    }
}
