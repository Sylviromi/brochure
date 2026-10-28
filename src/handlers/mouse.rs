//! Mouse support.
//!
//! - Wheel: scrolls whatever panel is under the pointer, without moving its cursor.
//! - Left click: selects tabs and rows; clicking the already-selected row of a
//!   focused list activates it (opens a feed/article, toggles a category).
//!   Links in the article text open in the browser.
//! - Scrollbars: drag the thumb, click the track to page, click arrows to step.
//!   Terminals that report pixel positions get a smooth, sub-cell thumb drag.
//! - Lists (sidebar, article list) select on release: pressing and dragging their
//!   rows scrolls them line by line instead.
//! - Drag (feed editor): press on a feed or category and drag to move it; release
//!   inside the panel drops it, outside cancels. Dragging a category sideways
//!   changes its nesting depth one level per indent step (overshooting the panel
//!   sideways still drops).
//!
//! Actions are routed through the keyboard handlers where possible so both input
//! paths behave identically. Hit regions come from the last drawn frame
//! (see [`crate::models::HitAreas`]).

mod editor_drag;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use super::article::prefetch_article_if_stub;
use crate::{
    app::{App, visible_cat_only_items},
    models::{
        AppEvent, AppState, EditorPanel, FeedEditorMode, FeedTreeItem, ListDrag, PendingClick,
        ScrollTarget, ScrollbarDrag, SelectionView, TextSelection, hits, list_row,
    },
    ui::content::helpers::split_articles,
};

/// Lines scrolled per wheel notch in article text and unfocused lists.
const WHEEL_LINES: i32 = 3;

/// Route one mouse event; `fine_row` is the pointer's fractional row when the
/// terminal reports pixels. Returns `true` when the app should quit (never, today —
/// kept for symmetry with key handling).
pub async fn handle_mouse(
    app: &mut App,
    ev: MouseEvent,
    fine_row: Option<f32>,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    let (col, row) = (ev.column, ev.row);
    if app.zoom.is_some() {
        // The zoomed image covers everything: a click closes it.
        if matches!(ev.kind, MouseEventKind::Down(_)) {
            app.zoom = None;
        }
        return false;
    }
    if scrollbar_mouse(app, ev, fine_row) {
        return false;
    }
    if app.help_open {
        // The keybindings popup is modal: wheel scrolls it, a click closes it.
        match ev.kind {
            MouseEventKind::ScrollDown => app.help_scroll = app.help_scroll.saturating_add(3),
            MouseEventKind::ScrollUp => app.help_scroll = app.help_scroll.saturating_sub(3),
            MouseEventKind::Down(_) => app.help_open = false,
            _ => {}
        }
        return false;
    }
    match ev.kind {
        MouseEventKind::ScrollDown => scroll(app, col, row, KeyCode::Down, tx).await,
        MouseEventKind::ScrollUp => scroll(app, col, row, KeyCode::Up, tx).await,
        MouseEventKind::Down(MouseButton::Left) => click(app, col, row, tx).await,
        MouseEventKind::Drag(MouseButton::Left) => drag(app, col, row, tx).await,
        MouseEventKind::Up(MouseButton::Left) => release(app, col, row, tx).await,
        _ => false,
    }
}

/// Feed a synthetic key press through the normal key handler.
async fn press(app: &mut App, code: KeyCode, tx: &UnboundedSender<AppEvent>) -> bool {
    super::handle_key(app, KeyEvent::new(code, KeyModifiers::NONE), tx).await
}

/// Single-list screens whose rows are recorded in `hit.rows` (see `click_row`).
fn has_row_list(app: &App) -> bool {
    match app.state {
        AppState::ThemeEditor
        | AppState::ThemeEditorNew
        | AppState::ThemeEditorColorEdit
        | AppState::SavedCategoryEditor => true,
        AppState::CategoryPicker => !app.category_picker.new_mode,
        _ => false,
    }
}

/// Picker popups: a click outside them cancels.
fn is_picker_popup(app: &App) -> bool {
    matches!(
        app.state,
        AppState::ThemeEditorNew | AppState::CategoryPicker
    )
}

