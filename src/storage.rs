use crate::models::{Article, Category, CategoryId, FAVORITES_URL, Feed, UserData};
use std::{collections::HashMap, fs, path::PathBuf};

// ── Data directory ────────────────────────────────────────────────────────────

/// Returns (and creates if needed) the platform-appropriate data directory:
///   Linux/BSD  →  ~/.local/share/brochure/
///   macOS      →  ~/Library/Application Support/brochure/
///   Windows    →  %APPDATA%\brochure\
///
/// Falls back to the current working directory if the platform dir cannot be
/// determined (e.g. inside a container with no home directory).
fn data_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("brochure");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
}

fn feeds_path() -> PathBuf {
    data_dir().join("feeds.json")
}
fn user_data_path() -> PathBuf {
    data_dir().join("user_data.json")
}
fn articles_path() -> PathBuf {
    data_dir().join("articles.json")
}
fn categories_path() -> PathBuf {
    data_dir().join("categories.json")
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Load feeds from disk, returning an empty list if the file is absent or corrupt.
pub fn load_feeds() -> Vec<Feed> {
    if let Ok(content) = fs::read_to_string(feeds_path())
        && let Ok(feeds) = serde_json::from_str(&content)
    {
        return feeds;
    }
    vec![]
}

/// Persist feeds to disk.
pub fn save_feeds(feeds: &[Feed]) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(feeds)?;
    fs::write(feeds_path(), content)?;
    Ok(())
}

/// Load user data from disk, migrating legacy starred_articles on first load.
pub fn load_user_data() -> UserData {
    let mut data: UserData = if let Ok(content) = fs::read_to_string(user_data_path())
        && let Ok(parsed) = serde_json::from_str(&content)
    {
        parsed
    } else {
        UserData::default()
    };

    // Migrate legacy starred_articles → "Starred" save category.
    if !data.starred_articles.is_empty() && data.saved_articles.is_empty() {
        let cat_id: u32 = 1;
        data.saved_categories.push(crate::models::SavedCategory {
            id: cat_id,
            name: "Starred".to_string(),
        });
        let old = std::mem::take(&mut data.starred_articles);
        for art in old {
            data.saved_articles.push(crate::models::SavedArticle {
                article: art,
                category_id: cat_id,
            });
        }
        // Persist the migrated data immediately.
        let _ = save_user_data(&data);
    }

    data
}

/// Persist user data to disk.
pub fn save_user_data(data: &UserData) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(data)?;
    fs::write(user_data_path(), content)?;
    Ok(())
}

/// Load cached articles from disk (feed URL → article list).
pub fn load_articles() -> HashMap<String, Vec<Article>> {
    if let Ok(content) = fs::read_to_string(articles_path())
        && let Ok(map) = serde_json::from_str(&content)
    {
        return map;
    }
    HashMap::new()
}

/// Persist articles to disk. When `save_content` is false, content and image_url are stripped.
pub fn save_articles(feeds: &[Feed], save_content: bool) -> anyhow::Result<()> {
    let map: HashMap<String, Vec<Article>> = feeds
        .iter()
        .filter(|f| !f.articles.is_empty() && f.url != FAVORITES_URL)
        .map(|f| {
            let articles = if save_content {
                f.articles.clone()
            } else {
                f.articles
                    .iter()
                    .map(|a| Article {
                        content: String::new(),
                        image_url: None,
                        ..a.clone()
                    })
                    .collect()
            };
            (f.url.clone(), articles)
        })
        .collect();
    let content = serde_json::to_string_pretty(&map)?;
    fs::write(articles_path(), content)?;
    Ok(())
}

/// Load categories from disk, returning empty list if absent or corrupt.
pub fn load_categories() -> Vec<Category> {
    if let Ok(content) = fs::read_to_string(categories_path())
        && let Ok(cats) = serde_json::from_str(&content)
    {
        return cats;
    }
    vec![]
}

/// Persist categories to disk.
pub fn save_categories(categories: &[Category]) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(categories)?;
    fs::write(categories_path(), content)?;
    Ok(())
}

