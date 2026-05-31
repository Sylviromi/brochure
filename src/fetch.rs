//! Async network I/O for brochure: feed fetching, MediaRSS image extraction, and Readability-based
//! article content retrieval. All HTTP requests share a single lazily-initialised client.

use crate::models::{Article, ReleaseNote, UpdateInfo};
use std::sync::OnceLock;

/// Extract image URLs from `<img src="...">` tags in HTML content.
fn extract_img_src(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let bytes = html.as_bytes();
    let mut offset = 0;
    while offset + 4 <= bytes.len() {
        if bytes[offset] == b'<'
            && (bytes[offset + 1] == b'i' || bytes[offset + 1] == b'I')
            && (bytes[offset + 2] == b'm' || bytes[offset + 2] == b'M')
            && (bytes[offset + 3] == b'g' || bytes[offset + 3] == b'G')
        {
            let rest = &html[offset..];
            let end = rest.find('>').map(|e| e + 1).unwrap_or(rest.len());
            let tag = &rest[..end];
            let low_tag = tag.to_lowercase();
            if let Some(src_pos) = low_tag.find(" src=") {
                let after_src = &tag[src_pos + 5..];
                let quote = after_src.chars().next().unwrap_or('"');
                if (quote == '"' || quote == '\'')
                    && let Some(qend) = after_src[1..].find(quote)
                {
                    let src = &after_src[1..=qend];
                    if !urls.contains(&src.to_string()) {
                        urls.push(src.to_string());
                    }
                }
            }
            offset += end;
        } else {
            offset += 1;
        }
    }
    urls
}

/// Maximum number of simultaneous feed-fetch requests.
const MAX_CONCURRENT_FETCHES: usize = 6;

/// Shared HTTP GET → bytes helper. Used by all fetch functions to avoid repeating the
/// send/bytes/error chain.
async fn http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = http_client()
        .get(url)
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let body = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    Ok(body)
}

/// Fetch bytes with an RSS-specific Accept header and challenge-page detection.
async fn http_get_feed_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = http_client()
        .get(url)
        .header(
            "Accept",
            "application/rss+xml, application/xml, text/xml, */*",
        )
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "none")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    let body = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    if !status.is_success() {
        if is_challenge_page(&body) {
            return Err(
                "Server returned a challenge page — the feed is behind bot protection.".to_string(),
            );
        }
        return Err(fmt_http_error(status, retry_after));
    }
    if is_challenge_page(&body) {
        return Err(
            "Server returned a challenge page — the feed is behind bot protection.".to_string(),
        );
    }
    Ok(body)
}

/// Returns the shared, lazily-initialised HTTP client used for all outgoing requests.
pub(crate) fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client")
    })
}

/// Check if a response body is an HTML challenge page rather than a valid feed.
fn is_challenge_page(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body).to_lowercase();
    text.contains("enable javascript and cookies")
        || text.contains("challenge-error-text")
        || text.contains("we have detected unusual activity")
        || text.contains("cf-browser-verification")
        || text.contains("_cf_chl_opt")
        || text.contains("just a moment")
        || text.contains("sgcaptcha")
        || text.contains("anubis")
        || text.contains("not a bot")
        || text.contains("making sure you're")
        || (text.starts_with("<!doctype html") || text.starts_with("<html"))
            && !text.contains("<rss")
            && !text.contains("<feed")
            && !text.contains("<rdf:rdf")
}

/// Produce a user-friendly error message for non-success HTTP status codes.
fn fmt_http_error(status: reqwest::StatusCode, retry_after: Option<u64>) -> String {
    match status.as_u16() {
        401 => "HTTP 401 Unauthorized — this feed requires authentication".to_string(),
        403 => "HTTP 403 Forbidden — server is blocking automated access".to_string(),
        404 => "HTTP 404 Not Found — the feed URL may have changed or been removed".to_string(),
        429 => match retry_after {
            Some(secs) => format!("HTTP 429 Too Many Requests — retry after {secs}s"),
            None => "HTTP 429 Too Many Requests — try again later".to_string(),
        },
        503 => "HTTP 503 Service Unavailable — server is temporarily down".to_string(),
        _ => format!("HTTP {status}"),
    }
}