/// Click on a row of a single-list screen: select it, or activate it (Enter) when
/// it is already selected.
async fn click_row(app: &mut App, idx: usize, tx: &UnboundedSender<AppEvent>) -> bool {
    let cursor = match app.state {
        AppState::ThemeEditor => &mut app.theme_editor.cursor,
        AppState::ThemeEditorNew => &mut app.theme_editor.clone_cursor,
        AppState::ThemeEditorColorEdit => &mut app.theme_editor.color_cursor,
        AppState::CategoryPicker => &mut app.category_picker.cursor,
        AppState::SavedCategoryEditor => {
            // No Enter action here (rename is `r`), so a click only selects.
            app.saved_cat_editor_scroll.set(idx);
            return false;
        }
        _ => return false,
    };
    if *cursor == idx {
        return press(app, KeyCode::Enter, tx).await;
    }
    *cursor = idx;
    false
}

/// States where the main panels are interactive (no popup or text input open).
fn panels_interactive(app: &App) -> bool {
    match app.state {
        AppState::FeedList
        | AppState::ArticleList
        | AppState::ArticleDetail
        | AppState::SavedCategoryList
        | AppState::SettingsList
        | AppState::Changelog => app.feed_editor.delete_cat.is_none(),
        AppState::FeedEditor => {
            app.feed_editor.delete_cat.is_none() && app.feed_editor.mode == FeedEditorMode::Normal
        }
        _ => has_row_list(app),
    }
}

async fn scroll(
    app: &mut App,
    col: u16,
    row: u16,
    arrow: KeyCode,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    let down = arrow == KeyCode::Down;
    let lines = if down { WHEEL_LINES } else { -WHEEL_LINES };
    if app.zen_mode {
        app.scroll_article(lines);
        return false;
    }
    if app.update_available.is_some() {
        return press(app, arrow, tx).await;
    }
    // Move mode in the editor: the wheel moves the drop target like j/k.
    if app.state == AppState::FeedEditor && app.feed_editor.mode != FeedEditorMode::Normal {
        return matches!(app.feed_editor.mode, FeedEditorMode::Moving { .. })
            && press(app, arrow, tx).await;
    }
    if !panels_interactive(app) {
        return false;
    }
    if has_row_list(app) {
        // One list on screen: the wheel moves its cursor wherever the pointer is.
        return press(app, arrow, tx).await;
    }
    let hit = &app.hit;
    // Panels scroll in place without moving their cursor or taking focus, so
    // browsing never changes the selection — only a click does.
    if hits(hit.article_detail, col, row) {
        app.scroll_article(lines);
    } else if let Some(target) = list_under(app, col, row) {
        scroll_list(app, target, lines);
    } else if hits(hit.editor_feeds, col, row) {
        focus_editor_panel(app, EditorPanel::Feeds);
        return press(app, arrow, tx).await;
    } else if hits(hit.editor_cats, col, row) {
        focus_editor_panel(app, EditorPanel::Categories);
        return press(app, arrow, tx).await;
    } else if matches!(app.state, AppState::SettingsList | AppState::Changelog) {
        // Single-panel tabs: scroll anywhere below the tab bar.
        return press(app, arrow, tx).await;
    }
    false
}

