mod app;
mod fetch;
mod handlers;
mod models;
mod storage;
mod ui;

use anyhow::Result;
use models::{AddFeedStep, AppEvent, AppState, FeedSource, CONTENT_STUB_MAX_LEN};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fetch::fetch_feed;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use tokio::sync::mpsc;

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
                art.is_starred = !art.link.is_empty()
                    && app
                        .user_data
                        .starred_articles
                        .iter()
                        .any(|a| a.link == art.link);
                art.source_feed = feed.title.clone();
            }
            feed.unread_count = arts.iter().filter(|a| !a.is_read).count();
            feed.articles = arts;
        }
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


            AppEvent::FullArticleFetched(source, art_idx, result) => match source {
                FeedSource::Favorites => {
                    let status_msg = if let Some(article) =
                        app.favorite_view_articles.get_mut(art_idx)
                    {
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
                            app.content_line_count =
                                article.content.lines().count().max(1);
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
            },

            AppEvent::FeedTitleFetched(result) => {
                if app.state == AppState::AddFeed && app.add_feed_step == AddFeedStep::Title {
                    app.add_feed_fetched_title = Some(result.unwrap_or_default());
                }
            }
        }
    }

    Ok(())
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

            // Preserve readability-enriched content for articles we already have.
            let preserved: std::collections::HashMap<String, String> = feed
                .articles
                .iter()
                .filter(|a| a.content.len() >= CONTENT_STUB_MAX_LEN)
                .map(|a| (a.link.clone(), a.content.clone()))
                .collect();

            for art in &mut articles {
                art.is_read = !art.link.is_empty() && app.user_data.read_links.contains(&art.link);
                art.is_starred = !art.link.is_empty()
                    && app
                        .user_data
                        .starred_articles
                        .iter()
                        .any(|a| a.link == art.link);
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

            feed.unread_count = articles.iter().filter(|a| !a.is_read).count();
            feed.articles = articles;
            feed.fetch_error = None;
            feed.fetched = true;
            feed.feed_updated_secs = xml_updated_secs;
            feed.last_fetched_secs = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            );
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
