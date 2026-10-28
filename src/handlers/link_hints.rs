//! Keys while link hints are shown: typing a label clicks (or copies) its target.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::App,
    models::AppEvent,
    state::{
        LinkHints,
        link_hints::{HintTarget, hint_targets},
    },
};

/// Show hints over the visible links and images (`copy`: `F`, copy instead of click).
pub(super) fn start(app: &mut App, copy: bool) {
    if hint_targets(&app.hit).is_empty() {
        app.notify("No links or images on screen");
    } else {
        app.link_hints = Some(LinkHints {
            copy,
            typed: String::new(),
        });
    }
}

/// Narrow the hints by the typed letter; a full label acts, anything else cancels.
pub(super) fn handle_key(app: &mut App, key: KeyEvent, tx: &UnboundedSender<AppEvent>) {
    let Some(hints) = app.link_hints.as_mut() else {
        return;
    };
    match key.code {
        // Key repeat from holding the `f` that opened the hints (terminals don't flag repeats).
        KeyCode::Char('f' | 'F') => return,
        KeyCode::Char(c) => hints.typed.push(c.to_ascii_lowercase()),
        KeyCode::Backspace if !hints.typed.is_empty() => {
            hints.typed.pop();
            return;
        }
        _ => {
            app.link_hints = None;
            return;
        }
    }
    let (typed, copy) = (hints.typed.clone(), hints.copy);
    let targets = hint_targets(&app.hit);
    let mut matching = targets.iter().filter(|(l, ..)| l.starts_with(&typed));
    let (is_link, url) = match (matching.next(), matching.next()) {
        (Some((_, _, HintTarget::Link(url))), None) => (true, url.to_string()),
        (Some((_, _, HintTarget::Image(url))), None) => (false, url.to_string()),
        (None, _) => {
            app.link_hints = None; // a typo: give up rather than trap the keys
            return;
        }
        _ => return, // several labels still match: wait for the next letter
    };
    app.link_hints = None;
    match (is_link, copy) {
        (true, false) => super::mouse::open_link(app, &url),
        (true, true) => super::article::copy_with_status(app, &url, "Copied link"),
        (false, false) => super::mouse::open_zoom(app, url, tx),
        (false, true) => {
            app.notify("Copying image…");
            crate::events::spawn_image_copy(url, tx);
        }
    }
}