/// Classify a fetch error string into a short tag for display in the sidebar.
/// Returns `(tag, is_permanent)` where `is_permanent` means the problem is
/// unlikely to resolve on its own (blocked, invalid URL) vs transient (rate
/// limit, network, server down).
pub(crate) fn feed_error_tag(error: &str) -> (&'static str, bool) {
    if error.starts_with("HTTP 401") {
        ("unauthorized", true)
    } else if error.starts_with("HTTP 403")
        || error.contains("challenge")
        || error.contains("bot protection")
    {
        ("blocked", true)
    } else if error.starts_with("HTTP 404") {
        ("not found", true)
    } else if error.starts_with("HTTP 429") {
        ("rate limit", false)
    } else if error.starts_with("HTTP 503") {
        ("down", false)
    } else if error.starts_with("Network error") || error.starts_with("Semaphore error") {
        ("unreachable", false)
    } else if error.contains("not a valid") || error.starts_with("Failed to parse") {
        ("invalid", true)
    } else {
        ("error", false)
    }
}

/// Semaphore that limits concurrent feed fetches to avoid overwhelming servers.
fn fetch_semaphore() -> &'static tokio::sync::Semaphore {
    static SEM: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    SEM.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_FETCHES))
}

/// Strips a UTF-8 BOM (`EF BB BF`) from the start of a byte slice if one is present.
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
    // Limit concurrent in-flight feed fetches to avoid overwhelming servers.
    let _permit = fetch_semaphore()
        .acquire()
        .await
        .map_err(|e| format!("Semaphore error: {e}"))?;
    let bytes = http_get_feed_bytes(url).await?;

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
            let html_body = entry
                .content
                .and_then(|c| c.body)
                .filter(|b| !b.trim().is_empty());
            let desc_html = if description != "No Description" {
                description.clone()
            } else {
                String::new()
            };
            let html_content = html_body.clone().unwrap_or(desc_html.clone());
            let mut images: Vec<String> = Vec::new();
            // Collect images from MediaRSS attachments.
            for media in &entry.media {
                for content in &media.content {
                    if let Some(url) = &content.url {
                        let s = url.to_string();
                        if !images.contains(&s) {
                            images.push(s);
                        }
                    }
                }
                for thumb in &media.thumbnails {
                    let s = thumb.image.uri.clone();
                    if !images.contains(&s) {
                        images.push(s);
                    }
                }
            }
            // Convert HTML to Markdown.
            let content = html_to_markdown_rs::convert(&html_content, None)
                .ok()
                .and_then(|r| r.content)
                .unwrap_or_default();
            // Extract image URLs directly from the markdown output to guarantee
            // they match the ![...](url) references that limner will scan for.
            let mut markdown_img_urls: Vec<String> = Vec::new();
            {
                let mut remaining = content.as_str();
                while let Some(start) = remaining.find("![") {
                    let after_alt = &remaining[start + 2..];
                    match after_alt.find("](") {
                        Some(paren) => {
                            let url_start = paren + 2;
                            let after_paren = &after_alt[url_start..];
                            match after_paren.find(')') {
                                Some(end) => {
                                    let url = &after_paren[..end];
                                    if !markdown_img_urls.contains(&url.to_string()) {
                                        markdown_img_urls.push(url.to_string());
                                    }
                                    remaining = &after_paren[end + 1..];
                                }
                                None => break,
                            }
                        }
                        None => {
                            remaining = &remaining[start + 2..];
                        }
                    }
                }
            }
            for url in &markdown_img_urls {
                if !images.contains(url) {
                    images.push(url.clone());
                }
            }
            // Extract image URLs from both body and description HTML.
            if let Some(body) = &html_body {
                for url in extract_img_src(body) {
                    if !images.contains(&url) {
                        images.push(url);
                    }
                }
            }
            if !desc_html.is_empty() {
                for url in extract_img_src(&desc_html) {
                    if !images.contains(&url) {
                        images.push(url);
                    }
                }
            }
            let published_secs = entry.published.or(entry.updated).map(|dt| dt.timestamp());

            Article {
                title,
                description,
                link,
                is_read: false,
                is_saved: false,
                content,
                images,
                source_feed: String::new(), // filled in by on_feed_fetched in main.rs
                published_secs,
                is_archived: false,
                error: None,
            }
        })
        .collect();

    Ok((articles, xml_updated_secs))
}

/// Fetch just the feed title from a URL (used for AddFeed title auto-fill).
pub async fn fetch_feed_title(url: &str) -> Result<String, String> {
    let bytes = http_get_feed_bytes(url).await?;

    let parsed = feed_rs::parser::parse(strip_bom(&bytes)).map_err(|e| e.to_string())?;
    Ok(parsed.title.map(|t| t.content).unwrap_or_default())
}

