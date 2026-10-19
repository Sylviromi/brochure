//! Tab-level layout routing: Feeds tab, Saved tab, and three-panel split.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use super::super::feed_editor::draw_feed_editor;
use super::article_detail::draw_article_detail;
use super::article_list::draw_article_list;
use super::category_view::draw_category_article_list;
use super::footer::{draw_article_footer, split_footer};
use super::saved_sidebar::draw_saved_sidebar;
use super::sidebar::draw_sidebar;
use crate::{app::App, models::AppState};

/// Renders a three-panel layout: article list (left) and article detail (right).
///
/// A single shared footer bar is carved from the bottom of `right_area` and rendered
/// after both panels, spanning the full combined width.
fn draw_three_panel(f: &mut Frame, app: &mut App, right_area: Rect, is_preview: bool) {
    // Carve shared footer from the bottom before splitting into panels.
    let (panels_area, footer_area) = split_footer(right_area);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(33), Constraint::Percentage(67)])
        .split(panels_area);

    draw_article_list(f, app, panels[0], false);
    draw_article_detail(f, app, panels[1], is_preview, false);

    // Shared footer: article info when viewing detail, feed stats otherwise.
    draw_article_footer(f, app, footer_area, !is_preview);
}

/// Renders the Feeds tab with sidebar (categories/feeds) and content area (list or detail).
pub fn draw_feeds_tab(f: &mut Frame, app: &mut App, area: Rect) {
    if matches!(app.state, AppState::FeedEditor | AppState::FeedEditorRename)
        || (app.state == AppState::AddFeed && app.add_feed.return_state == AppState::FeedEditor)
    {
        draw_feed_editor(f, app, area);
        return;
    }

    // Outer split: sidebar (25%) | right area (75%)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(area);

    draw_sidebar(f, app, cols[0]);

    match app.state {
        AppState::FeedList | AppState::AddFeed => {
            if app.selected_sidebar_category.is_some() {
                draw_category_article_list(f, app, cols[1]);
            } else {
                draw_article_list(f, app, cols[1], true);
            }
        }
        AppState::ArticleList => {
            draw_three_panel(f, app, cols[1], true);
        }
        AppState::ArticleDetail => {
            draw_three_panel(f, app, cols[1], false);
        }
        AppState::CategoryPicker => {
            let is_preview = app.category_picker.return_state != AppState::ArticleDetail;
            draw_three_panel(f, app, cols[1], is_preview);
        }
        _ => {}
    }
}

/// Renders the Saved tab with saved-categories sidebar and content area (list or detail).
pub fn draw_saved_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(area);

    draw_saved_sidebar(f, app, cols[0]);

    match app.state {
        AppState::SavedCategoryList => draw_article_list(f, app, cols[1], true),
        AppState::ArticleList => {
            draw_three_panel(f, app, cols[1], true);
        }
        AppState::ArticleDetail => {
            draw_three_panel(f, app, cols[1], false);
        }
        AppState::CategoryPicker => {
            let is_preview = app.category_picker.return_state != AppState::ArticleDetail;
            draw_three_panel(f, app, cols[1], is_preview);
        }
        _ => draw_article_list(f, app, cols[1], true),
    }
}
