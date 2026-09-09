//! Display helpers for Feed and Article types (rendering symbols and formatting).

use super::{Article, CategoryId, Feed, FeedKind};
use ratatui::prelude::Style;
use ratatui::style::Color;

impl Feed {
    /// Returns `" [N]"` if there are unread articles, empty string otherwise.
    pub fn unread_badge(&self) -> String {
        if self.unread_count > 0 {
            format!(" [{}]", self.unread_count)
        } else {
            String::new()
        }
    }

    /// Build a plain local RSS/Atom feed.
    pub fn new_local(
        title: String,
        url: String,
        category_id: Option<CategoryId>,
        order: usize,
    ) -> Self {
        Self {
            title,
            url,
            category_id,
            order,
            kind: FeedKind::Local,
            remote_id: None,
            unread_count: 0,
            articles: vec![],
            fetched: false,
            fetch_error: None,
            feed_updated_secs: None,
            last_fetched_secs: None,
        }
    }

    /// Build a feed subscribed via FreshRSS (`remote_id` is the `feed/N` stream id).
    pub fn new_freshrss(
        title: String,
        url: String,
        remote_id: String,
        category_id: Option<CategoryId>,
        order: usize,
    ) -> Self {
        Self {
            title,
            url,
            category_id,
            order,
            kind: FeedKind::FreshRss,
            remote_id: Some(remote_id),
            unread_count: 0,
            articles: vec![],
            fetched: false,
            fetch_error: None,
            feed_updated_secs: None,
            last_fetched_secs: None,
        }
    }

    /// Whether this feed syncs via FreshRSS rather than direct fetch.
    pub fn is_freshrss(&self) -> bool {
        self.kind == FeedKind::FreshRss
    }
}

impl Article {
    /// Returns a symbol indicating the article's read/saved state: filled/empty circle/square.
    pub fn get_icon(&self) -> &'static str {
        if self.is_saved {
            if self.is_read { "□ " } else { "■ " }
        } else {
            if self.is_read { "○ " } else { "● " }
        }
    }

    /// Returns the color for the article's state icon given theme colors.
    ///
    /// Takes the three relevant theme colors: `saved_color` (e.g. yellow),
    /// `read_color` (e.g. subtext0), and `unread_color` (e.g. blue).
    pub fn get_icon_style(
        &self,
        saved_color: Color,
        read_color: Color,
        unread_color: Color,
    ) -> Style {
        if self.is_saved {
            Style::default().fg(saved_color)
        } else if self.is_read {
            Style::default().fg(read_color)
        } else {
            Style::default().fg(unread_color)
        }
    }
}