/// Press, drag or release on a scrollbar. Returns `true` when the event was consumed.
fn scrollbar_mouse(app: &mut App, ev: MouseEvent, fine_row: Option<f32>) -> bool {
    let mut drag =
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(bar) =
                    app.hit.scrollbars.iter().rev().find(|b| {
                        hits(b.area, ev.column, ev.row) && scrollbar_enabled(app, b.target)
                    })
                else {
                    return false;
                };
                app.selection = None;
                ScrollbarDrag {
                    target: bar.target,
                    interaction: Default::default(),
                    fine_grab: None,
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => match app.scrollbar_drag {
                Some(d) => d,
                None => return false,
            },
            MouseEventKind::Up(MouseButton::Left) => return app.scrollbar_drag.take().is_some(),
            _ => return false,
        };
    // The bar may have gone (view changed mid-drag): keep swallowing the drag.
    if let Some(bar) = app.hit.scrollbars.iter().find(|b| b.target == drag.target) {
        use tui_scrollbar::{SUBCELL, ScrollCommand, ScrollLengths, ScrollMetrics};
        let lengths = ScrollLengths {
            content_len: bar.content_len,
            viewport_len: bar.viewport_len,
        };
        // Track = the bar minus its two arrow cells.
        let metrics = ScrollMetrics::new(lengths, bar.offset, bar.area.height.saturating_sub(2));
        let cells = |subcells: usize| subcells as f32 / SUBCELL as f32;
        let thumb_top = cells(metrics.thumb_start());
        // Pointer position down the track, in fractional cells (pixel-reporting terminals).
        let pos = fine_row.map(|r| r - f32::from(bar.area.y + 1));
        let fine = match (ev.kind, pos, drag.fine_grab) {
            (MouseEventKind::Drag(_), Some(p), Some(grab)) => {
                let travel = cells(metrics.thumb_travel());
                let top = (p - grab).clamp(0.0, travel);
                Some((top / travel * metrics.max_offset() as f32).round() as usize)
            }
            _ => None,
        };
        let widget = crate::ui::scrollbar_widget(bar.content_len, bar.viewport_len, bar.offset);
        let target = drag.target;
        if let Some(offset) = fine {
            set_scroll_offset(app, target, offset);
        } else if let Some(ScrollCommand::SetOffset(offset)) =
            widget.handle_mouse_event(bar.area, ev, &mut drag.interaction)
        {
            set_scroll_offset(app, target, offset);
        } else if let (MouseEventKind::Down(_), Some(p)) = (ev.kind, pos)
            && (thumb_top..thumb_top + cells(metrics.thumb_len())).contains(&p)
        {
            drag.fine_grab = Some(p - thumb_top);
        }
    }
    app.scrollbar_drag = Some(drag);
    true
}

/// Whether the scrollbar of `target` is reachable (not covered by a modal).
fn scrollbar_enabled(app: &App, target: ScrollTarget) -> bool {
    match target {
        ScrollTarget::Help => app.help_open,
        ScrollTarget::UpdatePopup => !app.help_open,
        _ => {
            !app.help_open
                && app.update_available.is_none()
                && (app.zen_mode || panels_interactive(app))
        }
    }
}

/// Scroll `target` so its first visible line is `offset`, leaving cursors alone.
fn set_scroll_offset(app: &mut App, target: ScrollTarget, offset: usize) {
    if let Some((detached, state)) = list_view(app, target) {
        *detached = true;
        *state.offset_mut() = offset;
        return;
    }
    let line = offset.min(u16::MAX as usize) as u16;
    match target {
        ScrollTarget::Article => {
            app.scroll_article(i32::from(line) - i32::from(app.article_scroll_offset))
        }
        ScrollTarget::Changelog => app.changelog_scroll = line,
        ScrollTarget::Help => app.help_scroll = line,
        ScrollTarget::UpdatePopup => app.update_popup_scroll = line,
        ScrollTarget::Sidebar | ScrollTarget::SavedSidebar | ScrollTarget::ArticleList => {}
    }
}

/// Detached flag and viewport of the lists that scroll without moving their cursor.
fn list_view(
    app: &mut App,
    target: ScrollTarget,
) -> Option<(&mut bool, &mut ratatui::widgets::ListState)> {
    match target {
        ScrollTarget::Sidebar => Some((&mut app.detached.sidebar, &mut app.sidebar_list_state)),
        ScrollTarget::SavedSidebar => Some((
            &mut app.detached.saved_sidebar,
            &mut app.saved_sidebar_list_state,
        )),
        ScrollTarget::ArticleList => {
            Some((&mut app.detached.article_list, &mut app.article_list_state))
        }
        _ => None,
    }
}

