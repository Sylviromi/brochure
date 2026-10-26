//! Key event handling for the settings screen and its modal sub-states.
//!
//! Covers `SettingsList`, the two-step `AddFeed` wizard, `ImportPath`/`ExportPath` text
//! inputs, and `ClearData`/`ClearArticleCache` confirmation dialogs.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::App,
    fetch::{fetch_feed, fetch_feed_title},
    models::{AddFeedStep, AppEvent, AppState, Feed, SettingsItem},
    saved_io,
    state::file_prompt::FileFormat,
    storage::{
        article_cache_size, clear_all_data, clear_article_cache, default_export_path,
        expand_home_dir, export_opml_to_path, import_opml_from_path, save_categories, save_feeds,
        save_user_data,
    },
    ui::theme::ColorTheme,
};

/// Toggle a boolean field in `app.user_data`, persist, and notify.
macro_rules! toggle_setting {
    ($app:expr, $field:expr, $label:expr) => {{
        $field = !$field;
        let _ = save_user_data(&$app.user_data);
        let state = if $field { "ON" } else { "OFF" };
        $app.notify(format!("{}: {state}", $label));
    }};
}

/// Set a boolean field in `app.user_data` to a fixed value, persist, and notify.
macro_rules! set_setting {
    ($app:expr, $field:expr, $value:expr, $label:expr) => {{
        $field = $value;
        let _ = save_user_data(&$app.user_data);
        let state = if $field { "ON" } else { "OFF" };
        $app.notify(format!("{}: {state}", $label));
    }};
}

#[derive(Clone, Copy)]
enum SettingDir {
    Enter,
    Left,
    Right,
}

/// Handles key events for the `SettingsList` state.
///
/// Refreshes the article cache size on every keypress, toggles boolean settings in-place, and
/// transitions to sub-states for destructive actions. Returns `true` to quit.
pub(super) fn handle_settings(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    app.article_cache_size = article_cache_size();
    let dir = match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Esc => {
            app.unselect();
            return false;
        }
        KeyCode::Tab => {
            app.switch_tab_right();
            return false;
        }
        KeyCode::BackTab => {
            app.switch_tab_left();
            return false;
        }
        KeyCode::Up => {
            app.previous();
            return false;
        }
        KeyCode::Down => {
            app.next();
            return false;
        }
        KeyCode::Enter => SettingDir::Enter,
        KeyCode::Left | KeyCode::Char('h') => SettingDir::Left,
        KeyCode::Right | KeyCode::Char('l') => SettingDir::Right,
        _ => return false,
    };
    apply_settings_action(app, dir, tx);
    false
}

