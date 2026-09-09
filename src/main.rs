//! Application entry point. Initializes the terminal, sets up the Tokio runtime, spawns background tasks (feed and image fetches), and drives the main MPSC event loop that coordinates Ratatui rendering with input handling.

mod app;
mod fetch;
mod handlers;
mod models;
mod state;
mod storage;
mod sync;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use models::{AddFeedStep, AppEvent, AppState, FeedSource, FetchPolicy};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::sync::mpsc;

use crate::app::App;
use crate::ui::content::utils::now_secs;

use image::DynamicImage;

fn is_svg(bytes: &[u8], url: &str) -> bool {
    if url.to_lowercase().ends_with(".svg") {
        return true;
    }
    let sample = bytes.get(..4096).unwrap_or(bytes);
    let lower = String::from_utf8_lossy(sample).to_lowercase();
    lower.contains("<svg")
}

fn render_svg(bytes: &[u8]) -> Option<DynamicImage> {
    let text = std::str::from_utf8(bytes).ok()?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(text, &opt).ok()?;
    let size = tree.size().to_int_size();
    let w = size.width().max(1);
    let h = size.height().max(1);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let data = pixmap.take();
    let rgba = image::RgbaImage::from_raw(w, h, data)?;
    Some(DynamicImage::ImageRgba8(rgba))
}

