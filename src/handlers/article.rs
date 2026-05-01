use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::App,
    fetch::{fetch_feed, fetch_readable_content},
    models::{
        AppEvent, AppState, Article, CONTENT_STUB_MAX_LEN, FeedSource, SavedArticle, SavedCategory,
    },
    storage::save_user_data,
};

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
                if app.state == AppState::ArticleList {
                    prefetch_article_if_stub(app, tx);
                }
            }
        }
        KeyCode::Up => {
            if app.state == AppState::ArticleDetail {
                if let Some(article) = get_selected_article(app) {
                    app.article_scroll.scroll_up(&article.link);
                }
            } else {
                app.previous();
                if app.state == AppState::ArticleList {
                    prefetch_article_if_stub(app, tx);
                }
            }
        }
        KeyCode::Enter if app.state == AppState::ArticleList => {
            open_article(app, tx);
        }
        KeyCode::Char('m') => toggle_read(app),
        KeyCode::Char('s') => open_category_picker(app),
        KeyCode::Char('o') => {
            if let Some(article) = get_selected_article(app) {
                let _ = open::that(&article.link);
            }
        }
        KeyCode::Esc => app.unselect(),
        _ => {}
    }
    false
}

pub(super) fn handle_category_picker(app: &mut App, key: KeyEvent) {
    let cats_len = app.user_data.saved_categories.len();
    let article_is_saved = get_selected_article(app).is_some_and(|art| {
        app.user_data
            .saved_articles
            .iter()
            .any(|s| s.article.link == art.link)
    });
    // Layout: [0..cats_len) = existing categories, cats_len = "New category...", cats_len+1 = "Unsave" (only if saved)
    let total_items = if article_is_saved {
        cats_len + 2
    } else {
        cats_len + 1
    };

    if app.category_picker_new_mode {
        match key.code {
            KeyCode::Enter => {
                let name = app.category_picker_input.trim().to_string();
                if !name.is_empty() {
                    // Reuse existing category if same name already exists.
                    let target_id = app
                        .user_data
                        .saved_categories
                        .iter()
                        .find(|c| c.name.eq_ignore_ascii_case(&name))
                        .map(|c| c.id)
                        .unwrap_or_else(|| {
                            let new_id = app
                                .user_data
                                .saved_categories
                                .iter()
                                .map(|c| c.id)
                                .max()
                                .unwrap_or(0)
                                + 1;
                            app.user_data.saved_categories.push(SavedCategory {
                                id: new_id,
                                name: name.clone(),
                            });
                            new_id
                        });
                    save_to_category(app, target_id);
                    app.set_status(format!("Saved to '{name}'!"));
                }
                app.category_picker_new_mode = false;
                app.category_picker_input.clear();
                app.state = app.category_picker_return_state.clone();
            }
            KeyCode::Char(c) => app.category_picker_input.push(c),
            KeyCode::Backspace => {
                app.category_picker_input.pop();
            }
            KeyCode::Esc => {
                app.category_picker_new_mode = false;
                app.category_picker_input.clear();
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Up => {
            app.category_picker_cursor = app
                .category_picker_cursor
                .checked_sub(1)
                .unwrap_or(total_items - 1);
        }
        KeyCode::Down => {
            app.category_picker_cursor = (app.category_picker_cursor + 1) % total_items;
        }
        KeyCode::Enter => {
            if app.category_picker_cursor < cats_len {
                // Save to existing category
                let cat_id = app.user_data.saved_categories[app.category_picker_cursor].id;
                let cat_name = app.user_data.saved_categories[app.category_picker_cursor]
                    .name
                    .clone();
                save_to_category(app, cat_id);
                app.set_status(format!("Saved to '{cat_name}'!"));
                app.state = app.category_picker_return_state.clone();
            } else if app.category_picker_cursor == cats_len {
                // "New category..." — enter text input mode
                app.category_picker_new_mode = true;
                app.category_picker_input.clear();
            } else if article_is_saved {
                // "Unsave"
                unsave_article(app);
                app.state = app.category_picker_return_state.clone();
            }
        }
        KeyCode::Esc => {
            app.state = app.category_picker_return_state.clone();
        }
        _ => {}
    }
}

fn open_category_picker(app: &mut App) {
    let article = match get_selected_article(app) {
        Some(a) => a,
        None => return,
    };

    // Pre-select current category if article is already saved.
    let current_cat_idx = app
        .user_data
        .saved_articles
        .iter()
        .find(|s| s.article.link == article.link)
        .and_then(|s| {
            app.user_data
                .saved_categories
                .iter()
                .position(|c| c.id == s.category_id)
        });

    app.category_picker_cursor = current_cat_idx.unwrap_or(0);
    app.category_picker_new_mode = false;
    app.category_picker_input.clear();
    app.category_picker_return_state = app.state.clone();
    app.state = AppState::CategoryPicker;
}

fn save_to_category(app: &mut App, category_id: u32) {
    let article = match get_selected_article(app) {
        Some(a) => a,
        None => return,
    };

    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == article.link)
    {
        s.category_id = category_id;
    } else {
        app.user_data.saved_articles.push(SavedArticle {
            article: article.clone(),
            category_id,
        });
    }

    update_is_saved_flag(app, true);
    let _ = save_user_data(&app.user_data);
    if app.in_saved_context {
        app.sync_saved_preview();
    }
}

fn unsave_article(app: &mut App) {
    let article = match get_selected_article(app) {
        Some(a) => a,
        None => return,
    };

    app.user_data
        .saved_articles
        .retain(|s| s.article.link != article.link);
    update_is_saved_flag(app, false);

    if app.in_saved_context {
        app.saved_view_articles.retain(|a| a.link != article.link);
        if app.selected_article >= app.saved_view_articles.len()
            && !app.saved_view_articles.is_empty()
        {
            app.selected_article = app.saved_view_articles.len() - 1;
        }
    }

    app.set_status("Article unsaved.");
    let _ = save_user_data(&app.user_data);
}

fn update_is_saved_flag(app: &mut App, is_saved: bool) {
    if app.in_category_context {
        if let Some(&(fi, ai)) = app.category_view_articles.get(app.selected_article)
            && let Some(art) = app.feeds.get_mut(fi).and_then(|f| f.articles.get_mut(ai))
        {
            art.is_saved = is_saved;
        }
    } else if app.in_saved_context {
        if let Some(art) = app.saved_view_articles.get_mut(app.selected_article) {
            art.is_saved = is_saved;
            let link = art.link.clone();
            let source_feed = art.source_feed.clone();
            if let Some(feed) = app.feeds.iter_mut().find(|f| f.title == source_feed)
                && let Some(src) = feed.articles.iter_mut().find(|a| a.link == link)
            {
                src.is_saved = is_saved;
            }
        }
    } else if let Some(art) = app
        .feeds
        .get_mut(app.selected_feed)
        .and_then(|f| f.articles.get_mut(app.selected_article))
    {
        art.is_saved = is_saved;
    }
}

fn prefetch_article_if_stub(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    if app.in_saved_context {
        return;
    }
    let (feed_idx, art_idx) = if app.in_category_context {
        match app
            .category_view_articles
            .get(app.selected_article)
            .copied()
        {
            Some(pair) => pair,
            None => return,
        }
    } else {
        (app.selected_feed, app.selected_article)
    };

    let article = match app
        .feeds
        .get(feed_idx)
        .and_then(|f| f.articles.get(art_idx))
    {
        Some(a) => a,
        None => return,
    };
    if article.content.len() >= CONTENT_STUB_MAX_LEN {
        return;
    }
    let url = article.link.clone();
    app.article_fetching = true;
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let result = fetch_readable_content(&url).await;
        let _ = tx2.send(AppEvent::FullArticleFetched(
            FeedSource::Feed(feed_idx),
            art_idx,
            result,
        ));
    });
}