/// Single dispatch for all settings actions, keyed by (setting, direction).
fn apply_settings_action(app: &mut App, dir: SettingDir, tx: &UnboundedSender<AppEvent>) {
    match (app.settings_selected, dir) {
        // ── Modal transitions (Enter only) ─────────────────────────────────
        (SettingsItem::ImportOpml, SettingDir::Enter) => open_import(app, FileFormat::Opml),
        (SettingsItem::ExportOpml, SettingDir::Enter) => open_export(app, FileFormat::Opml),
        (SettingsItem::ImportSaved, SettingDir::Enter) => open_import(app, FileFormat::Saved),
        (SettingsItem::ExportSaved, SettingDir::Enter) => open_export(app, FileFormat::Saved),
        (SettingsItem::ClearData, SettingDir::Enter) => {
            app.state = AppState::ClearData;
        }
        (SettingsItem::ClearArticleCache, SettingDir::Enter) => {
            app.state = AppState::ClearArticleCache;
        }
        (SettingsItem::FreshRss, SettingDir::Right) if app.user_data.freshrss.is_some() => {
            app.spawn_freshrss_sync(tx);
        }
        (SettingsItem::FreshRss, SettingDir::Enter | SettingDir::Right) => {
            let cfg = app.user_data.freshrss.as_ref();
            app.freshrss_setup.open(
                cfg.map(|c| c.base_url.as_str()),
                cfg.map(|c| c.username.as_str()),
            );
            app.state = AppState::FreshRssSetup;
        }
        (SettingsItem::FreshRss, SettingDir::Left) if app.user_data.freshrss.is_some() => {
            app.disconnect_freshrss();
        }
        (SettingsItem::Theme, SettingDir::Enter) => {
            let builtin_names = ColorTheme::builtin_names();
            app.theme_editor.cursor = if app.user_data.selected_theme == "custom" {
                let custom_pos = app
                    .user_data
                    .custom_themes
                    .iter()
                    .position(|t| Some(t.id) == app.user_data.selected_custom_id)
                    .unwrap_or(0);
                builtin_names.len() + custom_pos
            } else {
                builtin_names
                    .iter()
                    .position(|n| ColorTheme::slug(n) == app.user_data.selected_theme)
                    .unwrap_or(0)
            };
            app.state = AppState::ThemeEditor;
        }

        // ── Notifications ──────────────────────────────────────────────────
        (SettingsItem::ToastProgress, SettingDir::Enter) => {
            toggle_setting!(
                app,
                app.user_data.toasts.refresh_progress,
                "Refresh Progress"
            );
        }
        (SettingsItem::ToastProgress, SettingDir::Left) => {
            set_setting!(
                app,
                app.user_data.toasts.refresh_progress,
                false,
                "Refresh Progress"
            );
        }
        (SettingsItem::ToastProgress, SettingDir::Right) => {
            set_setting!(
                app,
                app.user_data.toasts.refresh_progress,
                true,
                "Refresh Progress"
            );
        }
        (
            item @ (SettingsItem::ToastPosition
            | SettingsItem::ToastDuration
            | SettingsItem::ToastCount),
            dir,
        ) => {
            let forward = !matches!(dir, SettingDir::Left);
            let t = &mut app.user_data.toasts;
            let msg = match item {
                SettingsItem::ToastPosition => {
                    t.position = t.position.cycle(forward);
                    format!("Notifications: {}", t.position.label())
                }
                SettingsItem::ToastDuration => {
                    t.cycle_seconds(forward);
                    format!("Notifications stay {}s", t.seconds)
                }
                _ => {
                    t.cycle_max_visible(forward);
                    format!("Notifications shown at once: {}", t.max_visible)
                }
            };
            let _ = save_user_data(&app.user_data);
            app.notify(msg);
        }

        // ── Cycleable enums ────────────────────────────────────────────────
        (SettingsItem::ArchivePolicy, SettingDir::Left) => {
            app.user_data.archive_policy = app.user_data.archive_policy.prev();
            let _ = save_user_data(&app.user_data);
            app.notify(format!(
                "Archive Policy: {}",
                app.user_data.archive_policy.label()
            ));
        }
        (SettingsItem::ArchivePolicy, _) => {
            app.user_data.archive_policy = app.user_data.archive_policy.next();
            let _ = save_user_data(&app.user_data);
            app.notify(format!(
                "Archive Policy: {}",
                app.user_data.archive_policy.label()
            ));
        }
        (SettingsItem::AutoFetchOnStart, SettingDir::Left) => {
            app.user_data.fetch_policy = app.user_data.fetch_policy.prev();
            let _ = save_user_data(&app.user_data);
            app.notify(format!(
                "Fetch Policy: {}",
                app.user_data.fetch_policy.label()
            ));
        }
        (SettingsItem::AutoFetchOnStart, _) => {
            app.user_data.fetch_policy = app.user_data.fetch_policy.next();
            let _ = save_user_data(&app.user_data);
            app.notify(format!(
                "Fetch Policy: {}",
                app.user_data.fetch_policy.label()
            ));
        }

        // ── Boolean toggles ────────────────────────────────────────────────
        (SettingsItem::CacheFullArticles, SettingDir::Enter) => {
            toggle_setting!(
                app,
                app.user_data.cache_full_articles,
                "Save Article Content"
            );
        }
        (SettingsItem::CacheFullArticles, SettingDir::Left) => {
            set_setting!(
                app,
                app.user_data.cache_full_articles,
                false,
                "Save Article Content"
            );
        }
        (SettingsItem::CacheFullArticles, SettingDir::Right) => {
            set_setting!(
                app,
                app.user_data.cache_full_articles,
                true,
                "Save Article Content"
            );
        }
        (SettingsItem::ScrollLoop, SettingDir::Enter) => {
            toggle_setting!(app, app.user_data.scroll_loop, "Scroll Loop");
        }
        (SettingsItem::ScrollLoop, SettingDir::Left) => {
            set_setting!(app, app.user_data.scroll_loop, false, "Scroll Loop");
        }
        (SettingsItem::ScrollLoop, SettingDir::Right) => {
            set_setting!(app, app.user_data.scroll_loop, true, "Scroll Loop");
        }
        (SettingsItem::BorderStyle, SettingDir::Enter) => {
            toggle_setting!(app, app.user_data.border_rounded, "Rounded Borders");
        }
        (SettingsItem::BorderStyle, SettingDir::Left) => {
            set_setting!(app, app.user_data.border_rounded, false, "Rounded Borders");
        }
        (SettingsItem::BorderStyle, SettingDir::Right) => {
            set_setting!(app, app.user_data.border_rounded, true, "Rounded Borders");
        }

        // ── App-cycled settings ────────────────────────────────────────────
        (SettingsItem::BodyAlignment, SettingDir::Left) => {
            app.cycle_alignment_prev();
            app.user_data.body_alignment = app.alignment_label().to_lowercase().to_string();
            let _ = save_user_data(&app.user_data);
            app.notify(format!("Body Alignment: {}", app.alignment_label()));
        }
        (SettingsItem::BodyAlignment, _) => {
            app.cycle_alignment_next();
            app.user_data.body_alignment = app.alignment_label().to_lowercase().to_string();
            let _ = save_user_data(&app.user_data);
            app.notify(format!("Body Alignment: {}", app.alignment_label()));
        }
        (SettingsItem::ZenWidth, SettingDir::Left) => {
            app.cycle_zen_width_prev();
            app.user_data.zen_width = app.zen_width;
            let _ = save_user_data(&app.user_data);
            app.notify(format!("Zen Width: {}%", app.zen_width));
        }
        (SettingsItem::ZenWidth, _) => {
            app.cycle_zen_width_next();
            app.user_data.zen_width = app.zen_width;
            let _ = save_user_data(&app.user_data);
            app.notify(format!("Zen Width: {}%", app.zen_width));
        }
        _ => {}
    }
}

