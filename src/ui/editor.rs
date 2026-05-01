use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use ratatui::prelude::Stylize;

use crate::{
    app::{visible_cat_only_items, visible_tree_items, App},
    models::{AppState, EditorPanel, FeedEditorMode, FeedTreeItem},
};

use super::{
    border_set, BASE, BLUE, CATEGORY_COLORS, GREEN, MANTLE, MAUVE, SUBTEXT0, SURFACE0, TEXT,
    YELLOW,
};

pub(super) fn draw_feed_editor(f: &mut Frame, app: &App, area: Rect) {
    // Background-only outer block (no borders)
    let bg_block = Block::default().bg(BASE);
    f.render_widget(bg_block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);

    draw_editor_feeds(f, app, cols[0]);
    draw_editor_categories(f, app, cols[1]);
}

fn draw_editor_feeds(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.editor_panel == EditorPanel::Feeds;
    let in_moving_mode = matches!(app.editor_mode, FeedEditorMode::Moving { .. });
    let is_rename = app.state == AppState::FeedEditorRename;

    let moving_origin = match &app.editor_mode {
        FeedEditorMode::Moving { origin_render_idx, .. } => Some(*origin_render_idx),
        _ => None,
    };

    let mode_label = if is_active {
        match &app.editor_mode {
            FeedEditorMode::Normal => "",
            FeedEditorMode::Moving { .. } => " MOVE — j/k navigate, Space drop, Esc cancel ",
            FeedEditorMode::Renaming { .. } => " RENAME ",
            _ => "",
        }
    } else {
        ""
    };
    let mode_color = match &app.editor_mode {
        FeedEditorMode::Moving { .. } => YELLOW,
        _ => GREEN,
    };

    let border_color = if is_active { MAUVE } else { SURFACE0 };
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .bg(BASE)
        .title(Line::from(vec![
            Span::styled(" Feeds ", Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
            Span::styled(mode_label, Style::default().fg(mode_color).add_modifier(Modifier::BOLD)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let tree = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);

    // Build mixed rendering: category headers (non-interactive) + feed items (interactive).
    // full_idx_to_visual maps full-tree index → visual list index for feeds only.
    let mut full_idx_to_visual: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    let mut visual_idx = 0usize;
    let mut has_any_feed = false;
    let mut items: Vec<ListItem> = Vec::new();

    for (full_idx, item) in tree.iter().enumerate() {
        match item {
            FeedTreeItem::Category { id, depth, collapsed } => {
                let cat_name = app
                    .categories
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                let color = CATEGORY_COLORS[(*id % CATEGORY_COLORS.len() as u64) as usize];
                let indent = "  ".repeat(*depth as usize);
                let icon = if *collapsed { "▶" } else { "▼" };

                // Category headers are always non-interactive; no cursor highlight.
                // During a feed move the drop-preview arrow (inserted after) shows the target.
                full_idx_to_visual.insert(full_idx, visual_idx);
                items.push(ListItem::new(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        format!("{cat_name} {icon}"),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                ])));
                visual_idx += 1;
            }
            FeedTreeItem::Feed { feeds_idx, depth } => {
                has_any_feed = true;
                let feed = &app.feeds[*feeds_idx];
                let indent = "  ".repeat(*depth as usize);
                let selected = app.editor_cursor == full_idx;
                let is_ghost = moving_origin == Some(full_idx);
                let is_on_origin = in_moving_mode && selected && is_ghost;
                let show_selected = selected && !in_moving_mode && is_active;

                full_idx_to_visual.insert(full_idx, visual_idx);
                visual_idx += 1;

                // Inline rename input
                if is_rename && selected && matches!(app.editor_mode, FeedEditorMode::Renaming { .. }) {
                    items.push(ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        Span::styled("  ✎ ", Style::default().fg(GREEN)),
                        Span::styled(app.editor_input.clone(), Style::default().fg(TEXT)),
                        Span::styled("█", Style::default().fg(GREEN)),
                    ])));
                    continue;
                }

                let style = if is_on_origin {
                    Style::default().fg(SUBTEXT0).bg(SURFACE0)
                } else if is_ghost {
                    Style::default().fg(SUBTEXT0).add_modifier(Modifier::DIM)
                } else if show_selected {
                    Style::default().fg(MAUVE).bg(SURFACE0).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                let origin_hint = if is_on_origin { " ↩" } else { "" };
                let drop_marker = if show_selected {
                    Span::styled("➤ ", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))
                } else {
                    Span::raw("")
                };

                items.push(ListItem::new(Line::from(vec![
                    Span::raw(indent),
                    drop_marker,
                    Span::styled(format!("{}{origin_hint}", feed.title), style),
                    Span::styled(feed.unread_badge(), Style::default().fg(YELLOW)),
                ])));
            }
        }
    }

    if !has_any_feed {
        f.render_widget(
            Paragraph::new(" No feeds. Press [a] to add one.")
                .style(Style::default().fg(SUBTEXT0)),
            inner,
        );
        return;
    }

    let mut final_items = items;
    let mut display_visual = full_idx_to_visual
        .get(&app.editor_cursor)
        .copied()
        .unwrap_or(0);

    // In Moving mode: insert drop-preview row
    if let Some(origin) = moving_origin {
        let cursor = app.editor_cursor;
        if let Some(&origin_vis) = full_idx_to_visual.get(&origin)
            && let Some(&cursor_vis) = full_idx_to_visual.get(&cursor)
            && cursor_vis != origin_vis
        {
            let preview = match tree.get(origin) {
                Some(FeedTreeItem::Feed { feeds_idx, depth }) => {
                    let f = &app.feeds[*feeds_idx];
                    let indent = "  ".repeat(*depth as usize);
                    let arrow =
                        Span::styled("➤ ", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD));
                    let name_style = Style::default().fg(YELLOW).add_modifier(Modifier::BOLD);
                    Some(ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        arrow,
                        Span::styled(f.title.clone(), name_style),
                        Span::styled(f.unread_badge(), Style::default().fg(YELLOW)),
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

    let mut state = ListState::default();
    // Show selection when the Feeds panel is active, or when a feed move is in progress
    // (to keep the ghost/preview visible even if the user tabs to categories mid-move).
    let is_feed_moving = in_moving_mode && matches!(moving_origin.and_then(|o| tree.get(o)), Some(FeedTreeItem::Feed { .. }));
    if is_active || is_feed_moving {
        state.select(Some(display_visual));
    }
    f.render_stateful_widget(List::new(final_items), inner, &mut state);
}

/// Render the right panel: categories-only tree with add/rename/delete controls.
fn draw_editor_categories(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.editor_panel == EditorPanel::Categories;
    let is_rename = app.state == AppState::FeedEditorRename;
    let in_moving_mode = matches!(app.editor_mode, FeedEditorMode::Moving { .. });

    // For cat-move ghost: find the category ID being moved
    let moving_cat_id: Option<u64> = if in_moving_mode {
        if let FeedEditorMode::Moving { origin_render_idx, .. } = &app.editor_mode {
            let items = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
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
        match &app.editor_mode {
            FeedEditorMode::Normal => "",
            FeedEditorMode::Moving { .. } => " MOVE — j/k navigate, ◀▶ depth, Space drop, Esc cancel ",
            FeedEditorMode::Renaming { .. } => " RENAME ",
            FeedEditorMode::NewCategory { .. } => " NEW CATEGORY ",
        }
    } else {
        ""
    };
    let mode_color = match &app.editor_mode {
        FeedEditorMode::Moving { .. } => YELLOW,
        _ => GREEN,
    };

    let border_color = if is_active { MAUVE } else { SURFACE0 };
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .bg(BASE)
        .title(Line::from(vec![
            Span::styled(
                " Categories ",
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                mode_label,
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cats = visible_cat_only_items(&app.categories, &app.feeds, &app.editor_collapsed);

    if cats.is_empty() {
        f.render_widget(
            Paragraph::new(" No categories. [n] Create one.")
                .style(Style::default().fg(SUBTEXT0)),
            inner,
        );
        return;
    }

    // Find the rename target category (by matching full-tree index to category ID)
    let renamed_cat_id: Option<u64> = if is_rename {
        if let FeedEditorMode::Renaming { render_idx } = &app.editor_mode {
            let full = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
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

    let in_new_cat_mode = matches!(app.editor_mode, FeedEditorMode::NewCategory { .. });

    let items: Vec<ListItem> = cats
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let FeedTreeItem::Category { id, depth, collapsed } = item else {
                return ListItem::new("");
            };
            // In NewCategory mode the input row is active; in Moving mode the preview shows position.
            // Neither should highlight the underlying category item.
            let selected = is_active && app.editor_cat_cursor == idx && !in_new_cat_mode && !in_moving_mode;
            let is_ghost = moving_cat_id == Some(*id);
            let color = CATEGORY_COLORS[(*id % CATEGORY_COLORS.len() as u64) as usize];
            let indent = tree_indent(&cats, idx, *depth);
            let connector = if *depth > 0 {
                let next_depth = cats
                    .get(idx + 1)
                    .map(|n| match n {
                        FeedTreeItem::Feed { depth, .. }
                        | FeedTreeItem::Category { depth, .. } => *depth,
                    })
                    .unwrap_or(0);
                if next_depth < *depth {
                    if app.user_data.border_rounded { "╰─ " } else { "└─ " }
                } else {
                    "├─ "
                }
            } else {
                ""
            };
            let icon = if *collapsed { "▶" } else { "▼" };

            // Show rename input for the category being renamed
            if renamed_cat_id == Some(*id) {
                let rename_color = color;
                return ListItem::new(Line::from(vec![
                    Span::styled(indent, Style::default().fg(SURFACE0)),
                    Span::styled(connector, Style::default().fg(SURFACE0)),
                    Span::styled("  ✎ ", Style::default().fg(rename_color)),
                    Span::styled(app.editor_input.clone(), Style::default().fg(TEXT)),
                    Span::styled("█", Style::default().fg(rename_color)),
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
                // Ghost: dimmed, source being moved
                Style::default().fg(SUBTEXT0).add_modifier(Modifier::DIM)
            } else if selected {
                Style::default()
                    .fg(MANTLE)
                    .bg(color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            };
            let badge_style = if selected && !is_ghost {
                Style::default().fg(MANTLE).bg(color)
            } else {
                Style::default().fg(SUBTEXT0)
            };

            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled(format!("{cat_name} {icon}"), style),
                Span::styled(badge, badge_style),
            ]))
        })
        .collect();

    // Insert new-category input row
    let mut final_items = items;
    if is_rename
        && let FeedEditorMode::NewCategory { parent_id } = &app.editor_mode
    {
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
        let insert_at = app.editor_cat_cursor.min(final_items.len());
        final_items.insert(
            insert_at,
            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled("  ✎ ", Style::default().fg(GREEN)),
                Span::styled(app.editor_input.clone(), Style::default().fg(TEXT)),
                Span::styled("█", Style::default().fg(GREEN)),
            ])),
        );
    }

    // In Moving mode: insert drop-preview for category
    let mut display_cursor = if is_active {
        app.editor_cat_cursor
    } else {
        usize::MAX
    };
    if in_moving_mode && moving_cat_id.is_some() && is_active {
        let cursor = app.editor_cat_cursor;
        let at_virtual_root = cursor >= cats.len();
        if !at_virtual_root {
            let src_name = moving_cat_id
                .and_then(|id| {
                    app.categories
                        .iter()
                        .find(|c| c.id == id)
                        .map(|c| c.name.as_str())
                })
                .unwrap_or("?");
            // depth_delta: 0 = sibling (same depth), +1 = child, -1 = shallower
            let depth_delta = match &app.editor_mode {
                FeedEditorMode::Moving { depth_delta, .. } => *depth_delta,
                _ => 0,
            };
            let cursor_depth = match cats.get(cursor) {
                Some(FeedTreeItem::Category { depth, .. }) => *depth as i8,
                _ => 0,
            };
            let preview_depth = (cursor_depth + depth_delta).max(0) as u8;
            let indent = "  ".repeat(preview_depth as usize);
            let arrow =
                Span::styled("➤ ", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD));
            let preview = ListItem::new(Line::from(vec![
                Span::raw(indent),
                arrow,
                Span::styled(
                    format!("{src_name} ▼"),
                    Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
                ),
            ]));
            let insert_at = (cursor + 1).min(final_items.len());
            final_items.insert(insert_at, preview);
            display_cursor = insert_at;
        }
    }

    let mut state = ListState::default();
    if is_active {
        state.select(Some(
            display_cursor.min(final_items.len().saturating_sub(1)),
        ));
    }
    f.render_stateful_widget(List::new(final_items), inner, &mut state);
}