/// The scroll-in-place list under `(col, row)`, if any.
fn list_under(app: &App, col: u16, row: u16) -> Option<ScrollTarget> {
    let hit = &app.hit;
    [
        (hit.sidebar, ScrollTarget::Sidebar),
        (hit.saved_sidebar, ScrollTarget::SavedSidebar),
        (hit.article_list, ScrollTarget::ArticleList),
    ]
    .into_iter()
    .find_map(|(area, target)| hits(area, col, row).then_some(target))
}

/// Shift a list's viewport by `rows` without moving its cursor (the renderer clamps
/// it to the content).
fn scroll_list(app: &mut App, target: ScrollTarget, rows: i32) {
    if let Some((detached, state)) = list_view(app, target) {
        *detached = true;
        let offset = state.offset() as i64 + i64::from(rows);
        *state.offset_mut() = offset.max(0) as usize;
    }
}

async fn click(app: &mut App, col: u16, row: u16, tx: &UnboundedSender<AppEvent>) -> bool {
    app.selection = None;
    app.link_hints = None;
    if app.update_available.is_some() {
        // Clicking anywhere dismisses the update notice, like Esc.
        return press(app, KeyCode::Esc, tx).await;
    }
    // Popup hint entries and form fields (only drawn while their popup is open).
    if let Some(&(_, key)) = app.hit.buttons.iter().find(|(r, _)| hits(*r, col, row)) {
        return super::handle_key(app, key, tx).await;
    }
    if let Some(&(_, field)) = app.hit.form_fields.iter().find(|(r, _)| hits(*r, col, row)) {
        return focus_form_field(app, field, tx).await;
    }
    let article_interactive = app.zen_mode || panels_interactive(app);
    if article_interactive
        && let Some((_, url)) = app
            .hit
            .article_images
            .iter()
            .find(|(r, _)| hits(*r, col, row))
    {
        open_zoom(app, url.clone(), tx);
        return false;
    }
    // Article text: a press may start a text selection, so its click action (open a
    // link / open the previewed article) waits for a release without dragging.
    if hits(app.hit.article_detail, col, row) && article_interactive {
        let pending = link_at(app, col, row).map(PendingClick::Link).or_else(|| {
            (!app.zen_mode && app.state == AppState::ArticleList)
                .then_some(PendingClick::OpenArticle)
        });
        if let Some(link) = app.selected_article().map(|a| a.link.clone()) {
            let view = SelectionView::Article(link);
            start_selection(app, view, app.hit.article_detail, col, row, pending);
        }
        return false;
    }
    if app.zen_mode || !panels_interactive(app) {
        return false;
    }
    if let Some(&(_, idx)) = app.hit.rows.iter().find(|(r, _)| hits(*r, col, row)) {
        return click_row(app, idx, tx).await;
    }
    if is_picker_popup(app) {
        if !hits(app.hit.popup, col, row) {
            return press(app, KeyCode::Esc, tx).await;
        }
        return false;
    }
    if hits(app.hit.help_hint, col, row) {
        app.help_open = true;
        app.help_scroll = 0;
        return false;
    }
    if hits(app.hit.changelog, col, row) {
        // Acts on release, so dragging over the text selects it instead.
        let line = app.changelog_scroll as usize + usize::from(row - app.hit.changelog.y);
        let entry = app
            .hit
            .changelog_entries
            .iter()
            .position(|&(first, last)| (first..=last).contains(&line));
        let pending = entry.map(PendingClick::ChangelogEntry);
        let view = SelectionView::Changelog;
        start_selection(app, view, app.hit.changelog, col, row, pending);
        return false;
    }

    if let Some(&(_, tab)) = app.hit.tabs.iter().find(|(r, _)| hits(*r, col, row)) {
        if app.state == AppState::FeedEditor {
            app.unselect();
        }
        app.select_tab(tab);
        return false;
    }
    if let Some(target) = list_under(app, col, row) {
        // Acts on release, so dragging scrolls the list instead of selecting.
        app.list_drag = Some(ListDrag {
            target,
            col,
            row,
            last_row: row,
            moved: false,
        });
        return false;
    }
    if let Some(&(_, item)) = app.hit.settings.iter().find(|(r, _)| hits(*r, col, row)) {
        if app.settings_selected == item {
            return press(app, KeyCode::Enter, tx).await;
        }
        app.settings_selected = item;
        return false;
    }
    if let Some(i) = list_row(
        app.hit.editor_feeds,
        app.editor_feeds_list_state.offset(),
        col,
        row,
    ) {
        if matches!(app.editor_items().get(i), Some(FeedTreeItem::Feed { .. })) {
            focus_editor_panel(app, EditorPanel::Feeds);
            app.feed_editor.cursor = i;
            editor_drag::arm(app, EditorPanel::Feeds, col, row, false);
        }
        return false;
    }
    if let Some(i) = list_row(
        app.hit.editor_cats,
        app.editor_cats_list_state.offset(),
        col,
        row,
    ) {
        let cats = visible_cat_only_items(&app.categories, &app.feeds, &app.feed_editor.collapsed);
        if i < cats.len() {
            let reclick =
                app.feed_editor.panel == EditorPanel::Categories && app.feed_editor.cat_cursor == i;
            focus_editor_panel(app, EditorPanel::Categories);
            app.feed_editor.cat_cursor = i;
            // A re-click toggles collapse on release, unless it turns into a drag.
            editor_drag::arm(app, EditorPanel::Categories, col, row, reclick);
        }
    }
    false
}