/// Entry point for the application. Sets up terminal raw mode, alternate screen, mouse capture, and delegates to `run()` for the main event loop.
#[tokio::main]
async fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Main event loop. Loads cached articles, spawns background fetch tasks, and continuously processes input events, feed updates, and UI ticks until the user quits.
async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = app::App::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Spawn the crossterm keyboard polling thread
    let tx_input = tx.clone();
    std::thread::spawn(move || {
        let tick = Duration::from_millis(250);
        loop {
            if event::poll(tick).unwrap_or(false)
                && let Ok(Event::Key(key)) = event::read()
            {
                let _ = tx_input.send(AppEvent::Input(key));
            }
            let _ = tx_input.send(AppEvent::Tick);
        }
    });

    // Check crates.io for a newer version in the background.
    {
        let tx_ver = tx.clone();
        tokio::spawn(async move {
            if let Some(info) = fetch::check_latest_version().await {
                let _ = tx_ver.send(AppEvent::UpdateAvailable(info));
            }
        });
    }

    // Load cached articles and apply to feeds before first fetch
    let cached = storage::load_articles();
    for feed in &mut app.feeds {
        if let Some(articles) = cached.get(&feed.url) {
            let mut arts = articles.clone();
            for art in &mut arts {
                apply_user_flags(
                    art,
                    &app.user_data.read_links,
                    &app.user_data.saved_articles,
                );
                art.source_feed = feed.title.clone();
            }
            feed.unread_count = arts.iter().filter(|a| !a.is_read).count();
            feed.articles = arts;
        }
    }

    // Re-populate category/all-feeds view now that feed articles are loaded from cache.
    if app.in_all_feeds_context {
        app.populate_all_feeds_view();
    } else if let Some(cat_id) = app.selected_sidebar_category {
        app.populate_category_view(cat_id);
    }

    // Kick off initial feed fetches for all persisted feeds (unless disabled in settings).
    if app.user_data.fetch_policy != FetchPolicy::Never {
        let ts = now_secs();

        // Determine which feeds need fetching based on the policy and last-fetched time.
        let fetch_indices: Vec<usize> = app
            .feeds
            .iter()
            .enumerate()
            .filter(|(_, feed)| match app.user_data.fetch_policy {
                FetchPolicy::OnStart => true,
                FetchPolicy::Never => false,
                FetchPolicy::EveryHour => {
                    feed.last_fetched_secs.is_none_or(|last| ts - last >= 3600)
                }
                FetchPolicy::EveryDay => {
                    feed.last_fetched_secs.is_none_or(|last| ts - last >= 86400)
                }
            })
            .map(|(idx, _)| idx)
            .collect();

        let fetch_count = fetch_indices.len();
        app.feeds_total = 0;
        app.feeds_pending = 0;
        if fetch_count > 0 {
            app.set_status("Fetching feeds...");
        }

        // Mark skipped feeds as already fetched so the spinner doesn't show for them.
        let fetch_set: std::collections::HashSet<usize> = fetch_indices.iter().copied().collect();
        for (idx, feed) in app.feeds.iter_mut().enumerate() {
            if !fetch_set.contains(&idx) {
                feed.fetched = true;
            }
        }

        for idx in fetch_indices {
            app.refresh_feed_at(idx, &tx);
        }
    } else {
        for feed in &mut app.feeds {
            feed.fetched = true;
        }
        app.set_status("");
    }

    // Detect terminal image protocol (Kitty / Sixel / halfblock fallback).
    app.picker = Some(
        limner::render_image::Picker::from_query_stdio()
            .unwrap_or_else(|_| limner::render_image::halfblock_picker()),
    );

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        let Some(event) = rx.recv().await else { break };

        match event {
            AppEvent::Input(key) => {
                if handlers::handle_key(&mut app, key, &tx).await {
                    return Ok(()); // quit requested
                }
            }

            AppEvent::Tick => {
                app.tick = app.tick.wrapping_add(1);
            }

            AppEvent::FeedFetched(idx, result) => {
                on_feed_fetched(&mut app, idx, result, &tx);
            }

            AppEvent::FreshRssFeedPulled(idx, result) => {
                on_freshrss_feed_pulled(&mut app, idx, result);
            }

            AppEvent::FreshRssSynced(result) => {
                on_freshrss_synced(&mut app, result);
            }

            AppEvent::FreshRssPushDone(result) => {
                if let Err(e) = result {
                    app.set_status(format!("⚠ FreshRSS push failed: {e}"));
                }
            }

            AppEvent::FeedTitleFetched(result) => {
                if app.state == AppState::AddFeed && app.add_feed.step == AddFeedStep::Title {
                    app.add_feed.fetched_title = Some(result.unwrap_or_default());
                }
            }

            AppEvent::UpdateAvailable(info) => {
                app.update_popup_scroll = 0;
                app.update_available = Some(info);
            }

            AppEvent::ImageDownloaded(url, result) => {
                app.image_pending.remove(&url);
                if let Ok(bytes) = result {
                    let img = if is_svg(&bytes, &url) {
                        render_svg(&bytes)
                    } else {
                        limner::render_image::img_crate::load_from_memory(&bytes).ok()
                    };
                    if let Some(img) = img {
                        app.image_cache.insert(url, img);
                    }
                }
            }

            AppEvent::FullArticleFetched(source, art_idx, result) => {
                app.article_fetching = false;
                match source {
                    FeedSource::Saved => {
                        if let Some(article) = app.saved_view_articles.get_mut(art_idx) {
                            let selected = app.selected_article == art_idx;
                            let (msg, lc) = apply_full_article_result(article, result);
                            if selected {
                                app.content_line_count = lc;
                            }
                            app.set_status(msg);
                        }
                    }
                    FeedSource::Feed(feed_idx) => {
                        if let Some(feed) = app.feeds.get_mut(feed_idx)
                            && let Some(article) = feed.articles.get_mut(art_idx)
                        {
                            let selected = art_idx == app.selected_article;
                            let (msg, lc) = apply_full_article_result(article, result);
                            if selected {
                                app.content_line_count = lc;
                            }
                            app.set_status(msg);
                        } else {
                            app.set_status("Unknown article.");
                        }
                    }
                }
            }
        }

        spawn_article_image_downloads(&mut app, &tx);
    }

    Ok(())
}

/// Merge the user's read-links and saved-articles state onto an article in-place.
fn apply_user_flags(
    art: &mut models::Article,
    read_links: &std::collections::HashSet<String>,
    saved_articles: &[models::SavedArticle],
) {
    art.is_read = !art.link.is_empty() && read_links.contains(&art.link);
    art.is_saved =
        !art.link.is_empty() && saved_articles.iter().any(|s| s.article.link == art.link);
}

