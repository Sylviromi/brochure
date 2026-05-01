use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::App,
    fetch::{fetch_feed, fetch_readable_content},
    models::{AppEvent, AppState, Article, FeedSource, CONTENT_STUB_MAX_LEN},
    storage::save_user_data,
};

pub(super) async fn handle_article(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('r') if !app.in_favorites_context => {
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
                    .saturating_sub(app.content_area_height as usize) as u16;
                app.scroll_offset = (app.scroll_offset + 1).min(max);
            } else {
                app.next();
            }
        }
        KeyCode::Up => {
            if app.state == AppState::ArticleDetail {
                app.scroll_offset = app.scroll_offset.saturating_sub(1);
            } else {
                app.previous();
            }
        }
        KeyCode::Enter if app.state == AppState::ArticleList => {
            open_article(app, tx);
        }
        KeyCode::Char('m') => toggle_read(app),
        KeyCode::Char('s') => toggle_starred(app),
        KeyCode::Esc => app.unselect(),
        _ => {}
    }
    false
}

fn open_article(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    let article = get_selected_article(app);
    let Some(article) = article else { return };

    mark_article_as_read(app, &article);
    fetch_full_article_if_stub(app, tx, &article);

    app.select();
}

fn get_selected_article(app: &App) -> Option<Article> {
    if app.in_favorites_context {
        app.favorite_view_articles
            .get(app.selected_article)
            .cloned()
    } else {
        app.feeds
            .get(app.selected_feed)
            .and_then(|f| f.articles.get(app.selected_article))
            .cloned()
    }
}

fn mark_article_as_read(app: &mut App, article: &Article) {
    if article.is_read {
        return;
    }

    app.user_data.read_links.insert(article.link.clone());
    let _ = save_user_data(&app.user_data);

    if app.in_favorites_context {
        mark_favorite_as_read(app, article);
    } else {
        mark_regular_article_as_read(app);
    }
}

fn mark_favorite_as_read(app: &mut App, article: &Article) {
    if let Some(a) = app.favorite_view_articles.get_mut(app.selected_article) {
        a.is_read = true;
    }
    if let Some(feed) = app
        .feeds
        .iter_mut()
        .find(|f| f.title == article.source_feed)
    {
        if let Some(a) = feed.articles.iter_mut().find(|a| a.link == article.link) {
            a.is_read = true;
        }
        feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
    }
    // Keep starred_articles snapshot in sync.
    if let Some(s) = app.user_data.starred_articles.iter_mut().find(|s| s.link == article.link) {
        s.is_read = true;
    }
}

fn mark_regular_article_as_read(app: &mut App) {
    if let Some(feed) = app.feeds.get_mut(app.selected_feed) {
        if let Some(a) = feed.articles.get_mut(app.selected_article) {
            a.is_read = true;
            // Keep starred_articles snapshot in sync so Favorites reflects the read state.
            if let Some(s) = app.user_data.starred_articles.iter_mut().find(|s| s.link == a.link) {
                s.is_read = true;
            }
        }
        feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
    }
}


fn fetch_full_article_if_stub(app: &mut App, tx: &UnboundedSender<AppEvent>, article: &Article) {
    if article.content.len() >= CONTENT_STUB_MAX_LEN {
        return;
    }

    app.set_status("Fetching full article...".to_string());
    update_article_content(app, "⏳ Fetching full article, please wait...".to_string());

    let tx2 = tx.clone();
    let url = article.link.clone();
    let source = if app.in_favorites_context {
        FeedSource::Favorites
    } else {
        FeedSource::Feed(app.selected_feed)
    };
    let art_idx = app.selected_article;
    tokio::spawn(async move {
        let result = fetch_readable_content(&url).await;
        let _ = tx2.send(AppEvent::FullArticleFetched(source, art_idx, result));
    });
}

fn update_article_content(app: &mut App, content: String) {
    if app.in_favorites_context {
        if let Some(a) = app.favorite_view_articles.get_mut(app.selected_article) {
            a.content = content;
        }
    } else if let Some(feed) = app.feeds.get_mut(app.selected_feed)
        && let Some(a) = feed.articles.get_mut(app.selected_article)
    {
        a.content = content;
    }
}

fn toggle_read(app: &mut App) {
    let update = if app.in_favorites_context {
        toggle_read_favorite(app)
    } else {
        toggle_read_regular(app)
    };

    if let Some((link, is_now_read)) = update {
        if is_now_read {
            app.user_data.read_links.insert(link);
        } else {
            app.user_data.read_links.remove(&link);
        }
        let _ = save_user_data(&app.user_data);
    }
}

