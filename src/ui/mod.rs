//! Terminal UI rendering: color constants, tree utilities, and top-level draw dispatcher.
//!
//! This module owns all rendering logic, including Catppuccin Mocha color constants,
//! tree indentation helpers, and the main `draw()` function that dispatches to per-tab renderers.

mod changelog;
mod chrome;
mod content;
mod editor;
mod popups;
mod settings;

use crate::app::App;
use crate::models::{AppState, FeedTreeItem, Tab};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::Color,
    symbols,
};

// ── Catppuccin Mocha palette ──────────────────────────────────────────────────
/// Catppuccin Mocha mauve color.
pub(crate) const MAUVE: Color = Color::Rgb(203, 166, 247);
/// Catppuccin Mocha blue color.
pub(crate) const BLUE: Color = Color::Rgb(137, 180, 250);
/// Catppuccin Mocha green color.
pub(crate) const GREEN: Color = Color::Rgb(166, 227, 161);
/// Catppuccin Mocha peach color.
pub(crate) const PEACH: Color = Color::Rgb(250, 179, 135);
/// Catppuccin Mocha base (dark background) color.
pub(crate) const BASE: Color = Color::Rgb(30, 30, 46);
/// Catppuccin Mocha mantle (darkest background) color.
pub(crate) const MANTLE: Color = Color::Rgb(24, 24, 37);
/// Catppuccin Mocha text (foreground) color.
pub(crate) const TEXT: Color = Color::Rgb(205, 214, 244);
/// Catppuccin Mocha subtext0 (muted text) color.
pub(crate) const SUBTEXT0: Color = Color::Rgb(166, 173, 200);
/// Catppuccin Mocha surface0 (light background) color.
pub(crate) const SURFACE0: Color = Color::Rgb(49, 50, 68);
/// Catppuccin Mocha yellow color.
pub(crate) const YELLOW: Color = Color::Rgb(249, 226, 175);
/// Catppuccin Mocha teal color.
pub(crate) const TEAL: Color = Color::Rgb(148, 226, 213);
/// Catppuccin Mocha sky color.
pub(crate) const SKY: Color = Color::Rgb(137, 220, 235);
/// Catppuccin Mocha pink color.
pub(crate) const PINK: Color = Color::Rgb(245, 194, 231);
/// Catppuccin Mocha red color.
pub(crate) const RED: Color = Color::Rgb(243, 139, 168);

/// Braille spinner animation frames for loading indicators.
pub(crate) const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Returns the border set based on the user's rounded-border preference.
pub(crate) fn border_set(rounded: bool) -> symbols::border::Set<'static> {
    if rounded {
        symbols::border::ROUNDED
    } else {
        symbols::border::PLAIN
    }
}

/// Fixed color palette that cycles through category IDs to assign unique colors to categories.
pub(crate) const CATEGORY_COLORS: &[Color] = &[MAUVE, BLUE, GREEN, PEACH, YELLOW, TEAL, SKY, PINK];

/// Compute the leading indent string for a tree item at `depth` positioned at `render_idx`.
/// For each ancestor level (1 to depth-1), emits "│  " if that level still has siblings
/// after the current item, or "   " if it was the last child.
pub(crate) fn tree_indent(tree: &[FeedTreeItem], render_idx: usize, depth: u8) -> String {
    if depth <= 1 {
        return String::new();
    }
    let mut s = String::new();
    for level in 1..depth {
        let next_at_level = tree[render_idx + 1..]
            .iter()
            .find(|n| {
                let d = match n {
                    FeedTreeItem::Feed { depth, .. } | FeedTreeItem::Category { depth, .. } => {
                        *depth
                    }
                };
                d <= level
            })
            .map(|n| match n {
                FeedTreeItem::Feed { depth, .. } | FeedTreeItem::Category { depth, .. } => *depth,
            });
        if next_at_level == Some(level) {
            s.push_str("│  ");
        } else {
            s.push_str("   ");
        }
    }
    s
}

/// Compute the tree connector prefix (`├─ `, `╰─ `/`└─ `, or `root_str`) for an item.
/// `root_str` is returned at depth 0 (e.g., `""` for categories, `"   "` for feeds).
pub(crate) fn tree_connector(
    tree: &[FeedTreeItem],
    idx: usize,
    depth: u8,
    rounded: bool,
    root_str: &'static str,
) -> &'static str {
    if depth == 0 {
        return root_str;
    }
    let is_last = tree[idx + 1..]
        .iter()
        .find(|n| {
            let d = match n {
                FeedTreeItem::Feed { depth, .. } | FeedTreeItem::Category { depth, .. } => *depth,
            };
            d <= depth
        })
        .is_none_or(|n| {
            let d = match n {
                FeedTreeItem::Feed { depth, .. } | FeedTreeItem::Category { depth, .. } => *depth,
            };
            d < depth
        });
    if is_last {
        if rounded { "╰─ " } else { "└─ " }
    } else {
        "├─ "
    }
}

/// Top-level draw dispatcher that renders the entire UI frame.
///
/// Dispatches to per-tab renderers (Feeds, Saved, Settings) and overlays state-specific popups
/// (add-feed wizard, OPML paths, confirm dialogs, category picker, saved category editor).
pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    chrome::draw_tab_bar(f, app, chunks[0]);
    chrome::draw_footer(f, app, chunks[2]);

    if matches!(
        app.state,
        AppState::SavedCategoryEditor
            | AppState::SavedCategoryEditorRename
            | AppState::SavedCategoryEditorDeleteConfirm
            | AppState::SavedCategoryEditorNew
    ) {
        settings::draw_saved_category_editor(f, app, chunks[1]);
        // For delete confirm, also overlay the confirmation popup.
        if app.state == AppState::SavedCategoryEditorDeleteConfirm {
            popups::draw_confirm_delete_saved_cat(f, app);
        }
        return;
    }

    match app.selected_tab {
        Tab::Feeds => content::draw_feeds_tab(f, app, chunks[1]),
        Tab::Saved => content::draw_saved_tab(f, app, chunks[1]),
        Tab::Settings => settings::draw_settings_tab(f, app, chunks[1]),
        Tab::Changelog => changelog::draw_changelog_tab(f, app, chunks[1]),
    }

    if app.state == AppState::AddFeed {
        popups::draw_add_feed_popup(f, app);
    }
    if matches!(
        app.state,
        AppState::OPMLExportPath | AppState::OPMLImportPath
    ) {
        popups::draw_opml_path_popup(f, app);
    }
    if app.state == AppState::ClearData {
        popups::draw_confirm_delete_all(f, app);
    }
    if app.state == AppState::ClearArticleCache {
        popups::draw_confirm_clear_cache(f, app);
    }
    if let Some((cat_id, feed_count)) = app.editor_delete_cat {
        popups::draw_confirm_delete_cat(f, app, cat_id, feed_count);
    }
    if app.state == AppState::CategoryPicker {
        popups::draw_category_picker(f, app);
    }
}