/// Field index of the open form popup, and whether Esc steps back (rather than
/// closing the popup).
fn form_step(app: &App) -> Option<(usize, bool)> {
    match app.state {
        AppState::FreshRssSetup => Some((app.freshrss_setup.step as usize, true)),
        AppState::AddFeed => Some((app.add_feed.step as usize, false)),
        _ => None,
    }
}

/// Move a form popup to `field` the way the keyboard does: Enter steps forward
/// (validating each field on the way), Esc steps back.
async fn focus_form_field(app: &mut App, field: usize, tx: &UnboundedSender<AppEvent>) -> bool {
    while let Some((step, can_go_back)) = form_step(app)
        && step != field
    {
        if step > field && !can_go_back {
            break;
        }
        let code = if step < field {
            KeyCode::Enter
        } else {
            KeyCode::Esc
        };
        if press(app, code, tx).await {
            return true;
        }
        if form_step(app).map(|(s, _)| s) == Some(step) {
            break; // the field refused (e.g. still empty)
        }
    }
    false
}

// ── Text selection ────────────────────────────────────────────────────────────

/// Begin a (potential) text selection in `view` at screen cell `(col, row)` of `bounds`.
fn start_selection(
    app: &mut App,
    view: SelectionView,
    bounds: Rect,
    col: u16,
    row: u16,
    pending: Option<PendingClick>,
) {
    let at = (col, view_scroll(app, &view) + usize::from(row - bounds.y));
    app.selection = Some(TextSelection {
        view,
        bounds,
        anchor: at,
        cursor: at,
        moved: false,
        pending,
    });
}

/// First document row on screen in `view`.
fn view_scroll(app: &App, view: &SelectionView) -> usize {
    match view {
        SelectionView::Article(_) => usize::from(app.article_scroll_offset),
        SelectionView::Changelog => usize::from(app.changelog_scroll),
    }
}

/// Copy the selected text if a release asked for it. Called after each draw, when
/// `selection_text` matches what is highlighted on screen.
pub fn finish_selection_copy(app: &mut App) {
    if !std::mem::take(&mut app.selection_copy_requested) {
        return;
    }
    let text = app.selection_text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let chars = text.chars().count();
    let msg = match super::article::copy_to_clipboard(&text) {
        None => format!("✓ Copied {chars} characters"),
        Some(_) => "⚠ Copy failed: no clipboard tool available".to_string(),
    };
    app.notify(msg);
}

// ── Drag and release ─────────────────────────────────────────────────────────

