//! Key event handling for the Changelog/About tab.

use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key input while viewing the changelog and about screen.
pub(super) fn handle_changelog(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Tab => app.switch_tab_right(),
        KeyCode::BackTab => app.switch_tab_left(),
        KeyCode::Char('j') | KeyCode::Down => {
            app.changelog_scroll = app.changelog_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.changelog_scroll = app.changelog_scroll.saturating_sub(1);
        }
        _ => {}
    }
    false
}
