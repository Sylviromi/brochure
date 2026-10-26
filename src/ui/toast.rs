//! Toast notifications: cards stacked in a screen corner, newest nearest the corner,
//! drawn above everything (zen mode too). A live card tracks a feed refresh.

use ratatui::{
    Frame,
    layout::Rect,
    prelude::Stylize,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph, Wrap},
};

use super::{SPINNER_FRAMES, border_set};
use crate::app::App;

/// Ticks per second (the event loop ticks every ~250 ms).
const TICKS_PER_SEC: usize = 4;
/// Wrapped message lines shown before a card is cut off.
const MAX_LINES: usize = 4;
/// Columns taken by a card's borders and padding.
const CHROME: u16 = 4;
/// Width of the live refresh card.
const PROGRESS_WIDTH: u16 = 40;

/// A card to draw: its text (wrapped to `rows` rows) and accent colour.
struct Card {
    lines: Vec<Line<'static>>,
    rows: u16,
    width: u16,
    color: Color,
}

/// Returns the rects of the cards drawn.
pub(super) fn draw_toasts(f: &mut Frame, app: &App) -> Vec<Rect> {
    let settings = &app.user_data.toasts;
    let screen = f.area();
    let max_width = (screen.width / 2)
        .clamp(28, 64)
        .min(screen.width.saturating_sub(2));
    if max_width <= CHROME + 2 {
        return Vec::new();
    }
    let ttl = usize::from(settings.seconds) * TICKS_PER_SEC;
    let max = usize::from(settings.max_visible);
    let messages: Vec<(Line<'static>, Color)> = app
        .toasts
        .visible(app.tick, ttl, max)
        .map(|t| styled_message(app, &t.msg))
        .collect();
    let previewing = app
        .toast_preview_tick
        .is_some_and(|t| app.tick.saturating_sub(t) < ttl);
    let refresh = if app.feeds_total > 0 {
        let blocked = app
            .refresh_errors
            .iter()
            .filter(|(tag, _)| *tag == "blocked")
            .count();
        Some(Refresh {
            done: app.feeds_total.saturating_sub(app.feeds_pending),
            total: app.feeds_total,
            blocked,
            failed: app.refresh_errors.len() - blocked,
        })
    } else {
        previewing.then_some(Refresh {
            done: 27,
            total: 40,
            blocked: 2,
            failed: 1,
        })
    };
    let refresh = refresh.filter(|_| settings.refresh_progress);
    let progress = refresh.is_some();
    // One width for the whole stack (the widest card), so the edges line up.
    let Some(width) = messages
        .iter()
        .map(|(line, _)| line.width() as u16 + CHROME)
        .chain(progress.then_some(PROGRESS_WIDTH))
        .max()
        .map(|w| w.min(max_width))
    else {
        return Vec::new();
    };
    let mut cards = Vec::new();
    if let Some(refresh) = refresh {
        cards.push(progress_card(app, refresh, width));
    }
    cards.extend(
        messages
            .into_iter()
            .map(|(line, color)| message_card(line, color, width)),
    );

    let (top, left) = (settings.position.is_top(), settings.position.is_left());
    // Edge the next card stacks against: below the previous one (top corners) or above it.
    // Top cards start below the tab bar; bottom ones keep a row free above the edge.
    let mut drawn = Vec::new();
    let mut edge = if top {
        screen.top() + 4
    } else {
        screen.bottom().saturating_sub(1)
    };
    for card in cards {
        let height = card.rows + 2;
        // Two columns in from the side, clear of the panel borders.
        let x = if left {
            screen.left() + 2
        } else {
            screen.right().saturating_sub(card.width + 2)
        };
        let y = if top {
            edge
        } else {
            edge.saturating_sub(height)
        };
        let rect = Rect::new(x, y, card.width, height);
        if rect.intersection(screen) != rect {
            break; // no room left on screen
        }
        let block = Block::bordered()
            .border_set(border_set(app.user_data.border_rounded))
            .border_style(Style::default().fg(card.color))
            .padding(Padding::horizontal(1))
            .bg(app.theme.bg);
        f.render_widget(Clear, rect);
        f.render_widget(
            Paragraph::new(card.lines)
                .wrap(Wrap { trim: true })
                .block(block),
            rect,
        );
        drawn.push(rect);
        edge = if top { y + height } else { y };
    }
    drawn
}

/// A notification line: icon and colour from its leading `✓`/`⚠` (or its wording).
fn styled_message(app: &App, msg: &str) -> (Line<'static>, Color) {
    let theme = &app.theme;
    let lower = msg.to_lowercase();
    let (icon, color, body) = if let Some(rest) = msg.strip_prefix("✓ ") {
        ("✓", theme.success, rest)
    } else if let Some(rest) = msg.strip_prefix("⚠ ") {
        ("⚠", theme.warning, rest)
    } else if ["failed", "error", "cannot", "could not"]
        .iter()
        .any(|w| lower.contains(w))
    {
        ("⚠", theme.warning, msg)
    } else {
        ("●", theme.accent, msg)
    };
    let line = Line::from(vec![
        Span::styled(icon, Style::default().fg(color).bold()),
        Span::raw(" "),
        Span::styled(body.to_string(), Style::default().fg(theme.text)),
    ]);
    (line, color)
}

/// A notification card `width` wide, its text wrapped to at most [`MAX_LINES`].
fn message_card(mut line: Line<'static>, color: Color, width: u16) -> Card {
    let inner = width - CHROME;
    let rows = |line: &Line<'static>| {
        Paragraph::new(line.clone())
            .wrap(Wrap { trim: true })
            .line_count(inner)
    };
    // Long messages keep their opening and end in "…".
    while rows(&line) > MAX_LINES
        && let Some(body) = line.spans.last_mut()
    {
        let keep = body.content.chars().count().saturating_sub(8);
        body.content = format!("{}…", body.content.chars().take(keep).collect::<String>()).into();
    }
    Card {
        rows: rows(&line).min(MAX_LINES) as u16,
        lines: vec![line],
        width,
        color,
    }
}

