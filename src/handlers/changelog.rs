//! Key event handling for the Changelog/About tab.

use crate::app::App;
use crate::storage::save_user_data;
use crate::ui::changelog_entry_count;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key input while viewing the changelog and about screen.
pub(super) fn handle_changelog(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Tab => app.switch_tab_right(),
        KeyCode::BackTab => app.switch_tab_left(),
        KeyCode::Char('j') | KeyCode::Down => {
            let total = changelog_entry_count();
            if app.changelog_cursor + 1 < total {
                app.changelog_cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up if app.changelog_cursor > 0 => {
            app.changelog_cursor -= 1;
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            if app.changelog_collapsed.contains(&app.changelog_cursor) {
                app.changelog_collapsed.remove(&app.changelog_cursor);
            } else {
                app.changelog_collapsed.insert(app.changelog_cursor);
            }
            app.user_data.changelog_collapsed = app.changelog_collapsed.iter().copied().collect();
            let _ = save_user_data(&app.user_data);
        }
        _ => {}
    }
    false
}
