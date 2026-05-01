//! Application entry point. Initializes the terminal, sets up the Tokio runtime, spawns background tasks (feed and image fetches), and drives the main MPSC event loop that coordinates Ratatui rendering with input handling.

mod app;
mod fetch;
mod handlers;
mod models;
mod storage;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use fetch::fetch_feed;
use models::{AddFeedStep, AppEvent, AppState, CONTENT_STUB_MAX_LEN, FeedSource};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::sync::mpsc;

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

    // Load cached articles and apply to feeds before first fetch
    let cached = storage::load_articles();
    for feed in &mut app.feeds {
        if let Some(articles) = cached.get(&feed.url) {
            let mut arts = articles.clone();
            for art in &mut arts {
                art.is_read = !art.link.is_empty() && app.user_data.read_links.contains(&art.link);
                art.is_saved = !art.link.is_empty()
                    && app
                        .user_data
                        .saved_articles
                        .iter()
                        .any(|s| s.article.link == art.link);
                art.source_feed = feed.title.clone();
            }
            feed.unread_count = arts.iter().filter(|a| !a.is_read).count();
            feed.articles = arts;
        }
    }

    // Re-populate category view now that feed articles are loaded from cache.
    if let Some(cat_id) = app.selected_sidebar_category {
        app.populate_category_view(cat_id);
    }

    // Kick off initial feed fetches for all persisted feeds (unless disabled in settings).
    if app.user_data.auto_fetch_on_start {
        let fetch_count = app.feeds.len();
        app.feeds_total = fetch_count;
        app.feeds_pending = fetch_count;
        if fetch_count > 0 {
            app.set_status("Fetching feeds...");
        }
        for (idx, feed) in app.feeds.iter().enumerate() {
            let tx2 = tx.clone();
            let url = feed.url.clone();
            tokio::spawn(async move {
                let result = fetch_feed(&url).await;
                let _ = tx2.send(AppEvent::FeedFetched(idx, result));
            });
        }
    } else {
        for feed in &mut app.feeds {
            feed.fetched = true;
        }
        app.set_status("");
    }

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

            AppEvent::FullArticleFetched(source, art_idx, result) => {
                app.article_fetching = false;
                match source {
                    FeedSource::Saved => {
                        let status_msg =
                            if let Some(article) = app.saved_view_articles.get_mut(art_idx) {
                                let msg = match result {
                                    Ok(html) => {
                                        article.content = html2md::parse_html(&html);
                                        "Article loaded.".to_string()
                                    }
                                    Err(e) => {
                                        article.content = format!("Failed to load article: {e}");
                                        format!("Extraction failed: {e}")
                                    }
                                };
                                if app.selected_article == art_idx {
                                    app.content_line_count = article.content.lines().count().max(1);
                                }
                                Some(msg)
                            } else {
                                None
                            };
                        if let Some(msg) = status_msg {
                            app.set_status(msg);
                        }
                    }
                    FeedSource::Feed(feed_idx) => {
                        if let Some(feed) = app.feeds.get_mut(feed_idx)
                            && let Some(article) = feed.articles.get_mut(art_idx)
                        {
                            match result {
                                Ok(html) => {
                                    article.content = html2md::parse_html(&html);
                                    app.set_status("Article loaded.");
                                }
                                Err(e) => {
                                    article.content = format!("Failed to load article: {e}");
                                    app.set_status(format!("Extraction failed: {e}"));
                                }
                            }
                            if app.selected_feed == feed_idx && app.selected_article == art_idx {
                                app.content_line_count = app.feeds[feed_idx].articles[art_idx]
                                    .content
                                    .lines()
                                    .count()
                                    .max(1);
                            }
                        }
                    }
                }
            }

            AppEvent::FeedTitleFetched(result) => {
                if app.state == AppState::AddFeed && app.add_feed_step == AddFeedStep::Title {
                    app.add_feed_fetched_title = Some(result.unwrap_or_default());
                }
            }
        }
    }

    Ok(())
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
            art.is_read = !art.link.is_empty() && read_links.contains(&art.link);
            art.is_saved =
                !art.link.is_empty() && saved_articles.iter().any(|s| s.article.link == art.link);
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

            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            // Preserve readability-enriched content for articles we already have.
            let preserved: std::collections::HashMap<String, String> = feed
                .articles
                .iter()
                .filter(|a| a.content.len() >= CONTENT_STUB_MAX_LEN)
                .map(|a| (a.link.clone(), a.content.clone()))
                .collect();

            for art in &mut articles {
                art.is_read = !art.link.is_empty() && app.user_data.read_links.contains(&art.link);
                art.is_saved = !art.link.is_empty()
                    && app
                        .user_data
                        .saved_articles
                        .iter()
                        .any(|s| s.article.link == art.link);
                art.source_feed = feed.title.clone();
                if let Some(saved) = preserved.get(&art.link) {
                    art.content = saved.clone();
                }
            }
            // When eager fetch is OFF, discard content for articles not yet enriched
            // by readability so they get fetched lazily on open.
            if !app.user_data.eager_article_fetch {
                for art in &mut articles {
                    if !preserved.contains_key(&art.link) {
                        art.content = String::new();
                        art.image_url = None;
                    }
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
                now_secs,
                &app.user_data.read_links,
                &app.user_data.saved_articles,
            );

            feed.unread_count = articles.iter().filter(|a| !a.is_read).count();
            feed.articles = articles;
            feed.fetch_error = None;
            feed.fetched = true;
            feed.feed_updated_secs = xml_updated_secs;
            feed.last_fetched_secs = Some(now_secs);
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

    // Update fetch progress counter
    app.feeds_pending = app.feeds_pending.saturating_sub(1);
    if app.feeds_pending == 0 {
        app.feeds_total = 0;
        app.set_status("Feeds loaded.");
    }

    // Persist article cache
    let _ = storage::save_articles(&app.feeds, app.user_data.save_article_content);
}