/// Handles key events for the two-step `AddFeed` wizard (`Url` then `Title`).
///
/// In the `Url` step, pressing Enter spawns a background title-fetch and advances to the `Title`
/// step. In the `Title` step, Enter creates and immediately fetches the new feed, then returns to
/// the previous state.
pub(super) fn handle_add_feed(app: &mut App, key: KeyEvent, tx: &UnboundedSender<AppEvent>) {
    if app.add_feed.step == AddFeedStep::Url {
        match key.code {
            KeyCode::Enter => {
                let url = app.add_feed.url_input.text.trim().to_string();
                if url.is_empty() {
                    return;
                }
                app.add_feed.url = url.clone();
                app.add_feed.url_input.clear();
                app.add_feed.fetched_title = None;
                app.add_feed.step = AddFeedStep::Title;
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let result = fetch_feed_title(&url).await;
                    let _ = tx2.send(AppEvent::FeedTitleFetched(result));
                });
            }
            KeyCode::Esc => app.unselect(),
            _ => app.add_feed.url_input.handle_key(key.code, None),
        }
    } else {
        match key.code {
            KeyCode::Enter => {
                let typed = app.add_feed.url_input.text.trim().to_string();
                let title = if typed.is_empty() {
                    match app.add_feed.fetched_title.clone() {
                        Some(t) if !t.is_empty() => t,
                        _ => {
                            app.notify("Title is required.".to_string());
                            return;
                        }
                    }
                } else {
                    typed
                };
                let url = app.add_feed.url.clone();
                let target_category = app.add_feed.target_category.take();
                let next_order = if let Some(insert_at) = app.add_feed.target_order.take() {
                    // Shift all sibling feeds with order >= insert_at up by 1 to make room.
                    for f in app.feeds.iter_mut() {
                        if f.category_id == target_category && f.order >= insert_at {
                            f.order += 1;
                        }
                    }
                    insert_at
                } else {
                    app.feeds.iter().map(|f| f.order).max().unwrap_or(0) + 1
                };
                app.feeds.push(Feed::new_local(
                    title.clone(),
                    url.clone(),
                    target_category,
                    next_order,
                ));
                let _ = save_feeds(&app.feeds);
                app.notify(format!("Feed '{title}' added!"));
                let tx2 = tx.clone();
                let idx = app.feeds.len() - 1;
                tokio::spawn(async move {
                    let result = fetch_feed(&url).await;
                    let _ = tx2.send(AppEvent::FeedFetched(idx, result));
                });
                app.add_feed.url_input.clear();
                app.add_feed.step = AddFeedStep::Url;
                app.add_feed.url.clear();
                app.add_feed.fetched_title = None;
                app.state = app.add_feed.return_state.clone();
            }
            KeyCode::Esc => app.unselect(),
            _ => app.add_feed.url_input.handle_key(key.code, None),
        }
    }
}

