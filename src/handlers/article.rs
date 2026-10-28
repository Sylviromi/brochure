//! Key event handling for the article views.
//!
//! Covers `ArticleList`, `ArticleDetail`, and `CategoryPicker` states: navigation, read/unread
//! toggling, star/save, opening in a browser, and the save-to-category flow.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

/// Copy text to the clipboard and notify with (a truncated copy of) it.
pub(crate) fn copy_with_status(app: &mut App, text: &str, label: &str) {
    match copy_to_clipboard(text) {
        None => {
            // Truncate on a char boundary: byte slicing panics on multi-byte text.
            let display = match text.char_indices().nth(50) {
                Some((cut, _)) => format!("{}...", &text[..cut]),
                None => text.to_string(),
            };
            app.notify(format!("✓ {label}: {display}"));
        }
        Some(fallback) => {
            app.notify(format!("⚠ Copy not available — {label}: {fallback}"));
        }
    }
}

/// Copy text to clipboard using system clipboard tools, with arboard as last resort.
/// Returns `None` on success, or `Some(text)` if all methods fail (caller should show the text).
pub(crate) fn copy_to_clipboard(text: &str) -> Option<&str> {
    // System tools are more portable across SSH/tmux/Wayland/X11 than arboard.
    let tools: [(&str, &[&str]); 5] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["-i", "-b"]),
        ("tmux", &["load-buffer", "-"]),
        ("pbcopy", &[]),
    ];
    if pipe_to_clipboard_tool(&tools, text.as_bytes()) {
        return None;
    }
    // Last resort: arboard (fails in most non-graphical environments).
    if let Ok(mut c) = arboard::Clipboard::new()
        && c.set_text(text.to_string()).is_ok()
    {
        return None;
    }
    Some(text)
}

/// Copy an image to the clipboard as PNG. Blocking (encodes and spawns processes).
pub(crate) fn copy_image_to_clipboard(
    image: &limner::render_image::img_crate::DynamicImage,
) -> Result<(), String> {
    use limner::render_image::img_crate::ImageFormat;
    let mut png = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let tools: [(&str, &[&str]); 2] = [
        ("wl-copy", &["--type", "image/png"]),
        ("xclip", &["-selection", "clipboard", "-t", "image/png"]),
    ];
    if pipe_to_clipboard_tool(&tools, png.get_ref()) {
        return Ok(());
    }
    let rgba = image.to_rgba8();
    arboard::Clipboard::new()
        .and_then(|mut c| {
            c.set_image(arboard::ImageData {
                width: rgba.width() as usize,
                height: rgba.height() as usize,
                bytes: rgba.as_raw().into(),
            })
        })
        .map_err(|e| e.to_string())
}

/// Feed `data` to the first clipboard tool that is installed and succeeds.
fn pipe_to_clipboard_tool(tools: &[(&str, &[&str])], data: &[u8]) -> bool {
    for (prog, args) in tools {
        let Ok(mut child) = std::process::Command::new(prog)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(data);
        }
        // stdin is dropped here, closing the pipe so the child can exit. A tool that
        // runs but can't reach a clipboard (wl-copy under X11) exits non-zero.
        if child.wait().is_ok_and(|s| s.success()) {
            return true;
        }
    }
    false
}

use crate::{
    app::App,
    fetch::fetch_readable_content,
    models::{AppEvent, AppState, Article, CONTENT_STUB_MAX_LEN, FeedSource},
    storage::save_user_data,
};

use super::category_picker::open_category_picker;