/// Merges archived articles from the previous fetch into the updated article list,
/// applying the user's archive policy to drop articles older than the threshold.
///
/// Steps:
/// - Carries forward already-archived articles not present in the new fetch.
/// - Marks newly-absent, non-saved articles as archived.
/// - Drops archived articles older than `archive_policy.threshold_secs()`.
/// - Appends surviving archived articles to `articles`.
fn apply_archive_policy(
    articles: &mut Vec<models::Article>,
    previous: &[models::Article],
    archive_policy: &models::ArchivePolicy,
    now_secs: i64,
    read_links: &std::collections::HashSet<String>,
    saved_articles: &[models::SavedArticle],
) {
    let new_links: std::collections::HashSet<&str> =
        articles.iter().map(|a| a.link.as_str()).collect();

    // Step A — carry forward already-archived articles absent from the new fetch.
    let already_archived: Vec<models::Article> = previous
        .iter()
        .filter(|a| a.is_archived && !new_links.contains(a.link.as_str()))
        .cloned()
        .collect();

    // Step B — newly archived: was present before, not saved, not in new fetch, not yet archived.
    let newly_archived: Vec<models::Article> = previous
        .iter()
        .filter(|a| !a.is_archived && !a.is_saved && !new_links.contains(a.link.as_str()))
        .map(|a| {
            let mut art = a.clone();
            art.is_archived = true;
            apply_user_flags(&mut art, read_links, saved_articles);
            art
        })
        .collect();

    // Combine archived candidates, then apply the retention threshold (Step C).
    let threshold = archive_policy.threshold_secs();
    let surviving: Vec<models::Article> = already_archived
        .into_iter()
        .chain(newly_archived)
        .filter(|a| {
            let effective_ts = a.published_secs.unwrap_or(now_secs);
            match threshold {
                Some(t) => now_secs - effective_ts <= t,
                None => true, // Forever — keep all
            }
        })
        .collect();

    // Step D — append surviving archived articles to the new article list.
    articles.extend(surviving);
}

/// Apply a full-article fetch result to an article in-place, updating content and error state.
/// Returns a `(status_msg, line_count)` pair.
fn apply_full_article_result(
    article: &mut models::Article,
    result: Result<String, String>,
) -> (String, usize) {
    match result {
        Ok(html) => {
            article.error = None;
            article.content = html_to_markdown_rs::convert(&html, None)
                .ok()
                .and_then(|r| r.content)
                .unwrap_or_default();
            let lc = article.content.lines().count().max(1);
            ("Article loaded.".to_string(), lc)
        }
        Err(e) => {
            article.error = Some(e.to_string());
            article.content = String::new();
            (format!("Extraction failed: {e}"), 1)
        }
    }
}

/// Handle a fetched feed result: merge read/starred state, update counts, save cache.
fn on_feed_fetched(
    app: &mut app::App,
    idx: usize,
    result: Result<(Vec<models::Article>, Option<i64>), String>,
    _tx: &mpsc::UnboundedSender<AppEvent>,
) {
    match result {
        Ok((mut articles, xml_updated_secs)) => {
            let Some(feed) = app.feeds.get_mut(idx) else {
                return;
            };

            let ts = now_secs();

            // Preserve existing article content across feed refreshes.
            let preserved: std::collections::HashMap<String, String> = feed
                .articles
                .iter()
                .filter(|a| !a.content.is_empty())
                .map(|a| (a.link.clone(), a.content.clone()))
                .collect();

            for art in &mut articles {
                apply_user_flags(
                    art,
                    &app.user_data.read_links,
                    &app.user_data.saved_articles,
                );
                art.source_feed = feed.title.clone();
                if let Some(saved) = preserved.get(&art.link) {
                    art.content = saved.clone();
                }
            }

            // Preserve saved articles that were dropped by this refresh.
            {
                let feed_title = &feed.title;
                let new_links: std::collections::HashSet<&str> =
                    articles.iter().map(|a| a.link.as_str()).collect();
                let missing_saved: Vec<crate::models::Article> = app
                    .user_data
                    .saved_articles
                    .iter()
                    .filter(|s| {
                        &s.article.source_feed == feed_title
                            && !new_links.contains(s.article.link.as_str())
                    })
                    .map(|s| {
                        let mut art = s.article.clone();
                        art.is_read = app.user_data.read_links.contains(&art.link);
                        art.is_saved = true;
                        art
                    })
                    .collect();
                articles.extend(missing_saved);
            }

            // Apply archive policy: mark newly-absent articles as archived, carry forward
            // previously-archived articles, and drop those older than the retention threshold.
            apply_archive_policy(
                &mut articles,
                &feed.articles.clone(),
                &app.user_data.archive_policy,
                ts,
                &app.user_data.read_links,
                &app.user_data.saved_articles,
            );

            feed.unread_count = articles.iter().filter(|a| !a.is_read).count();
            let new_len = articles.len();
            feed.articles = articles;
            // Clamp selected article if the list shrank (e.g. archive policy pruned many).
            if app.selected_feed == idx && app.selected_article >= new_len {
                app.selected_article = new_len.saturating_sub(1);
            }
            feed.fetch_error = None;
            feed.fetched = true;
            feed.feed_updated_secs = xml_updated_secs;
            feed.last_fetched_secs = Some(ts);
        }
        Err(e) => {
            let feed_title = app
                .feeds
                .get(idx)
                .map(|f| f.title.clone())
                .unwrap_or_default();
            app.set_status(format!("⚠ {feed_title}: {e}"));
            if let Some(feed) = app.feeds.get_mut(idx) {
                feed.fetch_error = Some(e);
                feed.fetched = true;
            }
        }
    }

    // Shared tail: progress counter, view repopulation, persistence.
    finish_fetch(app);
}