/// Feed refresh counts shown on the refresh card.
#[derive(Clone, Copy)]
struct Refresh {
    done: usize,
    total: usize,
    blocked: usize,
    failed: usize,
}

/// The live refresh card: spinner and count, a progress bar, and failures so far.
fn progress_card(app: &App, refresh: Refresh, width: u16) -> Card {
    let theme = &app.theme;
    let Refresh {
        done,
        total,
        blocked,
        failed,
    } = refresh;
    let inner = usize::from(width - CHROME);
    let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
    let count = format!("{done}/{total}");
    let label = "Refreshing feeds";
    let gap = inner.saturating_sub(2 + label.len() + count.len());
    let filled = (inner * done).checked_div(total).unwrap_or(0).min(inner);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(spinner, Style::default().fg(theme.accent)),
            Span::raw(" "),
            Span::styled(label, Style::default().fg(theme.text)),
            Span::raw(" ".repeat(gap)),
            Span::styled(count, Style::default().fg(theme.muted)),
        ]),
        Line::from(vec![
            Span::styled("━".repeat(filled), Style::default().fg(theme.accent)),
            Span::styled(
                "─".repeat(inner - filled),
                Style::default().fg(theme.border),
            ),
        ]),
    ];
    if blocked + failed > 0 {
        let parts: Vec<String> = [(blocked, "blocked"), (failed, "failed")]
            .into_iter()
            .filter(|&(n, _)| n > 0)
            .map(|(n, what)| format!("{n} {what}"))
            .collect();
        lines.push(Line::from(Span::styled(
            format!("⚠ {}", parts.join(" · ")),
            Style::default().fg(theme.warning),
        )));
    }
    Card {
        rows: lines.len() as u16,
        lines,
        width,
        color: theme.accent,
    }
}