/// Pointer moved with the button held: scroll a pressed list, extend a text
/// selection, or move a feed editor row.
async fn drag(app: &mut App, col: u16, row: u16, tx: &UnboundedSender<AppEvent>) -> bool {
    if let Some(d) = app.list_drag.as_mut() {
        // The content follows the pointer: dragging down reveals earlier rows.
        let rows = i32::from(d.last_row) - i32::from(row);
        d.last_row = row;
        d.moved |= rows != 0;
        let target = d.target;
        scroll_list(app, target, rows);
        return false;
    }
    if let Some(sel) = &app.selection {
        let b = sel.bounds;
        let screen_row = row.clamp(b.top(), b.bottom().saturating_sub(1)) - b.y;
        let doc_row = view_scroll(app, &sel.view) + usize::from(screen_row);
        let sel = app.selection.as_mut().expect("checked above");
        sel.cursor = (col.clamp(b.left(), b.right().saturating_sub(1)), doc_row);
        sel.moved |= sel.cursor != sel.anchor;
        return false;
    }
    editor_drag::drag(app, col, row, tx).await
}

/// Button released: click a pressed list row, finish a text selection (or run its
/// pending click), or end a feed editor drag.
async fn release(app: &mut App, col: u16, row: u16, tx: &UnboundedSender<AppEvent>) -> bool {
    if let Some(d) = app.list_drag.take() {
        return !d.moved && click_list(app, d, tx).await;
    }
    if let Some(sel) = app.selection.as_mut() {
        if sel.moved {
            // Copied after the next draw, once `selection_text` reflects the final drag.
            app.selection_copy_requested = true;
            return false;
        }
        let pending = sel.pending.take();
        app.selection = None;
        return match pending {
            Some(PendingClick::Link(url)) => {
                open_link(app, &url);
                false
            }
            Some(PendingClick::OpenArticle) => press(app, KeyCode::Enter, tx).await,
            Some(PendingClick::ChangelogEntry(i)) => {
                if app.changelog_cursor == i {
                    // Enter toggles the selected release's collapse state.
                    return press(app, KeyCode::Enter, tx).await;
                }
                app.changelog_cursor = i;
                false
            }
            None => false,
        };
    }
    editor_drag::release(app, col, row, tx).await
}

/// URL of the article link under `(col, row)`, if any (reader and zen mode only).
fn link_at(app: &App, col: u16, row: u16) -> Option<String> {
    let reading = app.zen_mode || app.state == AppState::ArticleDetail;
    if !reading || app.update_available.is_some() {
        return None;
    }
    app.hit
        .article_links
        .iter()
        .find(|(r, _)| hits(*r, col, row))
        .map(|(_, url)| url.clone())
}

/// Show image `url` full screen, downloading it again at full resolution.
pub(super) fn open_zoom(app: &mut App, url: String, tx: &UnboundedSender<AppEvent>) {
    crate::events::spawn_image_fetch(url.clone(), tx, AppEvent::ZoomImageDecoded);
    app.zoom = Some(crate::app::ImageZoom {
        url,
        image: None,
        failed: false,
    });
}

/// Open `url` in the default browser and report it in a toast.
pub(super) fn open_link(app: &mut App, url: &str) {
    match open::that(url) {
        Ok(()) => app.notify(format!("Opened: {url}")),
        Err(e) => app.notify(format!("⚠ Could not open link: {e}")),
    }
}

/// A press on a list released without dragging: click the row that was pressed.
async fn click_list(app: &mut App, d: ListDrag, tx: &UnboundedSender<AppEvent>) -> bool {
    let (col, row) = (d.col, d.row);
    match d.target {
        ScrollTarget::Sidebar => {
            match list_row(app.hit.sidebar, app.sidebar_list_state.offset(), col, row) {
                Some(i) => click_sidebar(app, i, tx).await,
                None => false,
            }
        }
        ScrollTarget::SavedSidebar => match list_row(
            app.hit.saved_sidebar,
            app.saved_sidebar_list_state.offset(),
            col,
            row,
        ) {
            Some(v) => click_saved_sidebar(app, v, tx).await,
            None => false,
        },
        ScrollTarget::ArticleList => match list_row(
            app.hit.article_list,
            app.article_list_state.offset(),
            col,
            row,
        ) {
            Some(v) => click_article_row(app, v, tx).await,
            None => false,
        },
        _ => false,
    }
}

