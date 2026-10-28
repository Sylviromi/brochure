//! FreshRSS GReader API types and pure mapping helpers.
//!
//! Side-effect free so the JSON shapes, feed/article mapping, and category
//! placement are unit-testable. Network I/O lives in [`super::client`].

use crate::fetch::html_to_markdown;
use crate::models::{Article, Category, CategoryId, Feed, FeedKind};
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
    /// User label names (`user/<id>/label/<name>`) the item carries, in order.
    pub labels: Vec<String>,
}

/// GReader state labels used by FreshRSS.
pub const LABEL_READ: &str = "user/-/state/com.google/read";
pub const LABEL_STARRED: &str = "user/-/state/com.google/starred";

/// True if any category entry marks the item read (covers `.../read` suffix variants).
pub fn categories_mark_read(categories: &[String]) -> bool {
    categories.iter().any(|c| c.ends_with("/read"))
}

/// The label name in a GReader label category (`user/-/label/Tech` → `Tech`).
/// Readers use `-` or a numeric user id in the middle.
pub fn label_name(category: &str) -> Option<&str> {
    let (_, name) = category.split_once("/label/")?;
    Some(name).filter(|n| !n.is_empty())
}

/// True if any category entry marks the item starred.
pub fn categories_mark_starred(categories: &[String]) -> bool {
    categories.iter().any(|c| c.ends_with("/starred"))
}

/// Parse `stream/contents` JSON into articles + optional `continuation` cursor.
pub fn parse_stream_contents(json: &str) -> Result<(Vec<ImportedArticle>, Option<String>), String> {
    let stream: StreamContents =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse articles: {e}"))?;
    let continuation = stream.continuation.filter(|c| !c.is_empty());
    let articles = stream.items.into_iter().map(map_stream_item).collect();
    Ok((articles, continuation))
}

#[derive(Debug, Deserialize)]
struct ItemIds {
    #[serde(default, rename = "itemRefs")]
    item_refs: Vec<ItemRef>,
    #[serde(default)]
    continuation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ItemRef {
    id: String,
}

/// Parse `stream/items/ids` JSON into item ids + optional `continuation` cursor.
pub fn parse_item_ids(json: &str) -> Result<(Vec<String>, Option<String>), String> {
    let ids: ItemIds =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse article ids: {e}"))?;
    let continuation = ids.continuation.filter(|c| !c.is_empty());
    Ok((
        ids.item_refs.into_iter().map(|r| r.id).collect(),
        continuation,
    ))
}

fn map_stream_item(item: StreamItem) -> ImportedArticle {
    let link = item
        .canonical
        .into_iter()
        .chain(item.alternate)
        .map(|l| l.href)
        .find(|h| !h.is_empty())
        .unwrap_or_default();
    let summary_html = item.summary.map(|s| s.content).unwrap_or_default();
    let body_html = item
        .content
        .map(|c| c.content)
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| summary_html.clone());
    let content = html_to_markdown(&body_html);
    let mut description = html_to_markdown(&summary_html);
    if description.is_empty() && !content.is_empty() {
        description = content.chars().take(280).collect();
    }
    let (origin_stream_id, source_feed) = match item.origin {
        Some(o) => (Some(o.stream_id), o.title),
        None => (None, String::new()),
    };
    let title = item.title.filter(|t| !t.is_empty()).unwrap_or_else(|| {
        if link.is_empty() {
            "No Title".to_string()
        } else {
            link.clone()
        }
    });
    let labels = item
        .categories
        .iter()
        .filter_map(|c| label_name(c))
        .map(str::to_string)
        .collect();
    ImportedArticle {
        labels,
        article: Article {
            title,
            description,
            link,
            is_read: categories_mark_read(&item.categories),
            is_saved: categories_mark_starred(&item.categories),
            content,
            images: vec![],
            source_feed,
            remote_id: Some(item.id).filter(|id| !id.is_empty()),
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
///
/// Matches by `remote_id`, then by `url`; existing feeds keep their category so
/// manual re-organisation survives syncs. New feeds are placed via
/// `resolve_category(label)`. Returns `(added, updated)`.
pub fn upsert_subscriptions(
    feeds: &mut Vec<Feed>,
    subs: Vec<ImportedSubscription>,
    mut resolve_category: impl FnMut(Option<&str>) -> Option<CategoryId>,
) -> (usize, usize) {
    let mut next_order = feeds.iter().map(|f| f.order + 1).max().unwrap_or(0);
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
        } else if let Some(feed) = feeds.iter_mut().find(|f| f.url == sub.url) {
            feed.remote_id = Some(sub.remote_id);
            feed.kind = FeedKind::FreshRss;
            feed.title = sub.title;
            updated += 1;
        } else {
            let category_id = resolve_category(sub.category_label.as_deref());
            feeds.push(Feed::new_freshrss(
                sub.title,
                sub.url,
                sub.remote_id,
                category_id,
                next_order,
            ));
            next_order += 1;
            added += 1;
        }
    }
    (added, updated)
}

// ── Category placement ───────────────────────────────────────────────────────

/// Name of the sidebar category that groups everything synced from FreshRSS.
pub const ROOT_CATEGORY_NAME: &str = "FreshRSS";

/// Order value that places a new child last under `parent`.
fn next_child_order(categories: &[Category], parent: Option<CategoryId>) -> usize {
    categories
        .iter()
        .filter(|c| c.parent_id == parent)
        .map(|c| c.order + 1)
        .max()
        .unwrap_or(0)
}

/// Find the category called `name` directly under `parent`, creating it as the
/// last sibling when missing. Returns its id.
pub fn ensure_category(
    categories: &mut Vec<Category>,
    name: &str,
    parent: Option<CategoryId>,
) -> CategoryId {
    if let Some(c) = categories
        .iter()
        .find(|c| c.parent_id == parent && c.name == name)
    {
        return c.id;
    }
    let id = categories.iter().map(|c| c.id + 1).max().unwrap_or(1);
    let order = next_child_order(categories, parent);
    categories.push(Category {
        id,
        name: name.to_string(),
        parent_id: parent,
        order,
    });
    id
}

/// Places synced feeds under a single [`ROOT_CATEGORY_NAME`] category. The root
/// and its per-label children are created lazily — only once a feed actually goes
/// into them — so users who already organised their synced feeds elsewhere don't
/// get empty duplicates.
pub struct SyncPlacement {
    root: Option<CategoryId>,
}

impl SyncPlacement {
    /// `known_root` is the root id remembered from a previous sync (ignored if the
    /// category has since been deleted). Without one, the flat layout older versions
    /// produced — server labels as root-level categories, unlabelled synced feeds at
    /// the root — is folded under the root.
    pub fn new(
        categories: &mut Vec<Category>,
        feeds: &mut [Feed],
        known_root: Option<CategoryId>,
        labels: &[String],
    ) -> Self {
        let root = known_root.filter(|id| categories.iter().any(|c| c.id == *id));
        let mut placement = Self { root };
        if root.is_none() {
            placement.adopt_flat_layout(categories, feeds, labels);
        }
        placement
    }

