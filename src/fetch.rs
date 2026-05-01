use crate::models::Article;
use std::sync::OnceLock;

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("brochure/0.1 (RSS reader)")
            .build()
            .expect("failed to build HTTP client")
    })
}

fn img_url_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"<img[^>]+src=["'](https?://[^"']+)["']"#).unwrap())
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    }
}

/// Fetch and parse a single RSS/Atom feed URL into a list of articles.
/// Returns `(articles, xml_updated_secs)` where `xml_updated_secs` is the feed-level
/// `<updated>` / `<lastBuildDate>` timestamp as Unix seconds, if present.
pub async fn fetch_feed(url: &str) -> Result<(Vec<Article>, Option<i64>), String> {
    let bytes = http_client()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let parsed = feed_rs::parser::parse(strip_bom(&bytes)).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("no element") || msg.contains("unable to parse feed") {
            "URL is not a valid RSS/Atom feed".to_string()
        } else {
            format!("Failed to parse feed: {msg}")
        }
    })?;
    let xml_updated_secs = parsed.updated.map(|dt| dt.timestamp());

    let articles = parsed
        .entries
        .into_iter()
        .map(|entry| {
            let title = entry
                .title
                .map(|t| t.content)
                .unwrap_or_else(|| "No Title".to_string());
            let description = entry
                .summary
                .map(|s| s.content)
                .unwrap_or_else(|| "No Description".to_string());
            let link = entry
                .links
                .into_iter()
                .next()
                .map(|l| l.href)
                .unwrap_or_default();
            let html_content = entry
                .content
                .and_then(|c| c.body)
                .unwrap_or_else(|| description.clone());
            let image_url = img_url_re()
                .captures(&html_content)
                .map(|caps| caps[1].to_string());
            let content = html2md::parse_html(&html_content);
            let published_secs = entry.published.or(entry.updated).map(|dt| dt.timestamp());

            Article {
                title,
                description,
                link,
                is_read: false,
                is_saved: false,
                content,
                image_url,
                source_feed: String::new(), // filled in by on_feed_fetched in main.rs
                published_secs,
            }
        })
        .collect();

    Ok((articles, xml_updated_secs))
}

/// Fetch just the feed title from a URL (used for AddFeed title auto-fill).
pub async fn fetch_feed_title(url: &str) -> Result<String, String> {
    let bytes = http_client()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let parsed = feed_rs::parser::parse(strip_bom(&bytes)).map_err(|e| e.to_string())?;
    Ok(parsed.title.map(|t| t.content).unwrap_or_default())
}

/// Fetch and extract readable article content from a URL using Mozilla's Readability algorithm.
pub async fn fetch_readable_content(url: &str) -> Result<String, String> {
    let bytes = http_client()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let parsed_url = reqwest::Url::parse(url).map_err(|_| "Invalid URL".to_string())?;
    let mut cursor = std::io::Cursor::new(bytes);
    readability::extractor::extract(&mut cursor, &parsed_url)
        .map(|product| product.content)
        .map_err(|e| format!("Readability error: {e}"))
}
