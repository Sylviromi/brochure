//! Feed refresh routing plus the FreshRSS side of [`App`]: credentials, full sync,
//! per-feed pulls, and read/star push.

use super::App;
use crate::models::{AppEvent, FAVORITES_URL, Feed, FeedKind};
use crate::storage::{data_dir, save_feeds, save_user_data};
use crate::sync::client;
use tokio::sync::mpsc::UnboundedSender;

type Tx = UnboundedSender<AppEvent>;

/// FreshRSS connection details resolved from the config and the secret store.
#[derive(Clone)]
pub(crate) struct FreshRssCreds {
    base_url: String,
    username: String,
    password: String,
}

impl FreshRssCreds {
    /// Reuse a cached `Auth` token when present, otherwise log in.
    async fn auth(&self, cached: Option<String>) -> Result<String, String> {
        match cached {
            Some(auth) => Ok(auth),
            None => client::client_login(&self.base_url, &self.username, &self.password).await,
        }
    }
}

/// A read/star mutation queued for push to FreshRSS.
#[derive(Debug, Clone, Copy)]
enum PushKind {
    Read(bool),
    Starred(bool),
}

/// Human-readable age (`just now`, `5m ago`, `3h ago`, `2d ago`) for sync timestamps.
fn format_age(secs: i64) -> String {
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

impl App {
    /// FreshRSS credentials when a server is configured and a secret is available
    /// (env → keyring → file).
    pub(crate) fn freshrss_creds(&self) -> Option<FreshRssCreds> {
        let cfg = self.user_data.freshrss.as_ref()?;
        let (password, _) = crate::sync::secret::get_password(&cfg.username, &data_dir())?;
        Some(FreshRssCreds {
            base_url: cfg.base_url.clone(),
            username: cfg.username.clone(),
            password,
        })
    }

    /// Refresh one feed (see [`App::refresh_feeds`]).
    pub fn refresh_feed_at(&mut self, idx: usize, tx: &Tx) {
        self.refresh_feeds(&[idx], tx);
    }

    /// Refresh the given feeds. Synced feeds are pulled through FreshRSS sharing one
    /// login; everything else (including synced feeds when no server is configured)
    /// is fetched directly from its URL.
    pub fn refresh_feeds(&mut self, indices: &[usize], tx: &Tx) {
        let creds = self.freshrss_creds();
        self.refresh_feeds_with(indices, creds, tx);
    }

    /// Refresh every feed: local feeds directly and, when a server is configured,
    /// one full FreshRSS sync (which also picks up new subscriptions).
    pub fn refresh_all(&mut self, tx: &Tx) -> bool {
        self.refresh_where(|_| true, tx)
    }

    /// Refresh the feeds for which `due` holds; feeds that are skipped are marked as
    /// fetched so they don't show a spinner. A FreshRSS sync runs when any synced feed
    /// is due (or none exist yet). Returns `true` when anything was started.
    pub fn refresh_where(&mut self, due: impl Fn(&Feed) -> bool, tx: &Tx) -> bool {
        let creds = self.freshrss_creds();
        let is_synced = |f: &Feed| creds.is_some() && f.is_freshrss() && f.remote_id.is_some();
        let mut local = Vec::new();
        let mut any_synced = false;
        let mut sync_due = false;
        for (idx, feed) in self.feeds.iter_mut().enumerate() {
            if feed.url == FAVORITES_URL {
                continue;
            }
            if is_synced(feed) {
                any_synced = true;
                sync_due |= due(feed);
            } else if due(feed) {
                local.push(idx);
            } else {
                feed.fetched = true;
            }
        }
        let sync_due = sync_due || (creds.is_some() && !any_synced);
        if any_synced && !sync_due {
            for feed in self.feeds.iter_mut().filter(|f| is_synced(f)) {
                feed.fetched = true;
            }
        }
        let started = !local.is_empty() || sync_due;
        self.refresh_feeds_with(&local, None, tx);
        if sync_due && let Some(creds) = creds {
            self.start_sync(creds, tx);
        }
        started
    }

    /// Kick off a full FreshRSS sync (subscriptions + articles).
    /// Returns false when no server is configured. Results arrive as `FreshRssSynced`.
    pub fn spawn_freshrss_sync(&mut self, tx: &Tx) -> bool {
        let Some(creds) = self.freshrss_creds() else {
            return false;
        };
        self.start_sync(creds, tx);
        true
    }

    /// Push one read/unread change to FreshRSS.
    /// No-op for local articles or when no server is configured.
    /// Failures surface via `AppEvent::FreshRssPushDone(Err)`.
    pub fn push_read_state(&self, remote_id: Option<String>, is_read: bool, tx: &Tx) {
        self.spawn_freshrss_push(remote_id, PushKind::Read(is_read), tx);
    }

    /// Push one star/unstar change to FreshRSS (local saves map to server stars).
    /// No-op for local articles or when no server is configured.
    pub fn push_star_state(&self, remote_id: Option<String>, starred: bool, tx: &Tx) {
        self.spawn_freshrss_push(remote_id, PushKind::Starred(starred), tx);
    }

    /// Disconnect the FreshRSS server: clear config + secret, keep feeds as local.
    /// Synced feeds retain their (real) URLs so direct fetching keeps working.
    pub fn disconnect_freshrss(&mut self) {
        if let Some(cfg) = self.user_data.freshrss.take() {
            crate::sync::secret::delete_password(&cfg.username, &data_dir());
        }
        for feed in self.feeds.iter_mut().filter(|f| f.is_freshrss()) {
            feed.kind = FeedKind::Local;
            feed.remote_id = None;
        }
        self.freshrss_auth = None;
        let _ = save_user_data(&self.user_data);
        let _ = save_feeds(&self.feeds);
        self.set_status("FreshRSS disconnected — feeds kept as local.");
    }

    /// One-line FreshRSS status for the settings row.
    pub fn freshrss_status(&self) -> String {
        match &self.user_data.freshrss {
            None => "Not connected".to_string(),
            Some(cfg) => match cfg.last_sync_secs {
                None => format!("{} — never synced", cfg.username),
                Some(ts) => {
                    let age = crate::ui::content::utils::now_secs().saturating_sub(ts);
                    format!("{} — synced {}", cfg.username, format_age(age))
                }
            },
        }
    }

    /// Mark feed `idx` as in flight (spinner + progress bar). Returns false for
    /// missing or virtual feeds.
    fn begin_fetch(&mut self, idx: usize) -> bool {
        let Some(feed) = self.feeds.get_mut(idx) else {
            return false;
        };
        if feed.url == FAVORITES_URL {
            return false;
        }
        feed.fetched = false;
        feed.fetch_error = None;
        self.feeds_pending += 1;
        self.feeds_total += 1;
        true
    }

    /// Shared body of [`App::refresh_feeds`] with credentials already resolved.
    fn refresh_feeds_with(&mut self, indices: &[usize], creds: Option<FreshRssCreds>, tx: &Tx) {
        let mut pulls = Vec::new();
        for &idx in indices {
            if !self.begin_fetch(idx) {
                continue;
            }
            let feed = &self.feeds[idx];
            match (
                &creds,
                feed.remote_id.clone().filter(|_| feed.is_freshrss()),
            ) {
                (Some(_), Some(remote_id)) => pulls.push((idx, remote_id)),
                _ => {
                    let url = feed.url.clone();
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let result = crate::fetch::fetch_feed(&url).await;
                        let _ = tx.send(AppEvent::FeedFetched(idx, result));
                    });
                }
            }
        }
        let Some(creds) = creds.filter(|_| !pulls.is_empty()) else {
            return;
        };
        let cached = self.freshrss_auth.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let auth = creds.auth(cached).await;
            for (idx, remote_id) in pulls {
                let result = match &auth {
                    Ok(auth) => client::pull_feed_articles(&creds.base_url, auth, &remote_id).await,
                    Err(e) => Err(e.clone()),
                };
                let _ = tx.send(AppEvent::FreshRssFeedPulled(idx, result));
            }
        });
    }

    /// Start a full sync: synced feeds show a spinner until `FreshRssSynced` arrives.
    fn start_sync(&mut self, creds: FreshRssCreds, tx: &Tx) {
        for feed in self.feeds.iter_mut().filter(|f| f.is_freshrss()) {
            feed.fetched = false;
            feed.fetch_error = None;
        }
        self.feeds_pending += 1;
        self.feeds_total += 1;
        self.set_status("Syncing FreshRSS...");
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = client::sync_all(&creds.base_url, &creds.username, &creds.password).await;
            let _ = tx.send(AppEvent::FreshRssSynced(result));
        });
    }

    /// Shared push-task spawner for read/star mutations.
    fn spawn_freshrss_push(&self, remote_id: Option<String>, kind: PushKind, tx: &Tx) {
        let Some(item_id) = remote_id else {
            return;
        };
        let Some(creds) = self.freshrss_creds() else {
            return;
        };
        let cached = self.freshrss_auth.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = async {
                let auth = creds.auth(cached).await?;
                match kind {
                    PushKind::Read(read) => {
                        client::set_read(&creds.base_url, &auth, &item_id, read).await
                    }
                    PushKind::Starred(starred) => {
                        client::set_starred(&creds.base_url, &auth, &item_id, starred).await
                    }
                }
            }
            .await;
            let _ = tx.send(AppEvent::FreshRssPushDone(result));
        });
    }
}
