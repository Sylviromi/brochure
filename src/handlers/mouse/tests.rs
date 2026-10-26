//! Mouse handling tests (headless draws via `TestBackend`).

use super::{editor_drag::unpreview, *};
use crate::models::{SettingsItem, Tab};
use crossterm::event::KeyModifiers;
use ratatui::{Terminal, backend::TestBackend};

/// Mouse event in plain cell coordinates (no pixel reports).
async fn handle_mouse(app: &mut App, ev: MouseEvent, tx: &UnboundedSender<AppEvent>) -> bool {
    super::handle_mouse(app, ev, None, tx).await
}

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

/// Press and release at one spot (lists select on release).
async fn tap(app: &mut App, column: u16, row: u16, tx: &UnboundedSender<AppEvent>) {
    handle_mouse(app, left_click(column, row), tx).await;
    let up = mouse(MouseEventKind::Up(MouseButton::Left), column, row);
    handle_mouse(app, up, tx).await;
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

/// `with_test_feeds`, but with `n` feeds so the sidebar overflows.
fn with_many_feeds(app: &mut App, n: usize) {
    with_test_feeds(app);
    let template = app.feeds[0].clone();
    app.feeds = (0..n)
        .map(|i| crate::models::Feed {
            url: format!("f{i}"),
            ..template.clone()
        })
        .collect();
}

fn sidebar_bar(app: &App) -> crate::models::ScrollbarHit {
    *app.hit
        .scrollbars
        .iter()
        .find(|b| b.target == ScrollTarget::Sidebar)
        .expect("sidebar scrollbar recorded")
}

#[tokio::test]
async fn dragging_sidebar_scrollbar_scrolls_without_moving_cursor() {
    let mut app = App::new();
    app.update_available = None;
    with_many_feeds(&mut app, 80);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    draw(&mut app);
    let bar = sidebar_bar(&app);
    // Thumb starts just below the top arrow; drag it to the bottom.
    handle_mouse(&mut app, left_click(bar.area.x, bar.area.y + 1), &tx).await;
    let to_bottom = mouse(
        MouseEventKind::Drag(MouseButton::Left),
        bar.area.x + 5, // sideways wander keeps the grab
        bar.area.bottom() + 3,
    );
    handle_mouse(&mut app, to_bottom, &tx).await;
    draw(&mut app); // the redraw must keep the dragged-to offset
    assert_eq!(
        app.sidebar_list_state.offset(),
        bar.content_len - bar.viewport_len
    );
    assert!(app.detached.sidebar);
    assert_eq!(app.sidebar_cursor, 0);
    let up = mouse(MouseEventKind::Up(MouseButton::Left), bar.area.x, 0);
    handle_mouse(&mut app, up, &tx).await;
    assert!(app.scrollbar_drag.is_none());
}

#[tokio::test]
async fn pixel_reports_drag_the_thumb_by_less_than_a_cell() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    // Offset after grabbing the thumb and dragging it down `dy` rows.
    async fn drag_by(dy: f32, fine: bool, tx: &UnboundedSender<AppEvent>) -> usize {
        let mut app = App::new();
        app.update_available = None;
        with_many_feeds(&mut app, 300);
        draw(&mut app);
        let bar = sidebar_bar(&app);
        let top = f32::from(bar.area.y + 1) + 0.5;
        let fine_row = |r: f32| fine.then_some(r);
        let down = left_click(bar.area.x, top as u16);
        super::handle_mouse(&mut app, down, fine_row(top), tx).await;
        let to = top + dy;
        let drag = mouse(
            MouseEventKind::Drag(MouseButton::Left),
            bar.area.x,
            to as u16,
        );
        super::handle_mouse(&mut app, drag, fine_row(to), tx).await;
        app.sidebar_list_state.offset()
    }
    let half_cell = drag_by(0.5, true, &tx).await;
    let one_cell = drag_by(1.0, false, &tx).await;
    assert!(
        0 < half_cell && half_cell < one_cell,
        "{half_cell} vs {one_cell}"
    );
}

