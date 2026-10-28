//! Keybindings popup (`?`): the keys for the current screen, then global keys.
//!
//! Keep these tables in sync with `src/handlers/` — they are the user-facing
//! reference. `j`/`k` are translated to `↓`/`↑` for every non-text-input screen in
//! `handlers::handle_key`, so lists only mention the arrows once.

use crate::{
    app::App,
    models::{AppState, EditorPanel, FeedEditorMode, ScrollTarget},
};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    prelude::Stylize,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};

use super::{content_block, render_scrollbar};

type Keys = &'static [(&'static str, &'static str)];

const FEED_LIST: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Enter", "Open feed, category, or All Feeds"),
    ("Space", "Collapse / expand category"),
    ("C", "Collapse / expand all categories"),
    ("g", "Jump to top"),
    ("r", "Refresh selection (feed, category, or all)"),
    ("R", "Refresh all feeds (and FreshRSS)"),
    ("H", "Hide / show read articles"),
    ("y", "Copy feed URL"),
    ("e", "Open feed editor"),
];

const ARTICLE_LIST: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Enter", "Read article"),
    ("m", "Toggle read / unread (next after read)"),
    ("A A", "Mark all articles in the list as read"),
    ("H", "Hide / show read articles"),
    ("s", "Save to category / unsave"),
    ("O", "Open article in browser"),
    ("C", "Copy feed URL (article link in Saved)"),
    ("r", "Refresh this feed"),
    ("Esc", "Back to sidebar"),
];

const ARTICLE_DETAIL: Keys = &[
    ("j/k ↑/↓", "Scroll"),
    ("PgUp/PgDn", "Scroll a page"),
    ("Home/End", "Jump to top / bottom"),
    ("z", "Zen mode (full screen)"),
    ("m", "Toggle read / unread"),
    ("s", "Save to category / unsave"),
    ("O", "Open article in browser"),
    ("C", "Copy article link"),
    ("r", "Refresh this feed"),
    ("Esc", "Back to article list"),
];

const ZEN: Keys = &[
    ("j/k ↑/↓", "Scroll"),
    ("PgUp/PgDn", "Scroll a page"),
    ("Home/End", "Jump to top / bottom"),
    ("n / p", "Next / previous article"),
    ("z / Esc", "Leave zen mode"),
];

const SAVED_LIST: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Enter", "Open saved articles"),
    ("e", "Manage saved categories"),
    ("Esc", "Back to Feeds"),
];

const SETTINGS: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Enter", "Activate / open (FreshRSS: edit connection)"),
    ("h/l ←/→", "Change value (FreshRSS: → syncs, ← disconnects)"),
    ("Esc", "Back to Feeds"),
];

const CHANGELOG: Keys = &[
    ("j/k ↑/↓", "Move between releases"),
    ("Enter / Space", "Collapse / expand release"),
];

const EDITOR_FEEDS: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Space", "Move feed (then j/k, Space to drop)"),
    ("a", "Add feed here"),
    ("r", "Rename feed"),
    ("u", "Edit feed URL"),
    ("d", "Delete feed (no confirmation)"),
    ("Tab", "Switch to categories panel"),
    ("Esc / q", "Leave editor"),
];

const EDITOR_CATEGORIES: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Enter", "Collapse / expand"),
    ("n", "New category inside the selected one"),
    ("r", "Rename category"),
    ("d", "Delete category and its feeds"),
    ("Space", "Move category (←/→ nesting, Space to drop)"),
    ("Tab", "Switch to feeds panel"),
    ("Esc / q", "Leave editor"),
];

const EDITOR_MOVING: Keys = &[
    ("j/k ↑/↓", "Choose drop position"),
    ("←/→", "Nesting depth (categories)"),
    ("Space", "Drop here"),
    ("Esc", "Cancel move"),
];

const SAVED_EDITOR: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("n", "New category"),
    ("r", "Rename category"),
    ("d", "Delete category"),
    ("Esc / q", "Back"),
];

const THEME_EDITOR: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Enter", "Use theme"),
    ("n", "New custom theme"),
    ("e", "Edit custom theme colours"),
    ("r", "Rename custom theme"),
    ("d", "Delete custom theme"),
    ("x / i", "Export / import theme file"),
    ("Esc / q", "Back"),
];

const THEME_COLORS: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Enter", "Edit colour"),
    ("Esc / s / q", "Back"),
];

