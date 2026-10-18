//! Applies FreshRSS pull/sync results to [`App`]: merges articles, folds server
//! read state into local data, and keeps synced feeds under the FreshRSS category.

use crate::app::App;
use crate::events::{clamp_selection_after_update, finish_fetch, record_fetch_error, saved_links};
use crate::storage;
use crate::sync::client::SyncBundle;
use crate::sync::freshrss::{ImportedArticle, SyncPlacement, merge_articles, upsert_subscriptions};
use crate::ui::content::utils::now_secs;

/// Merge pulled articles into feed `idx` and overlay local flags (local reads and
/// saves win). Server-side reads are folded into the local read set so badges,
/// archive policy, and cache-clear flows stay consistent. Returns the number of
/// newly added articles.
fn merge_into_feed(app: &mut App, idx: usize, pulled: Vec<ImportedArticle>) -> usize {
    let Some(feed) = app.feeds.get_mut(idx) else {
        return 0;
    };
    let added = merge_articles(&mut feed.articles, pulled);
    let saved = saved_links(&app.user_data.saved_articles);
    let read_links = &mut app.user_data.read_links;
    let mut read_changed = false;
    for art in feed.articles.iter_mut() {
        art.source_feed.clone_from(&feed.title);
        if art.link.is_empty() {
            continue;
        }
        if read_links.contains(&art.link) {
            art.is_read = true;
        } else if art.is_read {
            read_links.insert(art.link.clone());
            read_changed = true;
        }
        art.is_saved |= saved.contains(art.link.as_str());
    }
    feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
    feed.fetch_error = None;
    feed.fetched = true;
    feed.last_fetched_secs = Some(now_secs());
    if read_changed {
        let _ = storage::save_user_data(&app.user_data);
    }
    added
}

/// Handle a FreshRSS article pull for one feed.
pub fn on_freshrss_feed_pulled(
    app: &mut App,
    idx: usize,
    result: Result<Vec<ImportedArticle>, String>,
) {
    match result {
        Ok(pulled) => {
            merge_into_feed(app, idx, pulled);
            clamp_selection_after_update(app, idx);
        }
        Err(e) => record_fetch_error(app, idx, e),
    }
    finish_fetch(app);
}

/// Handle a full FreshRSS sync: place categories, upsert subscriptions, merge
/// articles per feed, persist.
pub fn on_freshrss_synced(app: &mut App, result: Result<SyncBundle, String>) {
    let mut bundle = match result {
        Ok(b) => b,
        Err(e) => {
            for feed in app.feeds.iter_mut().filter(|f| f.is_freshrss()) {
                feed.fetched = true;
                feed.fetch_error = Some(e.clone());
            }
            finish_fetch(app);
            app.set_status(format!("⚠ FreshRSS sync failed: {e}"));
            return;
        }
    };
    app.freshrss_auth = Some(bundle.auth.clone());

    let mut placement = synced_placement(app, &bundle);
    let (added_feeds, _) = {
        let categories = &mut app.categories;
        upsert_subscriptions(&mut app.feeds, std::mem::take(&mut bundle.subs), |label| {
            Some(placement.category_for(categories, label))
        })
    };
    if let Some(cfg) = app.user_data.freshrss.as_mut()
        && placement.root_id().is_some()
    {
        cfg.category_id = placement.root_id();
    }

    let mut new_articles = 0;
    for idx in 0..app.feeds.len() {
        let Some(result) = app.feeds[idx]
            .remote_id
            .as_ref()
            .and_then(|id| bundle.articles.remove(id))
        else {
            continue;
        };
        match result {
            Ok(pulled) => new_articles += merge_into_feed(app, idx, pulled),
            Err(e) => {
                let feed = &mut app.feeds[idx];
                feed.fetched = true;
                feed.fetch_error = Some(e);
            }
        }
    }
    // Synced feeds the server no longer lists must not spin forever.
    for feed in app.feeds.iter_mut().filter(|f| f.is_freshrss()) {
        feed.fetched = true;
    }

    if let Some(cfg) = app.user_data.freshrss.as_mut() {
        cfg.last_sync_secs = Some(now_secs());
    }
    let _ = storage::save_user_data(&app.user_data);
    let _ = storage::save_categories(&app.categories);
    finish_fetch(app);
    // finish_fetch only persists once the whole batch is done; a sync always
    // changes the subscription list, so save it now.
    let _ = storage::save_feeds(&app.feeds);
    app.set_status(format!(
        "FreshRSS sync complete: {added_feeds} new feeds, {new_articles} new articles."
    ));
}

/// Set up category placement for this sync, folding older flat layouts under the
/// FreshRSS root on the first sync.
fn synced_placement(app: &mut App, bundle: &SyncBundle) -> SyncPlacement {
    let mut labels: Vec<String> = bundle
        .subs
        .iter()
        .filter_map(|s| s.category_label.clone())
        .collect();
    labels.sort();
    labels.dedup();
    let known_root = app.user_data.freshrss.as_ref().and_then(|c| c.category_id);
    SyncPlacement::new(&mut app.categories, &mut app.feeds, known_root, &labels)
}
