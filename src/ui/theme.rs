//! Theme definitions: color palette struct, built-in themes, and custom TOML loading.

use crate::models::{CustomTheme, CustomThemeColors};
use ratatui::style::Color;

/// Metadata for each color slot: `(field_name, short_label)` in the order used by
/// [`CustomThemeColors::get`] / [`CustomThemeColors::set`].
pub const COLOR_SLOTS: &[(&str, &str)] = &[
    ("accent", "primary accent / focused border"),
    ("link", "links / highlights"),
    ("success", "success / read indicator"),
    ("notice", "section headers / warnings"),
    ("bg", "main background"),
    ("bg_dark", "darkest background"),
    ("text", "primary foreground"),
    ("muted_text", "secondary / muted text"),
    ("border", "unfocused borders"),
    ("unread", "warnings / stars / unread"),
    ("teal", "teal accent"),
    ("sky", "sky / lighter accent"),
    ("pink", "pink accent"),
    ("error", "errors / delete actions"),
];

/// Ordered list of `(slug, display_name, embedded TOML)` for every built-in theme.
///
/// Themes are grouped by family so they appear adjacent in the picker:
/// Catppuccin → Rosy/Purple → Blue/Navy → Arctic/Cool →
/// Warm/Earthy → Classic → Bold/Modern.
static BUILTIN_THEMES: &[(&str, &str, &str)] = &[
    // ── Catppuccin ────────────────────────────────────────────────────────────
    (
        "catppuccin-mocha",
        "Catppuccin Mocha",
        include_str!("themes/catppuccin-mocha.toml"),
    ),
    (
        "catppuccin-macchiato",
        "Catppuccin Macchiato",
        include_str!("themes/catppuccin-macchiato.toml"),
    ),
    // ── Rosy / Purple ─────────────────────────────────────────────────────────
    (
        "rose-pine",
        "Rose Pine",
        include_str!("themes/rose-pine.toml"),
    ),
    ("dracula", "Dracula", include_str!("themes/dracula.toml")),
    // ── Blue / Navy ───────────────────────────────────────────────────────────
    (
        "tokyo-night",
        "Tokyo Night",
        include_str!("themes/tokyo-night.toml"),
    ),
    (
        "tokyo-night-storm",
        "Tokyo Night Storm",
        include_str!("themes/tokyo-night-storm.toml"),
    ),
    (
        "one-dark",
        "One Dark",
        include_str!("themes/one-dark.toml"),
    ),
    (
        "nightfly",
        "Nightfly",
        include_str!("themes/nightfly.toml"),
    ),
    // ── Arctic / Cool ─────────────────────────────────────────────────────────
    ("nord", "Nord", include_str!("themes/nord.toml")),
    (
        "palenight",
        "Palenight",
        include_str!("themes/palenight.toml"),
    ),
    // ── Warm / Earthy ─────────────────────────────────────────────────────────
    (
        "gruvbox-dark",
        "Gruvbox Dark",
        include_str!("themes/gruvbox-dark.toml"),
    ),
    (
        "everforest",
        "Everforest",
        include_str!("themes/everforest.toml"),
    ),
    (
        "kanagawa-wave",
        "Kanagawa Wave",
        include_str!("themes/kanagawa-wave.toml"),
    ),
    (
        "ayu-dark",
        "Ayu Dark",
        include_str!("themes/ayu-dark.toml"),
    ),
    // ── Classic ───────────────────────────────────────────────────────────────
    (
        "solarized-dark",
        "Solarized Dark",
        include_str!("themes/solarized-dark.toml"),
    ),
    (
        "monokai",
        "Monokai",
        include_str!("themes/monokai.toml"),
    ),
    (
        "melange-dark",
        "Melange Dark",
        include_str!("themes/melange-dark.toml"),
    ),
    // ── Bold / Modern ─────────────────────────────────────────────────────────
    (
        "oxocarbon",
        "Oxocarbon",
        include_str!("themes/oxocarbon.toml"),
    ),
    (
        "synthwave-84",
        "Synthwave '84",
        include_str!("themes/synthwave-84.toml"),
    ),
    (
        "horizon",
        "Horizon",
        include_str!("themes/horizon.toml"),
    ),
];