/// Base structure for confirmation dialogs: Enter confirms, Esc/q cancels.
fn handle_confirm_dialog(app: &mut App, key: KeyEvent, on_enter: impl FnOnce(&mut App)) {
    match key.code {
        KeyCode::Enter => {
            on_enter(app);
            app.state = AppState::SettingsList;
        }
        KeyCode::Esc | KeyCode::Char('q') => app.state = AppState::SettingsList,
        _ => {}
    }
}

/// Handles key events for the `ClearData` confirmation dialog.
pub(super) fn handle_confirm_delete_all(app: &mut App, key: KeyEvent) {
    handle_confirm_dialog(app, key, |app| {
        app.feeds.clear();
        app.categories.clear();
        app.user_data = crate::models::UserData::default();
        app.saved_view_articles.clear();
        app.in_saved_context = false;
        app.selected_feed = 0;
        app.selected_article = 0;
        app.sidebar_cursor = 0;
        let _ = clear_all_data();
        app.notify("All data cleared.".to_string());
    });
}

/// Handles key events for the `ClearArticleCache` confirmation dialog.
pub(super) fn handle_confirm_clear_cache(app: &mut App, key: KeyEvent) {
    handle_confirm_dialog(app, key, |app| {
        let _ = clear_article_cache();
        app.article_cache_size = 0;
        // Reset in-memory article state; keep fetched=true so spinner doesn't show
        for feed in app.feeds.iter_mut() {
            feed.articles.clear();
            feed.fetched = true;
            feed.fetch_error = None;
            feed.unread_count = 0;
        }
        app.user_data.read_links.clear();
        let _ = save_user_data(&app.user_data);
        app.notify("Article cache cleared.".to_string());
    });
}

/// Open the import path prompt for `format`.
fn open_import(app: &mut App, format: FileFormat) {
    app.file_prompt.format = format;
    app.file_prompt.path_input.clear();
    app.state = AppState::ImportPath;
}

/// Open the export path prompt for `format`, prefilled with a suggested path.
fn open_export(app: &mut App, format: FileFormat) {
    let input = &mut app.file_prompt.path_input;
    input.text = default_export_path(format.default_file_name());
    input.cursor = input.text.chars().count();
    app.file_prompt.format = format;
    app.state = AppState::ExportPath;
}