/// Shared post-fetch bookkeeping used by both local and FreshRSS pulls.
fn finish_fetch(app: &mut app::App) {
    // Update fetch progress counter
    app.feeds_pending = app.feeds_pending.saturating_sub(1);

    // Repopulate category/all-feeds views after every fetch so stale
    // (feed_idx, article_idx) pairs never survive to a render frame.
    if app.in_all_feeds_context {
        app.populate_all_feeds_view();
    } else if let Some(cat_id) = app.selected_sidebar_category {
        app.populate_category_view(cat_id);
    }

    if app.feeds_pending == 0 {
        app.feeds_total = 0;
        app.set_status("Feeds loaded.");
        let _ = storage::save_feeds(&app.feeds);
    }

    // Persist article cache
    let _ = storage::save_articles(&app.feeds, app.user_data.cache_full_articles);
}

/// Merge server read/star state with local overlays (local reads and saves win).
fn apply_freshrss_flags(app: &mut app::App, feed_title: &str, articles: &mut [models::Article]) {
    let mut user_data_changed = false;
    for art in articles.iter_mut() {
        art.source_feed = feed_title.to_string();
        if !art.link.is_empty() && app.user_data.read_links.contains(&art.link) {
            art.is_read = true;
        }
        if !art.link.is_empty()
            && app
                .user_data
                .saved_articles
                .iter()
                .any(|s| s.article.link == art.link)
        {
            art.is_saved = true;
        }
        // Fold server-side reads into the local read set so badges, archive
        // policy, and cache-clear flows stay consistent.
        if art.is_read && !art.link.is_empty() && !app.user_data.read_links.contains(&art.link) {
            app.user_data.read_links.insert(art.link.clone());
            user_data_changed = true;
        }
    }
    if user_data_changed {
        let _ = storage::save_user_data(&app.user_data);
    }
}

/// Handle a FreshRSS article pull for one feed: merge, recount, save.
fn on_freshrss_feed_pulled(
    app: &mut app::App,
    idx: usize,
    result: Result<Vec<crate::sync::freshrss::ImportedArticle>, String>,
) {
    let ts = now_secs();
    match result {
        Ok(pulled) => {
            // Merge server articles (server read/star wins), then overlay local flags.
            let title = match app.feeds.get_mut(idx) {
                Some(feed) => {
                    crate::sync::freshrss::merge_articles(&mut feed.articles, pulled);
                    feed.fetch_error = None;
                    feed.fetched = true;
                    feed.last_fetched_secs = Some(ts);
                    feed.title.clone()
                }
                None => return,
            };
            // Take articles out so the flags helper can borrow `app` mutably.
            let mut articles = std::mem::take(&mut app.feeds[idx].articles);
            apply_freshrss_flags(app, &title, &mut articles);
            let feed = &mut app.feeds[idx];
            feed.articles = articles;
            feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
            let new_len = feed.articles.len();
            if app.selected_feed == idx && app.selected_article >= new_len {
                app.selected_article = new_len.saturating_sub(1);
            }
        }
        Err(e) => {
            let feed_title = app
                .feeds
                .get(idx)
                .map(|f| f.title.clone())
                .unwrap_or_default();
            app.set_status(format!("⚠ {feed_title}: {e}"));
            if let Some(feed) = app.feeds.get_mut(idx) {
                feed.fetch_error = Some(e);
                feed.fetched = true;
            }
        }
    }

    finish_fetch(app);
}

