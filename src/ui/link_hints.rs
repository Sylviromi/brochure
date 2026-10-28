//! Link hint overlay: dims the article, badges each link/image with its label and
//! explains the keys in a bar along the bottom.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    app::App,
    state::link_hints::{HintTarget, hint_targets},
};

/// Draw the hints over the article text in `area` (no-op when hints are off).
pub(crate) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(hints) = app.link_hints.as_ref().filter(|_| !area.is_empty()) else {
        return;
    };
    let (theme, hit) = (&app.theme, &app.hit);
    let screen = f.area();
    let buf = f.buffer_mut();

    // Image cells stay untouched: each row's first cell carries the escape that draws it.
    let on = |rects: &[(Rect, String)], x: u16, y: u16| {
        rects.iter().any(|(r, _)| r.contains((x, y).into()))
    };
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if !on(&hit.article_links, x, y) && !on(&hit.article_images, x, y) {
                buf[(x, y)].set_fg(theme.muted);
            }
        }
    }

    // Badges end just before their target, so may sit on the border: clip to the screen.
    let mut overlays = Vec::new();
    for (label, rect, target) in &hint_targets(hit) {
        let Some(rest) = label.strip_prefix(hints.typed.as_str()) else {
            continue;
        };
        let bg = match target {
            HintTarget::Link(_) => theme.accent,
            HintTarget::Image(_) => theme.pink,
        };
        let x = rect.x.saturating_sub(rest.len() as u16);
        let text = rest.to_uppercase();
        overlays.push(badge(buf, screen, x, rect.y, &text, bg, theme.bg_dark));
    }

    let (key, verb) = if hints.copy {
        ("F", "copy")
    } else {
        ("f", "open")
    };
    let bar = Line::from(vec![
        Span::styled(" ", Style::new().bg(theme.bg_dark)),
        Span::styled(
            format!(" {key} "),
            Style::new().fg(theme.bg_dark).bg(theme.accent).bold(),
        ),
        Span::styled(
            format!(" Type a letter to {verb} it  "),
            Style::new().fg(theme.text).bg(theme.bg_dark),
        ),
        Span::styled(" link ", Style::new().fg(theme.bg_dark).bg(theme.accent)),
        Span::styled(" ", Style::new().bg(theme.bg_dark)),
        Span::styled(" image ", Style::new().fg(theme.bg_dark).bg(theme.pink)),
        Span::styled(
            "  Esc cancel ",
            Style::new().fg(theme.muted).bg(theme.bg_dark),
        ),
    ]);
    let width = (bar.width() as u16).min(area.width);
    let bar_area = Rect::new(
        area.x + (area.width - width) / 2,
        area.bottom().saturating_sub(1),
        width,
        1,
    );
    buf.set_line(bar_area.x, bar_area.y, &bar, width);
    overlays.push(bar_area);
    app.hit.overlays.extend(overlays);
}

/// Write `text` as a badge at `(x, y)`, clipped to `clip`; returns the cells it covers.
fn badge(buf: &mut Buffer, clip: Rect, x: u16, y: u16, text: &str, bg: Color, fg: Color) -> Rect {
    let width = clip.right().saturating_sub(x);
    let style = Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD);
    let (end, _) = buf.set_stringn(x, y, text, width as usize, style);
    Rect::new(x, y, end - x, 1).intersection(clip)
}
