//! Mouse text selection: highlighted on screen and captured for copying, in document
//! rows so it scrolls with the text and can extend past the visible part.

use ratatui::{buffer::Buffer, layout::Rect, style::Modifier, text::Span};

use crate::{app::App, models::SelectionView};

/// Highlight the selection if it belongs to `view`, drawn in `area` showing document
/// rows from `scroll` (cells inside `exclude`, e.g. a loading skeleton, stay plain).
/// Returns the selected document rows `(first, last)`; the caller renders them with
/// [`capture`] so the text can be copied.
pub(crate) fn highlight(
    buf: &mut Buffer,
    app: &mut App,
    view: &SelectionView,
    area: Rect,
    scroll: usize,
    exclude: Rect,
) -> Option<(usize, usize)> {
    let sel = app.selection.as_mut().filter(|s| &s.view == view)?;
    app.selection_shown = true;
    sel.bounds = area;
    for y in area.top()..area.bottom() {
        let Some((from, to)) = sel.columns_on(scroll + usize::from(y - area.y)) else {
            continue;
        };
        for x in from..=to.min(area.right().saturating_sub(1)) {
            if !exclude.contains((x, y).into()) {
                buf[(x, y)].modifier.insert(Modifier::REVERSED);
            }
        }
    }
    let ((_, first), (_, last)) = sel.ordered();
    Some((first, last))
}

/// Store the selected text, read from `doc`: the selected document rows rendered at
/// the panel's columns, its row 0 being document row `first`.
pub(crate) fn capture(app: &mut App, doc: &Buffer, first: usize) {
    let Some(sel) = &app.selection else {
        return;
    };
    let area = doc.area;
    let mut rows = Vec::new();
    for y in area.top()..area.bottom() {
        let Some((from, to)) = sel.columns_on(first + usize::from(y - area.y)) else {
            continue;
        };
        let mut text = String::new();
        let mut filler = 0;
        for x in from.max(area.left())..=to.min(area.right().saturating_sub(1)) {
            // A wide grapheme's trailing cells hold filler, not text.
            if filler > 0 {
                filler -= 1;
                continue;
            }
            let symbol = doc[(x, y)].symbol();
            filler = Span::raw(symbol).width().saturating_sub(1);
            text.push_str(symbol);
        }
        rows.push(text.trim_end().to_string());
    }
    app.selection_text = rows.join("\n");
}

/// Number of selected rows from `first` to `last`, capped to what a buffer can hold.
pub(crate) fn row_count(first: usize, last: usize) -> u16 {
    (last - first + 1).min(usize::from(u16::MAX)) as u16
}