fn toggle_read_favorite(app: &mut App) -> Option<(String, bool)> {
    let art = app.favorite_view_articles.get_mut(app.selected_article)?;
    art.is_read = !art.is_read;
    let link = art.link.clone();
    let is_read = art.is_read;
    let source_feed = art.source_feed.clone();

    // Sync the source feed's copy of the article.
    if let Some(feed) = app.feeds.iter_mut().find(|f| f.title == source_feed) {
        if let Some(a) = feed.articles.iter_mut().find(|a| a.link == link) {
            a.is_read = is_read;
        }
        feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
    }

    // Sync the persistent starred_articles snapshot so re-entering Favorites reflects the change.
    if let Some(a) = app.user_data.starred_articles.iter_mut().find(|a| a.link == link) {
        a.is_read = is_read;
    }

    Some((link, is_read))
}

fn toggle_read_regular(app: &mut App) -> Option<(String, bool)> {
    if app.feeds.is_empty() || app.feeds[app.selected_feed].articles.is_empty() {
        return None;
    }
    let art = &mut app.feeds[app.selected_feed].articles[app.selected_article];
    art.is_read = !art.is_read;
    let link = art.link.clone();
    let is_now_read = art.is_read;

    app.feeds[app.selected_feed].unread_count = app.feeds[app.selected_feed]
        .articles
        .iter()
        .filter(|a| !a.is_read)
        .count();

    // Keep starred_articles snapshot in sync so Favorites reflects the toggle.
    if let Some(s) = app.user_data.starred_articles.iter_mut().find(|s| s.link == link) {
        s.is_read = is_now_read;
    }

    Some((link, is_now_read))
}

fn toggle_starred(app: &mut App) {
    let article_info = get_selected_article_info(app);
    let Some((link, source_feed)) = article_info else {
        return;
    };

    let new_starred = get_current_starred_state(app);
    update_starred_in_articles(app, new_starred);
    update_starred_in_user_data(app, &link, &source_feed, new_starred);

    let _ = save_user_data(&app.user_data);
    app.set_status(if new_starred {
        "Article starred! ⭐"
    } else {
        "Article un-starred."
    });
}

fn get_selected_article_info(app: &App) -> Option<(String, String)> {
    if app.in_favorites_context {
        let art = app.favorite_view_articles.get(app.selected_article)?;
        Some((art.link.clone(), art.source_feed.clone()))
    } else {
        if app.feeds.is_empty() || app.feeds[app.selected_feed].articles.is_empty() {
            return None;
        }
        let art = &app.feeds[app.selected_feed].articles[app.selected_article];
        Some((art.link.clone(), art.source_feed.clone()))
    }
}

fn get_current_starred_state(app: &App) -> bool {
    if app.in_favorites_context {
        app.favorite_view_articles
            .get(app.selected_article)
            .map(|a| !a.is_starred)
            .unwrap_or(false)
    } else {
        app.feeds
            .get(app.selected_feed)
            .and_then(|f| f.articles.get(app.selected_article))
            .map(|a| !a.is_starred)
            .unwrap_or(false)
    }
}

fn update_starred_in_articles(app: &mut App, new_starred: bool) {
    if app.in_favorites_context {
        if let Some(art) = app.favorite_view_articles.get_mut(app.selected_article) {
            art.is_starred = new_starred;
            let link = art.link.clone();
            let source_feed = art.source_feed.clone();
            if let Some(feed) = app.feeds.iter_mut().find(|f| f.title == source_feed)
                && let Some(source_art) = feed.articles.iter_mut().find(|a| a.link == link)
            {
                source_art.is_starred = new_starred;
            }
        }
    } else {
        if let Some(art) = app
            .feeds
            .get_mut(app.selected_feed)
            .and_then(|f| f.articles.get_mut(app.selected_article))
        {
            art.is_starred = new_starred;
        }
    }
}

fn update_starred_in_user_data(app: &mut App, link: &str, _source_feed: &str, new_starred: bool) {
    if new_starred {
        let art_clone = get_selected_article(app);
        if let Some(art) = art_clone
            && !app
                .user_data
                .starred_articles
                .iter()
                .any(|a| a.link == link)
        {
            app.user_data.starred_articles.push(art);
        }
    } else {
        app.user_data.starred_articles.retain(|a| a.link != link);
        if app.in_favorites_context {
            app.favorite_view_articles.retain(|a| a.link != link);
            if app.selected_article >= app.favorite_view_articles.len()
                && !app.favorite_view_articles.is_empty()
            {
                app.selected_article = app.favorite_view_articles.len() - 1;
            }
        }
    }
}
