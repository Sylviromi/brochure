//! Applies the results of background work (feed fetches, article bodies, images)
//! to [`App`], plus the startup cache load and initial fetch.

use crate::app::App;
use crate::models::{AppEvent, ArchivePolicy, Article, FeedSource, FetchPolicy, SavedArticle};
use crate::storage;
use crate::ui::content::utils::now_secs;
use limner::render_image::img_crate::{self, DynamicImage};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc::UnboundedSender;

/// Links of every saved article, for O(1) "is this saved?" checks.
pub(crate) fn saved_links(saved_articles: &[SavedArticle]) -> HashSet<&str> {
    saved_articles
        .iter()
        .map(|s| s.article.link.as_str())
        .collect()
}

/// Merge the user's read-links and saved-articles state onto an article in-place.
fn apply_user_flags(art: &mut Article, read_links: &HashSet<String>, saved: &HashSet<&str>) {
    let has_link = !art.link.is_empty();
    art.is_read = has_link && read_links.contains(&art.link);
    art.is_saved = has_link && saved.contains(art.link.as_str());
}

/// Attach cached articles (from the previous session) to the loaded feeds.
pub fn load_cached_articles(app: &mut App) {
    let mut cached = storage::load_articles();
    let saved = saved_links(&app.user_data.saved_articles);
    for feed in &mut app.feeds {
        let Some(mut articles) = cached.remove(&feed.url) else {
            continue;
        };
        for art in &mut articles {
            apply_user_flags(art, &app.user_data.read_links, &saved);
            art.source_feed.clone_from(&feed.title);
        }
        feed.unread_count = articles.iter().filter(|a| !a.is_read).count();
        feed.articles = articles;
    }
    app.refresh_article_views();
}

/// Start the launch-time fetch according to the user's fetch policy.
pub fn start_initial_fetch(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    let policy = app.user_data.fetch_policy.clone();
    let max_age = match policy {
        FetchPolicy::OnStart => Some(0),
        FetchPolicy::EveryHour => Some(3600),
        FetchPolicy::EveryDay => Some(86400),
        FetchPolicy::Never => None,
    };
    let started = match max_age {
        Some(max_age) => {
            let ts = now_secs();
            app.refresh_where(
                |feed| {
                    feed.last_fetched_secs
                        .is_none_or(|last| ts - last >= max_age)
                },
                tx,
            )
        }
        None => {
            for feed in &mut app.feeds {
                feed.fetched = true;
            }
            false
        }
    };
    app.set_status(if started { "Fetching feeds..." } else { "" });
}

/// Handle a fetched feed result: merge read/starred state, update counts, save cache.
pub fn on_feed_fetched(
    app: &mut App,
    idx: usize,
    result: Result<(Vec<Article>, Option<i64>), String>,
) {
    let (mut articles, xml_updated_secs) = match result {
        Ok(ok) => ok,
        Err(e) => {
            record_fetch_error(app, idx, e);
            finish_fetch(app);
            return;
        }
    };
    let Some(feed) = app.feeds.get_mut(idx) else {
        return;
    };
    let ts = now_secs();
    let user_data = &app.user_data;
    let saved = saved_links(&user_data.saved_articles);

    // Preserve existing article content across feed refreshes.
    let preserved: HashMap<&str, &str> = feed
        .articles
        .iter()
        .filter(|a| !a.content.is_empty())
        .map(|a| (a.link.as_str(), a.content.as_str()))
        .collect();
    for art in &mut articles {
        apply_user_flags(art, &user_data.read_links, &saved);
        art.source_feed.clone_from(&feed.title);
        if let Some(content) = preserved.get(art.link.as_str()) {
            art.content = (*content).to_string();
        }
    }

    // Keep saved articles that this refresh dropped.
    let new_links: HashSet<&str> = articles.iter().map(|a| a.link.as_str()).collect();
    let missing_saved: Vec<Article> = user_data
        .saved_articles
        .iter()
        .filter(|s| {
            s.article.source_feed == feed.title && !new_links.contains(s.article.link.as_str())
        })
        .map(|s| Article {
            is_read: user_data.read_links.contains(&s.article.link),
            is_saved: true,
            ..s.article.clone()
        })
        .collect();
    articles.extend(missing_saved);

    let archived = archived_survivors(
        &articles,
        &feed.articles,
        &user_data.archive_policy,
        ts,
        &user_data.read_links,
        &saved,
    );
    articles.extend(archived);

    feed.unread_count = articles.iter().filter(|a| !a.is_read).count();
    feed.articles = articles;
    feed.fetch_error = None;
    feed.fetched = true;
    feed.feed_updated_secs = xml_updated_secs;
    feed.last_fetched_secs = Some(ts);
    clamp_selection_after_update(app, idx);
    finish_fetch(app);
}