fn open_article(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    let article = get_selected_article(app);
    let Some(article) = article else { return };
    mark_article_as_read(app, &article);
    fetch_full_article_if_stub(app, tx, &article);
    app.select();
}

pub(super) fn get_selected_article(app: &App) -> Option<Article> {
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

fn mark_article_as_read(app: &mut App, article: &Article) {
    if article.is_read {
        return;
    }
    app.user_data.read_links.insert(article.link.clone());
    let _ = save_user_data(&app.user_data);

    if app.in_category_context {
        mark_category_article_as_read(app, article);
    } else if app.in_saved_context {
        mark_saved_as_read(app, article);
    } else {
        mark_regular_article_as_read(app);
    }
}

fn mark_category_article_as_read(app: &mut App, article: &Article) {
    // Find and update the article in its source feed by link.
    for feed in app.feeds.iter_mut() {
        if let Some(a) = feed.articles.iter_mut().find(|a| a.link == article.link) {
            a.is_read = true;
        }
        feed.unread_count = feed.articles.iter().filter(|a| !a.is_read).count();
    }
    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == article.link)
    {
        s.article.is_read = true;
    }
}

fn mark_saved_as_read(app: &mut App, article: &Article) {
    if let Some(a) = app.saved_view_articles.get_mut(app.selected_article) {
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
    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == article.link)
    {
        s.article.is_read = true;
    }
}

fn mark_regular_article_as_read(app: &mut App) {
    if let Some(feed) = app.feeds.get_mut(app.selected_feed) {
        if let Some(a) = feed.articles.get_mut(app.selected_article) {
            a.is_read = true;
            if let Some(s) = app
                .user_data
                .saved_articles
                .iter_mut()
                .find(|s| s.article.link == a.link)
            {
                s.article.is_read = true;
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
    let source = if app.in_category_context {
        let (fi, _ai) = app
            .category_view_articles
            .get(app.selected_article)
            .copied()
            .unwrap_or((app.selected_feed, app.selected_article));
        FeedSource::Feed(fi)
    } else if app.in_saved_context {
        FeedSource::Saved
    } else {
        FeedSource::Feed(app.selected_feed)
    };
    let art_idx = if app.in_category_context {
        app.category_view_articles
            .get(app.selected_article)
            .map(|&(_, ai)| ai)
            .unwrap_or(app.selected_article)
    } else {
        app.selected_article
    };
    app.article_fetching = true;
    tokio::spawn(async move {
        let result = fetch_readable_content(&url).await;
        let _ = tx2.send(AppEvent::FullArticleFetched(source, art_idx, result));
    });
}

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

fn toggle_read(app: &mut App) {
    let update = if app.in_category_context {
        toggle_read_category(app)
    } else if app.in_saved_context {
        toggle_read_saved(app)
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

fn toggle_read_category(app: &mut App) -> Option<(String, bool)> {
    let &(fi, ai) = app.category_view_articles.get(app.selected_article)?;
    let art = app.feeds.get_mut(fi)?.articles.get_mut(ai)?;
    art.is_read = !art.is_read;
    let link = art.link.clone();
    let is_read = art.is_read;
    app.feeds[fi].unread_count = app.feeds[fi].articles.iter().filter(|a| !a.is_read).count();
    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == link)
    {
        s.article.is_read = is_read;
    }
    Some((link, is_read))
}

fn toggle_read_saved(app: &mut App) -> Option<(String, bool)> {
    let art = app.saved_view_articles.get_mut(app.selected_article)?;
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

    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == link)
    {
        s.article.is_read = is_read;
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

    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == link)
    {
        s.article.is_read = is_now_read;
    }

    Some((link, is_now_read))
}