    /// The root category id, if it exists (persist it for the next sync).
    pub fn root_id(&self) -> Option<CategoryId> {
        self.root
    }

    /// Category for a newly synced feed carrying server `label`.
    pub fn category_for(
        &mut self,
        categories: &mut Vec<Category>,
        label: Option<&str>,
    ) -> CategoryId {
        let root = self.root(categories);
        match label {
            Some(label) => ensure_category(categories, label, Some(root)),
            None => root,
        }
    }

    fn root(&mut self, categories: &mut Vec<Category>) -> CategoryId {
        *self
            .root
            .get_or_insert_with(|| ensure_category(categories, ROOT_CATEGORY_NAME, None))
    }

    /// Move root-level label categories holding only synced feeds under the root,
    /// and pull unlabelled synced feeds from the root level into it.
    fn adopt_flat_layout(
        &mut self,
        categories: &mut Vec<Category>,
        feeds: &mut [Feed],
        labels: &[String],
    ) {
        for label in labels {
            let Some(id) = categories
                .iter()
                .find(|c| c.parent_id.is_none() && &c.name == label && Some(c.id) != self.root)
                .map(|c| c.id)
            else {
                continue;
            };
            let mut direct = feeds
                .iter()
                .filter(|f| f.category_id == Some(id))
                .peekable();
            let only_synced = direct.peek().is_some() && direct.all(Feed::is_freshrss);
            let has_subcategories = categories.iter().any(|c| c.parent_id == Some(id));
            if !only_synced || has_subcategories {
                continue;
            }
            let root = self.root(categories);
            if id == root {
                // The legacy label category is itself named like the root: reuse it.
                continue;
            }
            let clashes = categories
                .iter()
                .any(|c| c.parent_id == Some(root) && &c.name == label);
            if clashes {
                continue;
            }
            let order = next_child_order(categories, Some(root));
            if let Some(cat) = categories.iter_mut().find(|c| c.id == id) {
                cat.parent_id = Some(root);
                cat.order = order;
            }
        }
        if feeds
            .iter()
            .any(|f| f.is_freshrss() && f.category_id.is_none())
        {
            let root = self.root(categories);
            for feed in feeds
                .iter_mut()
                .filter(|f| f.is_freshrss() && f.category_id.is_none())
            {
                feed.category_id = Some(root);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_ids_parse_with_and_without_continuation() {
        let (ids, next) =
            parse_item_ids(r#"{"itemRefs":[{"id":"483"},{"id":"482"}],"continuation":"3"}"#)
                .unwrap();
        assert_eq!(ids, ["483", "482"]);
        assert_eq!(next.as_deref(), Some("3"));
        let (ids, next) = parse_item_ids(r#"{"itemRefs":[{"id":"1"}]}"#).unwrap();
        assert_eq!(ids, ["1"]);
        assert!(next.is_none());
    }

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
            labels: vec![],
        }];
        let added = merge_articles(&mut existing, pulled);
        assert_eq!(added, 0);
        assert!(existing[0].is_read && existing[0].is_saved);
        assert_eq!(existing[0].content, "body");
    }
    fn cat(id: CategoryId, name: &str, parent_id: Option<CategoryId>) -> Category {
        Category {
            id,
            name: name.into(),
            parent_id,
            order: 0,
        }
    }

    fn synced(url: &str, category_id: Option<CategoryId>) -> Feed {
        Feed::new_freshrss(
            url.into(),
            url.into(),
            format!("feed/{url}"),
            category_id,
            0,
        )
    }

    #[test]
    fn placement_creates_root_and_children_lazily() {
        let mut cats = vec![cat(1, "Local", None)];
        let mut feeds = vec![];
        let mut placement = SyncPlacement::new(&mut cats, &mut feeds, None, &["Tech".into()]);
        assert_eq!(placement.root_id(), None, "nothing placed yet");
        assert_eq!(cats.len(), 1);

        let tech = placement.category_for(&mut cats, Some("Tech"));
        let root = placement.root_id().unwrap();
        let root_cat = cats.iter().find(|c| c.id == root).unwrap();
        assert_eq!(
            (root_cat.name.as_str(), root_cat.parent_id),
            (ROOT_CATEGORY_NAME, None)
        );
        assert_eq!(
            cats.iter().find(|c| c.id == tech).unwrap().parent_id,
            Some(root)
        );
        assert_eq!(placement.category_for(&mut cats, None), root);

        // A later sync with the remembered root reuses everything.
        let len = cats.len();
        let mut again = SyncPlacement::new(&mut cats, &mut feeds, Some(root), &["Tech".into()]);
        assert_eq!(again.category_for(&mut cats, Some("Tech")), tech);
        assert_eq!(cats.len(), len);
    }

    #[test]
    fn placement_adopts_flat_layout_from_older_syncs() {
        let mut cats = vec![
            cat(1, "Tech", None),
            cat(2, "News", None),
            cat(3, "Mine", None),
            cat(4, "Comics", Some(3)),
        ];
        let mut feeds = vec![
            synced("a", Some(1)),
            synced("b", None),
            // "News" mixes local and synced feeds, so it must stay put.
            synced("c", Some(2)),
            Feed::new_local("d".into(), "d".into(), Some(2), 0),
            // Already organised by the user under "Mine": left alone.
            synced("e", Some(4)),
        ];
        let labels = ["Comics".to_string(), "News".to_string(), "Tech".to_string()];
        let placement = SyncPlacement::new(&mut cats, &mut feeds, None, &labels);
        let root = placement.root_id().unwrap();
        let parent = |id| cats.iter().find(|c| c.id == id).unwrap().parent_id;
        assert_eq!(parent(1), Some(root));
        assert_eq!(parent(2), None);
        assert_eq!(parent(4), Some(3));
        assert_eq!(feeds[1].category_id, Some(root));
        // No empty label categories were created under the root.
        assert_eq!(cats.iter().filter(|c| c.parent_id == Some(root)).count(), 1);
    }

    #[test]
    fn placement_creates_nothing_when_user_already_organised() {
        let mut cats = vec![cat(1, "Mine", None), cat(2, "Blogs", Some(1))];
        let mut feeds = vec![synced("a", Some(2))];
        let placement = SyncPlacement::new(&mut cats, &mut feeds, None, &["Blogs".into()]);
        assert_eq!(placement.root_id(), None);
        assert_eq!(cats.len(), 2);
    }

    #[test]
    fn upsert_places_new_feeds_and_keeps_existing_categories() {
        let mut feeds = vec![synced("a", Some(7))];
        let subs = vec![
            ImportedSubscription {
                remote_id: "feed/a".into(),
                title: "A".into(),
                url: "a".into(),
                category_label: Some("Tech".into()),
            },
            ImportedSubscription {
                remote_id: "feed/b".into(),
                title: "B".into(),
                url: "b".into(),
                category_label: None,
            },
        ];
        let (added, updated) = upsert_subscriptions(&mut feeds, subs, |label| {
            Some(if label.is_some() { 2 } else { 1 })
        });
        assert_eq!((added, updated), (1, 1));
        assert_eq!(feeds[0].category_id, Some(7));
        assert_eq!(feeds[1].category_id, Some(1));
    }
}
