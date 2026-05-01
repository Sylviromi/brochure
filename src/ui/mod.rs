mod chrome;
mod content;
mod editor;
mod popups;
mod settings;

use crate::app::App;
use crate::models::{AppState, FeedTreeItem, Tab};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::Color,
    symbols,
    Frame,
};

// ── Catppuccin Mocha palette ──────────────────────────────────────────────────
pub(crate) const MAUVE: Color = Color::Rgb(203, 166, 247);
pub(crate) const BLUE: Color = Color::Rgb(137, 180, 250);
pub(crate) const GREEN: Color = Color::Rgb(166, 227, 161);
pub(crate) const PEACH: Color = Color::Rgb(250, 179, 135);
pub(crate) const BASE: Color = Color::Rgb(30, 30, 46);
pub(crate) const MANTLE: Color = Color::Rgb(24, 24, 37);
pub(crate) const TEXT: Color = Color::Rgb(205, 214, 244);
pub(crate) const SUBTEXT0: Color = Color::Rgb(166, 173, 200);
pub(crate) const SURFACE0: Color = Color::Rgb(49, 50, 68);
pub(crate) const YELLOW: Color = Color::Rgb(249, 226, 175);
pub(crate) const TEAL: Color = Color::Rgb(148, 226, 213);
pub(crate) const SKY: Color = Color::Rgb(137, 220, 235);
pub(crate) const PINK: Color = Color::Rgb(245, 194, 231);
pub(crate) const RED: Color = Color::Rgb(243, 139, 168);

pub(crate) const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Returns the border set based on the user's rounded-border preference.
pub(crate) fn border_set(rounded: bool) -> symbols::border::Set<'static> {
    if rounded { symbols::border::ROUNDED } else { symbols::border::PLAIN }
}

/// Fixed palette for category headers (cycles by category id).
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
    }

    if app.state == AppState::AddFeed {
        popups::draw_add_feed_popup(f, app);
    }
    if matches!(app.state, AppState::OPMLExportPath | AppState::OPMLImportPath) {
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
