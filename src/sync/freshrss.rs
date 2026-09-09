//! FreshRSS GReader API types and pure mapping helpers.
//!
//! Network I/O lands in a follow-up commit; this module stays side-effect
//! free so the JSON shapes and feed/article mapping are unit-testable.

use crate::models::{Article, Feed};
use serde::Deserialize;

/// Normalise a user-typed server address into a `.../api/greader.php` base URL.
///
/// Accepts a bare host (`https://rss.example.com`), a host with path, or a
/// full `greader.php` URL (both `/api/` and legacy `/p/api/` flavours).
pub fn normalise_base_url(input: &str) -> String {
    let t = input.trim().trim_end_matches('/');
    if t.ends_with("greader.php") {
        return t.to_string();
    }
    if t.ends_with("/p/api") || t.ends_with("/p/api/") {
        return format!("{}/greader.php", t.trim_end_matches('/'));
    }
    if t.ends_with("/api") {
        return format!("{t}/greader.php");
    }
    format!("{t}/api/greader.php")
}

/// Parse a `ClientLogin` response body (`SID=...\nAuth=...`) into the `Auth` token.
pub fn parse_client_login(body: &str) -> Option<String> {
    for line in body.lines() {
        let line = line.trim();
        if let Some(auth) = line.strip_prefix("Auth=") {
            let auth = auth.trim();
            if !auth.is_empty() {
                return Some(auth.to_string());
            }
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct SubscriptionList {
    #[serde(default)]
    subscriptions: Vec<Subscription>,
}

#[derive(Debug, Deserialize)]
struct Subscription {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    categories: Vec<SubscriptionCategory>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionCategory {
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: String,
}

/// A subscription ready to become (or match) a local `Feed`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedSubscription {
    pub remote_id: String,
    pub title: String,
    pub url: String,
    pub category_label: Option<String>,
}

/// Parse `subscription/list?output=json` into importable subscriptions.
/// Skips entries with an empty id or url.
pub fn parse_subscriptions(json: &str) -> Result<Vec<ImportedSubscription>, String> {
    let list: SubscriptionList =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse subscriptions: {e}"))?;
    Ok(list
        .subscriptions
        .into_iter()
        .filter(|s| !s.id.is_empty() && !s.url.is_empty())
        .map(|s| {
            let title = if s.title.is_empty() {
                s.url.clone()
            } else {
                s.title
            };
            ImportedSubscription {
                remote_id: s.id,
                title,
                url: s.url,
                category_label: s
                    .categories
                    .into_iter()
                    .next()
                    .map(|c| if c.label.is_empty() { c.id } else { c.label }),
            }
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct StreamContents {
    #[serde(default)]
    items: Vec<StreamItem>,
    #[serde(default)]
    continuation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    canonical: Vec<StreamLink>,
    #[serde(default)]
    alternate: Vec<StreamLink>,
    #[serde(default)]
    summary: Option<StreamContent>,
    #[serde(default)]
    content: Option<StreamContent>,
    #[serde(default)]
    published: Option<i64>,
    #[serde(default)]
    updated: Option<i64>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    origin: Option<StreamOrigin>,
}

#[derive(Debug, Deserialize)]
struct StreamLink {
    #[serde(default)]
    href: String,
}

#[derive(Debug, Deserialize)]
struct StreamContent {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct StreamOrigin {
    #[serde(default, rename = "streamId")]
    stream_id: String,
    #[serde(default)]
    title: String,
}

/// An article pulled from FreshRSS, before merging into a `Feed`.
#[derive(Debug, Clone)]
pub struct ImportedArticle {
    pub article: Article,
    /// Server feed id (`feed/12`) for fan-out, if the origin was present.
    /// Unread in the per-feed pull path (keyed by subscription id instead);
    /// kept for future `reading-list` fan-out.
    #[allow(dead_code)]
    pub origin_stream_id: Option<String>,
}

/// GReader state labels used by FreshRSS.
pub const LABEL_READ: &str = "user/-/state/com.google/read";
pub const LABEL_STARRED: &str = "user/-/state/com.google/starred";

/// True if any category entry marks the item read (covers `.../read` suffix variants).
pub fn categories_mark_read(categories: &[String]) -> bool {
    categories.iter().any(|c| c.ends_with("/read"))
}

/// True if any category entry marks the item starred.
pub fn categories_mark_starred(categories: &[String]) -> bool {
    categories.iter().any(|c| c.ends_with("/starred"))
}

fn html_to_markdown(html: &str) -> String {
    html_to_markdown_rs::convert(html, None)
        .ok()
        .and_then(|r| r.content)
        .unwrap_or_default()
}

/// Parse `stream/contents` JSON into articles + optional `continuation` cursor.
pub fn parse_stream_contents(json: &str) -> Result<(Vec<ImportedArticle>, Option<String>), String> {
    let stream: StreamContents =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse articles: {e}"))?;
    let continuation = stream.continuation.filter(|c| !c.is_empty());
    let articles = stream.items.into_iter().map(map_stream_item).collect();
    Ok((articles, continuation))
}

fn map_stream_item(item: StreamItem) -> ImportedArticle {
    let link = item
        .canonical
        .first()
        .map(|l| l.href.clone())
        .filter(|h| !h.is_empty())
        .or_else(|| {
            item.alternate
                .first()
                .map(|l| l.href.clone())
                .filter(|h| !h.is_empty())
        })
        .unwrap_or_default();
    let summary_html = item.summary.map(|s| s.content).unwrap_or_default();
    let body_html = item
        .content
        .map(|c| c.content)
        .filter(|b| !b.trim().is_empty())
        .or_else(|| {
            if summary_html.trim().is_empty() {
                None
            } else {
                Some(summary_html.clone())
            }
        })
        .unwrap_or_default();
    let content = html_to_markdown(&body_html);
    let mut description = html_to_markdown(&summary_html);
    // Re-borrow-free: summary was moved above; recompute description from html fallback.
    if description.is_empty() && !content.is_empty() {
        description = content.chars().take(280).collect();
    }
    let is_read = categories_mark_read(&item.categories);
    let is_starred = categories_mark_starred(&item.categories);
    let origin = item.origin;
    let (origin_stream_id, source_feed) = match origin {
        Some(o) => (
            Some(o.stream_id),
            if o.title.is_empty() {
                String::new()
            } else {
                o.title
            },
        ),
        None => (None, String::new()),
    };
    ImportedArticle {
        article: Article {
            title: item.title.filter(|t| !t.is_empty()).unwrap_or_else(|| {
                if link.is_empty() {
                    "No Title".to_string()
                } else {
                    link.clone()
                }
            }),
            description,
            link,
            is_read,
            is_saved: is_starred,
            content,
            images: vec![],
            source_feed,
            remote_id: if item.id.is_empty() {
                None
            } else {
                Some(item.id)
            },
            published_secs: item.published.or(item.updated),
            is_archived: false,
            error: None,
        },
        origin_stream_id,
    }
}

/// Match an imported article against existing feed articles (remote id first, link fallback).
/// Returns the index in `existing` when matched.
pub fn match_existing(existing: &[Article], imported: &Article) -> Option<usize> {
    if let Some(rid) = &imported.remote_id
        && let Some(idx) = existing
            .iter()
            .position(|a| a.remote_id.as_ref() == Some(rid))
    {
        return Some(idx);
    }
    if !imported.link.is_empty()
        && let Some(idx) = existing.iter().position(|a| a.link == imported.link)
    {
        return Some(idx);
    }
    None
}

/// Merge pulled articles into a feed's article list (server-wins for read/star).
/// Returns the number of newly added articles.
pub fn merge_articles(existing: &mut Vec<Article>, pulled: Vec<ImportedArticle>) -> usize {
    let mut added = 0;
    for imported in pulled {
        match match_existing(existing, &imported.article) {
            Some(idx) => {
                existing[idx].is_read = imported.article.is_read;
                existing[idx].is_saved = imported.article.is_saved;
                if existing[idx].remote_id.is_none() {
                    existing[idx].remote_id = imported.article.remote_id.clone();
                }
                if existing[idx].content.is_empty() && !imported.article.content.is_empty() {
                    existing[idx].content = imported.article.content.clone();
                }
            }
            None => {
                existing.push(imported.article);
                added += 1;
            }
        }
    }
    // Newest first; items without timestamps sink to the bottom.
    existing.sort_by_key(|b| std::cmp::Reverse(b.published_secs));
    added
}

/// Upsert imported subscriptions into the feed list.
/// Matches by `remote_id`, then by `url`. Returns `(added, updated)`.
pub fn upsert_subscriptions(
    feeds: &mut Vec<Feed>,
    subs: Vec<ImportedSubscription>,
    resolve_category: &mut dyn FnMut(Option<String>) -> Option<crate::models::CategoryId>,
    next_order: &mut dyn FnMut() -> usize,
) -> (usize, usize) {
    let mut added = 0;
    let mut updated = 0;
    for sub in subs {
        if let Some(feed) = feeds
            .iter_mut()
            .find(|f| f.remote_id.as_deref() == Some(&sub.remote_id))
        {
            feed.title = sub.title;
            if feed.url.is_empty() {
                feed.url = sub.url;
            }
            updated += 1;
            continue;
        }
        if let Some(feed) = feeds.iter_mut().find(|f| f.url == sub.url) {
            feed.remote_id = Some(sub.remote_id);
            feed.kind = crate::models::FeedKind::FreshRss;
            feed.title = sub.title;
            updated += 1;
            continue;
        }
        let category_id = resolve_category(sub.category_label);
        feeds.push(Feed::new_freshrss(
            sub.title,
            sub.url,
            sub.remote_id,
            category_id,
            next_order(),
        ));
        added += 1;
    }
    (added, updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_base_url_handles_variants() {
        assert_eq!(
            normalise_base_url("https://rss.example.com"),
            "https://rss.example.com/api/greader.php"
        );
        assert_eq!(
            normalise_base_url("https://rss.example.com/"),
            "https://rss.example.com/api/greader.php"
        );
        assert_eq!(
            normalise_base_url("https://rss.example.com/api/greader.php"),
            "https://rss.example.com/api/greader.php"
        );
        assert_eq!(
            normalise_base_url("https://rss.example.com/p/api/greader.php"),
            "https://rss.example.com/p/api/greader.php"
        );
        assert_eq!(
            normalise_base_url("https://rss.example.com/api"),
            "https://rss.example.com/api/greader.php"
        );
    }

    #[test]
    fn parse_client_login_extracts_auth() {
        let body = "SID=demo/abc\nLSID=demo/abc\nAuth=demo/abc123\n";
        assert_eq!(parse_client_login(body), Some("demo/abc123".to_string()));
        assert_eq!(parse_client_login("Unauthorized\n"), None);
    }

    #[test]
    fn parse_subscriptions_skips_invalid() {
        let json = r#"{"subscriptions":[
            {"id":"feed/1","title":"One","url":"https://one.example/feed","htmlUrl":"https://one.example","categories":[{"id":"user/-/label/Tech","label":"Tech"}]},
            {"id":"","title":"Bad","url":"https://bad.example/feed","categories":[]},
            {"id":"feed/3","title":"","url":"","categories":[]}
        ]}"#;
        let subs = parse_subscriptions(json).unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].remote_id, "feed/1");
        assert_eq!(subs[0].category_label.as_deref(), Some("Tech"));
    }

    #[test]
    fn parse_stream_contents_maps_read_star_and_continuation() {
        let json = r#"{"continuation":"abc123","items":[{
            "id":"tag:google.com,2005:reader/item/1",
            "title":"Hello","canonical":[{"href":"https://a.example/1"}],
            "summary":{"content":"<p>hi</p>"},
            "published":1700000000,
            "categories":["user/-/state/com.google/read","user/-/state/com.google/starred"],
            "origin":{"streamId":"feed/1","title":"One"}
        }]}"#;
        let (articles, cont) = parse_stream_contents(json).unwrap();
        assert_eq!(cont.as_deref(), Some("abc123"));
        assert_eq!(articles.len(), 1);
        assert!(articles[0].article.is_read);
        assert!(articles[0].article.is_saved);
        assert_eq!(
            articles[0].article.remote_id.as_deref(),
            Some("tag:google.com,2005:reader/item/1")
        );
        assert_eq!(articles[0].origin_stream_id.as_deref(), Some("feed/1"));
    }

    #[test]
    fn merge_prefers_remote_id_then_link_and_applies_server_state() {
        let mut existing = vec![Article {
            title: "Old".into(),
            description: String::new(),
            link: "https://a.example/1".into(),
            is_read: false,
            is_saved: false,
            content: String::new(),
            images: vec![],
            source_feed: String::new(),
            remote_id: Some("tag:google.com,2005:reader/item/1".into()),
            published_secs: Some(1),
            is_archived: false,
            error: None,
        }];
        let pulled = vec![ImportedArticle {
            article: Article {
                title: "New title".into(),
                description: String::new(),
                link: "https://a.example/changed".into(),
                is_read: true,
                is_saved: true,
                content: "body".into(),
                images: vec![],
                source_feed: String::new(),
                remote_id: Some("tag:google.com,2005:reader/item/1".into()),
                published_secs: Some(2),
                is_archived: false,
                error: None,
            },
            origin_stream_id: None,
        }];
        let added = merge_articles(&mut existing, pulled);
        assert_eq!(added, 0);
        assert!(existing[0].is_read && existing[0].is_saved);
        assert_eq!(existing[0].content, "body");
    }
}