#[tokio::test]
async fn dragging_list_content_scrolls_line_by_line_without_selecting() {
    let mut app = App::new();
    app.update_available = None;
    with_many_feeds(&mut app, 80);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    *app.sidebar_list_state.offset_mut() = 10;
    app.detached.sidebar = true;
    draw(&mut app);
    let s = app.hit.sidebar;
    handle_mouse(&mut app, left_click(s.x + 2, s.y + 5), &tx).await;
    // Content follows the pointer: two rows down shows two earlier rows.
    let drag = mouse(MouseEventKind::Drag(MouseButton::Left), s.x + 2, s.y + 7);
    handle_mouse(&mut app, drag, &tx).await;
    assert_eq!(app.sidebar_list_state.offset(), 8);
    let up = mouse(MouseEventKind::Up(MouseButton::Left), s.x + 2, s.y + 7);
    handle_mouse(&mut app, up, &tx).await;
    assert_eq!(app.sidebar_cursor, 0, "a drag must not select");
    // A press and release without moving still selects.
    draw(&mut app);
    tap(&mut app, s.x + 2, s.y + 1, &tx).await;
    assert_eq!(app.sidebar_cursor, 9);
}

#[tokio::test]
async fn wheel_over_focused_sidebar_keeps_cursor() {
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    draw(&mut app);
    let sidebar = app.hit.sidebar;
    let wheel = mouse(MouseEventKind::ScrollDown, sidebar.x + 2, sidebar.y);
    handle_mouse(&mut app, wheel, &tx).await;
    assert_eq!(app.sidebar_cursor, 0);
    assert!(app.detached.sidebar);
    assert_eq!(app.state, AppState::FeedList);
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
    tap(&mut app, sidebar.x + 2, sidebar.y + 2, &tx).await;
    assert_eq!(app.sidebar_cursor, 2);
    assert_eq!(app.selected_feed, 1);
    assert_eq!(app.state, AppState::FeedList);

    // Clicking the selected feed again opens its article list.
    tap(&mut app, sidebar.x + 2, sidebar.y + 2, &tx).await;
    assert_eq!(app.state, AppState::ArticleList);

    draw(&mut app);
    let list = app.hit.article_list;
    tap(&mut app, list.x + 2, list.y + 2, &tx).await;
    assert_eq!(app.selected_article, 2);
    handle_mouse(
        &mut app,
        mouse(MouseEventKind::ScrollUp, list.x + 2, list.y),
        &tx,
    )
    .await;
    // The wheel only scrolls the focused list; the selection stays.
    assert_eq!(app.selected_article, 2);
    assert!(app.detached.article_list);

    // Wheel over the unfocused sidebar scrolls it in place: reading continues and
    // the selection stays; the next key press snaps the view back to the cursor.
    draw(&mut app);
    let sidebar = app.hit.sidebar;
    handle_mouse(
        &mut app,
        mouse(MouseEventKind::ScrollDown, sidebar.x + 2, sidebar.y),
        &tx,
    )
    .await;
    assert_eq!(app.state, AppState::ArticleList);
    assert_eq!(app.sidebar_cursor, 2);
    assert!(app.detached.sidebar);
    press(&mut app, KeyCode::Down, &tx).await;
    assert!(!app.detached.sidebar);
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
async fn dragging_a_category_left_climbs_to_the_root_level() {
    use crate::models::Category;
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    let cat = |id, parent_id| Category {
        id,
        name: format!("Cat {id}"),
        parent_id,
        order: 0,
    };
    // Cat 1 > Cat 2 > Cat 3
    app.categories = vec![cat(1, None), cat(2, Some(1)), cat(3, Some(2))];
    app.feed_editor.collapsed.clear();
    app.state = AppState::FeedEditor;
    app.feed_editor.panel = EditorPanel::Categories;
    app.feed_editor.cat_cursor = 2;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    draw(&mut app);
    let area = app.hit.editor_cats;
    let (x, y) = (area.x + 10, area.y + 2);

    handle_mouse(&mut app, left_click(x, y), &tx).await;
    let drag_to = |col| mouse(MouseEventKind::Drag(MouseButton::Left), col, y);
    handle_mouse(&mut app, drag_to(x - 2), &tx).await;
    let depth = |app: &App| match app.feed_editor.mode {
        FeedEditorMode::Moving { depth_delta, .. } => depth_delta,
        _ => panic!("not moving"),
    };
    assert_eq!(depth(&app), -1);
    handle_mouse(&mut app, drag_to(x - 8), &tx).await;
    assert_eq!(depth(&app), -2, "clamped at the root level");
}

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

/// Scrolling to the bottom and switching to the (wider) zen view must not leave
/// the article scrolled past its end.
#[tokio::test]
async fn zen_clamps_scroll_from_narrower_view() {
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    app.feeds[0].articles[0].content = (0..60)
        .map(|i| format!("Paragraph {i} with plenty of words so that it wraps a few times.\n\n"))
        .collect();
    app.selected_feed = 0;
    app.selected_article = 0;
    app.clear_category_view();
    app.state = AppState::ArticleDetail;
    draw(&mut app);
    app.scroll_article(i32::MAX / 2);
    draw(&mut app);
    let narrow_bottom = app.article_scroll.get("https://example.test/0");
    assert!(narrow_bottom > 0);

    app.zen_mode = true;
    draw(&mut app);
    let max = app
        .content_line_count
        .saturating_sub(app.content_area_height as usize) as u16;
    let offset = app.article_scroll.get("https://example.test/0");
    assert!(offset <= max, "offset {offset} > max {max}");
    assert!(offset < narrow_bottom, "wider view has fewer rows");
}

#[tokio::test]
async fn wheel_scrolls_preview_without_focusing_it() {
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    app.feeds[0].articles[0].content = (0..60)
        .map(|i| format!("Paragraph {i} with plenty of words so that it wraps.\n\n"))
        .collect();
    app.selected_feed = 0;
    app.selected_article = 0;
    app.clear_category_view();
    app.state = AppState::ArticleList;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    draw(&mut app);
    let detail = app.hit.article_detail;
    handle_mouse(
        &mut app,
        mouse(MouseEventKind::ScrollDown, detail.x + 2, detail.y + 2),
        &tx,
    )
    .await;
    assert_eq!(app.state, AppState::ArticleList);
    assert_eq!(app.article_scroll.get("https://example.test/0"), 3);
}

#[tokio::test]
async fn dragging_over_article_text_selects_it() {
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    app.feeds[0].articles[0].content = format!(
        "Alpha beta gamma delta.\n\nSecond paragraph here.\n\n{}{}",
        "x".repeat(crate::models::CONTENT_STUB_MAX_LEN),
        "\n\nFiller paragraph.".repeat(60)
    );
    app.selected_feed = 0;
    app.selected_article = 0;
    app.clear_category_view();
    app.state = AppState::ArticleDetail;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();

    // Find the row showing "Alpha" and drag across "Alpha beta".
    let buffer = terminal.backend().buffer().clone();
    let area = app.hit.article_detail;
    let (x, y) = (area.top()..area.bottom())
        .find_map(|y| {
            let line: String = (area.left()..area.right())
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect();
            line.find("Alpha").map(|i| (area.left() + i as u16, y))
        })
        .expect("article text on screen");
    handle_mouse(&mut app, left_click(x, y), &tx).await;
    let drag = mouse(MouseEventKind::Drag(MouseButton::Left), x + 9, y);
    handle_mouse(&mut app, drag, &tx).await;
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    assert_eq!(app.selection_text, "Alpha beta");
    assert!(
        terminal.backend().buffer()[(x, y)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );

    let up = mouse(MouseEventKind::Up(MouseButton::Left), x + 9, y);
    handle_mouse(&mut app, up, &tx).await;
    assert!(app.selection_copy_requested, "release asks for a copy");
    assert_eq!(app.state, AppState::ArticleDetail);

    // The selection belongs to the text: scrolling carries the highlight along.
    let link = "https://example.test/0";
    let before = app.article_scroll.get(link);
    press(&mut app, KeyCode::Down, &tx).await;
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let moved = app.article_scroll.get(link) - before;
    assert!(moved > 0, "the article scrolled");
    assert_eq!(app.selection_text, "Alpha beta");
    let reversed = |terminal: &Terminal<TestBackend>, y: u16| {
        terminal.backend().buffer()[(x, y)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    };
    assert!(reversed(&terminal, y - moved));
    assert!(!reversed(&terminal, y));

    // Dragging past the end of the line highlights only up to the text.
    let row = y - moved;
    let end = x + "Alpha beta gamma delta.".len() as u16;
    handle_mouse(&mut app, left_click(x, row), &tx).await;
    let drag = mouse(MouseEventKind::Drag(MouseButton::Left), end + 5, row);
    handle_mouse(&mut app, drag, &tx).await;
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let cell = |x: u16| terminal.backend().buffer()[(x, row)].modifier;
    assert!(cell(end - 1).contains(ratatui::style::Modifier::REVERSED));
    assert!(!cell(end).contains(ratatui::style::Modifier::REVERSED));

    // Showing another article drops it.
    app.selected_article = 1;
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    assert!(app.selection.is_none());
}

#[tokio::test]
async fn help_popup_and_jk_follow_text_input_rules() {
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    // j moves the feed list (it used to only accept arrow keys).
    press(&mut app, KeyCode::Char('j'), &tx).await;
    assert_eq!(app.sidebar_cursor, 1);

    press(&mut app, KeyCode::Char('?'), &tx).await;
    assert!(app.help_open);
    draw(&mut app);
    // Keys go to the popup while it is open.
    press(&mut app, KeyCode::Char('j'), &tx).await;
    assert_eq!(app.sidebar_cursor, 1);
    press(&mut app, KeyCode::Esc, &tx).await;
    assert!(!app.help_open);

    // In a text field, ? and j are typed, not shortcuts.
    app.state = AppState::AddFeed;
    app.add_feed.return_state = AppState::FeedList;
    press(&mut app, KeyCode::Char('?'), &tx).await;
    press(&mut app, KeyCode::Char('j'), &tx).await;
    assert!(!app.help_open);
    assert_eq!(app.add_feed.url_input.text, "?j");
}

#[tokio::test]
async fn clicking_a_changelog_entry_selects_it() {
    let mut app = App::new();
    app.update_available = None;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.select_tab(Tab::Changelog);
    app.changelog_cursor = 0;
    app.changelog_scroll = 0;
    draw(&mut app);
    let (first, _) = *app.hit.changelog_entries.get(1).expect("two releases");
    let area = app.hit.changelog;
    let y = area.y + (first - app.changelog_scroll as usize) as u16;
    handle_mouse(&mut app, left_click(area.x + 5, y), &tx).await;
    assert_eq!(app.changelog_cursor, 0, "acts on release, not press");
    let up = mouse(MouseEventKind::Up(MouseButton::Left), area.x + 5, y);
    handle_mouse(&mut app, up, &tx).await;
    assert_eq!(app.changelog_cursor, 1);
}

#[tokio::test]
async fn theme_list_rows_skip_headers_and_picker_cancels_outside() {
    use crate::ui::theme::{BUILTIN_DARK_COUNT, ColorTheme};
    let mut app = App::new();
    app.update_available = None;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.state = AppState::ThemeEditor;
    app.theme_editor.cursor = 0;
    let mut terminal = Terminal::new(TestBackend::new(140, 50)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();

    // The row recorded for the first light theme really shows that theme.
    let (rect, _) = *app
        .hit
        .rows
        .iter()
        .find(|(_, idx)| *idx == BUILTIN_DARK_COUNT)
        .expect("first light theme visible");
    let buffer = terminal.backend().buffer();
    let text: String = (rect.left()..rect.right())
        .map(|x| buffer[(x, rect.y)].symbol().to_string())
        .collect();
    let name = ColorTheme::builtin_names()[BUILTIN_DARK_COUNT];
    assert!(text.contains(name), "{text:?} should show {name}");

    handle_mouse(&mut app, left_click(rect.x + 2, rect.y), &tx).await;
    assert_eq!(app.theme_editor.cursor, BUILTIN_DARK_COUNT);
    handle_mouse(
        &mut app,
        mouse(MouseEventKind::ScrollDown, rect.x, rect.y),
        &tx,
    )
    .await;
    assert_eq!(app.theme_editor.cursor, BUILTIN_DARK_COUNT + 1);

    // Clone picker popup: rows map 1:1, clicking outside cancels it.
    app.state = AppState::ThemeEditorNew;
    app.theme_editor.clone_cursor = 0;
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let (rect, _) = *app.hit.rows.iter().find(|(_, i)| *i == 2).unwrap();
    handle_mouse(&mut app, left_click(rect.x + 2, rect.y), &tx).await;
    assert_eq!(app.theme_editor.clone_cursor, 2);
    handle_mouse(&mut app, left_click(0, 49), &tx).await;
    assert_eq!(app.state, AppState::ThemeEditor);
}

#[test]
fn toasts_stack_up_to_the_limit_and_expire() {
    let mut app = App::new();
    app.update_available = None;
    app.zen_mode = true; // no tab bar: toasts are the only place messages show
    app.user_data.toasts = Default::default(); // 3 shown, 4 s
    for n in 1..=4 {
        app.notify(format!("✓ Copied {n} characters"));
    }
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
    let shown = |terminal: &mut Terminal<TestBackend>, app: &mut App| -> Vec<usize> {
        let frame = terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
        let text: String = frame.buffer.content.iter().map(|c| c.symbol()).collect();
        (1..=4)
            .filter(|n| text.contains(&format!("Copied {n} characters")))
            .collect()
    };
    assert_eq!(shown(&mut terminal, &mut app), [2, 3, 4], "newest three");
    app.tick += 16;
    assert!(shown(&mut terminal, &mut app).is_empty(), "expired");
}

#[tokio::test]
async fn selecting_then_opening_a_stub_fetches_it_once() {
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    for a in &mut app.feeds[1].articles {
        a.content = "short feed summary".into();
        a.is_read = true; // opening an unread article saves to the real data dir
    }
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    draw(&mut app);
    let sidebar = app.hit.sidebar;
    tap(&mut app, sidebar.x + 2, sidebar.y + 2, &tx).await; // select Feed 1
    tap(&mut app, sidebar.x + 2, sidebar.y + 2, &tx).await; // focus its articles
    draw(&mut app);
    let list = app.hit.article_list;
    tap(&mut app, list.x + 2, list.y + 1, &tx).await; // select (fetch starts)
    tap(&mut app, list.x + 2, list.y + 1, &tx).await; // open (no second fetch)
    assert_eq!(app.state, AppState::ArticleDetail);
    let link = app.selected_article().unwrap().link.clone();
    assert_eq!(app.full_fetch_tried.len(), 1);
    assert!(app.full_fetch_pending.contains(&link));

    // A failed fetch keeps the feed's content rather than blanking the article.
    let source = crate::models::FeedSource::Feed(app.selected_feed);
    let idx = app.selected_article;
    crate::events::on_full_article_fetched(&mut app, source, idx, &link, Err("boom".into()));
    assert!(app.full_fetch_pending.is_empty());
    assert_eq!(
        app.selected_article().unwrap().content,
        "short feed summary"
    );
    // Going back to the list and re-selecting doesn't fetch again.
    tap(&mut app, list.x + 2, list.y + 1, &tx).await;
    assert!(app.full_fetch_pending.is_empty());
}

#[tokio::test]
async fn loading_skeleton_cannot_be_selected() {
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    app.feeds[0].articles[0].content = "stub".into();
    app.full_fetch_pending
        .insert("https://example.test/0".into());
    app.selected_feed = 0;
    app.selected_article = 0;
    app.clear_category_view();
    app.state = AppState::ArticleDetail;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    // The bottom rows of the panel are skeleton noise.
    let area = app.hit.article_detail;
    let (x, y) = (area.x + 2, area.bottom() - 3);
    handle_mouse(&mut app, left_click(x, y), &tx).await;
    let drag = mouse(MouseEventKind::Drag(MouseButton::Left), x + 20, y + 1);
    handle_mouse(&mut app, drag, &tx).await;
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    assert!(app.selection.is_some());
    assert_eq!(app.selection_text.trim(), "");
    let cell = &terminal.backend().buffer()[(x + 1, y)];
    assert!(!cell.modifier.contains(ratatui::style::Modifier::REVERSED));
}

#[test]
fn refresh_toast_shows_progress_and_failures() {
    let mut app = App::new();
    app.update_available = None;
    app.zen_mode = true;
    app.user_data.toasts = Default::default();
    app.feeds_total = 40;
    app.feeds_pending = 13;
    app.refresh_errors = vec![
        ("blocked", "a".into()),
        ("blocked", "b".into()),
        ("error", "c".into()),
    ];
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    let frame = terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let text: String = frame.buffer.content.iter().map(|c| c.symbol()).collect();
    assert!(text.contains("27/40"));
    assert!(text.contains("2 blocked · 1 failed"));
}

#[tokio::test]
async fn clicking_an_image_zooms_it_until_a_key_or_click() {
    let mut app = App::new();
    app.update_available = None;
    with_test_feeds(&mut app);
    app.state = AppState::ArticleDetail;
    app.picker = Some(limner::render_image::halfblock_picker());
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let url = "https://example.test/pic.png".to_string();
    let area = app.hit.article_detail;
    app.hit.article_images = vec![(Rect::new(area.x, area.y, 10, 4), url.clone())];
    handle_mouse(&mut app, left_click(area.x + 3, area.y + 1), &tx).await;
    assert_eq!(
        app.zoom.as_ref().map(|z| z.url.as_str()),
        Some(url.as_str())
    );
    assert!(
        app.selection.is_none(),
        "an image click is not a text selection"
    );

    // A failed download says so; a decoded one fills the screen.
    crate::events::on_zoom_image(&mut app, url.clone(), None);
    let frame = terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let text: String = frame.buffer.content.iter().map(|c| c.symbol()).collect();
    assert!(text.contains("Couldn't load this image"));
    app.zoom.as_mut().unwrap().failed = false;
    let image = limner::render_image::img_crate::DynamicImage::new_rgb8(40, 20);
    crate::events::on_zoom_image(&mut app, url, Some(image));
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    assert!(app.zoom.as_ref().unwrap().image.is_some());

    // Any key closes it without acting on the article underneath.
    press(&mut app, KeyCode::Down, &tx).await;
    assert!(app.zoom.is_none());
    assert_eq!(app.article_scroll.get("https://example.test/0"), 0);
}

#[tokio::test]
async fn popup_fields_and_hints_are_clickable() {
    let mut app = App::new();
    app.update_available = None;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.select_tab(Tab::Settings);
    app.freshrss_setup.open(None, None);
    app.state = AppState::FreshRssSetup;
    draw(&mut app);
    let field = |app: &App, i: usize| {
        let (r, _) = app.hit.form_fields[i];
        (r.x + 2, r.y + 1)
    };
    // An empty server field refuses to advance, like Enter does.
    app.freshrss_setup.server_input.text.clear();
    let (x, y) = field(&app, 1);
    tap(&mut app, x, y, &tx).await;
    assert_eq!(app.freshrss_setup.step, crate::models::FreshRssStep::Server);
    app.freshrss_setup.server_input.text = "https://rss.example.test".into();
    tap(&mut app, x, y, &tx).await;
    assert_eq!(
        app.freshrss_setup.step,
        crate::models::FreshRssStep::Username
    );
    draw(&mut app);
    let (x, y) = field(&app, 0);
    tap(&mut app, x, y, &tx).await;
    assert_eq!(app.freshrss_setup.step, crate::models::FreshRssStep::Server);

    // The [Esc] hint cancels; a new connection offers no Disconnect.
    draw(&mut app);
    assert_eq!(app.hit.buttons.len(), 2);
    let (esc, _) = app.hit.buttons[1];
    tap(&mut app, esc.x + 1, esc.y, &tx).await;
    assert_eq!(app.state, AppState::SettingsList);

    // Confirm dialogs: the Cancel button backs out (Confirm is never clicked here).
    app.state = AppState::ClearData;
    draw(&mut app);
    let (cancel, key) = app.hit.buttons[1];
    assert_eq!(key.code, KeyCode::Esc);
    tap(&mut app, cancel.x + 1, cancel.y, &tx).await;
    assert_eq!(app.state, AppState::SettingsList);
}

#[test]
fn notification_settings_preview_a_full_stack() {
    let mut app = App::new();
    app.update_available = None;
    app.zen_mode = true;
    app.user_data.toasts = Default::default();
    app.user_data.toasts.max_visible = 4;
    app.preview_toasts("Notifications shown at once: 4".into());
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let frame = terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let text: String = frame.buffer.content.iter().map(|c| c.symbol()).collect();
    assert!(text.contains("Notifications shown at once: 4"));
    assert_eq!(text.matches("Example:").count(), 3, "fills up to the max");
    assert!(text.contains("27/40"), "mock refresh card");
    // The mock card goes away with the samples.
    app.tick += 16;
    let frame = terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let text: String = frame.buffer.content.iter().map(|c| c.symbol()).collect();
    assert!(!text.contains("27/40"));
}