/// Archived articles to append after a refresh, given the previous article list.
///
/// - Carries forward already-archived articles not present in the new fetch.
/// - Marks newly-absent, non-saved articles as archived.
/// - Drops archived articles older than `archive_policy.threshold_secs()`.
fn archived_survivors(
    fresh: &[Article],
    previous: &[Article],
    archive_policy: &ArchivePolicy,
    now_secs: i64,
    read_links: &HashSet<String>,
    saved: &HashSet<&str>,
) -> Vec<Article> {
    let fresh_links: HashSet<&str> = fresh.iter().map(|a| a.link.as_str()).collect();
    let threshold = archive_policy.threshold_secs();
    previous
        .iter()
        .filter(|a| !fresh_links.contains(a.link.as_str()))
        .filter(|a| a.is_archived || !a.is_saved)
        .filter(|a| {
            let age = now_secs - a.published_secs.unwrap_or(now_secs);
            threshold.is_none_or(|t| age <= t)
        })
        .map(|a| {
            let mut art = a.clone();
            if !art.is_archived {
                art.is_archived = true;
                apply_user_flags(&mut art, read_links, saved);
            }
            art
        })
        .collect()
}

/// Record a failed refresh on feed `idx` and surface it in the status bar.
pub(crate) fn record_fetch_error(app: &mut App, idx: usize, error: String) {
    let Some(feed) = app.feeds.get_mut(idx) else {
        return;
    };
    let msg = format!("⚠ {}: {error}", feed.title);
    feed.fetch_error = Some(error);
    feed.fetched = true;
    app.set_status(msg);
}

/// Clamp the article cursor if feed `idx` is the one being viewed and its list shrank.
pub(crate) fn clamp_selection_after_update(app: &mut App, idx: usize) {
    if app.selected_feed == idx
        && let Some(feed) = app.feeds.get(idx)
    {
        let len = feed.articles.len();
        app.selected_article = app.selected_article.min(len.saturating_sub(1));
    }
}

/// Shared post-fetch bookkeeping used by both local and FreshRSS pulls.
///
/// The article cache is written once per batch (when the last pending fetch lands)
/// rather than after every feed, since it serialises every article of every feed.
pub(crate) fn finish_fetch(app: &mut App) {
    app.feeds_pending = app.feeds_pending.saturating_sub(1);
    app.refresh_article_views();
    if app.feeds_pending == 0 {
        app.feeds_total = 0;
        app.set_status("Feeds loaded.");
        let _ = storage::save_feeds(&app.feeds);
        let _ = storage::save_articles(&app.feeds, app.user_data.cache_full_articles);
    }
}