/// Handles key events for the `ImportPath` and `ExportPath` text-input states.
///
/// On Enter, the path is expanded (tilde support) and the file is imported or
/// exported in the prompt's format.
pub(super) fn handle_file_prompt(app: &mut App, key: KeyEvent, tx: &UnboundedSender<AppEvent>) {
    match key.code {
        KeyCode::Enter => {
            let raw = app.file_prompt.path_input.text.trim().to_string();
            if raw.is_empty() {
                app.notify("Path cannot be empty.".to_string());
                return;
            }
            let path = expand_home_dir(&raw);
            let export = app.state == AppState::ExportPath;
            match (app.file_prompt.format, export) {
                (FileFormat::Opml, true) => {
                    match export_opml_to_path(&path, &app.feeds, &app.categories) {
                        Ok(()) => app.notify(format!("Exported to {raw}")),
                        Err(e) => app.notify(format!("Export failed: {e}")),
                    }
                }
                (FileFormat::Opml, false) => import_opml(app, &path, tx),
                (FileFormat::Saved, true) => {
                    match saved_io::export_saved(Path::new(&path), &app.user_data, &app.feeds) {
                        Ok(n) => app.notify(format!("Exported {n} saved article(s) to {raw}")),
                        Err(e) => app.notify(format!("Export failed: {e}")),
                    }
                }
                (FileFormat::Saved, false) => import_saved(app, &path),
            }
            app.file_prompt.path_input.clear();
            app.state = AppState::SettingsList;
        }
        KeyCode::Esc => app.unselect(),
        _ => app.file_prompt.path_input.handle_key(key.code, None),
    }
}

/// Add the feeds and categories from an OPML file, fetching each new feed.
fn import_opml(app: &mut App, path: &str, tx: &UnboundedSender<AppEvent>) {
    match import_opml_from_path(path, &app.feeds, &app.categories) {
        Ok((new_feeds, new_cats)) if new_feeds.is_empty() && new_cats.is_empty() => {
            app.notify("No new feeds found in OPML file.".to_string());
        }
        Ok((new_feeds, new_cats)) => {
            let feed_count = new_feeds.len();
            let cat_count = new_cats.len();
            let first_new_idx = app.feeds.len();
            app.feeds_total += feed_count;
            app.feeds_pending += feed_count;
            for (i, feed) in new_feeds.iter().enumerate() {
                let tx2 = tx.clone();
                let url = feed.url.clone();
                let idx = first_new_idx + i;
                tokio::spawn(async move {
                    let result = fetch_feed(&url).await;
                    let _ = tx2.send(AppEvent::FeedFetched(idx, result));
                });
            }
            app.feeds.extend(new_feeds);
            app.categories.extend(new_cats);
            let _ = save_feeds(&app.feeds);
            let _ = save_categories(&app.categories);
            app.notify(format!(
                "Imported {feed_count} feed(s), {cat_count} category(s)"
            ));
        }
        Err(e) => app.notify(format!("Import failed: {e}")),
    }
}

/// Merge a starred-items export into the saved articles.
fn import_saved(app: &mut App, path: &str) {
    let imported = match saved_io::read_saved(Path::new(path)) {
        Ok(imported) => imported,
        Err(e) => {
            app.notify(format!("Import failed: {e}"));
            return;
        }
    };
    let (articles, categories) = saved_io::merge_saved(&mut app.user_data, imported);
    if articles == 0 {
        app.notify("No new saved articles in that file.".to_string());
        return;
    }
    let _ = save_user_data(&app.user_data);
    // Articles already in a feed show their saved marker right away.
    let saved = crate::events::saved_links(&app.user_data.saved_articles);
    for article in app.feeds.iter_mut().flat_map(|f| f.articles.iter_mut()) {
        article.is_saved |= saved.contains(article.link.as_str());
    }
    app.notify(format!(
        "Imported {articles} saved article(s), {categories} new category(s)"
    ));
}
