//! Settings menu rendering: toggles and action buttons for user preferences.

use crate::{
    app::App,
    models::{AppState, SettingsItem},
};
use ratatui::{
    Frame,
    layout::Rect,
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use super::border_set;

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

    let save = app.user_data.cache_full_articles;
    let rounded = app.user_data.border_rounded;
    let scroll_loop = app.user_data.scroll_loop;
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
            item: SettingsItem::CacheFullArticles,
            label: "[ Cache Full Articles ]",
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
        Row::Cycle {
            item: SettingsItem::AutoFetchOnStart,
            label: "[ Fetch Policy ]",
            in_last: false,
            value: app.user_data.fetch_policy.label().to_string(),
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
            item: SettingsItem::ScrollLoop,
            label: "[ Scroll Loop ]",
            in_last: true,
            on: scroll_loop,
        },
        Row::Toggle {
            item: SettingsItem::BorderStyle,
            label: "[ Rounded Borders ]",
            in_last: true,
            on: rounded,
        },
        Row::Cycle {
            item: SettingsItem::BodyAlignment,
            label: "[ Body Alignment ]",
            in_last: true,
            value: app.alignment_label().to_string(),
        },
        Row::Cycle {
            item: SettingsItem::Theme,
            label: "[ Theme ]",
            in_last: true,
            value: app.theme.name.clone(),
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
                    connector.fg(app.theme.border),
                    label.fg(app.theme.header).bold(),
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
                        .fg(app.theme.bg_dark)
                        .bg(app.theme.accent)
                        .bold()
                } else {
                    Style::default().fg(app.theme.text)
                };
                ListItem::new(Line::from(vec![
                    prefix.fg(app.theme.border),
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
                        .fg(app.theme.bg_dark)
                        .bg(app.theme.accent)
                        .bold()
                } else {
                    Style::default().fg(app.theme.text)
                };
                let badge_style = if selected {
                    Style::default()
                        .fg(app.theme.bg_dark)
                        .bg(app.theme.accent)
                        .bold()
                } else if *on {
                    Style::default().fg(app.theme.success).bold()
                } else {
                    Style::default().fg(app.theme.muted)
                };
                ListItem::new(Line::from(vec![
                    prefix.fg(app.theme.border),
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
                        .fg(app.theme.bg_dark)
                        .bg(app.theme.accent)
                        .bold()
                } else {
                    Style::default().fg(app.theme.text)
                };
                let badge_style = if selected {
                    Style::default()
                        .fg(app.theme.bg_dark)
                        .bg(app.theme.accent)
                        .bold()
                } else {
                    Style::default().fg(app.theme.warning).bold()
                };
                ListItem::new(Line::from(vec![
                    prefix.fg(app.theme.border),
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
                        .fg(app.theme.bg_dark)
                        .bg(app.theme.accent)
                        .bold()
                } else {
                    Style::default().fg(app.theme.text)
                };
                let badge_style = if selected {
                    Style::default()
                        .fg(app.theme.bg_dark)
                        .bg(app.theme.accent)
                        .bold()
                } else {
                    Style::default().fg(app.theme.error)
                };
                ListItem::new(Line::from(vec![
                    prefix.fg(app.theme.border),
                    Span::styled("[ Clear Article Cache ]", base_style),
                    Span::styled(format!("  {} ", size_label), badge_style),
                ]))
            }
            Row::Spacer => ListItem::new(Line::from(" │".fg(app.theme.border))),
        })
        .collect();

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.state == AppState::SettingsList {
            app.theme.accent
        } else {
            app.theme.border
        }))
        .bg(app.theme.bg)
        .title(" Settings ".fg(app.theme.header).bold())
        .title_bottom(
            format!(" {} ", app.settings_selected.description()).fg(app.theme.muted),
        );

    f.render_widget(List::new(list_items).block(block), area);
}
