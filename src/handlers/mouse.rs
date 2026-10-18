//! Mouse support.
//!
//! - Wheel: scrolls whatever panel is under the pointer (moving focus to it).
//! - Left click: selects tabs and rows; clicking the already-selected row of a
//!   focused list activates it (opens a feed/article, toggles a category).
//!   Links in the article text open in the browser.
//! - Drag (feed editor): press on a feed or category and drag to move it; release
//!   inside the panel drops it, outside cancels. Dragging a category sideways
//!   changes its nesting depth.
//!
//! Actions are routed through the keyboard handlers where possible so both input
//! paths behave identically. Hit regions come from the last drawn frame
//! (see [`crate::models::HitAreas`]).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use super::article::prefetch_article_if_stub;
use crate::{
    app::{App, visible_cat_only_items},
    models::{AppEvent, AppState, EditorPanel, FeedEditorMode, FeedTreeItem, hits, list_row},
    state::EditorDrag,
    ui::content::helpers::split_articles,
};

/// Lines scrolled per wheel notch in article text.
const WHEEL_LINES: usize = 3;

/// Route one mouse event. Returns `true` when the app should quit (never, today —
/// kept for symmetry with key handling).
pub async fn handle_mouse(app: &mut App, ev: MouseEvent, tx: &UnboundedSender<AppEvent>) -> bool {
    let (col, row) = (ev.column, ev.row);
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

async fn press_n(app: &mut App, code: KeyCode, n: usize, tx: &UnboundedSender<AppEvent>) -> bool {
    for _ in 0..n {
        if press(app, code, tx).await {
            return true;
        }
    }
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
        _ => false,
    }
}