/// Click on sidebar row `i`: select it, or activate it when it is already selected
/// and the sidebar has focus (categories toggle, feeds / All Feeds open).
async fn click_sidebar(app: &mut App, i: usize, tx: &UnboundedSender<AppEvent>) -> bool {
    let Some(item) = app.sidebar_items().get(i).cloned() else {
        return false;
    };
    let had_focus = app.state == AppState::FeedList;
    focus_sidebar(app);
    if i != app.sidebar_cursor {
        app.set_sidebar_cursor(i);
        return false;
    }
    if !had_focus {
        return false;
    }
    let code = match item {
        FeedTreeItem::Category { .. } => KeyCode::Char(' '),
        _ => KeyCode::Enter,
    };
    press(app, code, tx).await
}

/// Click on visual row `v` of the saved sidebar ("All Saved", separator, categories).
async fn click_saved_sidebar(app: &mut App, v: usize, tx: &UnboundedSender<AppEvent>) -> bool {
    let entry = match v {
        0 => 0,
        1 => return false, // separator
        n => n - 1,
    };
    if entry > app.user_data.saved_categories.len() {
        return false;
    }
    let had_focus = app.state == AppState::SavedCategoryList;
    focus_sidebar(app);
    if entry != app.saved_sidebar_cursor || !had_focus {
        app.set_saved_cursor(entry);
        return false;
    }
    press(app, KeyCode::Enter, tx).await
}

/// Click on visual row `v` of the article list: select the article, or open it when
/// it is already selected and the list has focus.
async fn click_article_row(app: &mut App, v: usize, tx: &UnboundedSender<AppEvent>) -> bool {
    let had_focus = app.state == AppState::ArticleList;
    if !focus_article_list(app) {
        return false;
    }
    let Some(idx) = article_at_row(app, v) else {
        return false;
    };
    if had_focus && idx == app.selected_article {
        return press(app, KeyCode::Enter, tx).await;
    }
    app.set_article_cursor(idx);
    prefetch_article_if_stub(app, tx);
    false
}

/// Map a visual article-list row to an article index. Feed and saved lists show
/// current articles, then an "Archived" separator row, then archived ones.
fn article_at_row(app: &App, v: usize) -> Option<usize> {
    if app.in_category_context {
        return (v < app.category_view_articles.len()).then_some(v);
    }
    let articles = if app.in_saved_context {
        app.saved_view_articles.as_slice()
    } else {
        app.feeds.get(app.selected_feed)?.articles.as_slice()
    };
    let (current, archived, _) = split_articles(articles);
    match v.checked_sub(current.len()) {
        None => current.get(v).copied(),
        Some(0) => None, // separator
        Some(k) => archived.get(k - 1).copied(),
    }
}

/// Leave the article views so the sidebar has focus.
fn focus_sidebar(app: &mut App) {
    if app.state == AppState::ArticleDetail {
        app.unselect();
    }
    if app.state == AppState::ArticleList {
        app.unselect();
    }
}

/// Give the article list focus (entering it from the sidebar if needed).
/// Returns false when there is no list to focus (e.g. empty feed).
fn focus_article_list(app: &mut App) -> bool {
    match app.state {
        AppState::FeedList | AppState::SavedCategoryList => {
            app.select();
            app.state == AppState::ArticleList
        }
        AppState::ArticleDetail => {
            app.state = AppState::ArticleList;
            true
        }
        AppState::ArticleList => true,
        _ => false,
    }
}

/// Switch editor focus to `panel`, mirroring what Tab does when leaving Categories.
fn focus_editor_panel(app: &mut App, panel: EditorPanel) {
    if app.feed_editor.panel != panel {
        if panel == EditorPanel::Feeds {
            app.clamp_editor_cursor_to_feed();
        }
        app.feed_editor.panel = panel;
    }
}

#[cfg(test)]
mod tests;
