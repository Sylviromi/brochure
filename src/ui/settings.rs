//! Settings menu and saved-category editor rendering.
//!
//! This module renders the Settings tab with toggles and buttons for user preferences,
//! and the full-screen saved-category editor for managing custom article categories.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};

use ratatui::prelude::Stylize;

use crate::{
    app::App,
    models::{AppState, SettingsItem},
};

use super::{BASE, GREEN, MANTLE, MAUVE, PEACH, RED, SUBTEXT0, SURFACE0, TEXT, YELLOW, border_set};

/// Dispatches to the settings list renderer when in a settings-related state.
pub(super) fn draw_settings_tab(f: &mut Frame, app: &App, area: Rect) {
    match app.state {
        AppState::SettingsList
        | AppState::OPMLExportPath
        | AppState::OPMLImportPath
        | AppState::ClearData
        | AppState::ClearArticleCache => draw_settings(f, app, area),
        _ => {}
    }
}

/// Formats a byte count as a human-readable size string (B, KB, MB).
fn format_cache_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes == 0 {
        "empty".to_string()
    } else {
        format!("{bytes} B")
    }
}

/// Renders the settings menu with grouped sections, toggles, and action buttons.
fn draw_settings(f: &mut Frame, app: &App, area: Rect) {
    enum Row {
        SectionHeader {
            label: &'static str,
            is_last: bool,
        },
        Item {
            item: SettingsItem,
            label: &'static str,
            in_last: bool,
        },
        Toggle {
            item: SettingsItem,
            label: &'static str,
            in_last: bool,
            on: bool,
        },
        Cycle {
            item: SettingsItem,
            label: &'static str,
            in_last: bool,
            value: String,
        },
        CacheItem {
            in_last: bool,
            size_label: String,
        },
        Spacer,
    }

    let save = app.user_data.save_article_content;
    let eager = app.user_data.eager_article_fetch;
    let auto_fetch = app.user_data.auto_fetch_on_start;
    let rounded = app.user_data.border_rounded;
    let cache_label = format_cache_size(app.article_cache_size);
    let rows = [
        Row::SectionHeader {
            label: " Data",
            is_last: false,
        },
        Row::Item {
            item: SettingsItem::ImportOpml,
            label: "[ Import OPML ]",
            in_last: false,
        },
        Row::Item {
            item: SettingsItem::ExportOpml,
            label: "[ Export OPML ]",
            in_last: false,
        },
        Row::Item {
            item: SettingsItem::ClearData,
            label: "[ Clear All Data ]",
            in_last: false,
        },
        Row::Spacer,
        Row::SectionHeader {
            label: " Article Storage",
            is_last: false,
        },
        Row::Toggle {
            item: SettingsItem::SaveArticleContent,
            label: "[ Save Article Content ]",
            in_last: false,
            on: save,
        },
        Row::CacheItem {
            in_last: false,
            size_label: cache_label,
        },
        Row::Spacer,
        Row::SectionHeader {
            label: " Fetching",
            is_last: false,
        },
        Row::Toggle {
            item: SettingsItem::EagerArticleFetch,
            label: "[ Eager Article Fetch ]",
            in_last: false,
            on: eager,
        },
        Row::Toggle {
            item: SettingsItem::AutoFetchOnStart,
            label: "[ Auto Fetch On Start ]",
            in_last: false,
            on: auto_fetch,
        },
        Row::Cycle {
            item: SettingsItem::ArchivePolicy,
            label: "[ Archive Policy ]",
            in_last: false,
            value: app.user_data.archive_policy.label().to_string(),
        },
        Row::Spacer,
        Row::SectionHeader {
            label: " Appearance",
            is_last: true,
        },
        Row::Toggle {
            item: SettingsItem::BorderStyle,
            label: "[ Rounded Borders ]",
            in_last: true,
            on: rounded,
        },
    ];

    let list_items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            Row::SectionHeader { label, is_last } => {
                let connector = if *is_last {
                    if app.user_data.border_rounded {
                        " ╰─"
                    } else {
                        " └─"
                    }
                } else {
                    " ├─"
                };
                ListItem::new(Line::from(vec![
                    Span::styled(connector, Style::default().fg(SURFACE0)),
                    Span::styled(
                        *label,
                        Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
                    ),
                ]))
            }
            Row::Item {
                item,
                label,
                in_last,
            } => {
                let prefix = if *in_last { "     " } else { " │   " };
                let selected = app.settings_selected == *item;
                let style = if selected {
                    Style::default()
                        .fg(MANTLE)
                        .bg(MAUVE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(SURFACE0)),
                    Span::styled(*label, style),
                ]))
            }
            Row::Toggle {
                item,
                label,
                in_last,
                on,
            } => {
                let prefix = if *in_last { "     " } else { " │   " };
                let selected = app.settings_selected == *item;
                let base_style = if selected {
                    Style::default()
                        .fg(MANTLE)
                        .bg(MAUVE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                let badge_style = if selected {
                    Style::default()
                        .fg(MANTLE)
                        .bg(MAUVE)
                        .add_modifier(Modifier::BOLD)
                } else if *on {
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(SUBTEXT0)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(SURFACE0)),
                    Span::styled(*label, base_style),
                    Span::styled(if *on { "  ON " } else { "  OFF " }, badge_style),
                ]))
            }
            Row::Cycle {
                item,
                label,
                in_last,
                value,
            } => {
                let prefix = if *in_last { "     " } else { " │   " };
                let selected = app.settings_selected == *item;
                let base_style = if selected {
                    Style::default()
                        .fg(MANTLE)
                        .bg(MAUVE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                let badge_style = if selected {
                    Style::default()
                        .fg(MANTLE)
                        .bg(MAUVE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(SURFACE0)),
                    Span::styled(*label, base_style),
                    Span::styled(format!("  {}", value), badge_style),
                ]))
            }
            Row::CacheItem {
                in_last,
                size_label,
            } => {
                let prefix = if *in_last { "     " } else { " │   " };
                let selected = app.settings_selected == SettingsItem::ClearArticleCache;
                let base_style = if selected {
                    Style::default()
                        .fg(MANTLE)
                        .bg(MAUVE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                let badge_style = if selected {
                    Style::default()
                        .fg(MANTLE)
                        .bg(MAUVE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(RED)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(SURFACE0)),
                    Span::styled("[ Clear Article Cache ]", base_style),
                    Span::styled(format!("  {} ", size_label), badge_style),
                ]))
            }
            Row::Spacer => ListItem::new(Line::from(Span::styled(
                " │",
                Style::default().fg(SURFACE0),
            ))),
        })
        .collect();

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.state == AppState::SettingsList {
            MAUVE
        } else {
            SURFACE0
        }))
        .bg(BASE)
        .title(Span::styled(
            " Settings ",
            Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(List::new(list_items).block(block), area);
}

/// Renders the full-screen saved-category editor for renaming and deleting custom categories.
pub(super) fn draw_saved_category_editor(f: &mut Frame, app: &mut App, area: Rect) {
    let rounded = app.user_data.border_rounded;
    let block = Block::default()
        .title(" Saved Category Editor  [r] rename  [d] delete  [n] new  [Esc] back ")
        .borders(Borders::ALL)
        .border_set(border_set(rounded))
        .border_style(Style::default().fg(MAUVE))
        .style(Style::default().bg(BASE));

    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    if app.user_data.saved_categories.is_empty() {
        let msg = Paragraph::new("  No categories yet. Save an article with [s] to create one.")
            .style(Style::default().fg(SUBTEXT0));
        f.render_widget(msg, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .user_data
        .saved_categories
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let count = app
                .user_data
                .saved_articles
                .iter()
                .filter(|s| s.category_id == cat.id)
                .count();

            let name_span = if app.state == AppState::SavedCategoryEditorRename
                && i == app.saved_cat_editor_scroll.cursor
            {
                Span::styled(
                    format!("  {}|", app.editor_input),
                    Style::default().fg(YELLOW),
                )
            } else {
                let style = if i == app.saved_cat_editor_scroll.cursor {
                    Style::default().bg(SURFACE0).fg(MAUVE)
                } else {
                    Style::default().fg(TEXT)
                };
                Span::styled(format!("  {}", cat.name), style)
            };

            ListItem::new(Line::from(vec![
                name_span,
                Span::styled(
                    format!("  [{count} article{}]", if count == 1 { "" } else { "s" }),
                    Style::default().fg(SUBTEXT0),
                ),
            ]))
        })
        .collect();

    let total = app.user_data.saved_categories.len();
    let has_scrollbar = total > inner.height as usize;
    let list_render_area = if has_scrollbar {
        Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        }
    } else {
        inner
    };
    f.render_stateful_widget(
        List::new(items),
        list_render_area,
        &mut app.saved_cat_editor_scroll.list_state,
    );
    if has_scrollbar {
        let mut sb_state = ScrollbarState::new(total).position(app.saved_cat_editor_scroll.cursor);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(super::SURFACE0)),
            inner,
            &mut sb_state,
        );
    }

    // Render input row when creating a new category.
    if app.state == AppState::SavedCategoryEditorNew {
        let input_area = Rect {
            y: area.bottom().saturating_sub(2),
            height: 1,
            ..area
        };
        let input_text = format!("  Category name: {}", app.editor_input);
        let input_para = Paragraph::new(input_text).style(Style::default().fg(YELLOW));
        f.render_widget(input_para, input_area);
    }
}
