//! Toast notifications: short messages stacked in a corner of the screen, and the
//! settings that control them.

use serde::{Deserialize, Serialize};

/// Most toasts remembered; the "Max shown" setting goes up to this.
pub const MAX_TOASTS: u8 = 5;

/// How long a toast can stay up, in seconds (the "Duration" setting cycles these).
pub const DURATIONS: [u8; 4] = [2, 4, 8, 15];

/// Screen corner the toasts stack in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum ToastPosition {
    #[default]
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

impl ToastPosition {
    const ALL: [Self; 4] = [
        Self::BottomRight,
        Self::BottomLeft,
        Self::TopRight,
        Self::TopLeft,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::BottomRight => "Bottom right",
            Self::BottomLeft => "Bottom left",
            Self::TopRight => "Top right",
            Self::TopLeft => "Top left",
        }
    }

    pub fn is_top(self) -> bool {
        matches!(self, Self::TopRight | Self::TopLeft)
    }

    pub fn is_left(self) -> bool {
        matches!(self, Self::BottomLeft | Self::TopLeft)
    }

    /// The next (or previous) corner, wrapping around.
    pub fn cycle(self, forward: bool) -> Self {
        Self::ALL[cycle_index(&Self::ALL, &self, forward)]
    }
}

/// Toast preferences (persisted in `UserData`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ToastSettings {
    pub position: ToastPosition,
    /// Seconds each toast stays on screen.
    pub seconds: u8,
    /// Most toasts shown at once (the live refresh toast doesn't count).
    pub max_visible: u8,
    /// Show a live toast with progress and failure counts while feeds refresh.
    pub refresh_progress: bool,
}

impl Default for ToastSettings {
    fn default() -> Self {
        Self {
            position: ToastPosition::default(),
            seconds: 4,
            max_visible: 3,
            refresh_progress: true,
        }
    }
}

impl ToastSettings {
    pub fn cycle_seconds(&mut self, forward: bool) {
        self.seconds = DURATIONS[cycle_index(&DURATIONS, &self.seconds, forward)];
    }

    pub fn cycle_max_visible(&mut self, forward: bool) {
        let n = self.max_visible.clamp(1, MAX_TOASTS);
        self.max_visible = if forward {
            n % MAX_TOASTS + 1
        } else {
            (n + MAX_TOASTS - 2) % MAX_TOASTS + 1
        };
    }
}

/// Index after (or before) `current` in `all`, wrapping; unknown values restart at 0.
fn cycle_index<T: PartialEq>(all: &[T], current: &T, forward: bool) -> usize {
    match all.iter().position(|v| v == current) {
        Some(i) if forward => (i + 1) % all.len(),
        Some(i) => (i + all.len() - 1) % all.len(),
        None => 0,
    }
}

/// A toast and the tick it appeared at.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub msg: String,
    pub tick: usize,
}

/// Recent toasts, oldest first.
#[derive(Debug, Default)]
pub struct Toasts {
    items: Vec<Toast>,
}

impl Toasts {
    /// Add `msg` (ignored when empty). Repeating a message moves it to the newest
    /// slot instead of stacking duplicates.
    pub fn push(&mut self, msg: String, tick: usize) {
        if msg.is_empty() {
            return;
        }
        self.items.retain(|t| t.msg != msg);
        self.items.push(Toast { msg, tick });
        let excess = self.items.len().saturating_sub(usize::from(MAX_TOASTS));
        self.items.drain(..excess);
    }

    /// Up to `max` toasts younger than `ttl` ticks at `tick`, newest first.
    pub fn visible(&self, tick: usize, ttl: usize, max: usize) -> impl Iterator<Item = &Toast> {
        self.items
            .iter()
            .rev()
            .filter(move |t| tick.saturating_sub(t.tick) < ttl)
            .take(max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toasts_dedupe_cap_and_expire() {
        let mut t = Toasts::default();
        for (i, m) in ["a", "b", "a", "", "c", "d", "e", "f"].iter().enumerate() {
            t.push(m.to_string(), i);
        }
        let msgs = |t: &Toasts, tick, max| -> Vec<String> {
            t.visible(tick, 4, max).map(|t| t.msg.clone()).collect()
        };
        // "a" was re-pushed, "" ignored; capped at 5 kept, newest first.
        assert_eq!(msgs(&t, 7, 5), ["f", "e", "d", "c"]);
        assert_eq!(msgs(&t, 7, 2), ["f", "e"]);
        assert_eq!(msgs(&t, 9, 5), ["f", "e"]);
        assert!(msgs(&t, 20, 5).is_empty());
    }

    #[test]
    fn settings_cycle_both_ways() {
        let mut s = ToastSettings::default();
        s.cycle_seconds(true);
        assert_eq!(s.seconds, 8);
        s.cycle_seconds(false);
        s.cycle_seconds(false);
        assert_eq!(s.seconds, 2);
        s.max_visible = 5;
        s.cycle_max_visible(true);
        assert_eq!(s.max_visible, 1);
        s.cycle_max_visible(false);
        assert_eq!(s.max_visible, 5);
        assert_eq!(
            ToastPosition::BottomRight.cycle(false),
            ToastPosition::TopLeft
        );
    }
}
