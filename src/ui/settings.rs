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
pub(super) fn draw_settings_tab(f: &mut Frame, app: &mut App, area: Rect) {
    match app.state {
        AppState::SettingsList
        | AppState::ExportPath
        | AppState::ImportPath
        | AppState::ClearData
        | AppState::ClearArticleCache
        | AppState::FreshRssSetup => draw_settings(f, app, area),
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
fn draw_settings(f: &mut Frame, app: &mut App, area: Rect) {
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
    let toasts = &app.user_data.toasts;
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
            item: SettingsItem::ImportSaved,
            label: "[ Import Saved ]",
            in_last: false,
        },
        Row::Item {
            item: SettingsItem::ExportSaved,
            label: "[ Export Saved ]",
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
            label: " Sync",
            is_last: false,
        },
        Row::Cycle {
            item: SettingsItem::FreshRss,
            label: "[ FreshRSS Sync ]",
            in_last: false,
            value: app.freshrss_status(),
        },
        Row::Spacer,
        Row::SectionHeader {
            label: " Notifications",
            is_last: false,
        },
        Row::Cycle {
            item: SettingsItem::ToastPosition,
            label: "[ Position ]",
            in_last: false,
            value: toasts.position.label().to_string(),
        },
        Row::Cycle {
            item: SettingsItem::ToastDuration,
            label: "[ Duration ]",
            in_last: false,
            value: format!("{}s", toasts.seconds),
        },
        Row::Cycle {
            item: SettingsItem::ToastCount,
            label: "[ Max Shown ]",
            in_last: false,
            value: toasts.max_visible.to_string(),
        },
        Row::Toggle {
            item: SettingsItem::ToastProgress,
            label: "[ Refresh Progress ]",
            in_last: false,
            on: toasts.refresh_progress,
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
            item: SettingsItem::ZenWidth,
            label: "[ Zen Width ]",
            in_last: true,
            value: format!("{}%", app.zen_width),
        },
        Row::Cycle {
            item: SettingsItem::Theme,
            label: "[ Theme ]",
            in_last: true,
            value: app.theme.name.clone(),
        },
    ];

    let theme = &app.theme;
    let highlight = Style::default().fg(theme.bg_dark).bg(theme.accent).bold();
    let tree_prefix = |in_last: bool| if in_last { "     " } else { " │   " };
    // One selectable row: tree prefix, label, and an optional badge whose style is
    // replaced by the highlight when the row is selected.
    let item_row =
        |item: SettingsItem, in_last: bool, label: String, badge: Option<Span<'static>>| {
            let selected = app.settings_selected == item;
            let label_style = if selected {
                highlight
            } else {
                Style::default().fg(theme.text)
            };
            let mut spans = vec![
                tree_prefix(in_last).fg(theme.border),
                Span::styled(label, label_style),
            ];
            if let Some(badge) = badge {
                spans.push(if selected {
                    badge.style(highlight)
                } else {
                    badge
                });
            }
            ListItem::new(Line::from(spans))
        };

    let mut targets: Vec<Option<SettingsItem>> = Vec::with_capacity(rows.len());
    let list_items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            let (target, item) = match row {
                Row::SectionHeader { label, is_last } => {
                    let connector = match (*is_last, app.user_data.border_rounded) {
                        (false, _) => " ├─",
                        (true, true) => " ╰─",
                        (true, false) => " └─",
                    };
                    let line = Line::from(vec![
                        connector.fg(theme.border),
                        label.fg(theme.header).bold(),
                    ]);
                    (None, ListItem::new(line))
                }
                Row::Item {
                    item,
                    label,
                    in_last,
                } => (
                    Some(*item),
                    item_row(*item, *in_last, label.to_string(), None),
                ),
                Row::Toggle {
                    item,
                    label,
                    in_last,
                    on,
                } => {
                    let badge = if *on {
                        "  ON ".fg(theme.success).bold()
                    } else {
                        "  OFF ".fg(theme.muted)
                    };
                    (
                        Some(*item),
                        item_row(*item, *in_last, label.to_string(), Some(badge)),
                    )
                }
                Row::Cycle {
                    item,
                    label,
                    in_last,
                    value,
                } => {
                    let badge = format!("  {value}").fg(theme.warning).bold();
                    (
                        Some(*item),
                        item_row(*item, *in_last, label.to_string(), Some(badge)),
                    )
                }
                Row::CacheItem {
                    in_last,
                    size_label,
                } => {
                    let item = SettingsItem::ClearArticleCache;
                    let badge = format!("  {size_label} ").fg(theme.error);
                    let label = "[ Clear Article Cache ]".to_string();
                    (Some(item), item_row(item, *in_last, label, Some(badge)))
                }
                Row::Spacer => (None, ListItem::new(Line::from(" │".fg(theme.border)))),
            };
            targets.push(target);
            item
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
        .title_bottom(format!(" {} ", app.settings_selected.description()).fg(app.theme.muted));

    let inner = block.inner(area);
    f.render_widget(List::new(list_items).block(block), area);

    for (i, target) in targets.into_iter().enumerate() {
        let y = inner.y + i as u16;
        if let Some(item) = target
            && y < inner.bottom()
        {
            app.hit
                .settings
                .push((Rect::new(inner.x, y, inner.width, 1), item));
        }
    }
}
