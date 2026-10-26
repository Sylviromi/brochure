//! Feed editor drag-and-drop: press on a feed or category and drag to move it;
//! release inside the panel drops it, outside cancels.

use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use super::press;
use crate::{
    app::{App, visible_cat_only_items},
    models::{AppEvent, AppState, EditorPanel, FeedEditorMode, FeedTreeItem, hits},
    state::EditorDrag,
};

/// Remember a press on an editor row so a following drag can move it.
pub(super) fn arm(app: &mut App, panel: EditorPanel, col: u16, row: u16, reclick: bool) {
    app.feed_editor.drag = Some(EditorDrag {
        panel,
        start_col: col,
        start_row: row,
        active: false,
        reclick,
    });
}

/// Enter move mode on the first motion, then keep the drop target under the pointer.
pub(super) async fn drag(
    app: &mut App,
    col: u16,
    row: u16,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    let Some(mut d) = app.feed_editor.drag else {
        return false;
    };
    if app.state != AppState::FeedEditor {
        app.feed_editor.drag = None;
        return false;
    }
    if !d.active {
        if (col, row) == (d.start_col, d.start_row) {
            return false;
        }
        // Space starts a move of the selected row, exactly like the keyboard.
        if press(app, KeyCode::Char(' '), tx).await {
            return true;
        }
        if !matches!(app.feed_editor.mode, FeedEditorMode::Moving { .. }) {
            app.feed_editor.drag = None;
            return false;
        }
        d.active = true;
        app.feed_editor.drag = Some(d);
    }
    drag_target_to(app, d, col, row);
    false
}

/// Drop (inside the panel) or cancel (outside); a plain re-click on a category
/// toggles it.
pub(super) async fn release(
    app: &mut App,
    col: u16,
    row: u16,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    let Some(d) = app.feed_editor.drag.take() else {
        return false;
    };
    if app.state != AppState::FeedEditor {
        return false;
    }
    if d.active {
        let area = match d.panel {
            EditorPanel::Feeds => app.hit.editor_feeds,
            EditorPanel::Categories => app.hit.editor_cats,
        };
        // A sideways category drag (changing nesting depth) may overshoot the panel
        // edge, so for categories only leaving it above or below cancels.
        let drops = match d.panel {
            EditorPanel::Feeds => hits(area, col, row),
            EditorPanel::Categories => (area.top()..area.bottom()).contains(&row),
        };
        let code = if drops {
            KeyCode::Char(' ')
        } else {
            KeyCode::Esc
        };
        return press(app, code, tx).await;
    }
    if d.reclick {
        // Enter toggles the category's collapse state.
        return press(app, KeyCode::Enter, tx).await;
    }
    false
}

/// Model index for display row `r` while a move preview row sits at `preview_at`
/// (the preview row itself maps to `cursor`, rows after it shift up by one).
pub(super) fn unpreview(r: usize, cursor: usize, preview_at: Option<usize>) -> usize {
    match preview_at {
        Some(p) if r == p => cursor,
        Some(p) if r > p => r - 1,
        _ => r,
    }
}

/// Display row under the pointer in `area` (ignoring the column so a drag may
/// wander sideways); rows above/below clamp to the edges.
fn drag_row(area: Rect, offset: usize, row: u16) -> Option<usize> {
    if area.height == 0 {
        return None;
    }
    let row = row.clamp(area.top(), area.bottom() - 1);
    Some(offset + usize::from(row - area.y))
}

/// Point the move target at the row under the pointer.
fn drag_target_to(app: &mut App, d: EditorDrag, col: u16, row: u16) {
    let FeedEditorMode::Moving {
        origin_render_idx,
        original_cursor,
        ..
    } = app.feed_editor.mode
    else {
        return;
    };
    match d.panel {
        EditorPanel::Feeds => {
            let Some(r) = drag_row(
                app.hit.editor_feeds,
                app.editor_feeds_list_state.offset(),
                row,
            ) else {
                return;
            };
            let cursor = app.feed_editor.cursor;
            let preview_at = (cursor != origin_render_idx).then_some(cursor + 1);
            let len = app.editor_items().len();
            app.feed_editor.cursor = unpreview(r, cursor, preview_at).min(len.saturating_sub(1));
        }
        EditorPanel::Categories => {
            let Some(r) = drag_row(
                app.hit.editor_cats,
                app.editor_cats_list_state.offset(),
                row,
            ) else {
                return;
            };
            // The panel shows the post-drop preview: map the row under the pointer
            // back to that category's row in the real tree.
            let (shown, moving_id) = app.editor_category_rows();
            let cats =
                visible_cat_only_items(&app.categories, &app.feeds, &app.feed_editor.collapsed);
            let target_id = match shown.get(r) {
                Some(FeedTreeItem::Category { id, .. }) => Some(*id),
                _ => None,
            };
            if target_id != moving_id {
                // Below the last row: one slot past the end drops last at the root level.
                app.feed_editor.cat_cursor = target_id
                    .and_then(|t| {
                        cats.iter().position(
                            |item| matches!(item, FeedTreeItem::Category { id, .. } if *id == t),
                        )
                    })
                    .unwrap_or(cats.len());
            }
            // Each indent step (2 columns) sideways is one nesting level; dragging
            // left climbs ancestors all the way to the root level.
            let dx = i32::from(col) - i32::from(d.start_col);
            let min = -i32::from(app.editor_cat_cursor_depth());
            let depth_delta = (dx / 2).clamp(min, 1) as i8;
            app.feed_editor.mode = FeedEditorMode::Moving {
                origin_render_idx,
                original_cursor,
                depth_delta,
            };
        }
    }
}
