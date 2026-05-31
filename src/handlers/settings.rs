//! Key event handling for the settings screen and its modal sub-states.
//!
//! Covers `SettingsList`, the two-step `AddFeed` wizard, `OPMLImportPath`/`OPMLExportPath` text
//! inputs, and `ClearData`/`ClearArticleCache` confirmation dialogs.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::App,
    fetch::{fetch_feed, fetch_feed_title},
    models::{AddFeedStep, AppEvent, AppState, Feed, SettingsItem},
    storage::{
        article_cache_size, clear_all_data, clear_article_cache, default_export_path,
        expand_home_dir, export_opml_to_path, import_opml_from_path, save_categories, save_feeds,
        save_user_data,
    },
    ui::theme::ColorTheme,
};

/// Toggle a boolean field in `app.user_data`, persist, and show status.
macro_rules! toggle_setting {
    ($app:expr, $field:expr, $label:expr) => {{
        $field = !$field;
        let _ = save_user_data(&$app.user_data);
        let state = if $field { "ON" } else { "OFF" };
        $app.set_status(format!("{}: {state}", $label));
    }};
}

/// Set a boolean field in `app.user_data` to a fixed value, persist, and show status.
macro_rules! set_setting {
    ($app:expr, $field:expr, $value:expr, $label:expr) => {{
        $field = $value;
        let _ = save_user_data(&$app.user_data);
        let state = if $field { "ON" } else { "OFF" };
        $app.set_status(format!("{}: {state}", $label));
    }};
}

