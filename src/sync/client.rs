//! Async FreshRSS GReader API client.
//!
//! All functions take `base_url` (a `.../api/greader.php` root from
//! [`normalise_base_url`](crate::sync::freshrss::normalise_base_url)) and an
//! `Auth` token from [`client_login`]. Secrets are never logged.

use crate::fetch::http_client;
use crate::sync::freshrss::{
    ImportedArticle, ImportedSubscription, LABEL_READ, LABEL_STARRED, parse_client_login,
    parse_stream_contents, parse_subscriptions,
};
use std::collections::HashMap;
use std::sync::Arc;

const PAGE_SIZE: u32 = 100;
const MAX_PAGES_PER_FEED: u32 = 5;
/// Feeds pulled in parallel during a full sync.
const SYNC_CONCURRENCY: usize = 4;

/// Everything pulled by [`sync_all`]: subscriptions plus articles per server feed id.
#[derive(Debug, Clone)]
pub struct SyncBundle {
    pub subs: Vec<ImportedSubscription>,
    /// Per-feed pull result; one failing feed does not abort the sync.
    pub articles: HashMap<String, Result<Vec<ImportedArticle>, String>>,
    /// The `Auth` token used for the pull (runtime only — never persisted).
    pub auth: String,
}

/// Percent-encode a form/path value (`application/x-www-form-urlencoded` subset).
fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn auth_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 | 403 => "FreshRSS auth failed — check username and API password".to_string(),
        404 => "FreshRSS API not found — check the server address".to_string(),
        _ => format!("FreshRSS request failed: HTTP {status}"),
    }
}

async fn authed_get(base_url: &str, auth: &str, path: &str) -> Result<String, String> {
    let url = format!("{base_url}/{path}");
    let resp = http_client()
        .get(&url)
        .header("Authorization", format!("GoogleLogin auth={auth}"))
        .send()
        .await
        .map_err(|e| format!("FreshRSS network error: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(auth_error(status));
    }
    resp.text()
        .await
        .map_err(|e| format!("FreshRSS read error: {e}"))
}

/// Log in with username + API password, returning the `Auth` token.
pub async fn client_login(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let url = format!("{base_url}/accounts/ClientLogin");
    let body = format!(
        "Email={}&Passwd={}",
        form_encode(username),
        form_encode(password)
    );
    let resp = http_client()
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("FreshRSS network error: {e}"))?;
    if !resp.status().is_success() {
        return Err(auth_error(resp.status()));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("FreshRSS read error: {e}"))?;
    parse_client_login(&text).ok_or_else(|| "FreshRSS login rejected the credentials".to_string())
}

/// Fetch and parse the subscription list.
pub async fn fetch_subscriptions(
    base_url: &str,
    auth: &str,
) -> Result<Vec<ImportedSubscription>, String> {
    let json = authed_get(base_url, auth, "reader/api/0/subscription/list?output=json").await?;
    parse_subscriptions(&json)
}

/// Pull up to `PAGE_SIZE * MAX_PAGES_PER_FEED` articles for one server feed id.
pub async fn pull_feed_articles(
    base_url: &str,
    auth: &str,
    remote_feed_id: &str,
) -> Result<Vec<ImportedArticle>, String> {
    let mut all = Vec::new();
    let mut continuation: Option<String> = None;
    for _ in 0..MAX_PAGES_PER_FEED {
        let mut path = format!(
            "reader/api/0/stream/contents/{}?n={PAGE_SIZE}",
            form_encode(remote_feed_id)
        );
        if let Some(c) = &continuation {
            path.push_str("&continuation=");
            path.push_str(&form_encode(c));
        }
        let json = authed_get(base_url, auth, &path).await?;
        let (mut articles, next) = parse_stream_contents(&json)?;
        if articles.is_empty() {
            break;
        }
        all.append(&mut articles);
        continuation = next;
        if continuation.is_none() {
            break;
        }
    }
    Ok(all)
}

/// Fetch the write token required for `edit-tag` mutations.
pub async fn fetch_token(base_url: &str, auth: &str) -> Result<String, String> {
    let token = authed_get(base_url, auth, "reader/api/0/token").await?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("FreshRSS returned an empty write token".to_string());
    }
    Ok(token)
}

/// Add (`read == true`) or remove the read label on one item.
pub async fn set_read(base_url: &str, auth: &str, item_id: &str, read: bool) -> Result<(), String> {
    edit_tag(base_url, auth, item_id, LABEL_READ, read).await
}

/// Star (`starred == true`) or unstar one item.
pub async fn set_starred(
    base_url: &str,
    auth: &str,
    item_id: &str,
    starred: bool,
) -> Result<(), String> {
    edit_tag(base_url, auth, item_id, LABEL_STARRED, starred).await
}

async fn edit_tag(
    base_url: &str,
    auth: &str,
    item_id: &str,
    label: &str,
    add: bool,
) -> Result<(), String> {
    let token = fetch_token(base_url, auth).await?;
    let op = if add { 'a' } else { 'r' };
    let body = format!(
        "s={}&{}={}&T={}",
        form_encode(item_id),
        op,
        form_encode(label),
        form_encode(&token)
    );
    let url = format!("{base_url}/reader/api/0/edit-tag");
    let resp = http_client()
        .post(&url)
        .header("Authorization", format!("GoogleLogin auth={auth}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("FreshRSS network error: {e}"))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(auth_error(resp.status()))
    }
}

/// Full pull: login, subscriptions, then articles for every subscribed feed
/// (up to [`SYNC_CONCURRENCY`] feeds at a time).
pub async fn sync_all(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<SyncBundle, String> {
    let auth = client_login(base_url, username, password).await?;
    let subs = fetch_subscriptions(base_url, &auth).await?;
    let permits = Arc::new(tokio::sync::Semaphore::new(SYNC_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for sub in &subs {
        let (base_url, auth, remote_id) =
            (base_url.to_string(), auth.clone(), sub.remote_id.clone());
        let permits = Arc::clone(&permits);
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await;
            let result = pull_feed_articles(&base_url, &auth, &remote_id).await;
            (remote_id, result)
        });
    }
    let mut articles = HashMap::with_capacity(subs.len());
    while let Some(joined) = tasks.join_next().await {
        if let Ok((remote_id, result)) = joined {
            articles.insert(remote_id, result);
        }
    }
    Ok(SyncBundle {
        subs,
        articles,
        auth,
    })
}