const THEME_NEW: Keys = &[
    ("j/k ↑/↓", "Choose a theme to copy"),
    ("Enter", "Create custom theme from it"),
    ("Esc / q", "Cancel"),
];

const CONFIRM: Keys = &[("Enter", "Confirm"), ("Esc / q", "Cancel")];

const PICKER: Keys = &[
    ("j/k ↑/↓", "Move selection"),
    ("Enter", "Save here / unsave / new category"),
    ("Esc", "Cancel"),
];

const GLOBAL: Keys = &[
    (
        "Tab/Shift+Tab",
        "Switch tabs (from sidebars, Settings, Changelog)",
    ),
    ("f", "Link hints: open a link / image in the article"),
    ("F", "Link hints: copy a link / image"),
    ("?", "Show / hide this help"),
    ("q", "Quit (from any list)"),
];

/// `(title, bindings)` for the screen the user is on.
fn screen_keys(app: &App) -> (&'static str, Keys) {
    if app.zen_mode {
        return ("Zen mode", ZEN);
    }
    match app.state {
        AppState::FeedList => ("Feeds", FEED_LIST),
        AppState::ArticleList => ("Article list", ARTICLE_LIST),
        AppState::ArticleDetail => ("Reading", ARTICLE_DETAIL),
        AppState::SavedCategoryList => ("Saved", SAVED_LIST),
        AppState::SettingsList => ("Settings", SETTINGS),
        AppState::Changelog => ("Changelog", CHANGELOG),
        AppState::FeedEditor if matches!(app.feed_editor.mode, FeedEditorMode::Moving { .. }) => {
            ("Feed editor — moving", EDITOR_MOVING)
        }
        AppState::FeedEditor if app.feed_editor.panel == EditorPanel::Categories => {
            ("Feed editor — categories", EDITOR_CATEGORIES)
        }
        AppState::FeedEditor => ("Feed editor — feeds", EDITOR_FEEDS),
        AppState::SavedCategoryEditor => ("Saved categories", SAVED_EDITOR),
        AppState::ThemeEditor => ("Themes", THEME_EDITOR),
        AppState::ThemeEditorNew => ("New theme", THEME_NEW),
        AppState::ThemeEditorColorEdit => ("Theme colours", THEME_COLORS),
        AppState::CategoryPicker => ("Save article", PICKER),
        AppState::ClearData
        | AppState::ClearArticleCache
        | AppState::SavedCategoryEditorDeleteConfirm => ("Confirm", CONFIRM),
        _ => ("This screen", &[]),
    }
}

/// All help lines: this screen first, then global keys.
fn help_lines(app: &App) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let (title, keys) = screen_keys(app);
    let mut lines = Vec::new();
    for (heading, keys) in [(title, keys), ("Everywhere", GLOBAL)] {
        if keys.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(format!(" {heading}").fg(theme.header).bold()));
        for (key, action) in keys {
            lines.push(Line::from(vec![
                Span::raw("   "),
                format!("{key:<16}").fg(theme.accent).bold(),
                action.fg(theme.text),
            ]));
        }
    }
    lines
}

/// Draw the keybindings popup centred over the screen.
pub(super) fn draw_help_popup(f: &mut Frame, app: &mut App) {
    let lines = help_lines(app);
    let area = f.area();
    let width = 72.min(area.width);
    let total = Paragraph::new(lines.clone())
        .wrap(Wrap { trim: false })
        .line_count(width.saturating_sub(3));
    let height = (total as u16 + 2).min(area.height.saturating_sub(2));
    let [popup] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [popup] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(popup);

    f.render_widget(Clear, popup);
    let block = content_block(
        Line::from(" Keybindings ".fg(app.theme.header).bold()),
        true,
        app.user_data.border_rounded,
        &app.theme,
    )
    .title_bottom(Line::from(" ? / Esc close ".fg(app.theme.muted)).right_aligned());
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let max_scroll = total.saturating_sub(inner.height as usize) as u16;
    app.help_scroll = app.help_scroll.min(max_scroll);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.help_scroll, 0)),
        inner,
    );
    if total > inner.height as usize {
        let bar = Rect {
            x: inner.right().saturating_sub(1),
            width: 1,
            ..inner
        };
        render_scrollbar(
            f,
            bar,
            total,
            inner.height as usize,
            app.help_scroll as usize,
            &app.theme,
            Some((&mut app.hit, ScrollTarget::Help)),
        );
    }
}
