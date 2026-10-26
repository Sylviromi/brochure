//! Full-screen view of an article image, scaled to fill the screen. Small images
//! gain the most: half-block terminals get many more cells to draw them with.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect, Size},
    prelude::Stylize,
    text::Line,
    widgets::{Block, Clear, Paragraph},
};
use ratatui_image::{FilterType, Resize, StatefulImage};

use super::SPINNER_FRAMES;
use crate::app::App;

pub(super) fn draw_zoom(f: &mut Frame, app: &mut App) {
    let App {
        zoom, theme, tick, ..
    } = app;
    let Some(zoom) = zoom.as_mut() else {
        return;
    };
    let screen = f.area();
    f.render_widget(Clear, screen);
    f.render_widget(Block::default().bg(theme.bg), screen);
    let [body, hint] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(screen);

    let message = match (&mut zoom.image, zoom.failed) {
        (Some(image), _) => {
            let resize = Resize::Scale(Some(FilterType::Triangle));
            let size = image.size_for(resize.clone(), body.as_size());
            f.render_stateful_widget(
                StatefulImage::default().resize(resize),
                centered(body, size),
                image,
            );
            None
        }
        (None, true) => Some(Line::from("⚠ Couldn't load this image".fg(theme.warning))),
        (None, false) => {
            let spinner = SPINNER_FRAMES[*tick % SPINNER_FRAMES.len()];
            Some(Line::from(vec![
                spinner.fg(theme.accent),
                " Loading full-size image…".fg(theme.muted),
            ]))
        }
    };
    if let Some(line) = message {
        let row = Rect {
            y: body.y + body.height / 2,
            height: 1,
            ..body
        };
        f.render_widget(Paragraph::new(line).centered(), row);
    }
    f.render_widget(
        Paragraph::new("Click or press any key to close".fg(theme.muted)).centered(),
        hint,
    );
}

/// A `size` rect centred in `area`.
fn centered(area: Rect, size: Size) -> Rect {
    let width = size.width.min(area.width);
    let height = size.height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
