//! Saved-articles import/export as Google Reader-style starred JSON.
//!
//! This is the format FreshRSS, Inoreader, The Old Reader and Tiny Tiny RSS use for
//! their starred-item exports, so files move between Brochure and those readers.
//! Saved categories travel as GReader labels (`user/-/label/<name>`).

use std::{collections::HashSet, fs, path::Path};

use pulldown_cmark::{Options, Parser, html};
use serde_json::{Value, json};

use crate::{
    models::{Article, Feed, SavedArticle, UserData},
    storage::write_json,
    sync::freshrss::{LABEL_READ, LABEL_STARRED, parse_stream_contents},
};

/// Saved category for imported items that carry no label.
const DEFAULT_CATEGORY: &str = "Starred";

/// Write every saved article to `path`. Returns how many were exported.
pub fn export_saved(path: &Path, user_data: &UserData, feeds: &[Feed]) -> anyhow::Result<usize> {
    let now = crate::ui::content::utils::now_secs();
    write_json(path, &starred_document(user_data, feeds, now), true)?;
    Ok(user_data.saved_articles.len())
}

/// Read a starred export from `path` as `(category name, article)` pairs.
pub fn read_saved(path: &Path) -> anyhow::Result<Vec<(String, Article)>> {
    parse_starred(&fs::read_to_string(path)?)
}

/// Add `imported` to the user's saved articles, creating categories by name and
/// skipping links that are already saved. Returns `(articles added, categories added)`.
pub fn merge_saved(user_data: &mut UserData, imported: Vec<(String, Article)>) -> (usize, usize) {
    let mut seen: HashSet<String> = user_data
        .saved_articles
        .iter()
        .map(|s| s.article.link.clone())
        .collect();
    let (mut articles, mut categories) = (0, 0);
    for (name, article) in imported {
        if !seen.insert(article.link.clone()) {
            continue;
        }
        let (category_id, created) = user_data.saved_category_for(&name);
        categories += usize::from(created);
        user_data.saved_articles.push(SavedArticle {
            article,
            category_id,
        });
        articles += 1;
    }
    (articles, categories)
}

fn parse_starred(json: &str) -> anyhow::Result<Vec<(String, Article)>> {
    let (items, _) = parse_stream_contents(json).map_err(anyhow::Error::msg)?;
    Ok(items
        .into_iter()
        .filter(|item| !item.article.link.is_empty())
        .map(|item| {
            let mut article = item.article;
            article.is_saved = true;
            // Ids from another reader's server mean nothing to the FreshRSS sync.
            article.remote_id = None;
            let category = item
                .labels
                .into_iter()
                .next()
                .unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
            (category, article)
        })
        .collect())
}

fn starred_document(user_data: &UserData, feeds: &[Feed], now_secs: i64) -> Value {
    let items: Vec<Value> = user_data
        .saved_articles
        .iter()
        .map(|saved| {
            let label = user_data
                .saved_categories
                .iter()
                .find(|c| c.id == saved.category_id)
                .map(|c| c.name.as_str());
            starred_item(&saved.article, label, feeds)
        })
        .collect();
    json!({
        "id": LABEL_STARRED,
        "title": "Brochure saved articles",
        "updated": now_secs,
        "items": items,
    })
}

fn starred_item(article: &Article, label: Option<&str>, feeds: &[Feed]) -> Value {
    let mut categories = vec![LABEL_STARRED.to_string()];
    if article.is_read {
        categories.push(LABEL_READ.to_string());
    }
    if let Some(label) = label {
        categories.push(format!("user/-/label/{label}"));
    }
    let body = if article.content.trim().is_empty() {
        &article.description
    } else {
        &article.content
    };
    // Readers file imported items under their origin feed, so name it as precisely
    // as we can: the subscribed feed's URL, else the article's site.
    let site = site_root(&article.link).unwrap_or_default();
    let feed_url = feeds
        .iter()
        .find(|f| !article.source_feed.is_empty() && f.title == article.source_feed)
        .map_or_else(|| site.clone(), |f| f.url.clone());
    let mut item = json!({
        "id": article.remote_id.as_deref().unwrap_or(&article.link),
        "title": article.title,
        "canonical": [{ "href": article.link }],
        "alternate": [{ "href": article.link, "type": "text/html" }],
        "categories": categories,
        "summary": { "direction": "ltr", "content": markdown_to_html(&article.description) },
        "content": { "direction": "ltr", "content": markdown_to_html(body) },
        "origin": {
            "streamId": format!("feed/{feed_url}"),
            "title": article.source_feed,
            "htmlUrl": site,
        },
    });
    if let Some(secs) = article.published_secs {
        item["published"] = json!(secs);
        item["updated"] = json!(secs);
    }
    item
}

/// `https://example.com` for `https://example.com/posts/1`.
fn site_root(link: &str) -> Option<String> {
    let url = reqwest::Url::parse(link).ok()?;
    Some(url.origin().ascii_serialization()).filter(|o| o != "null")
}

