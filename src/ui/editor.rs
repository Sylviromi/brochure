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
    let is_rename = app.state == AppState::FeedEditorRename;
    let mode_label = match &app.editor_mode {
        FeedEditorMode::Normal => " NORMAL ",
        FeedEditorMode::Moving { .. } => " MOVE — j/k navigate, Enter to drop, Esc to cancel ",
        FeedEditorMode::Renaming { .. } => " RENAME ",
        FeedEditorMode::NewCategory { .. } => " NEW CATEGORY ",
    };
    let mode_color = match &app.editor_mode {
        FeedEditorMode::Normal => BLUE,
        FeedEditorMode::Moving { .. } => YELLOW,
        _ => GREEN,
    };

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MAUVE))
        .bg(BASE)
        .title(Line::from(vec![
            Span::styled(
                " Feed Editor ",
                Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                mode_label,
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let tree = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);

    let moving_origin = match &app.editor_mode {
        FeedEditorMode::Moving { origin_render_idx, .. } => Some(*origin_render_idx),
        _ => None,
    };
    let in_moving_mode = moving_origin.is_some();
    // True when moving a category (not a feed) — enables virtual root position.
    let moving_src_is_category = moving_origin
        .is_some_and(|o| matches!(tree.get(o), Some(FeedTreeItem::Category { .. })));

    let items: Vec<ListItem> = tree
        .iter()
        .enumerate()
        .map(|(render_idx, item)| {
            let selected = app.editor_cursor == render_idx;
            let is_ghost = moving_origin == Some(render_idx);
            // In Moving mode: only the origin item gets special styling (↩ hint on ghost).
            // The ➤ preview row below the cursor is the sole drop indicator.
            let is_on_origin = in_moving_mode && selected && is_ghost;
            let show_selected = selected && !in_moving_mode;
            let drop_marker = if show_selected { "➤ " } else { "" };

            match item {
                FeedTreeItem::Category {
                    id,
                    depth,
                    collapsed,
                } => {
                    let color = CATEGORY_COLORS[(id % CATEGORY_COLORS.len() as u64) as usize];
                    let indent = "   ".repeat(*depth as usize);
                    let icon = if *collapsed { " ▶" } else { " ▼" };
                    let cat_name = app
                        .categories
                        .iter()
                        .find(|c| c.id == *id)
                        .map(|c| c.name.as_str())
                        .unwrap_or("?");
                    let style = if is_on_origin {
                        Style::default().fg(SUBTEXT0).bg(SURFACE0)
                    } else if is_ghost {
                        Style::default().fg(SUBTEXT0)
                    } else if show_selected {
                        Style::default()
                            .fg(MANTLE)
                            .bg(color)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color).add_modifier(Modifier::BOLD)
                    };
                    let origin_hint = if is_on_origin { " ↩" } else { "" };
                    ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        Span::styled(
                            drop_marker,
                            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(cat_name, style),
                        Span::styled(format!("{icon}{origin_hint}"), style),
                    ]))
                }
                FeedTreeItem::Feed { feeds_idx, depth } => {
                    let feed = &app.feeds[*feeds_idx];
                    let indent = "   ".repeat(*depth as usize);

                    // Inline rename input for the selected feed
                    if is_rename
                        && selected
                        && matches!(app.editor_mode, FeedEditorMode::Renaming { .. })
                    {
                        return ListItem::new(Line::from(vec![
                            Span::raw(indent),
                            Span::styled("  ✎ ", Style::default().fg(GREEN)),
                            Span::styled(app.editor_input.clone(), Style::default().fg(TEXT)),
                            Span::styled("█", Style::default().fg(GREEN)),
                        ]));
                    }

                    let style = if is_on_origin {
                        Style::default().fg(SUBTEXT0).bg(SURFACE0)
                    } else if is_ghost {
                        Style::default().fg(SUBTEXT0)
                    } else if show_selected {
                        Style::default()
                            .fg(MAUVE)
                            .bg(SURFACE0)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TEXT)
                    };
                    let origin_hint = if is_on_origin { " ↩" } else { "" };
                    ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        Span::styled(
                            drop_marker,
                            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!("{}{origin_hint}", feed.title), style),
                        Span::styled(feed.unread_badge(), Style::default().fg(YELLOW)),
                    ]))
                }
            }
        })
        .collect();

    // Overlay new-category or category-rename input row at cursor
    let mut final_items = items;
    if let FeedEditorMode::NewCategory { parent_id } = &app.editor_mode
        && is_rename
    {
        let parent_id = *parent_id;
        // Determine depth and indent from parent category.
        let depth = if parent_id.is_some() {
            let parent_depth = tree.iter().find_map(|item| match item {
                FeedTreeItem::Category { id, depth, .. } if Some(*id) == parent_id => Some(*depth),
                _ => None,
            });
            parent_depth.map(|d| d + 1).unwrap_or(1)
        } else {
            0
        };
        let indent = "  ".repeat(depth as usize);
        let insert_at = app.editor_cursor.min(final_items.len());
        final_items.insert(
            insert_at,
            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled("  ✎ ", Style::default().fg(GREEN)),
                Span::styled(app.editor_input.clone(), Style::default().fg(TEXT)),
                Span::styled("█", Style::default().fg(GREEN)),
            ])),
        );
    } else if is_rename && matches!(app.editor_mode, FeedEditorMode::Renaming { .. }) {
        let cursor = app.editor_cursor;
        if let Some(FeedTreeItem::Category { id, depth, .. }) = tree.get(cursor) {
            let indent = "  ".repeat(*depth as usize);
            let color = CATEGORY_COLORS[(id % CATEGORY_COLORS.len() as u64) as usize];
            final_items[cursor] = ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled("  ✎ ", Style::default().fg(color)),
                Span::styled(app.editor_input.clone(), Style::default().fg(TEXT)),
                Span::styled("█", Style::default().fg(color)),
            ]));
        }
    }

    // In Moving mode, insert a drop-preview row showing exactly where the dragged item will land.
    // The ➤ row is the sole visual cursor indicator; no background is added to the cursor item.
    let mut display_cursor = app.editor_cursor;
    if let Some(origin) = moving_origin {
        let cursor = app.editor_cursor;
        // cursor == tree.len() is the virtual root position (category moves only).
        let at_virtual_root = moving_src_is_category && cursor == tree.len();

        if at_virtual_root || cursor != origin {
            let arrow =
                Span::styled("➤ ", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD));
            let name_style = Style::default().fg(YELLOW).add_modifier(Modifier::BOLD);

            let (preview, insert_at) = if at_virtual_root {
                // Drop at root: preview at depth 0, inserted before everything.
                (cat_preview_item(app, origin, 0, arrow, name_style), 0)
            } else {
                // Depth of the preview: onto a category → child (depth+1); onto a feed → sibling.
                let preview_depth = match tree.get(cursor) {
                    Some(FeedTreeItem::Category { depth, .. }) => depth + 1,
                    Some(FeedTreeItem::Feed { depth, .. }) => *depth,
                    None => 0,
                };
                let indent = "   ".repeat(preview_depth as usize);
                let item = match tree.get(origin) {
                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                        let feed = &app.feeds[*feeds_idx];
                        ListItem::new(Line::from(vec![
                            Span::raw(indent),
                            arrow,
                            Span::styled(feed.title.clone(), name_style),
                            Span::styled(feed.unread_badge(), Style::default().fg(YELLOW)),
                        ]))
                    }
                    Some(FeedTreeItem::Category { .. }) => {
                        cat_preview_item(app, origin, preview_depth as u8, arrow, name_style)
                    }
                    None => ListItem::new(""),
                };
                let insert_at = (cursor + 1).min(final_items.len());
                (item, insert_at)
            };

            final_items.insert(insert_at, preview);
            display_cursor = insert_at;
        }
    }

    // Split inner area: left = category manager, right = feed+category tree.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(inner);

    draw_editor_categories(f, app, cols[0]);

    // Right panel: existing feed+category tree.
    let right_block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(if app.editor_panel == EditorPanel::Feeds {
            MAUVE
        } else {
            SURFACE0
        }))
        .title(Span::styled(
            " Feeds ",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ))
        .bg(BASE);
    let right_inner = right_block.inner(cols[1]);
    f.render_widget(right_block, cols[1]);

    if final_items.is_empty() {
        f.render_widget(
            Paragraph::new(" No feeds. [a] Add feed, [n] New category.")
                .style(Style::default().fg(SUBTEXT0)),
            right_inner,
        );
    } else {
        let mut state = ListState::default();
        state.select(Some(display_cursor));
        f.render_stateful_widget(List::new(final_items), right_inner, &mut state);
    }
}

