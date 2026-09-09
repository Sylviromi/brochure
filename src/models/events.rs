//! Events flowing over the MPSC channel into the main event loop.

use super::{Article, FeedSource, UpdateInfo};
use crate::sync::client::SyncBundle;
use crate::sync::freshrss::ImportedArticle;

/// Events flowing over the MPSC channel into the main event loop.
#[derive(Debug)]
pub enum AppEvent {
    /// A key input from the user.
    Input(crossterm::event::KeyEvent),
    /// Periodic tick for UI updates and animations.
    Tick,
    /// Result of a background feed fetch: (feed_idx, Result<(articles, xml_updated_secs), error>).
    FeedFetched(usize, Result<(Vec<Article>, Option<i64>), String>),
    /// Result of a background FreshRSS article pull for one feed.
    FreshRssFeedPulled(usize, Result<Vec<ImportedArticle>, String>),
    /// Result of a full FreshRSS sync (subscriptions + articles).
    FreshRssSynced(Result<SyncBundle, String>),
    /// Result of pushing one read/star change (Some(error) on failure, silent on success).
    FreshRssPushDone(Result<(), String>),
    /// Result of background feed-title fetch during the AddFeed flow.
    FeedTitleFetched(Result<String, String>),
    /// Result of fetching the full article content: (source, article_idx, Result<content, error>).
    FullArticleFetched(FeedSource, usize, Result<String, String>),
    /// A newer version of brochure is available on crates.io.
    UpdateAvailable(UpdateInfo),
    /// An image has been downloaded for display in the article detail.
    ImageDownloaded(String, Result<Vec<u8>, String>),
}
