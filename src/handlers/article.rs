//! Key event handling for the article views.
//!
//! Covers `ArticleList`, `ArticleDetail`, and `CategoryPicker` states: navigation, read/unread
//! toggling, star/save, opening in a browser, and the save-to-category flow.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

/// Copy text to clipboard and show a truncated status message on the app.
pub(crate) fn copy_with_status(app: &mut App, text: &str, label: &str) {
    match copy_to_clipboard(text) {
        None => {
            let display = if text.len() > 50 {
                format!("{}...", &text[..50])
            } else {
                text.to_string()
            };
            app.set_status(format!("{label}: {display}"));
        }
        Some(fallback) => {
            app.set_status(format!("Copy not available — {label}: {fallback}"));
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
    for (prog, args) in &tools {
        let Ok(mut child) = std::process::Command::new(*prog)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        }
        // stdin is dropped here, closing the pipe so the child can exit.
        if child.wait().is_ok() {
            return None;
        }
    }
    // Last resort: arboard (fails in most non-graphical environments).
    if let Ok(mut c) = arboard::Clipboard::new()
        && c.set_text(text.to_string()).is_ok()
    {
        return None;
    }
    Some(text)
}

use crate::{
    app::App,
    fetch::{fetch_feed, fetch_readable_content},
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
            let idx = app.selected_feed;
            if let Some(feed) = app.feeds.get_mut(idx) {
                let url = feed.url.clone();
                let title = feed.title.clone();
                feed.fetched = false;
                feed.fetch_error = None;
                app.set_status(format!("Refreshing {title}..."));
                app.feeds_pending += 1;
                app.feeds_total += 1;
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let result = fetch_feed(&url).await;
                    let _ = tx2.send(AppEvent::FeedFetched(idx, result));
                });
            }
        }
        KeyCode::Down => {
            if app.state == AppState::ArticleDetail {
                let max = app
                    .content_line_count
                    .saturating_sub(app.content_area_height as usize)
                    as u16;
                if let Some(article) = get_selected_article(app) {
                    app.article_scroll.scroll_down(&article.link, max);
                }
            } else {
                app.next();
                prefetch_article_if_stub(app, tx);
            }
        }
        KeyCode::Up => {
            if app.state == AppState::ArticleDetail {
                if let Some(article) = get_selected_article(app) {
                    app.article_scroll.scroll_up(&article.link);
                }
            } else {
                app.previous();
                prefetch_article_if_stub(app, tx);
            }
        }
        KeyCode::Enter if app.state == AppState::ArticleList => {
            open_article(app, tx);
        }
        KeyCode::Char('m') => toggle_read(app),
        KeyCode::Char('s') => open_category_picker(app),
        KeyCode::Char('O') => {
            if let Some(article) = get_selected_article(app) {
                let _ = open::that(&article.link);
            }
        }
        KeyCode::Char('C') => {
            if app.state == AppState::ArticleDetail || app.in_saved_context {
                if let Some(article) = get_selected_article(app) {
                    copy_with_status(app, &article.link.clone(), "Copied");
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
    mark_article_as_read(app, &article);
    fetch_full_article_if_stub(app, tx, &article);
    app.select();
}

/// Returns a clone of the article that is currently highlighted, regardless of view context.
///
/// Returns `None` when no feed is selected, the article list is empty, or indices are out of
/// bounds.
pub fn get_selected_article(app: &App) -> Option<Article> {
    if app.in_category_context {
        let &(fi, ai) = app.category_view_articles.get(app.selected_article)?;
        app.feeds.get(fi)?.articles.get(ai).cloned()
    } else if app.in_saved_context {
        app.saved_view_articles.get(app.selected_article).cloned()
    } else {
        app.feeds
            .get(app.selected_feed)
            .and_then(|f| f.articles.get(app.selected_article))
            .cloned()
    }
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
fn mark_article_as_read(app: &mut App, article: &Article) {
    if article.is_read {
        return;
    }
    app.user_data.read_links.insert(article.link.clone());
    let _ = save_user_data(&app.user_data);
    mark_feed_by_link(app, &article.link);

    // In saved context, also update the in-memory saved-view list.
    if app.in_saved_context
        && let Some(a) = app.saved_view_articles.get_mut(app.selected_article)
    {
        a.is_read = true;
    }
}

/// Toggles the read/unread state of the currently selected article and persists the change.
fn toggle_read(app: &mut App) {
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
        app.user_data.read_links.insert(link);
    } else {
        app.user_data.read_links.remove(&link);
    }
    let _ = save_user_data(&app.user_data);
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

/// Proactively fetches full article content when the cursor lands on a stub-length article.
pub(super) fn prefetch_article_if_stub(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    let Some((source, art_idx, link)) = resolve_article_source(app) else {
        return;
    };
    let is_stub = match source {
        FeedSource::Feed(fi) => app
            .feeds
            .get(fi)
            .and_then(|f| f.articles.get(art_idx))
            .is_some_and(|a| a.content.len() < CONTENT_STUB_MAX_LEN && a.error.is_none()),
        FeedSource::Saved => app
            .saved_view_articles
            .get(art_idx)
            .is_some_and(|a| a.content.len() < CONTENT_STUB_MAX_LEN && a.error.is_none()),
    };
    if !is_stub {
        return;
    }
    app.article_fetching = true;
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let result = fetch_readable_content(&link).await;
        let _ = tx2.send(AppEvent::FullArticleFetched(source, art_idx, result));
    });
}

/// Spawns a background task to fetch readable content for the article if it is still a stub.
///
/// Sets a loading placeholder in the article content and flips `article_fetching` while the
/// request is in flight.  Does nothing when content is already at full length.
fn fetch_full_article_if_stub(app: &mut App, tx: &UnboundedSender<AppEvent>, article: &Article) {
    if article.content.len() >= CONTENT_STUB_MAX_LEN {
        return;
    }

    app.set_status("Fetching full article...".to_string());
    update_article_content(app, "⏳ Fetching full article, please wait...".to_string());

    let Some((source, art_idx, _link)) = resolve_article_source(app) else {
        return;
    };
    let url = article.link.clone();
    let tx = tx.clone();
    app.article_fetching = true;
    tokio::spawn(async move {
        let result = fetch_readable_content(&url).await;
        let _ = tx.send(AppEvent::FullArticleFetched(source, art_idx, result));
    });
}

/// Replaces the in-memory content field of the currently selected article.
///
/// Writes to the correct backing store depending on the active view context.
fn update_article_content(app: &mut App, content: String) {
    if app.in_category_context {
        if let Some(&(fi, ai)) = app.category_view_articles.get(app.selected_article)
            && let Some(feed) = app.feeds.get_mut(fi)
            && let Some(a) = feed.articles.get_mut(ai)
        {
            a.content = content;
        }
    } else if app.in_saved_context {
        if let Some(a) = app.saved_view_articles.get_mut(app.selected_article) {
            a.content = content;
        }
    } else if let Some(feed) = app.feeds.get_mut(app.selected_feed)
        && let Some(a) = feed.articles.get_mut(app.selected_article)
    {
        a.content = content;
    }
}