/// Find a category by name or create it at root level. Returns its id.
fn sync_category_id(app: &mut app::App, label: &str) -> Option<models::CategoryId> {
    if let Some(id) = app
        .categories
        .iter()
        .find(|c| c.name == label)
        .map(|c| c.id)
    {
        return Some(id);
    }
    let next_id = app.categories.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let next_order = app.categories.iter().map(|c| c.order).max().unwrap_or(0) + 1;
    app.categories.push(models::Category {
        id: next_id,
        name: label.to_string(),
        parent_id: None,
        order: next_order,
    });
    Some(next_id)
}

/// Handle a full FreshRSS sync: upsert subscriptions, merge articles, persist.
fn on_freshrss_synced(app: &mut app::App, result: Result<crate::sync::client::SyncBundle, String>) {
    let mut bundle = match result {
        Ok(b) => b,
        Err(e) => {
            app.set_status(format!("⚠ FreshRSS sync failed: {e}"));
            app.feeds_pending = app.feeds_pending.saturating_sub(1);
            return;
        }
    };
    app.freshrss_auth = Some(bundle.auth.clone());

    // Ensure one category per distinct server label.
    let mut labels: Vec<String> = bundle
        .subs
        .iter()
        .filter_map(|s| s.category_label.clone())
        .collect();
    labels.sort();
    labels.dedup();
    for label in labels {
        sync_category_id(app, &label);
    }
    let _ = storage::save_categories(&app.categories);

    let mut next_order = app.feeds.iter().map(|f| f.order).max().unwrap_or(0) + 1;
    let (added_feeds, _) = {
        let categories = &app.categories;
        crate::sync::freshrss::upsert_subscriptions(
            &mut app.feeds,
            bundle.subs,
            &mut |label| label.and_then(|l| categories.iter().find(|c| c.name == l).map(|c| c.id)),
            &mut || {
                let o = next_order;
                next_order += 1;
                o
            },
        )
    };

    // Fan out pulled articles to their feeds.
    let mut new_articles = 0;
    let feed_indices: Vec<usize> = (0..app.feeds.len()).collect();
    for idx in feed_indices {
        let remote_id = app.feeds[idx].remote_id.clone();
        let Some(remote_id) = remote_id else {
            continue;
        };
        let Some(pulled) = bundle.articles.remove(&remote_id) else {
            continue;
        };
        if pulled.is_empty() {
            continue;
        }
        let title = app.feeds[idx].title.clone();
        let mut articles = std::mem::take(&mut app.feeds[idx].articles);
        let added = crate::sync::freshrss::merge_articles(&mut articles, pulled);
        new_articles += added;
        apply_freshrss_flags(app, &title, &mut articles);
        let feed = &mut app.feeds[idx];
        feed.articles = articles;
        feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
        feed.fetched = true;
        feed.fetch_error = None;
        feed.last_fetched_secs = Some(now_secs());
    }

    if let Some(cfg) = app.user_data.freshrss.as_mut() {
        cfg.last_sync_secs = Some(now_secs());
    }
    let _ = storage::save_user_data(&app.user_data);
    let _ = storage::save_feeds(&app.feeds);
    let _ = storage::save_articles(&app.feeds, app.user_data.cache_full_articles);
    app.set_status(format!(
        "FreshRSS sync complete: {added_feeds} new feeds, {new_articles} new articles."
    ));
    app.feeds_pending = app.feeds_pending.saturating_sub(1);
    if app.in_all_feeds_context {
        app.populate_all_feeds_view();
    } else if let Some(cat_id) = app.selected_sidebar_category {
        app.populate_category_view(cat_id);
    }
}

/// Spawn background downloads for article images not yet cached or pending.
fn spawn_article_image_downloads(app: &mut App, tx: &tokio::sync::mpsc::UnboundedSender<AppEvent>) {
    for img in app.article_images.clone() {
        if app.image_cache.contains_key(&img.url) || app.image_pending.contains(&img.url) {
            continue;
        }
        app.image_pending.insert(img.url.clone());
        let tx = tx.clone();
        let url = img.url.clone();
        tokio::spawn(async move {
            let result = fetch::fetch_image(&url).await;
            let _ = tx.send(AppEvent::ImageDownloaded(url, result));
        });
    }
}
