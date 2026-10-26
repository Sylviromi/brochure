//! Screen regions recorded while drawing, so mouse events can be mapped back to
//! the widget (and row) under the pointer.

use super::{SettingsItem, Tab};
use ratatui::layout::{Position, Rect};

/// Clickable regions from the last frame. Empty rects mean "not on screen".
#[derive(Debug, Default)]
pub struct HitAreas {
    /// One rect per tab label in the tab bar.
    pub tabs: Vec<(Rect, Tab)>,
    /// The `[?] keys` hint in the tab bar.
    pub help_hint: Rect,
    /// Feeds sidebar list (Feeds tab).
    pub sidebar: Rect,
    /// Saved-categories sidebar list (Saved tab).
    pub saved_sidebar: Rect,
    /// Article list panel (also the category preview list).
    pub article_list: Rect,
    /// Article detail / preview panel.
    pub article_detail: Rect,
    /// Links drawn in the article text, with their URLs.
    pub article_links: Vec<(Rect, String)>,
    /// Images drawn in the article, with their URLs (a click zooms them).
    pub article_images: Vec<(Rect, String)>,
    /// One rect per selectable settings row.
    pub settings: Vec<(Rect, SettingsItem)>,
    /// Changelog panel.
    pub changelog: Rect,
    /// `(first_line, last_line)` of each changelog entry, before scrolling.
    pub changelog_entries: Vec<(usize, usize)>,
    /// Feed editor: feeds/tree panel list.
    pub editor_feeds: Rect,
    /// Feed editor: categories panel list.
    pub editor_cats: Rect,
    /// Rows of the active list on screens with a single list (themes, colour slots,
    /// saved-category manager, pickers): `(row rect, cursor index it selects)`.
    pub rows: Vec<(Rect, usize)>,
    /// Outline of the open picker popup (clicks outside it cancel).
    pub popup: Rect,
    /// Scrollbars drawn this frame, in draw order.
    pub scrollbars: Vec<ScrollbarHit>,
}

/// View a scrollbar scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    Sidebar,
    SavedSidebar,
    ArticleList,
    Article,
    Changelog,
    Help,
    UpdatePopup,
}

/// A drawn scrollbar: where it is and the lengths it was drawn with.
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarHit {
    pub area: Rect,
    pub target: ScrollTarget,
    pub content_len: usize,
    pub viewport_len: usize,
    pub offset: usize,
}

/// Scrollbar held by the mouse.
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarDrag {
    pub target: ScrollTarget,
    pub interaction: tui_scrollbar::ScrollBarInteraction,
    /// Where the thumb was grabbed, in cells below its top edge, when the terminal
    /// reports pixel positions (the drag then follows the pointer smoothly).
    pub fine_grab: Option<f32>,
}

/// A press on a list's rows: dragging scrolls the list with the pointer, line by
/// line; releasing without moving selects the row that was pressed.
#[derive(Debug, Clone, Copy)]
pub struct ListDrag {
    pub target: ScrollTarget,
    pub col: u16,
    pub row: u16,
    /// Pointer row as of the last drag event.
    pub last_row: u16,
    pub moved: bool,
}

impl HitAreas {
    /// Forget everything; called at the start of each frame.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Action deferred from a press to its release, so a press that turns into a text
/// drag doesn't trigger it.
#[derive(Debug, Clone, PartialEq)]
pub enum PendingClick {
    /// Open this link in the browser.
    Link(String),
    /// Open the previewed article (press landed on the preview pane).
    OpenArticle,
    /// Select this changelog entry, or toggle it if already selected.
    ChangelogEntry(usize),
}

/// View a text selection belongs to.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionView {
    /// The article with this link (detail, preview or zen).
    Article(String),
    Changelog,
}

/// A mouse text selection in document rows, so it scrolls with the text. Columns are
/// screen columns within the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSelection {
    pub view: SelectionView,
    /// Panel the selection is clipped to, as of the last frame.
    pub bounds: Rect,
    /// `(col, row)` where the button went down.
    pub anchor: (u16, usize),
    /// `(col, row)` under the pointer now.
    pub cursor: (u16, usize),
    /// True once the pointer moved (a plain click selects nothing).
    pub moved: bool,
    /// What a plain click (no drag) at the anchor should do on release.
    pub pending: Option<PendingClick>,
}

impl TextSelection {
    /// `(start, end)` in reading order as `(col, row)` pairs.
    pub fn ordered(&self) -> ((u16, usize), (u16, usize)) {
        let (a, b) = (self.anchor, self.cursor);
        if (a.1, a.0) <= (b.1, b.0) {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// Column span `[from, to]` selected on document row `row`, if any.
    pub fn columns_on(&self, row: usize) -> Option<(u16, u16)> {
        let ((c0, r0), (c1, r1)) = self.ordered();
        if !self.moved || row < r0 || row > r1 {
            return None;
        }
        let from = if row == r0 { c0 } else { self.bounds.left() };
        let to = if row == r1 {
            c1
        } else {
            self.bounds.right().saturating_sub(1)
        };
        (from <= to).then_some((from, to))
    }
}

/// Lists whose viewport was wheel-scrolled away from their cursor (the panel wasn't
/// focused). Cleared by the next key press or selection change.
#[derive(Debug, Default, Clone, Copy)]
pub struct DetachedViews {
    pub sidebar: bool,
    pub saved_sidebar: bool,
    pub article_list: bool,
}

impl HitAreas {
    /// Record a row rect for each visible row of a list drawn in `area` scrolled by
    /// `offset`; `target` maps a list index to the cursor index it selects (`None`
    /// for headers and separators).
    pub fn record_rows(
        &mut self,
        area: Rect,
        offset: usize,
        len: usize,
        target: impl Fn(usize) -> Option<usize>,
    ) {
        for (y, idx) in (area.top()..area.bottom()).zip(offset..len) {
            if let Some(t) = target(idx) {
                self.rows.push((Rect::new(area.x, y, area.width, 1), t));
            }
        }
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
