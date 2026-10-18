//! Screen regions recorded while drawing, so mouse events can be mapped back to
//! the widget (and row) under the pointer.

use super::{SettingsItem, Tab};
use ratatui::layout::{Position, Rect};

/// Clickable regions from the last frame. Empty rects mean "not on screen".
#[derive(Debug, Default)]
pub struct HitAreas {
    /// One rect per tab label in the tab bar.
    pub tabs: Vec<(Rect, Tab)>,
    /// Feeds sidebar list (Feeds tab).
    pub sidebar: Rect,
    /// Saved-categories sidebar list (Saved tab).
    pub saved_sidebar: Rect,
    /// Article list panel (also the category preview list).
    pub article_list: Rect,
    /// Article detail / preview panel.
    pub article_detail: Rect,
    /// One rect per selectable settings row.
    pub settings: Vec<(Rect, SettingsItem)>,
    /// Changelog panel.
    pub changelog: Rect,
    /// Feed editor: feeds/tree panel list.
    pub editor_feeds: Rect,
    /// Feed editor: categories panel list.
    pub editor_cats: Rect,
}

impl HitAreas {
    /// Forget everything; called at the start of each frame.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// True when the cell `(col, row)` lies inside `area`.
pub fn hits(area: Rect, col: u16, row: u16) -> bool {
    area.contains(Position::new(col, row))
}

/// Index of the list row under `(col, row)` for a list drawn in `area` and
/// scrolled by `offset` rows. `None` when the point is outside the list.
pub fn list_row(area: Rect, offset: usize, col: u16, row: u16) -> Option<usize> {
    hits(area, col, row).then(|| offset + usize::from(row - area.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_row_accounts_for_offset_and_bounds() {
        let area = Rect::new(2, 5, 10, 4);
        assert_eq!(list_row(area, 0, 2, 5), Some(0));
        assert_eq!(list_row(area, 7, 11, 8), Some(10));
        assert_eq!(list_row(area, 0, 12, 5), None);
        assert_eq!(list_row(area, 0, 2, 9), None);
        assert_eq!(list_row(Rect::default(), 0, 0, 0), None);
    }
}
