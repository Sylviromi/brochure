//! Full-screen feed editor view for organizing feeds and categories.
//!
//! This module renders the feed editor with a two-panel layout: left panel shows feeds/categories
//! with drag-to-move support, right panel shows categories with add/rename/delete controls.

use crate::{
    app::{App, visible_cat_only_items, visible_tree_items},
    models::{AppState, EditorPanel, FeedEditorMode, FeedTreeItem},
    ui::content::utils::split_cursor,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::{border_set, tree_connector, tree_indent};

/// Returns the accent color for the editor mode label — yellow during moves, green otherwise.
fn editor_mode_color(
    mode: &FeedEditorMode,
    theme: &crate::ui::theme::ColorTheme,
) -> ratatui::style::Color {
    match mode {
        FeedEditorMode::Moving { .. } => theme.warning,
        _ => theme.success,
    }
}

/// Renders the full-screen feed editor with feeds and categories panels.
pub(super) fn draw_feed_editor(f: &mut Frame, app: &mut App, area: Rect) {
    // Background-only outer block (no borders)
    let bg_block = Block::default().bg(app.theme.bg);
    f.render_widget(bg_block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);

    draw_editor_feeds(f, app, cols[0]);
    draw_editor_categories(f, app, cols[1]);
}

/// Renders the left editor panel showing feeds with tree indentation and move/rename/delete modes.
fn draw_editor_feeds(f: &mut Frame, app: &mut App, area: Rect) {
    let is_active = app.feed_editor.panel == EditorPanel::Feeds;
    let in_moving_mode = matches!(app.feed_editor.mode, FeedEditorMode::Moving { .. });
    let is_rename = app.state == AppState::FeedEditorRename;

    let moving_origin = match &app.feed_editor.mode {
        FeedEditorMode::Moving {
            origin_render_idx, ..
        } => Some(*origin_render_idx),
        _ => None,
    };

    let mode_label = if is_active {
        match &app.feed_editor.mode {
            FeedEditorMode::Normal => "",
            FeedEditorMode::Moving { .. } => " MOVE — j/k navigate, Space drop, Esc cancel ",
            FeedEditorMode::Renaming { .. } => " RENAME ",
            FeedEditorMode::EditingUrl { .. } => " EDIT URL ",
            _ => "",
        }
    } else {
        ""
    };
    let mode_color = editor_mode_color(&app.feed_editor.mode, &app.theme);

    let border_color = if is_active {
        app.theme.accent
    } else {
        app.theme.border
    };
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .bg(app.theme.bg)
        .title(Line::from(vec![
            " Feeds ".fg(app.theme.link).bold(),
            mode_label.fg(mode_color).bold(),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);
    let list_area = vert[0];
    let url_area = vert[1];

    let tree = visible_tree_items(&app.categories, &app.feeds, &app.feed_editor.collapsed);
    let rounded = app.user_data.border_rounded;

    // full_idx_to_visual maps full-tree index → visual list index (for scroll tracking).
    let mut full_idx_to_visual: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    let mut visual_idx = 0usize;
    let mut has_any_feed = false;
    let mut items: Vec<ListItem> = Vec::new();

    for (full_idx, item) in tree.iter().enumerate() {
        match item {
            FeedTreeItem::AllFeeds => {
                // AllFeeds never appears in the editor tree; skip gracefully.
            }
            FeedTreeItem::Category {
                id,
                depth,
                collapsed,
            } => {
                let cat_name = app
                    .categories
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                let cat_colors = app.theme.category_colors();
                let color = cat_colors[(*id % cat_colors.len() as u64) as usize];
                let indent = tree_indent(&tree, full_idx, *depth);
                let connector = tree_connector(&tree, full_idx, *depth, rounded, "");
                let icon = if *collapsed { "▶" } else { "▼" };

                // Category headers are always non-interactive; no cursor highlight.
                // During a feed move the drop-preview arrow (inserted after) shows the target.
                full_idx_to_visual.insert(full_idx, visual_idx);
                items.push(ListItem::new(Line::from(vec![
                    indent.fg(app.theme.border),
                    connector.fg(app.theme.border),
                    format!("{cat_name} {icon}").fg(color).bold(),
                ])));
                visual_idx += 1;
            }
            FeedTreeItem::Feed { feeds_idx, depth } => {
                has_any_feed = true;
                let feed = &app.feeds[*feeds_idx];
                let indent = tree_indent(&tree, full_idx, *depth);
                let connector = tree_connector(&tree, full_idx, *depth, rounded, "");
                let selected = app.feed_editor.cursor == full_idx;
                let is_ghost = moving_origin == Some(full_idx);
                let is_on_origin = in_moving_mode && selected && is_ghost;
                let show_selected = selected && !in_moving_mode && is_active;

                let connector_style = if show_selected {
                    Style::default()
                        .fg(app.theme.accent)
                        .bg(app.theme.selection)
                } else {
                    Style::default().fg(app.theme.border)
                };

                full_idx_to_visual.insert(full_idx, visual_idx);
                visual_idx += 1;

                // Inline rename/URL-edit input
                if is_rename
                    && selected
                    && matches!(
                        app.feed_editor.mode,
                        FeedEditorMode::Renaming { .. } | FeedEditorMode::EditingUrl { .. }
                    )
                {
                    let (before, cursor_ch, after) =
                        split_cursor(&app.feed_editor.input.text, app.feed_editor.input.cursor);
                    items.push(ListItem::new(Line::from(vec![
                        indent.fg(app.theme.border),
                        Span::styled(connector, connector_style),
                        "  ✎ ".fg(app.theme.success),
                        before.fg(app.theme.text),
                        Span::styled(
                            cursor_ch,
                            Style::default().fg(app.theme.bg).bg(app.theme.success),
                        ),
                        after.fg(app.theme.text),
                    ])));
                    continue;
                }

                let style = if is_on_origin {
                    Style::default().fg(app.theme.muted).bg(app.theme.selection)
                } else if is_ghost {
                    Style::default()
                        .fg(app.theme.muted)
                        .add_modifier(Modifier::DIM)
                } else if show_selected {
                    Style::default()
                        .fg(app.theme.accent)
                        .bg(app.theme.selection)
                        .bold()
                } else {
                    Style::default().fg(app.theme.text)
                };
                let origin_hint = if is_on_origin { " ↩" } else { "" };
                let drop_marker = if show_selected {
                    "➤ ".fg(app.theme.warning).bold()
                } else {
                    Span::raw("")
                };

                items.push(ListItem::new(Line::from(vec![
                    indent.fg(app.theme.border),
                    Span::styled(connector, connector_style),
                    drop_marker,
                    Span::styled(format!("{}{origin_hint}", feed.title), style),
                    feed.unread_badge().fg(app.theme.warning),
                ])));
            }
        }
    }

    if !has_any_feed {
        f.render_widget(
            Paragraph::new(" No feeds. Press [a] to add one.")
                .style(Style::default().fg(app.theme.muted)),
            list_area,
        );
        return;
    }

    let mut final_items = items;
    let mut display_visual = full_idx_to_visual
        .get(&app.feed_editor.cursor)
        .copied()
        .unwrap_or(0);

    // In Moving mode: insert drop-preview row after the cursor position.
    if let Some(origin) = moving_origin {
        let cursor = app.feed_editor.cursor;
        if let Some(&origin_vis) = full_idx_to_visual.get(&origin)
            && let Some(&cursor_vis) = full_idx_to_visual.get(&cursor)
            && cursor_vis != origin_vis
        {
            let preview = match tree.get(origin) {
                Some(FeedTreeItem::Feed { feeds_idx, depth }) => {
                    let f = &app.feeds[*feeds_idx];
                    let indent = tree_indent(&tree, origin, *depth);
                    let connector = tree_connector(&tree, origin, *depth, rounded, "   ");
                    Some(ListItem::new(Line::from(vec![
                        indent.fg(app.theme.border),
                        connector.fg(app.theme.warning).bold(),
                        "➤ ".fg(app.theme.warning).bold(),
                        f.title.clone().fg(app.theme.warning).bold(),
                        f.unread_badge().fg(app.theme.warning),
                    ])))
                }
                _ => None,
            };
            if let Some(preview) = preview {
                let insert_at = (cursor_vis + 1).min(final_items.len());
                final_items.insert(insert_at, preview);
                display_visual = insert_at;
            }
        }
    }

    // Show selection when active, or during feed move (keeps ghost/preview visible when tabbed away).
    let is_feed_moving = in_moving_mode
        && matches!(
            moving_origin.and_then(|o| tree.get(o)),
            Some(FeedTreeItem::Feed { .. })
        );
    if is_active || is_feed_moving {
        app.editor_feeds_list_state.select(Some(display_visual));
    } else {
        app.editor_feeds_list_state.select(None);
    }
    f.render_stateful_widget(
        List::new(final_items),
        list_area,
        &mut app.editor_feeds_list_state,
    );

    // URL footer: show the selected feed's URL when on a Feed item.
    let url_text = if is_active {
        tree.get(app.feed_editor.cursor)
            .and_then(|item| match item {
                FeedTreeItem::Feed { feeds_idx, .. } => {
                    app.feeds.get(*feeds_idx).map(|f| f.url.clone())
                }
                _ => None,
            })
    } else {
        None
    };
    if let Some(url) = url_text {
        let max_chars = url_area.width as usize;
        let truncated: String = url.chars().take(max_chars.saturating_sub(1)).collect();
        f.render_widget(
            Paragraph::new(format!(" {truncated}")).style(Style::default().fg(app.theme.muted)),
            url_area,
        );
    }
}

/// Render the right panel: categories-only tree with add/rename/delete controls.
fn draw_editor_categories(f: &mut Frame, app: &mut App, area: Rect) {
    let is_active = app.feed_editor.panel == EditorPanel::Categories;
    let is_rename = app.state == AppState::FeedEditorRename;
    let in_moving_mode = matches!(app.feed_editor.mode, FeedEditorMode::Moving { .. });

    // For cat-move ghost: find the category ID being moved.
    let moving_cat_id: Option<u64> = if in_moving_mode {
        if let FeedEditorMode::Moving {
            origin_render_idx, ..
        } = &app.feed_editor.mode
        {
            let items = visible_tree_items(&app.categories, &app.feeds, &app.feed_editor.collapsed);
            match items.get(*origin_render_idx) {
                Some(FeedTreeItem::Category { id, .. }) => Some(*id),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    let mode_label = if is_active {
        match &app.feed_editor.mode {
            FeedEditorMode::Normal => "",
            FeedEditorMode::Moving { .. } => {
                " MOVE — j/k navigate, ◀▶ depth, Space drop, Esc cancel "
            }
            FeedEditorMode::Renaming { .. } => " RENAME ",
            FeedEditorMode::NewCategory { .. } => " NEW CATEGORY ",
            FeedEditorMode::EditingUrl { .. } => " EDIT URL ",
        }
    } else {
        ""
    };
    let mode_color = editor_mode_color(&app.feed_editor.mode, &app.theme);

    let border_color = if is_active {
        app.theme.accent
    } else {
        app.theme.border
    };
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .bg(app.theme.bg)
        .title(Line::from(vec![
            " Categories ".fg(app.theme.link).bold(),
            mode_label.fg(mode_color).bold(),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cats = visible_cat_only_items(&app.categories, &app.feeds, &app.feed_editor.collapsed);
    let rounded = app.user_data.border_rounded;

    if cats.is_empty() {
        f.render_widget(
            Paragraph::new(" No categories. [n] Create one.")
                .style(Style::default().fg(app.theme.muted)),
            inner,
        );
        return;
    }

    // Find rename target by matching full-tree index to category ID.
    let renamed_cat_id: Option<u64> = if is_rename {
        if let FeedEditorMode::Renaming { render_idx } = &app.feed_editor.mode {
            let full = visible_tree_items(&app.categories, &app.feeds, &app.feed_editor.collapsed);
            match full.get(*render_idx) {
                Some(FeedTreeItem::Category { id, .. }) => Some(*id),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    let in_new_cat_mode = matches!(app.feed_editor.mode, FeedEditorMode::NewCategory { .. });

    let items: Vec<ListItem> = cats
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let FeedTreeItem::Category {
                id,
                depth,
                collapsed,
            } = item
            else {
                return ListItem::new("");
            };
            let selected = is_active
                && app.feed_editor.cat_cursor == idx
                && !in_new_cat_mode
                && !in_moving_mode;
            let is_ghost = moving_cat_id == Some(*id);
            let cat_colors = app.theme.category_colors();
            let color = cat_colors[(*id % cat_colors.len() as u64) as usize];
            let indent = tree_indent(&cats, idx, *depth);
            let connector = tree_connector(&cats, idx, *depth, rounded, "");
            let icon = if *collapsed { "▶" } else { "▼" };

            // Rename input row
            if renamed_cat_id == Some(*id) {
                let (before, cursor_ch, after) =
                    split_cursor(&app.feed_editor.input.text, app.feed_editor.input.cursor);
                return ListItem::new(Line::from(vec![
                    indent.fg(app.theme.border),
                    connector.fg(app.theme.border),
                    "  ✎ ".fg(color),
                    before.fg(app.theme.text),
                    cursor_ch.fg(app.theme.bg).bg(color),
                    after.fg(app.theme.text),
                ]));
            }

            let direct = app
                .feeds
                .iter()
                .filter(|f| f.category_id == Some(*id))
                .count();
            let badge = if direct > 0 {
                format!(" [{direct}]")
            } else {
                String::new()
            };
            let cat_name = app
                .categories
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.name.as_str())
                .unwrap_or("?");

            let style = if is_ghost {
                Style::default()
                    .fg(app.theme.muted)
                    .add_modifier(Modifier::DIM)
            } else if selected {
                Style::default()
                    .fg(app.theme.bg_dark)
                    .bg(color)
                    .bold()
                    .bold()
            } else {
                Style::default().fg(color).bold()
            };
            let connector_style = if selected && !is_ghost {
                Style::default().fg(app.theme.bg_dark).bg(color)
            } else {
                Style::default().fg(app.theme.border)
            };
            let badge_style = if selected && !is_ghost {
                Style::default().fg(app.theme.bg_dark).bg(color)
            } else {
                Style::default().fg(app.theme.muted)
            };

            ListItem::new(Line::from(vec![
                indent.fg(app.theme.border),
                Span::styled(connector, connector_style),
                Span::styled(format!("{cat_name} {icon}"), style),
                Span::styled(badge, badge_style),
            ]))
        })
        .collect();

    // Insert new-category input row.
    let mut final_items = items;
    if is_rename && let FeedEditorMode::NewCategory { parent_id } = &app.feed_editor.mode {
        let parent_id = *parent_id;
        let depth = if let Some(pid) = parent_id {
            cats.iter()
                .find_map(|item| match item {
                    FeedTreeItem::Category { id, depth, .. } if *id == pid => Some(depth + 1),
                    _ => None,
                })
                .unwrap_or(1)
        } else {
            0
        };
        let indent = "  ".repeat(depth as usize);
        let insert_at = app.feed_editor.cat_cursor.min(final_items.len());
        let (before, cursor_ch, after) =
            split_cursor(&app.feed_editor.input.text, app.feed_editor.input.cursor);
        final_items.insert(
            insert_at,
            ListItem::new(Line::from(vec![
                indent.fg(app.theme.border),
                "  ✎ ".fg(app.theme.success),
                before.fg(app.theme.text),
                Span::styled(
                    cursor_ch,
                    Style::default().fg(app.theme.bg).bg(app.theme.success),
                ),
                after.fg(app.theme.text),
            ])),
        );
    }

    // In Moving mode: insert drop-preview row for category.
    let mut display_cursor = if is_active {
        app.feed_editor.cat_cursor
    } else {
        usize::MAX
    };
    if in_moving_mode && moving_cat_id.is_some() && is_active {
        let cursor = app.feed_editor.cat_cursor;
        if cursor < cats.len() {
            let src_name = moving_cat_id
                .and_then(|id| {
                    app.categories
                        .iter()
                        .find(|c| c.id == id)
                        .map(|c| c.name.as_str())
                })
                .unwrap_or("?");
            let depth_delta = match &app.feed_editor.mode {
                FeedEditorMode::Moving { depth_delta, .. } => *depth_delta,
                _ => 0,
            };
            let cursor_depth = match cats.get(cursor) {
                Some(FeedTreeItem::Category { depth, .. }) => *depth as i8,
                _ => 0,
            };
            let preview_depth = (cursor_depth + depth_delta).max(0) as u8;
            let indent = "  ".repeat(preview_depth as usize);
            let preview = ListItem::new(Line::from(vec![
                indent.fg(app.theme.border),
                "➤ ".fg(app.theme.warning).bold(),
                format!("{src_name} ▼").fg(app.theme.warning).bold(),
            ]));
            let insert_at = (cursor + 1).min(final_items.len());
            final_items.insert(insert_at, preview);
            display_cursor = insert_at;
        }
    }

    if is_active {
        app.editor_cats_list_state.select(Some(
            display_cursor.min(final_items.len().saturating_sub(1)),
        ));
    } else {
        app.editor_cats_list_state.select(None);
    }
    f.render_stateful_widget(
        List::new(final_items),
        inner,
        &mut app.editor_cats_list_state,
    );
}