/// Handles key events for `ArticleList` and `ArticleDetail` states.
///
/// Returns `true` if the application should quit.
pub(super) async fn handle_article(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('r') if !app.in_saved_context && !app.in_category_context => {
            app.refresh_feed_at(app.selected_feed, tx);
        }
        KeyCode::Down => {
            if app.state == AppState::ArticleDetail {
                app.scroll_article(1);
            } else {
                app.next();
                prefetch_article_if_stub(app, tx);
            }
        }
        KeyCode::Up => {
            if app.state == AppState::ArticleDetail {
                app.scroll_article(-1);
            } else {
                app.previous();
                prefetch_article_if_stub(app, tx);
            }
        }
        KeyCode::PageDown if app.state == AppState::ArticleDetail => {
            app.scroll_article(app.article_page());
        }
        KeyCode::PageUp if app.state == AppState::ArticleDetail => {
            app.scroll_article(-app.article_page());
        }
        KeyCode::Home if app.state == AppState::ArticleDetail => app.scroll_article(i32::MIN / 2),
        KeyCode::End if app.state == AppState::ArticleDetail => app.scroll_article(i32::MAX / 2),
        KeyCode::Enter if app.state == AppState::ArticleList => {
            open_article(app, tx);
        }
        KeyCode::Char('m') => toggle_read(app, tx),
        KeyCode::Char('s') => open_category_picker(app),
        KeyCode::Char('O') => {
            if let Some(article) = app.selected_article() {
                let _ = open::that(&article.link);
            }
        }
        KeyCode::Char('C') => {
            if app.state == AppState::ArticleDetail || app.in_saved_context {
                if let Some(link) = app.selected_article().map(|a| a.link.clone()) {
                    copy_with_status(app, &link, "Copied");
                }
            } else {
                let url = if app.in_category_context {
                    let (fi, _ai) = app
                        .category_view_articles
                        .get(app.selected_article)
                        .copied()
                        .unwrap_or((app.selected_feed, 0));
                    app.feeds.get(fi).map(|f| f.url.clone())
                } else {
                    app.feeds.get(app.selected_feed).map(|f| f.url.clone())
                };
                if let Some(url) = url {
                    copy_with_status(app, &url, "Copied feed URL");
                }
            }
        }
        KeyCode::Char('z') if app.state == AppState::ArticleDetail => {
            app.zen_mode = true;
        }
        KeyCode::Esc => app.unselect(),
        _ => {}
    }
    false
}

/// Opens the selected article in detail view, marks it as read, and fetches full content if needed.
fn open_article(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    let article = get_selected_article(app);
    let Some(article) = article else { return };
    mark_article_as_read(app, tx, &article);
    prefetch_article_if_stub(app, tx);
    app.select();
}

/// Returns a clone of the article that is currently highlighted, regardless of view context.
///
/// Returns `None` when no feed is selected, the article list is empty, or indices are out of
/// bounds.
/// Prefer [`App::selected_article`] when a borrow suffices.
pub fn get_selected_article(app: &App) -> Option<Article> {
    app.selected_article().cloned()
}

/// Mark an article as read in every feed that contains it, and sync saved-article state.
fn mark_feed_by_link(app: &mut App, link: &str) {
    for feed in app.feeds.iter_mut() {
        if let Some(a) = feed.articles.iter_mut().find(|a| a.link == link) {
            a.is_read = true;
        }
        feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
    }
    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == link)
    {
        s.article.is_read = true;
    }
}

/// Marks an article as read and persists the updated read-links set.
fn mark_article_as_read(app: &mut App, tx: &UnboundedSender<AppEvent>, article: &Article) {
    if article.is_read {
        return;
    }
    app.user_data.read_links.insert(article.link.clone());
    let _ = save_user_data(&app.user_data);
    mark_feed_by_link(app, &article.link);
    app.push_read_state(article.remote_id.clone(), true, tx);

    // In saved context, also update the in-memory saved-view list.
    if app.in_saved_context
        && let Some(a) = app.saved_view_articles.get_mut(app.selected_article)
    {
        a.is_read = true;
    }
}