/// Full color palette for the application UI.
///
/// Every named slot maps to one semantic role (e.g. `accent` = focused border,
/// `border` = unfocused border, `bg` = main background). Built-in constructors
/// return ready-to-use palettes; `from_toml_str` loads custom user themes.
#[derive(Debug, Clone)]
pub struct ColorTheme {
    /// Theme display name (shown in the theme picker).
    pub name: String,
    /// Focused-border / primary accent.
    pub accent: Color,
    /// Highlight.
    pub link: Color,
    /// Read-indicator / positive.
    pub success: Color,
    /// Section-header / warning.
    pub notice: Color,
    /// Main background.
    pub bg: Color,
    /// Darkest background (tab bar, footer).
    pub bg_dark: Color,
    /// Primary foreground text.
    pub text: Color,
    /// Secondary / muted text.
    pub muted_text: Color,
    /// Unfocused border / muted element.
    pub border: Color,
    /// Warning / star / unread accent.
    pub unread: Color,
    /// Teal accent variant.
    pub teal: Color,
    /// Sky / lighter blue accent.
    pub sky: Color,
    /// Pink accent variant.
    pub pink: Color,
    /// Error / delete action.
    pub error: Color,
}

impl ColorTheme {
    /// Colors cycled by category ID in the sidebar (8-element fixed array).
    pub fn category_colors(&self) -> [Color; 8] {
        [
            self.accent,
            self.link,
            self.success,
            self.notice,
            self.unread,
            self.teal,
            self.sky,
            self.pink,
        ]
    }

    /// Convenience alias used as the default fallback throughout the app.
    pub fn catppuccin_mocha() -> Self {
        Self::builtin("catppuccin-mocha").expect("catppuccin-mocha is always present")
    }

    /// Returns the built-in theme matching `slug`, or `None` if not found.
    pub fn builtin(slug: &str) -> Option<Self> {
        BUILTIN_THEMES
            .iter()
            .find(|(s, _, _)| *s == slug)
            .and_then(|(_, _, toml)| Self::from_toml_str(toml).ok())
    }

    /// Returns the slug (persisted key) for a built-in theme display name.
    pub fn slug(display_name: &str) -> &'static str {
        BUILTIN_THEMES
            .iter()
            .find(|(_, name, _)| *name == display_name)
            .map(|(slug, _, _)| *slug)
            .unwrap_or("custom")
    }

    /// All built-in theme display names, in picker order.
    pub fn builtin_names() -> Vec<&'static str> {
        BUILTIN_THEMES.iter().map(|(_, name, _)| *name).collect()
    }

    /// Convert a `Color::Rgb` value to a `#rrggbb` hex string.
    pub fn color_to_hex(color: Color) -> String {
        match color {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            _ => "#000000".to_string(),
        }
    }

    /// Build a runtime `Theme` from a stored [`CustomTheme`].
    pub fn from_custom_theme(ct: &CustomTheme) -> anyhow::Result<Self> {
        use anyhow::Context as _;
        let c = &ct.colors;
        let p = |key: &str, hex: &str| -> anyhow::Result<Color> {
            parse_hex(hex).with_context(|| format!("invalid hex for {key}: {hex}"))
        };
        Ok(Self {
            name: ct.name.clone(),
            accent: p("accent", &c.accent)?,
            link: p("link", &c.link)?,
            success: p("success", &c.success)?,
            notice: p("notice", &c.notice)?,
            bg: p("bg", &c.bg)?,
            bg_dark: p("bg_dark", &c.bg_dark)?,
            text: p("text", &c.text)?,
            muted_text: p("muted_text", &c.muted_text)?,
            border: p("border", &c.border)?,
            unread: p("unread", &c.unread)?,
            teal: p("teal", &c.teal)?,
            sky: p("sky", &c.sky)?,
            pink: p("pink", &c.pink)?,
            error: p("error", &c.error)?,
        })
    }

    /// Convert this runtime theme into [`CustomThemeColors`] hex strings.
    ///
    /// Used when cloning a built-in theme as the starting point for a new custom theme.
    pub fn to_custom_colors(&self) -> CustomThemeColors {
        CustomThemeColors {
            accent: Self::color_to_hex(self.accent),
            link: Self::color_to_hex(self.link),
            success: Self::color_to_hex(self.success),
            notice: Self::color_to_hex(self.notice),
            bg: Self::color_to_hex(self.bg),
            bg_dark: Self::color_to_hex(self.bg_dark),
            text: Self::color_to_hex(self.text),
            muted_text: Self::color_to_hex(self.muted_text),
            border: Self::color_to_hex(self.border),
            unread: Self::color_to_hex(self.unread),
            teal: Self::color_to_hex(self.teal),
            sky: Self::color_to_hex(self.sky),
            pink: Self::color_to_hex(self.pink),
            error: Self::color_to_hex(self.error),
        }
    }

    /// Parse a custom theme from TOML source text.
    ///
    /// The TOML format is:
    /// ```toml
    /// name = "My Theme"
    ///
    /// [colors]
    /// accent    = "#cba6f7"
    /// link      = "#89b4fa"
    /// success   = "#a6e3a1"
    /// notice    = "#fab387"
    /// bg        = "#1e1e2e"
    /// bg_dark   = "#181825"
    /// text      = "#cdd6f4"
    /// muted_text = "#a6adc8"
    /// border    = "#313244"
    /// unread    = "#f9e2af"
    /// teal      = "#94e2d5"
    /// sky       = "#89dceb"
    /// pink      = "#f5c2e7"
    /// error     = "#f38ba8"
    /// ```
    pub fn from_toml_str(src: &str) -> anyhow::Result<Self> {
        use anyhow::Context as _;

        let table: toml::Table = toml::from_str(src).context("invalid TOML")?;
        let name = table
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Custom")
            .to_string();
        let colors = table
            .get("colors")
            .and_then(|v| v.as_table())
            .context("[colors] table missing")?;

        let parse = |key: &str| -> anyhow::Result<Color> {
            let hex = colors
                .get(key)
                .and_then(|v| v.as_str())
                .with_context(|| format!("missing color: {key}"))?;
            parse_hex(hex).with_context(|| format!("invalid hex for {key}: {hex}"))
        };

        Ok(Self {
            name,
            accent: parse("accent")?,
            link: parse("link")?,
            success: parse("success")?,
            notice: parse("notice")?,
            bg: parse("bg")?,
            bg_dark: parse("bg_dark")?,
            text: parse("text")?,
            muted_text: parse("muted_text")?,
            border: parse("border")?,
            unread: parse("unread")?,
            teal: parse("teal")?,
            sky: parse("sky")?,
            pink: parse("pink")?,
            error: parse("error")?,
        })
    }
}