/// Articles are stored as Markdown; other readers expect HTML.
fn markdown_to_html(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(markdown, options));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SavedCategory;

    fn article(link: &str) -> Article {
        Article {
            title: format!("Title {link}"),
            description: "A *short* summary".to_string(),
            link: link.to_string(),
            is_read: true,
            is_saved: true,
            content: "Body with **bold** text".to_string(),
            images: vec![],
            source_feed: "Example Blog".to_string(),
            remote_id: None,
            published_secs: Some(1_700_000_000),
            is_archived: false,
            error: None,
        }
    }

    fn user_data() -> UserData {
        UserData {
            saved_categories: vec![SavedCategory {
                id: 7,
                name: "Rust".to_string(),
            }],
            saved_articles: vec![SavedArticle {
                article: article("https://example.com/posts/1"),
                category_id: 7,
            }],
            ..UserData::default()
        }
    }

    #[test]
    fn export_writes_greader_starred_items() {
        let feeds = vec![Feed::new_local(
            "Example Blog".to_string(),
            "https://example.com/feed.xml".to_string(),
            None,
            0,
        )];
        let doc = starred_document(&user_data(), &feeds, 5);
        let item = &doc["items"][0];
        assert_eq!(doc["id"], LABEL_STARRED);
        assert_eq!(item["alternate"][0]["href"], "https://example.com/posts/1");
        assert_eq!(item["published"], 1_700_000_000);
        assert_eq!(
            item["origin"]["streamId"],
            "feed/https://example.com/feed.xml"
        );
        assert_eq!(item["origin"]["htmlUrl"], "https://example.com");
        let cats: Vec<&str> = item["categories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert_eq!(cats, [LABEL_STARRED, LABEL_READ, "user/-/label/Rust"]);
        let html = item["content"]["content"].as_str().unwrap();
        assert!(html.contains("<strong>bold</strong>"), "{html}");
    }

    #[test]
    fn export_round_trips_through_import() {
        let json = starred_document(&user_data(), &[], 5).to_string();
        let imported = parse_starred(&json).unwrap();
        assert_eq!(imported.len(), 1);
        let (category, article) = &imported[0];
        assert_eq!(category, "Rust");
        assert_eq!(article.link, "https://example.com/posts/1");
        assert_eq!(article.title, "Title https://example.com/posts/1");
        assert!(article.is_saved && article.is_read);
        assert!(article.content.contains("**bold**"), "{}", article.content);
    }

    /// Shape of a FreshRSS / Inoreader starred export (numeric user ids in labels,
    /// extra fields we don't use).
    #[test]
    fn imports_other_readers_exports() {
        let json = r#"{
            "id": "user/1005921515/state/com.google/starred",
            "title": "Starred items",
            "author": "someone",
            "updated": 1700000000,
            "items": [
                {
                    "id": "tag:google.com,2005:reader/item/00000000148b9369",
                    "crawlTimeMsec": "1700000000000",
                    "published": 1699990000,
                    "title": "Labelled",
                    "categories": [
                        "user/1005921515/state/com.google/starred",
                        "user/1005921515/label/Tech"
                    ],
                    "alternate": [{ "href": "https://a.example/1", "type": "text/html" }],
                    "summary": { "direction": "ltr", "content": "<p>Hello <b>world</b></p>" },
                    "origin": { "streamId": "feed/https://a.example/rss", "title": "A", "htmlUrl": "https://a.example" }
                },
                {
                    "id": "tag:google.com,2005:reader/item/2",
                    "title": "Unlabelled",
                    "alternate": [{ "href": "https://b.example/2" }]
                },
                { "id": "no-link", "title": "Skipped: nothing to open" }
            ]
        }"#;
        let imported = parse_starred(json).unwrap();
        let summary: Vec<(&str, &str)> = imported
            .iter()
            .map(|(c, a)| (c.as_str(), a.link.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("Tech", "https://a.example/1"),
                (DEFAULT_CATEGORY, "https://b.example/2"),
            ]
        );
        let first = &imported[0].1;
        assert_eq!(first.source_feed, "A");
        assert_eq!(first.published_secs, Some(1_699_990_000));
        assert!(first.description.contains("**world**"));
        assert!(first.remote_id.is_none());
    }

    #[test]
    fn merge_skips_saved_links_and_reuses_categories_by_name() {
        let mut data = user_data();
        let imported = vec![
            ("rust".to_string(), article("https://example.com/posts/1")),
            ("RUST".to_string(), article("https://example.com/posts/2")),
            ("News".to_string(), article("https://news.example/3")),
            ("News".to_string(), article("https://news.example/3")),
        ];
        assert_eq!(merge_saved(&mut data, imported), (2, 1));
        let placed: Vec<(&str, u32)> = data
            .saved_articles
            .iter()
            .map(|s| (s.article.link.as_str(), s.category_id))
            .collect();
        let news = data
            .saved_categories
            .iter()
            .find(|c| c.name == "News")
            .unwrap()
            .id;
        assert_eq!(
            placed,
            [
                ("https://example.com/posts/1", 7),
                ("https://example.com/posts/2", 7),
                ("https://news.example/3", news),
            ]
        );
    }
}