/// Build a ➤ preview row for a category being moved to `depth`.
fn cat_preview_item<'a>(
    app: &'a App,
    origin: usize,
    depth: u8,
    arrow: Span<'a>,
    name_style: ratatui::style::Style,
) -> ListItem<'a> {
    let tree = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
    let indent = "   ".repeat(depth as usize);
    let (cat_name, icon) = match tree.get(origin) {
        Some(FeedTreeItem::Category { id, collapsed, .. }) => {
            let name = app
                .categories
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.name.as_str())
                .unwrap_or("?");
            let icon = if *collapsed { " ▶" } else { " ▼" };
            (name, icon)
        }
        _ => ("?", " ▼"),
    };
    ListItem::new(Line::from(vec![
        Span::raw(indent),
        arrow,
        Span::styled(cat_name, name_style),
        Span::styled(icon, name_style),
    ]))
}

/// Render the left panel: categories-only tree with add/rename/delete controls.
fn draw_editor_categories(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.editor_panel == EditorPanel::Categories;
    let border_color = if is_active { MAUVE } else { SURFACE0 };

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Categories ",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ))
        .bg(BASE);
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

    let items: Vec<ListItem> = cats
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let FeedTreeItem::Category { id, depth, collapsed } = item else {
                return ListItem::new("");
            };
            let selected = is_active && app.editor_cat_cursor == idx;
            let color = CATEGORY_COLORS[(*id % CATEGORY_COLORS.len() as u64) as usize];
            let indent = "   ".repeat(*depth as usize);
            let icon = if *collapsed { " ▶" } else { " ▼" };

            // Count direct feeds.
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

            let style = if selected {
                Style::default()
                    .fg(MANTLE)
                    .bg(color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            };
            let badge_style = if selected {
                Style::default().fg(MANTLE).bg(color)
            } else {
                Style::default().fg(SUBTEXT0)
            };

            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled(cat_name, style),
                Span::styled(icon, style),
                Span::styled(badge, badge_style),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    if is_active {
        state.select(Some(app.editor_cat_cursor.min(cats.len().saturating_sub(1))));
    }
    f.render_stateful_widget(List::new(items), inner, &mut state);
}

