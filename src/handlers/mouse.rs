//! Mouse support.
//!
//! - Wheel: scrolls whatever panel is under the pointer (moving focus to it).
//! - Left click: selects tabs and rows; clicking the already-selected row of a
//!   focused list activates it (opens a feed/article, toggles a category).
//!
//! Actions are routed through the keyboard handlers where possible so both input
//! paths behave identically. Hit regions come from the last drawn frame
//! (see [`crate::models::HitAreas`]).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use tokio::sync::mpsc::UnboundedSender;

use super::article::prefetch_article_if_stub;
use crate::{
    app::{App, visible_cat_only_items},
    models::{AppEvent, AppState, EditorPanel, FeedEditorMode, FeedTreeItem, hits, list_row},
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
            if reclick {
                // Enter toggles the category's collapse state.
                return press(app, KeyCode::Enter, tx).await;
            }
        }
    }
    false
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
}