/// Handles key events for the `SettingsList` state.
///
/// Refreshes the article cache size on every keypress, toggles boolean settings in-place, and
/// transitions to sub-states for destructive actions. Returns `true` to quit.
pub(super) fn handle_settings(app: &mut App, key: KeyEvent) -> bool {
    // Refresh cache size each time the user interacts with the settings screen.
    app.article_cache_size = article_cache_size();
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Esc => app.unselect(),
        KeyCode::Tab => app.switch_tab_right(),
        KeyCode::BackTab => app.switch_tab_left(),
        KeyCode::Up => app.previous(),
        KeyCode::Down => app.next(),
        KeyCode::Enter => match app.settings_selected {
            SettingsItem::ImportOpml => {
                app.opml.path_input.clear();
                app.opml.input_cursor = 0;
                app.state = AppState::OPMLImportPath;
            }
            SettingsItem::ExportOpml => {
                app.opml.path_input = default_export_path();
                app.opml.input_cursor = app.opml.path_input.chars().count();
                app.state = AppState::OPMLExportPath;
            }
            SettingsItem::ClearData => {
                app.state = AppState::ClearData;
            }
            SettingsItem::CacheFullArticles => {
                toggle_setting!(
                    app,
                    app.user_data.cache_full_articles,
                    "Save Article Content"
                );
            }
            SettingsItem::ClearArticleCache => {
                app.state = AppState::ClearArticleCache;
            }
            SettingsItem::AutoFetchOnStart => {
                app.user_data.fetch_policy = app.user_data.fetch_policy.next();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!(
                    "Fetch Policy: {}",
                    app.user_data.fetch_policy.label()
                ));
            }
            SettingsItem::ArchivePolicy => {
                app.user_data.archive_policy = app.user_data.archive_policy.next();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!(
                    "Archive Policy: {}",
                    app.user_data.archive_policy.label()
                ));
            }
            SettingsItem::ScrollLoop => {
                toggle_setting!(app, app.user_data.scroll_loop, "Scroll Loop");
            }
            SettingsItem::BorderStyle => {
                toggle_setting!(app, app.user_data.border_rounded, "Rounded Borders");
            }
            SettingsItem::BodyAlignment => {
                app.cycle_alignment_next();
                app.user_data.body_alignment = app.alignment_label().to_lowercase().to_string();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!("Body Alignment: {}", app.alignment_label()));
            }
            SettingsItem::ZenWidth => {
                app.cycle_zen_width_next();
                app.user_data.zen_width = app.zen_width;
                let _ = save_user_data(&app.user_data);
                app.set_status(format!("Zen Width: {}%", app.zen_width));
            }
            SettingsItem::Theme => {
                // Position cursor on the currently active theme.
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
        },
        KeyCode::Left | KeyCode::Char('h') => match app.settings_selected {
            SettingsItem::ArchivePolicy => {
                app.user_data.archive_policy = app.user_data.archive_policy.prev();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!(
                    "Archive Policy: {}",
                    app.user_data.archive_policy.label()
                ));
            }
            SettingsItem::AutoFetchOnStart => {
                app.user_data.fetch_policy = app.user_data.fetch_policy.prev();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!(
                    "Fetch Policy: {}",
                    app.user_data.fetch_policy.label()
                ));
            }
            SettingsItem::CacheFullArticles => {
                set_setting!(
                    app,
                    app.user_data.cache_full_articles,
                    false,
                    "Save Article Content"
                );
            }
            SettingsItem::ScrollLoop => {
                set_setting!(app, app.user_data.scroll_loop, false, "Scroll Loop");
            }
            SettingsItem::BorderStyle => {
                set_setting!(app, app.user_data.border_rounded, false, "Rounded Borders");
            }
            SettingsItem::BodyAlignment => {
                app.cycle_alignment_prev();
                app.user_data.body_alignment = app.alignment_label().to_lowercase().to_string();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!("Body Alignment: {}", app.alignment_label()));
            }
            SettingsItem::ZenWidth => {
                app.cycle_zen_width_prev();
                app.user_data.zen_width = app.zen_width;
                let _ = save_user_data(&app.user_data);
                app.set_status(format!("Zen Width: {}%", app.zen_width));
            }
            _ => {}
        },
        KeyCode::Right | KeyCode::Char('l') => match app.settings_selected {
            SettingsItem::ArchivePolicy => {
                app.user_data.archive_policy = app.user_data.archive_policy.next();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!(
                    "Archive Policy: {}",
                    app.user_data.archive_policy.label()
                ));
            }
            SettingsItem::AutoFetchOnStart => {
                app.user_data.fetch_policy = app.user_data.fetch_policy.next();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!(
                    "Fetch Policy: {}",
                    app.user_data.fetch_policy.label()
                ));
            }
            SettingsItem::CacheFullArticles => {
                set_setting!(
                    app,
                    app.user_data.cache_full_articles,
                    true,
                    "Save Article Content"
                );
            }
            SettingsItem::ScrollLoop => {
                set_setting!(app, app.user_data.scroll_loop, true, "Scroll Loop");
            }
            SettingsItem::BorderStyle => {
                set_setting!(app, app.user_data.border_rounded, true, "Rounded Borders");
            }
            SettingsItem::BodyAlignment => {
                app.cycle_alignment_next();
                app.user_data.body_alignment = app.alignment_label().to_lowercase().to_string();
                let _ = save_user_data(&app.user_data);
                app.set_status(format!("Body Alignment: {}", app.alignment_label()));
            }
            SettingsItem::ZenWidth => {
                app.cycle_zen_width_next();
                app.user_data.zen_width = app.zen_width;
                let _ = save_user_data(&app.user_data);
                app.set_status(format!("Zen Width: {}%", app.zen_width));
            }
            _ => {}
        },
        _ => {}
    }
    false
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
                let url = app.add_feed.url_input.trim().to_string();
                if url.is_empty() {
                    return;
                }
                app.add_feed.url = url.clone();
                app.add_feed.url_input.clear();
                app.add_feed.input_cursor = 0;
                app.add_feed.fetched_title = None;
                app.add_feed.step = AddFeedStep::Title;
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let result = fetch_feed_title(&url).await;
                    let _ = tx2.send(AppEvent::FeedTitleFetched(result));
                });
            }
            KeyCode::Esc => app.unselect(),
            _ => super::handle_text_input(
                &mut app.add_feed.url_input,
                &mut app.add_feed.input_cursor,
                key.code,
                None,
            ),
        }
    } else {
        match key.code {
            KeyCode::Enter => {
                let typed = app.add_feed.url_input.trim().to_string();
                let title = if typed.is_empty() {
                    match app.add_feed.fetched_title.clone() {
                        Some(t) if !t.is_empty() => t,
                        _ => {
                            app.set_status("Title is required.".to_string());
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
                app.feeds.push(Feed {
                    title: title.clone(),
                    url: url.clone(),
                    category_id: target_category,
                    order: next_order,
                    unread_count: 0,
                    articles: vec![],
                    fetched: false,
                    fetch_error: None,
                    feed_updated_secs: None,
                    last_fetched_secs: None,
                });
                let _ = save_feeds(&app.feeds);
                app.set_status(format!("Feed '{title}' added!"));
                let tx2 = tx.clone();
                let idx = app.feeds.len() - 1;
                tokio::spawn(async move {
                    let result = fetch_feed(&url).await;
                    let _ = tx2.send(AppEvent::FeedFetched(idx, result));
                });
                app.add_feed.url_input.clear();
                app.add_feed.input_cursor = 0;
                app.add_feed.step = AddFeedStep::Url;
                app.add_feed.url.clear();
                app.add_feed.fetched_title = None;
                app.state = app.add_feed.return_state.clone();
            }
            KeyCode::Esc => app.unselect(),
            _ => super::handle_text_input(
                &mut app.add_feed.url_input,
                &mut app.add_feed.input_cursor,
                key.code,
                None,
            ),
        }
    }
}

/// Handles key events for the `ClearData` confirmation dialog.
///
/// Enter wipes all feeds, categories, and user data from both memory and disk; Esc or `q` cancels.
pub(super) fn handle_confirm_delete_all(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            app.feeds.clear();
            app.categories.clear();
            app.user_data = crate::models::UserData::default();
            app.saved_view_articles.clear();
            app.in_saved_context = false;
            app.selected_feed = 0;
            app.selected_article = 0;
            app.sidebar_cursor = 0;
            let _ = clear_all_data();
            app.set_status("All data cleared.".to_string());
            app.state = AppState::SettingsList;
        }
        KeyCode::Esc | KeyCode::Char('q') => app.state = AppState::SettingsList,
        _ => {}
    }
}