/// Toggles the read/unread state of the currently selected article and persists the change.
fn toggle_read(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    let (link, is_read) = if app.in_category_context {
        let &(fi, ai) = match app.category_view_articles.get(app.selected_article) {
            Some(v) => v,
            None => return,
        };
        let feed = match app.feeds.get_mut(fi) {
            Some(v) => v,
            None => return,
        };
        let art = match feed.articles.get_mut(ai) {
            Some(v) => v,
            None => return,
        };
        art.is_read = !art.is_read;
        let link = art.link.clone();
        let is_read = art.is_read;
        let _ = art;
        feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
        (link, is_read)
    } else if app.in_saved_context {
        let art = match app.saved_view_articles.get_mut(app.selected_article) {
            Some(v) => v,
            None => return,
        };
        art.is_read = !art.is_read;
        let link = art.link.clone();
        let is_read = art.is_read;
        let source_feed = art.source_feed.clone();
        if let Some(feed) = app.feeds.iter_mut().find(|f| f.title == source_feed) {
            if let Some(a) = feed.articles.iter_mut().find(|a| a.link == link) {
                a.is_read = is_read;
            }
            feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
        }
        (link, is_read)
    } else {
        let feed = match app.feeds.get_mut(app.selected_feed) {
            Some(v) => v,
            None => return,
        };
        let art = match feed.articles.get_mut(app.selected_article) {
            Some(v) => v,
            None => return,
        };
        art.is_read = !art.is_read;
        let link = art.link.clone();
        let is_read = art.is_read;
        let _ = art;
        feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
        (link, is_read)
    };

    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == link)
    {
        s.article.is_read = is_read;
    }
    if is_read {
        app.user_data.read_links.insert(link.clone());
    } else {
        app.user_data.read_links.remove(&link);
    }
    let _ = save_user_data(&app.user_data);
    let remote_id = app
        .feeds
        .iter()
        .flat_map(|f| f.articles.iter())
        .find(|a| a.link == link)
        .and_then(|a| a.remote_id.clone());
    app.push_read_state(remote_id, is_read, tx);
}

/// Resolve the feed source, article index, and link for the currently selected article
/// across the three view contexts (category, saved, feed).
fn resolve_article_source(app: &App) -> Option<(FeedSource, usize, String)> {
    if app.in_category_context {
        let &(feed_idx, art_idx) = app.category_view_articles.get(app.selected_article)?;
        let article = app.feeds.get(feed_idx)?.articles.get(art_idx)?;
        Some((FeedSource::Feed(feed_idx), art_idx, article.link.clone()))
    } else if app.in_saved_context {
        let article = app.saved_view_articles.get(app.selected_article)?;
        Some((
            FeedSource::Saved,
            app.selected_article,
            article.link.clone(),
        ))
    } else {
        let article = app
            .feeds
            .get(app.selected_feed)?
            .articles
            .get(app.selected_article)?;
        Some((
            FeedSource::Feed(app.selected_feed),
            app.selected_article,
            article.link.clone(),
        ))
    }
}

/// Fetch the full content of the selected article when the feed only gave a stub —
/// once per article per session, never while a fetch for it is already running.
/// Used both when the cursor lands on an article and when it is opened.
pub(crate) fn prefetch_article_if_stub(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    let Some((source, art_idx, link)) = resolve_article_source(app) else {
        return;
    };
    let is_stub = match source {
        FeedSource::Feed(fi) => app
            .feeds
            .get(fi)
            .and_then(|f| f.articles.get(art_idx))
            .is_some_and(|a| a.content.len() < CONTENT_STUB_MAX_LEN),
        FeedSource::Saved => app
            .saved_view_articles
            .get(art_idx)
            .is_some_and(|a| a.content.len() < CONTENT_STUB_MAX_LEN),
    };
    if !is_stub || !app.full_fetch_tried.insert(link.clone()) {
        return;
    }
    app.full_fetch_pending.insert(link.clone());
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = fetch_readable_content(&link).await;
        let _ = tx.send(AppEvent::FullArticleFetched(source, art_idx, link, result));
    });
}