/// Parse a CSS hex color string (`#rrggbb`) into a `Color::Rgb`.
fn parse_hex(hex: &str) -> anyhow::Result<Color> {
    let hex = hex.trim_start_matches('#');
    anyhow::ensure!(hex.len() == 6, "expected 6 hex digits, got {}", hex.len());
    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;
    Ok(Color::Rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catppuccin_mocha_loads_correctly() {
        let t = ColorTheme::catppuccin_mocha();
        assert_eq!(t.accent, Color::Rgb(203, 166, 247));
        assert_eq!(t.name, "Catppuccin Mocha");
    }

    #[test]
    fn builtin_lookup_by_slug() {
        for (slug, _, _) in BUILTIN_THEMES {
            assert!(
                ColorTheme::builtin(slug).is_some(),
                "failed to load slug: {slug}"
            );
        }
        assert!(ColorTheme::builtin("unknown").is_none());
    }

    #[test]
    fn builtin_names_returns_all_20() {
        assert_eq!(ColorTheme::builtin_names().len(), 20);
    }

    #[test]
    fn slug_round_trips() {
        for (slug, name, _) in BUILTIN_THEMES {
            assert_eq!(ColorTheme::slug(name), *slug, "slug mismatch for {name}");
        }
        assert_eq!(ColorTheme::slug("Unknown Theme"), "custom");
    }

    #[test]
    fn category_colors_returns_8_entries() {
        let t = ColorTheme::catppuccin_mocha();
        assert_eq!(t.category_colors().len(), 8);
    }

    #[test]
    fn parse_hex_valid() {
        assert_eq!(parse_hex("#cba6f7").unwrap(), Color::Rgb(203, 166, 247));
        assert_eq!(parse_hex("cba6f7").unwrap(), Color::Rgb(203, 166, 247));
    }

    #[test]
    fn parse_hex_invalid_length() {
        assert!(parse_hex("#abc").is_err());
    }

    #[test]
    fn from_toml_str_valid() {
        let src = r##"
name = "Test"
[colors]
accent    = "#cba6f7"
link      = "#89b4fa"
success   = "#a6e3a1"
notice    = "#fab387"
bg        = "#1e1e2e"
bg_dark   = "#181825"
text      = "#cdd6f4"
muted_text = "#a6adc8"
border    = "#313244"
unread    = "#f9e2af"
teal      = "#94e2d5"
sky       = "#89dceb"
pink      = "#f5c2e7"
error     = "#f38ba8"
"##;
        let t = ColorTheme::from_toml_str(src).unwrap();
        assert_eq!(t.name, "Test");
        assert_eq!(t.accent, Color::Rgb(203, 166, 247));
    }

    #[test]
    fn from_toml_str_missing_color_errors() {
        let src = r##"
name = "Broken"
[colors]
accent = "#cba6f7"
"##;
        assert!(ColorTheme::from_toml_str(src).is_err());
    }
}