/// Apply a full-article fetch (already converted to markdown) to the article it was for.
pub fn on_full_article_fetched(
    app: &mut App,
    source: FeedSource,
    art_idx: usize,
    link: &str,
    result: Result<String, String>,
) {
    app.full_fetch_pending.remove(link);
    let selected = app.selected_article().is_some_and(|a| a.link == link);
    let article = match source {
        FeedSource::Saved => app.saved_view_articles.get_mut(art_idx),
        FeedSource::Feed(fi) => app
            .feeds
            .get_mut(fi)
            .and_then(|f| f.articles.get_mut(art_idx)),
    };
    // The list may have changed while fetching (e.g. a refresh reordered it).
    let Some(article) = article.filter(|a| a.link == link) else {
        return;
    };
    let msg = match result {
        Ok(markdown) => {
            article.error = None;
            article.content = markdown;
            "Article loaded.".to_string()
        }
        Err(e) => {
            // Keep the feed's own content (and its images) to read instead.
            let msg = format!("Extraction failed: {e}");
            article.error = Some(e);
            msg
        }
    };
    if selected {
        app.content_line_count = article.content.lines().count().max(1);
    }
    app.set_status(msg);
}

/// Spawn background download + decode for article images not yet cached or pending.
pub fn spawn_article_image_downloads(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    for img in &app.article_images {
        if app.image_cache.contains_key(&img.url) || !app.image_pending.insert(img.url.clone()) {
            continue;
        }
        let tx = tx.clone();
        let url = img.url.clone();
        tokio::spawn(async move {
            let decoded = match crate::fetch::fetch_image(&url).await {
                Ok(bytes) => {
                    let url = url.clone();
                    tokio::task::spawn_blocking(move || decode_image(&bytes, &url))
                        .await
                        .ok()
                        .flatten()
                }
                Err(_) => None,
            };
            let _ = tx.send(AppEvent::ImageDecoded(url, decoded));
        });
    }
}

/// Store a decoded image (or just clear its pending flag when decoding failed).
pub fn on_image_decoded(app: &mut App, url: String, image: Option<DynamicImage>) {
    app.image_pending.remove(&url);
    if let Some(image) = image {
        app.image_cache.insert(url, image);
    }
}

/// Decode raster or SVG image bytes.
fn decode_image(bytes: &[u8], url: &str) -> Option<DynamicImage> {
    if is_svg(bytes, url) {
        render_svg(bytes)
    } else {
        img_crate::load_from_memory(bytes).ok()
    }
}

fn is_svg(bytes: &[u8], url: &str) -> bool {
    if url.to_lowercase().ends_with(".svg") {
        return true;
    }
    let sample = bytes.get(..4096).unwrap_or(bytes);
    String::from_utf8_lossy(sample)
        .to_lowercase()
        .contains("<svg")
}

fn render_svg(bytes: &[u8]) -> Option<DynamicImage> {
    let text = std::str::from_utf8(bytes).ok()?;
    let tree = resvg::usvg::Tree::from_str(text, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size().to_int_size();
    let (w, h) = (size.width().max(1), size.height().max(1));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let rgba = img_crate::RgbaImage::from_raw(w, h, pixmap.take())?;
    Some(DynamicImage::ImageRgba8(rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn article(link: &str, published_secs: Option<i64>) -> Article {
        Article {
            title: link.into(),
            description: String::new(),
            link: link.into(),
            is_read: false,
            is_saved: false,
            content: String::new(),
            images: vec![],
            source_feed: String::new(),
            remote_id: None,
            published_secs,
            is_archived: false,
            error: None,
        }
    }

    #[test]
    fn archive_keeps_recent_drops_old_and_skips_saved() {
        let now = 10 * 86400;
        let fresh = vec![article("kept", Some(now))];
        let mut saved = article("saved", Some(now));
        saved.is_saved = true;
        let previous = vec![
            article("kept", Some(now)),
            article("recent", Some(now - 86400)),
            article("old", Some(now - 9 * 86400)),
            saved,
        ];
        let out = archived_survivors(
            &fresh,
            &previous,
            &ArchivePolicy::OneWeek,
            now,
            &HashSet::new(),
            &HashSet::new(),
        );
        let links: Vec<&str> = out.iter().map(|a| a.link.as_str()).collect();
        assert_eq!(links, vec!["recent"]);
        assert!(out[0].is_archived);
    }
}