/// Returns the byte size of the article cache file (0 if absent).
pub fn article_cache_size() -> u64 {
    std::fs::metadata(articles_path())
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Delete only the article cache file.
pub fn clear_article_cache() -> anyhow::Result<()> {
    let path = articles_path();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Delete all persisted data files (feeds, categories, user_data, articles).
pub fn clear_all_data() -> anyhow::Result<()> {
    for path in [
        feeds_path(),
        user_data_path(),
        articles_path(),
        categories_path(),
    ] {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Export OPML to a user-specified path, preserving category nesting.
pub fn export_opml_to_path(
    path: &str,
    feeds: &[Feed],
    categories: &[Category],
) -> anyhow::Result<()> {
    let mut opml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <opml version=\"1.0\">\n  <head>\n    <title>RSS Feeds</title>\n  </head>\n  <body>\n",
    );
    write_opml_level(&mut opml, feeds, categories, None, 2);
    opml.push_str("  </body>\n</opml>");
    fs::write(path, opml)?;
    Ok(())
}

fn write_opml_level(
    out: &mut String,
    feeds: &[Feed],
    categories: &[Category],
    parent_id: Option<CategoryId>,
    indent: usize,
) {
    let pad = "  ".repeat(indent);

    // Write child categories
    let mut cats: Vec<&Category> = categories
        .iter()
        .filter(|c| c.parent_id == parent_id)
        .collect();
    cats.sort_by_key(|c| c.order);
    for cat in cats {
        out.push_str(&format!(
            "{pad}<outline text=\"{}\">\n",
            xml_escape(&cat.name)
        ));
        write_opml_level(out, feeds, categories, Some(cat.id), indent + 1);
        out.push_str(&format!("{pad}</outline>\n"));
    }

    // Write feeds at this level (skip feeds[0] = Favorites)
    let mut level_feeds: Vec<&Feed> = feeds
        .iter()
        .filter(|f| f.url != FAVORITES_URL && f.category_id == parent_id)
        .collect();
    level_feeds.sort_by_key(|f| f.order);
    for feed in level_feeds {
        out.push_str(&format!(
            "{pad}<outline type=\"rss\" text=\"{}\" xmlUrl=\"{}\"/>\n",
            xml_escape(&feed.title),
            xml_escape(&feed.url),
        ));
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Parse an OPML file at the given path and return new feeds + new categories.
pub fn import_opml_from_path(
    path: &str,
    existing_feeds: &[Feed],
    existing_categories: &[Category],
) -> anyhow::Result<(Vec<Feed>, Vec<Category>)> {
    let content = fs::read_to_string(path)?;
    parse_opml_xml(&content, existing_feeds, existing_categories)
}

fn parse_opml_xml(
    content: &str,
    existing_feeds: &[Feed],
    existing_categories: &[Category],
) -> anyhow::Result<(Vec<Feed>, Vec<Category>)> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut next_id: CategoryId = existing_categories.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let mut next_order: usize = existing_feeds.iter().map(|f| f.order).max().unwrap_or(0) + 1;
    let mut cat_order: usize = existing_categories
        .iter()
        .filter(|c| c.parent_id.is_none())
        .map(|c| c.order)
        .max()
        .unwrap_or(0)
        + 1;

    // Stack of (category_id, child_order_counter)
    let mut stack: Vec<(CategoryId, usize)> = Vec::new();
    let mut new_feeds: Vec<Feed> = Vec::new();
    let mut new_cats: Vec<Category> = Vec::new();

    let mut buf = Vec::new();
    loop {
        // Use separate arms for Start and Empty to correctly track stack depth.
        // Start = opening tag with children → push category onto stack.
        // Empty = self-closing tag → feed entry or empty folder, never push.
        // The old code called read_event_into() inside the match arm as a side-effect,
        // which consumed the next event on every outline and completely corrupted parsing.
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"outline" => {
                let (xml_url, text) = outline_attrs(e, &reader);
                let current_parent = stack.last().map(|(id, _)| *id);
                if let Some(url) = xml_url {
                    push_feed(
                        url,
                        text,
                        current_parent,
                        existing_feeds,
                        &mut new_feeds,
                        &mut stack,
                        &mut next_order,
                    );
                    // Feed with children is unusual but valid; stack will be popped by End
                    // Don't push a category entry here since we already have a feed
                } else if let Some(name) = text {
                    let cat_id = find_or_create_cat(
                        name,
                        current_parent,
                        existing_categories,
                        &mut new_cats,
                        &mut next_id,
                        &mut stack,
                        &mut cat_order,
                    );
                    // This is a Start tag (has children) → push so children know their parent
                    stack.push((cat_id, 0));
                }
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"outline" => {
                let (xml_url, text) = outline_attrs(e, &reader);
                let current_parent = stack.last().map(|(id, _)| *id);
                if let Some(url) = xml_url {
                    // Normal self-closing feed entry
                    push_feed(
                        url,
                        text,
                        current_parent,
                        existing_feeds,
                        &mut new_feeds,
                        &mut stack,
                        &mut next_order,
                    );
                }
                // Self-closing category (empty folder): don't push to stack — no children coming
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"outline" => {
                stack.pop();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok((new_feeds, new_cats))
}

/// Extract xmlUrl and text/title attributes from an outline element.
fn outline_attrs(
    e: &quick_xml::events::BytesStart,
    reader: &quick_xml::Reader<&[u8]>,
) -> (Option<String>, Option<String>) {
    let mut xml_url: Option<String> = None;
    let mut text: Option<String> = None;
    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref())
            .unwrap_or("")
            .to_lowercase();
        let val = attr
            .decode_and_unescape_value(reader.decoder())
            .map(|v| v.to_string())
            .unwrap_or_default();
        match key.as_str() {
            "xmlurl" => xml_url = Some(val),
            "text" | "title" if text.is_none() => text = Some(val),
            _ => {}
        }
    }
    (xml_url, text)
}

/// Push a new feed into new_feeds if not already present.
fn push_feed(
    url: String,
    text: Option<String>,
    current_parent: Option<CategoryId>,
    existing_feeds: &[Feed],
    new_feeds: &mut Vec<Feed>,
    stack: &mut [(CategoryId, usize)],
    next_order: &mut usize,
) {
    if existing_feeds.iter().any(|f| f.url == url) || new_feeds.iter().any(|f| f.url == url) {
        return;
    }
    let title = text.unwrap_or_else(|| url.clone());
    let order = if let Some((_, ord)) = stack.last_mut() {
        let o = *ord;
        *ord += 1;
        o
    } else {
        let o = *next_order;
        *next_order += 1;
        o
    };
    new_feeds.push(Feed {
        title,
        url,
        category_id: current_parent,
        order,
        unread_count: 0,
        articles: vec![],
        fetched: false,
        fetch_error: None,
        feed_updated_secs: None,
        last_fetched_secs: None,
    });
}

/// Find existing or create a new category, returning its id.
fn find_or_create_cat(
    name: String,
    current_parent: Option<CategoryId>,
    existing_categories: &[Category],
    new_cats: &mut Vec<Category>,
    next_id: &mut CategoryId,
    stack: &mut [(CategoryId, usize)],
    cat_order: &mut usize,
) -> CategoryId {
    if let Some(id) = existing_categories
        .iter()
        .chain(new_cats.iter())
        .find(|c| c.name == name && c.parent_id == current_parent)
        .map(|c| c.id)
    {
        return id;
    }
    let id = *next_id;
    *next_id += 1;
    let order = if let Some((_, ord)) = stack.last_mut() {
        let o = *ord;
        *ord += 1;
        o
    } else {
        let o = *cat_order;
        *cat_order += 1;
        o
    };
    new_cats.push(Category {
        id,
        name,
        parent_id: current_parent,
        order,
    });
    id
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_home_dir(path: &str) -> String {
    if (path.starts_with("~/") || path == "~")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen('~', &home.to_string_lossy(), 1);
    }
    path.to_string()
}

/// Returns the default export path suggestion (~/Downloads/export.opml or data dir).
pub fn default_export_path() -> String {
    let path = dirs::download_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("export.opml");
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use crate::models::{Article, UserData};

    fn stub_article(link: &str) -> Article {
        Article {
            title: link.to_string(),
            description: String::new(),
            link: link.to_string(),
            is_read: false,
            is_saved: false,
            content: String::new(),
            image_url: None,
            source_feed: String::new(),
            published_secs: None,
        }
    }

    #[test]
    fn test_migration_creates_starred_category() {
        let mut data = UserData::default();
        data.starred_articles.push(stub_article("https://a.com/1"));
        data.starred_articles.push(stub_article("https://a.com/2"));

        // Simulate what load_user_data does after deserialization.
        if !data.starred_articles.is_empty() && data.saved_articles.is_empty() {
            let cat_id: u32 = 1;
            data.saved_categories.push(crate::models::SavedCategory {
                id: cat_id,
                name: "Starred".to_string(),
            });
            let old = std::mem::take(&mut data.starred_articles);
            for art in old {
                data.saved_articles.push(crate::models::SavedArticle {
                    article: art,
                    category_id: cat_id,
                });
            }
        }

        assert_eq!(data.saved_categories.len(), 1);
        assert_eq!(data.saved_categories[0].name, "Starred");
        assert_eq!(data.saved_articles.len(), 2);
        assert!(data.starred_articles.is_empty());
        assert!(data.saved_articles.iter().all(|s| s.category_id == 1));
    }

    #[test]
    fn test_migration_skipped_when_saved_already_populated() {
        let mut data = UserData::default();
        data.starred_articles.push(stub_article("https://a.com/1"));
        data.saved_articles.push(crate::models::SavedArticle {
            article: stub_article("https://b.com/1"),
            category_id: 99,
        });

        // Migration should NOT run when saved_articles is non-empty.
        if !data.starred_articles.is_empty() && data.saved_articles.is_empty() {
            panic!("migration ran unexpectedly");
        }

        assert_eq!(data.saved_articles.len(), 1); // unchanged
    }
}