/// Handles key events for the `ClearArticleCache` confirmation dialog.
///
/// Enter clears the on-disk article cache, resets all in-memory article lists, and clears the
/// read-links set; Esc or `q` cancels.
pub(super) fn handle_confirm_clear_cache(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let _ = clear_article_cache();
            app.article_cache_size = 0;
            // Reset in-memory article state; keep fetched=true so spinner doesn't show
            for feed in app.feeds.iter_mut() {
                feed.articles.clear();
                feed.fetched = true;
                feed.fetch_error = None;
                feed.unread_count = 0;
            }
            // Clear read list and persist
            app.user_data.read_links.clear();
            let _ = save_user_data(&app.user_data);
            app.set_status("Article cache cleared.".to_string());
            app.state = AppState::SettingsList;
        }
        KeyCode::Esc | KeyCode::Char('q') => app.state = AppState::SettingsList,
        _ => {}
    }
}

/// Handles key events for the `OPMLImportPath` and `OPMLExportPath` text-input states.
///
/// On Enter, the path is expanded (tilde support) and either exported or imported. A successful
/// import spawns one background fetch task per new feed and extends the live feed list.
pub(super) fn handle_opml_path(app: &mut App, key: KeyEvent, tx: &UnboundedSender<AppEvent>) {
    match key.code {
        KeyCode::Enter => {
            let raw = app.opml.path_input.trim().to_string();
            if raw.is_empty() {
                app.set_status("Path cannot be empty.".to_string());
                return;
            }
            let path = expand_home_dir(&raw);
            if app.state == AppState::OPMLExportPath {
                match export_opml_to_path(&path, &app.feeds, &app.categories) {
                    Ok(()) => app.set_status(format!("Exported to {raw}")),
                    Err(e) => app.set_status(format!("Export failed: {e}")),
                }
            } else {
                match import_opml_from_path(&path, &app.feeds, &app.categories) {
                    Ok((new_feeds, new_cats)) if new_feeds.is_empty() && new_cats.is_empty() => {
                        app.set_status("No new feeds found in OPML file.".to_string());
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
                        app.set_status(format!(
                            "Imported {feed_count} feed(s), {cat_count} category(s)"
                        ));
                    }
                    Err(e) => app.set_status(format!("Import failed: {e}")),
                }
            }
            app.opml.path_input.clear();
            app.opml.input_cursor = 0;
            app.state = AppState::SettingsList;
        }
        KeyCode::Esc => app.unselect(),
        _ => super::handle_text_input(
            &mut app.opml.path_input,
            &mut app.opml.input_cursor,
            key.code,
            None,
        ),
    }
}