async fn scroll(
    app: &mut App,
    col: u16,
    row: u16,
    arrow: KeyCode,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    // Full-screen overlays take the wheel wherever the pointer is.
    if app.zen_mode {
        return press_n(app, arrow, WHEEL_LINES, tx).await;
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
    let hit = &app.hit;
    if hits(hit.article_detail, col, row) {
        if app.state == AppState::ArticleDetail {
            return press_n(app, arrow, WHEEL_LINES, tx).await;
        }
    } else if hits(hit.article_list, col, row) {
        if focus_article_list(app) {
            return press(app, arrow, tx).await;
        }
    } else if hits(hit.sidebar, col, row) || hits(hit.saved_sidebar, col, row) {
        focus_sidebar(app);
        return press(app, arrow, tx).await;
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

async fn click(app: &mut App, col: u16, row: u16, tx: &UnboundedSender<AppEvent>) -> bool {
    if let Some(url) = link_at(app, col, row) {
        open_link(app, &url);
        return false;
    }
    if app.zen_mode {
        return false;
    }
    if app.update_available.is_some() {
        // Clicking anywhere dismisses the update notice, like Esc.
        return press(app, KeyCode::Esc, tx).await;
    }
    if !panels_interactive(app) {
        return false;
    }

    if let Some(&(_, tab)) = app.hit.tabs.iter().find(|(r, _)| hits(*r, col, row)) {
        if app.state == AppState::FeedEditor {
            app.unselect();
        }
        app.select_tab(tab);
        return false;
    }
    if let Some(i) = list_row(app.hit.sidebar, app.sidebar_list_state.offset(), col, row) {
        return click_sidebar(app, i, tx).await;
    }
    if let Some(v) = list_row(
        app.hit.saved_sidebar,
        app.saved_sidebar_list_state.offset(),
        col,
        row,
    ) {
        return click_saved_sidebar(app, v, tx).await;
    }
    if let Some(v) = list_row(
        app.hit.article_list,
        app.article_list_state.offset(),
        col,
        row,
    ) {
        return click_article_row(app, v, tx).await;
    }
    if hits(app.hit.article_detail, col, row) {
        // Clicking the preview opens the article, like Enter on the list.
        if app.state == AppState::ArticleList {
            return press(app, KeyCode::Enter, tx).await;
        }
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
            arm_drag(app, EditorPanel::Feeds, col, row, false);
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
            arm_drag(app, EditorPanel::Categories, col, row, reclick);
        }
    }
    false
}

// ── Feed editor drag-and-drop ────────────────────────────────────────────────

/// Remember a press on an editor row so a following drag can move it.
fn arm_drag(app: &mut App, panel: EditorPanel, col: u16, row: u16, reclick: bool) {
    app.feed_editor.drag = Some(EditorDrag {
        panel,
        start_col: col,
        start_row: row,
        active: false,
        reclick,
    });
}

/// Pointer moved with the button held: enter move mode on the first motion, then
/// keep the drop target under the pointer.
async fn drag(app: &mut App, col: u16, row: u16, tx: &UnboundedSender<AppEvent>) -> bool {
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

/// Button released: drop (inside the panel) or cancel (outside); a plain re-click
/// on a category toggles it.
async fn release(app: &mut App, col: u16, row: u16, tx: &UnboundedSender<AppEvent>) -> bool {
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
        let code = if hits(area, col, row) {
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
fn unpreview(r: usize, cursor: usize, preview_at: Option<usize>) -> usize {
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
            let cats =
                visible_cat_only_items(&app.categories, &app.feeds, &app.feed_editor.collapsed)
                    .len();
            let cursor = app.feed_editor.cat_cursor;
            let preview_at = (cursor < cats).then_some(cursor + 1);
            // One slot past the last category drops at the root level.
            app.feed_editor.cat_cursor = unpreview(r, cursor, preview_at).min(cats);
            let dx = i32::from(col) - i32::from(d.start_col);
            let depth_delta = match dx {
                3.. => 1,
                ..=-3 => -1,
                _ => 0,
            };
            app.feed_editor.mode = FeedEditorMode::Moving {
                origin_render_idx,
                original_cursor,
                depth_delta,
            };
        }
    }
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

/// Open `url` in the default browser and report it in the status bar.
fn open_link(app: &mut App, url: &str) {
    match open::that(url) {
        Ok(()) => app.set_status(format!("Opened: {url}")),
        Err(e) => app.set_status(format!("⚠ Could not open link: {e}")),
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
mod tests {
    use super::*;
    use crate::models::{SettingsItem, Tab};
    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    fn draw(app: &mut App) {
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn left_click(column: u16, row: u16) -> MouseEvent {
        mouse(MouseEventKind::Down(MouseButton::Left), column, row)
    }

    #[tokio::test]
    async fn clicking_tabs_switches_tabs() {
        let mut app = App::new();
        app.update_available = None;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        draw(&mut app);
        let (rect, _) = *app
            .hit
            .tabs
            .iter()
            .find(|(_, t)| *t == Tab::Settings)
            .expect("settings tab recorded");
        handle_mouse(&mut app, left_click(rect.x + 1, rect.y), &tx).await;
        assert_eq!(app.selected_tab, Tab::Settings);
        assert_eq!(app.state, AppState::SettingsList);
    }

    #[tokio::test]
    async fn clicking_settings_rows_selects_then_wheel_moves() {
        let mut app = App::new();
        app.update_available = None;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        app.select_tab(Tab::Settings);
        draw(&mut app);
        let (rect, _) = *app
            .hit
            .settings
            .iter()
            .find(|(_, item)| *item == SettingsItem::ScrollLoop)
            .expect("settings rows recorded");
        handle_mouse(&mut app, left_click(rect.x + 2, rect.y), &tx).await;
        assert_eq!(app.settings_selected, SettingsItem::ScrollLoop);
        handle_mouse(
            &mut app,
            mouse(MouseEventKind::ScrollDown, rect.x + 2, rect.y),
            &tx,
        )
        .await;
        assert_eq!(app.settings_selected, SettingsItem::ScrollLoop.next());
    }

    /// Replace loaded data with two uncategorised feeds of three long articles each.
    /// Nothing here persists: none of the exercised actions save to disk.
    fn with_test_feeds(app: &mut App) {
        use crate::models::{Article, Feed};
        let article = |n: usize| Article {
            title: format!("Article {n}"),
            description: String::new(),
            link: format!("https://example.test/{n}"),
            is_read: false,
            is_saved: false,
            // Long enough that selecting it does not trigger a readability fetch.
            content: "x".repeat(crate::models::CONTENT_STUB_MAX_LEN),
            images: vec![],
            source_feed: String::new(),
            remote_id: None,
            published_secs: Some(1_000 - n as i64),
            is_archived: false,
            error: None,
        };
        app.categories.clear();
        app.sidebar_collapsed.clear();
        app.feeds = (0..2)
            .map(|f| {
                let mut feed = Feed::new_local(format!("Feed {f}"), format!("f{f}"), None, f);
                feed.articles = (0..3).map(article).collect();
                feed.fetched = true;
                feed
            })
            .collect();
        app.state = AppState::FeedList;
        app.selected_tab = Tab::Feeds;
        app.set_sidebar_cursor(0);
    }

    #[tokio::test]
    async fn sidebar_and_article_clicks_select_then_open() {
        let mut app = App::new();
        app.update_available = None;
        with_test_feeds(&mut app);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        draw(&mut app);

        // Row 2 of the sidebar is "Feed 1" (row 0 = All Feeds).
        let sidebar = app.hit.sidebar;
        handle_mouse(&mut app, left_click(sidebar.x + 2, sidebar.y + 2), &tx).await;
        assert_eq!(app.sidebar_cursor, 2);
        assert_eq!(app.selected_feed, 1);
        assert_eq!(app.state, AppState::FeedList);

        // Clicking the selected feed again opens its article list.
        handle_mouse(&mut app, left_click(sidebar.x + 2, sidebar.y + 2), &tx).await;
        assert_eq!(app.state, AppState::ArticleList);

        draw(&mut app);
        let list = app.hit.article_list;
        handle_mouse(&mut app, left_click(list.x + 2, list.y + 2), &tx).await;
        assert_eq!(app.selected_article, 2);
        handle_mouse(
            &mut app,
            mouse(MouseEventKind::ScrollUp, list.x + 2, list.y),
            &tx,
        )
        .await;
        assert_eq!(app.selected_article, 1);

        // Wheel over the sidebar hands focus back to it and moves its cursor.
        draw(&mut app);
        let sidebar = app.hit.sidebar;
        handle_mouse(
            &mut app,
            mouse(MouseEventKind::ScrollUp, sidebar.x + 2, sidebar.y),
            &tx,
        )
        .await;
        assert_eq!(app.state, AppState::FeedList);
        assert_eq!(app.sidebar_cursor, 1);
    }

    #[tokio::test]
    async fn article_render_is_cached_and_links_are_clickable() {
        let mut app = App::new();
        app.update_available = None;
        with_test_feeds(&mut app);
        app.feeds[0].articles[0].content = format!(
            "Read [the docs](https://docs.test) now.\n\n{}",
            "x".repeat(crate::models::CONTENT_STUB_MAX_LEN)
        );
        app.selected_feed = 0;
        app.selected_article = 0;
        app.clear_category_view();
        app.state = AppState::ArticleDetail;

        draw(&mut app);
        let key = app.article_render.as_ref().unwrap().key();
        draw(&mut app);
        assert_eq!(
            app.article_render.as_ref().unwrap().key(),
            key,
            "re-rendered"
        );
        app.cycle_alignment_next();
        draw(&mut app);
        assert_ne!(
            app.article_render.as_ref().unwrap().key(),
            key,
            "stale cache"
        );

        let (rect, url) = app.hit.article_links.first().cloned().expect("link region");
        assert_eq!(url, "https://docs.test");
        assert_eq!(
            link_at(&app, rect.x, rect.y).as_deref(),
            Some("https://docs.test")
        );
        assert_eq!(link_at(&app, rect.x, rect.y.saturating_sub(1)), None);
    }

    #[test]
    fn unpreview_skips_the_preview_row() {
        // Cursor on row 2 with the preview inserted at row 3.
        assert_eq!(unpreview(1, 2, Some(3)), 1);
        assert_eq!(unpreview(3, 2, Some(3)), 2);
        assert_eq!(unpreview(5, 2, Some(3)), 4);
        assert_eq!(unpreview(5, 2, None), 5);
    }

    /// Drag a feed onto another row, then release outside the panel: move mode
    /// follows the pointer and the release cancels (nothing is saved).
    #[tokio::test]
    async fn dragging_a_feed_moves_the_target_and_cancels_outside() {
        let mut app = App::new();
        app.update_available = None;
        with_test_feeds(&mut app);
        app.state = AppState::FeedEditor;
        app.feed_editor.panel = EditorPanel::Feeds;
        app.feed_editor.cursor = 0;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        draw(&mut app);
        let area = app.hit.editor_feeds;

        handle_mouse(&mut app, left_click(area.x + 4, area.y), &tx).await;
        assert!(app.feed_editor.drag.is_some());
        let drag_to = |row| mouse(MouseEventKind::Drag(MouseButton::Left), area.x + 4, row);
        handle_mouse(&mut app, drag_to(area.y + 1), &tx).await;
        assert!(matches!(
            app.feed_editor.mode,
            FeedEditorMode::Moving {
                origin_render_idx: 0,
                ..
            }
        ));
        assert_eq!(app.feed_editor.cursor, 1);

        draw(&mut app);
        let outside = mouse(
            MouseEventKind::Up(MouseButton::Left),
            area.right() + 2,
            area.y,
        );
        handle_mouse(&mut app, outside, &tx).await;
        assert_eq!(app.feed_editor.mode, FeedEditorMode::Normal);
        assert_eq!(app.feed_editor.cursor, 0, "cancel restores the cursor");
        assert!(app.feed_editor.drag.is_none());
        assert_eq!(app.feeds[0].title, "Feed 0");
    }

    /// Images are converted to terminal protocols on the first frame (which empties
    /// `image_cache`); later frames must keep placing them.
    #[tokio::test]
    async fn inline_images_survive_after_first_frame() {
        let mut app = App::new();
        app.update_available = None;
        with_test_feeds(&mut app);
        app.picker = Some(limner::render_image::halfblock_picker());
        let url = "https://img.test/a.png".to_string();
        app.feeds[0].articles[0].content = format!(
            "![pic]({url})\n\n{}",
            "x".repeat(crate::models::CONTENT_STUB_MAX_LEN)
        );
        app.image_cache.insert(
            url.clone(),
            limner::render_image::img_crate::DynamicImage::new_rgb8(40, 40),
        );
        app.selected_feed = 0;
        app.selected_article = 0;
        app.clear_category_view();
        app.state = AppState::ArticleDetail;

        draw(&mut app);
        assert!(app.protocol_cache.contains_key(&url));
        assert!(app.image_cache.is_empty(), "decoded image consumed");
        let rows_with_image = app.content_line_count;
        for _ in 0..3 {
            draw(&mut app);
        }
        assert_eq!(app.content_line_count, rows_with_image, "image rows kept");
        assert!(rows_with_image > 3);
    }
}
