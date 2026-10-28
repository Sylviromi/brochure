//! Link hint overlay: dims the article, badges each link/image with its label and
//! explains the keys in a bar along the bottom.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    app::App,
    state::link_hints::{HintTarget, hint_targets},
    ui::theme::ColorTheme,
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

    let rounded = app.user_data.border_rounded;
    overlays.push(key_bar(buf, area, screen, hints.copy, theme, rounded));
    app.hit.overlays.extend(overlays);
}

/// Bordered tab centred on the bottom of `area` explaining the keys. Its sides join the
/// panel's bottom line when there is one below, else it closes itself. Returns its rect.
fn key_bar(
    buf: &mut Buffer,
    area: Rect,
    screen: Rect,
    copy: bool,
    theme: &ColorTheme,
    rounded: bool,
) -> Rect {
    let (key, verb) = if copy { ("F", "copy") } else { ("f", "open") };
    let chip = |text: String, bg: Color| Span::styled(text, Style::new().fg(theme.bg_dark).bg(bg));
    let bar = Line::from(vec![
        Span::raw(" "),
        chip(format!(" {key} "), theme.accent).bold(),
        Span::styled(format!(" Type a letter to {verb} it  "), theme.text),
        chip(" link ".into(), theme.accent),
        Span::raw(" "),
        chip(" image ".into(), theme.pink),
        Span::styled("  Esc cancel ", theme.muted),
    ])
    .bg(theme.bg);

    if area.width < 3 {
        return Rect::default();
    }
    let set = crate::ui::border_set(rounded);
    let inner = (bar.width() as u16).min(area.width.saturating_sub(2));
    let left = area.x + area.width.saturating_sub(inner + 2) / 2;
    let right = left + inner + 1;
    let text_y = area.bottom().saturating_sub(1);
    let top_y = text_y.saturating_sub(1).max(area.y);
    let below = area.bottom();
    // Join a horizontal border below (the panel's own); copy its colour so focus shows.
    let joins = buf
        .cell((left, below))
        .is_some_and(|c| c.symbol() == set.horizontal_bottom);
    let line = if joins {
        buf[(left, below)].style()
    } else {
        Style::new().fg(theme.border).bg(theme.bg)
    };

    if top_y < text_y {
        let top = format!(
            "{}{}{}",
            set.top_left,
            set.horizontal_top.repeat(inner as usize),
            set.top_right
        );
        buf.set_string(left, top_y, top, line);
    }
    buf.set_string(left, text_y, set.vertical_left, line);
    buf.set_line(left + 1, text_y, &bar, inner);
    buf.set_string(right, text_y, set.vertical_right, line);
    let bottom_y = if joins {
        buf.set_string(left, below, "┴", line);
        buf.set_string(right, below, "┴", line);
        below
    } else if below < screen.bottom() {
        let bottom = format!(
            "{}{}{}",
            set.bottom_left,
            set.horizontal_bottom.repeat(inner as usize),
            set.bottom_right
        );
        buf.set_string(left, below, bottom, line);
        below
    } else {
        text_y
    };
    Rect::new(left, top_y, inner + 2, bottom_y - top_y + 1)
}

/// Write `text` as a badge at `(x, y)`, clipped to `clip`; returns the cells it covers.
fn badge(buf: &mut Buffer, clip: Rect, x: u16, y: u16, text: &str, bg: Color, fg: Color) -> Rect {
    let width = clip.right().saturating_sub(x);
    let style = Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD);
    let (end, _) = buf.set_stringn(x, y, text, width as usize, style);
    Rect::new(x, y, end - x, 1).intersection(clip)
}