/// Fetch the full article HTML from the article's URL using Readability, then convert to markdown.
pub async fn fetch_readable_content(url: &str) -> Result<String, String> {
    let bytes = http_get_bytes(url).await?;

    let parsed_url = reqwest::Url::parse(url).map_err(|_| "Invalid URL".to_string())?;
    let mut cursor = std::io::Cursor::new(bytes);
    readability::extractor::extract(&mut cursor, &parsed_url)
        .map(|product| product.content)
        .map_err(|e| format!("Readability error: {e}"))
}

/// Fetch the latest published versions of brochure from crates.io and GitHub releases.
/// Filters GitHub releases to only those newer than the current version, sorted newest-first.
/// Returns `Some(UpdateInfo)` if any newer versions exist, `None` if already up to date.
/// GitHub release fetch failures are graceful — falls back to returning `Some` with empty releases list.
pub async fn check_latest_version() -> Option<UpdateInfo> {
    // Step 1: Crates.io check (early exit if version matches)
    let resp = http_client()
        .get("https://crates.io/api/v1/crates/brochure")
        .header(
            "User-Agent",
            concat!("brochure/", env!("CARGO_PKG_VERSION"), " (version-check)"),
        )
        .send()
        .await
        .ok()?;
    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let crates_latest = json["crate"]["newest_version"].as_str()?.to_string();
    let current = env!("CARGO_PKG_VERSION");
    if crates_latest == current {
        return None;
    }

    // Step 2: GitHub releases fetch (returns array, graceful failure)
    let releases = if let Ok(gh_resp) = http_client()
        .get("https://api.github.com/repos/Sylviromi/brochure/releases")
        .header(
            "User-Agent",
            concat!("brochure/", env!("CARGO_PKG_VERSION"), " (version-check)"),
        )
        .send()
        .await
    {
        if let Ok(gh_text) = gh_resp.text().await {
            if let Ok(gh_json) = serde_json::from_str::<serde_json::Value>(&gh_text) {
                if let Some(releases_array) = gh_json.as_array() {
                    let mut release_notes = Vec::new();

                    for release in releases_array {
                        // Parse tag_name and strip leading 'v'
                        let tag_name = match release["tag_name"].as_str() {
                            Some(tag) => tag.strip_prefix('v').unwrap_or(tag),
                            None => continue,
                        };

                        // Step 3: Version filtering - only keep newer versions
                        if !is_newer_version(tag_name, current) {
                            continue;
                        }

                        // Step 4: Parse release into ReleaseNote
                        let date = release["published_at"]
                            .as_str()
                            .map(|s| s.chars().take(10).collect::<String>())
                            .unwrap_or_default();

                        let highlights = if let Some(body) = release["body"].as_str() {
                            let mut highlights_vec = Vec::new();
                            let mut in_highlights_section = false;
                            for line in body.lines() {
                                if line.starts_with("## Highlights") {
                                    in_highlights_section = true;
                                } else if line.starts_with("## ") {
                                    in_highlights_section = false;
                                } else if in_highlights_section && line.starts_with("- ") {
                                    highlights_vec.push(line[2..].to_string());
                                }
                            }
                            highlights_vec
                        } else {
                            Vec::new()
                        };

                        release_notes.push(ReleaseNote {
                            version: tag_name.to_string(),
                            date,
                            highlights,
                        });
                    }

                    // Step 5: Sort newest-first by semver
                    release_notes.sort_by(|a, b| {
                        let a_semver = parse_semver(&a.version);
                        let b_semver = parse_semver(&b.version);
                        b_semver.cmp(&a_semver)
                    });

                    release_notes
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Return None if no newer versions, otherwise return Some with releases list
    if releases.is_empty() {
        None
    } else {
        Some(UpdateInfo { releases })
    }
}

/// Parse a semantic version string into (major, minor, patch) tuple.
fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// Check if `tag_version` is strictly newer than `current_version` using semver comparison.
fn is_newer_version(tag_version: &str, current_version: &str) -> bool {
    match (parse_semver(tag_version), parse_semver(current_version)) {
        (Some(tag_semver), Some(current_semver)) => tag_semver > current_semver,
        _ => false,
    }
}

/// Download raw image bytes from a URL using the shared HTTP client.
///
/// Protocol-relative URLs (starting with `//`) are expanded to `https:`.
pub async fn fetch_image(url: &str) -> Result<Vec<u8>, String> {
    let resolved = if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_string()
    };
    if !resolved.starts_with("http://") && !resolved.starts_with("https://") {
        return Err(format!("skipping non-HTTP URL: {url}"));
    }
    http_get_bytes(&resolved).await
}
