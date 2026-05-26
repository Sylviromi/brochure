//! Theme definitions: color palette struct, built-in themes, and custom TOML loading.

use crate::models::{CustomTheme, CustomThemeColors};
use ratatui::style::Color;

/// Metadata for each color slot: `(field_name, short_label)` in the order used by
/// [`CustomThemeColors::get`] / [`CustomThemeColors::set`].
pub const COLOR_SLOTS: &[(&str, &str)] = &[
    ("accent",    "primary accent / focused border"),
    ("link",      "panel titles / interactive elements"),
    ("success",   "success / read indicator"),
    ("header",    "section headers / popup titles"),
    ("bg",        "main background"),
    ("bg_dark",   "darkest background / chrome"),
    ("text",      "primary foreground"),
    ("muted",     "secondary / muted text"),
    ("border",    "unfocused borders"),
    ("warning",   "unread counts / fetch status / warnings"),
    ("code",      "code syntax foreground"),
    ("sky",       "article metadata / dates"),
    ("pink",      "category palette accent"),
    ("error",     "errors / delete actions"),
    ("selection", "list item selection background"),
    ("code_bg",   "code block background"),
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
    (
        "catppuccin-latte",
        "Catppuccin Latte",
        include_str!("themes/catppuccin-latte.toml"),
    ),
    // ── Rosy / Purple ─────────────────────────────────────────────────────────
    (
        "rose-pine",
        "Rose Pine",
        include_str!("themes/rose-pine.toml"),
    ),
    (
        "rose-pine-dawn",
        "Rose Pine Dawn",
        include_str!("themes/rose-pine-dawn.toml"),
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
        "one-light",
        "One Light",
        include_str!("themes/one-light.toml"),
    ),
    // ── Arctic / Cool ─────────────────────────────────────────────────────────
    ("nord", "Nord", include_str!("themes/nord.toml")),
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
    // ── Classic ───────────────────────────────────────────────────────────────
    (
        "solarized-dark",
        "Solarized Dark",
        include_str!("themes/solarized-dark.toml"),
    ),
    (
        "solarized-light",
        "Solarized Light",
        include_str!("themes/solarized-light.toml"),
    ),
    (
        "monokai",
        "Monokai",
        include_str!("themes/monokai.toml"),
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
/// Every named slot maps to one semantic role. Built-in themes are loaded from
/// embedded TOML files; custom user themes are parsed via `from_toml_str`.
#[derive(Debug, Clone)]
pub struct ColorTheme {
    /// Theme display name (shown in the theme picker).
    pub name: String,
    /// Focused-border / primary accent.
    pub accent: Color,
    /// Panel titles / interactive elements.
    pub link: Color,
    /// Read-indicator / positive state.
    pub success: Color,
    /// Section headers / popup titles.
    pub header: Color,
    /// Main background.
    pub bg: Color,
    /// Darkest background (tab bar, footer, chrome).
    pub bg_dark: Color,
    /// Primary foreground text.
    pub text: Color,
    /// Secondary / muted text.
    pub muted: Color,
    /// Unfocused border (structural use only).
    pub border: Color,
    /// Unread counts / fetch status / warnings.
    pub warning: Color,
    /// Code syntax foreground.
    pub code: Color,
    /// Article metadata / dates.
    pub sky: Color,
    /// Category palette accent (cycling only).
    pub pink: Color,
    /// Error / delete action.
    pub error: Color,
    /// List item selection background.
    pub selection: Color,
    /// Inline code / code block background.
    pub code_bg: Color,
}

impl ColorTheme {
    /// Colors cycled by category ID in the sidebar (8-element fixed array).
    pub fn category_colors(&self) -> [Color; 8] {
        [
            self.accent,
            self.link,
            self.success,
            self.header,
            self.warning,
            self.code,
            self.sky,
            self.pink,
        ]
    }

    /// Returns a foreground color that contrasts against `bg_color`.
    ///
    /// Uses perceived luminance to decide between `bg` (light) and `bg_dark` (dark).
    /// Call this whenever text is rendered on a colored surface (selections, badges).
    pub fn contrast_color(&self, bg_color: Color) -> Color {
        let lum = match bg_color {
            Color::Rgb(r, g, b) => {
                let r = r as f32 / 255.0;
                let g = g as f32 / 255.0;
                let b = b as f32 / 255.0;
                0.2126 * r.powf(2.2) + 0.7152 * g.powf(2.2) + 0.0722 * b.powf(2.2)
            }
            _ => 0.0,
        };
        if lum > 0.35 { self.bg_dark } else { self.bg }
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

    /// Build a runtime `ColorTheme` from a stored [`CustomTheme`].
    pub fn from_custom_theme(ct: &CustomTheme) -> anyhow::Result<Self> {
        use anyhow::Context as _;
        let c = &ct.colors;
        let p = |key: &str, hex: &str| -> anyhow::Result<Color> {
            parse_hex(hex).with_context(|| format!("invalid hex for {key}: {hex}"))
        };
        Ok(Self {
            name:      ct.name.clone(),
            accent:    p("accent",    &c.accent)?,
            link:      p("link",      &c.link)?,
            success:   p("success",   &c.success)?,
            header:    p("header",    &c.header)?,
            bg:        p("bg",        &c.bg)?,
            bg_dark:   p("bg_dark",   &c.bg_dark)?,
            text:      p("text",      &c.text)?,
            muted:     p("muted",     &c.muted)?,
            border:    p("border",    &c.border)?,
            warning:   p("warning",   &c.warning)?,
            code:      p("code",      &c.code)?,
            sky:       p("sky",       &c.sky)?,
            pink:      p("pink",      &c.pink)?,
            error:     p("error",     &c.error)?,
            selection: p("selection", &c.selection)?,
            code_bg:   p("code_bg",   &c.code_bg)?,
        })
    }

    /// Convert this runtime theme into [`CustomThemeColors`] hex strings.
    pub fn to_custom_colors(&self) -> CustomThemeColors {
        CustomThemeColors {
            accent:    Self::color_to_hex(self.accent),
            link:      Self::color_to_hex(self.link),
            success:   Self::color_to_hex(self.success),
            header:    Self::color_to_hex(self.header),
            bg:        Self::color_to_hex(self.bg),
            bg_dark:   Self::color_to_hex(self.bg_dark),
            text:      Self::color_to_hex(self.text),
            muted:     Self::color_to_hex(self.muted),
            border:    Self::color_to_hex(self.border),
            warning:   Self::color_to_hex(self.warning),
            code:      Self::color_to_hex(self.code),
            sky:       Self::color_to_hex(self.sky),
            pink:      Self::color_to_hex(self.pink),
            error:     Self::color_to_hex(self.error),
            selection: Self::color_to_hex(self.selection),
            code_bg:   Self::color_to_hex(self.code_bg),
        }
    }

    /// Parse a theme from TOML source text.
    ///
    /// Accepts both new key names (`header`, `muted`, `warning`, `code`, `selection`, `code_bg`)
    /// and legacy aliases (`notice`, `muted_text`, `unread`, `teal`) for user theme compatibility.
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

        // Helper that tries the canonical key first, then a legacy alias.
        let parse_or = |key: &str, alias: &str| -> anyhow::Result<Color> {
            if colors.contains_key(key) { parse(key) } else { parse(alias) }
        };

        Ok(Self {
            name,
            accent:    parse("accent")?,
            link:      parse("link")?,
            success:   parse("success")?,
            header:    parse_or("header",    "notice")?,
            bg:        parse("bg")?,
            bg_dark:   parse("bg_dark")?,
            text:      parse("text")?,
            muted:     parse_or("muted",     "muted_text")?,
            border:    parse("border")?,
            warning:   parse_or("warning",   "unread")?,
            code:      parse_or("code",      "teal")?,
            sky:       parse("sky")?,
            pink:      parse("pink")?,
            error:     parse("error")?,
            // New slots: fall back to sensible defaults for old themes that lack them.
            selection: parse("selection").or_else(|_| parse("border"))?,
            code_bg:   parse("code_bg").or_else(|_| parse("bg_dark"))?,
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
    fn contrast_color_dark_bg_returns_light() {
        let t = ColorTheme::catppuccin_mocha();
        // Dark bg (#1e1e2e) → should return bg (light-ish in context of selection)
        let result = t.contrast_color(Color::Rgb(30, 30, 46));
        assert_eq!(result, t.bg);
    }

    #[test]
    fn contrast_color_light_bg_returns_dark() {
        let t = ColorTheme::catppuccin_mocha();
        // Light bg (white) → should return bg_dark (near-black)
        let result = t.contrast_color(Color::Rgb(255, 255, 255));
        assert_eq!(result, t.bg_dark);
    }

    #[test]
    fn legacy_toml_aliases_parse_correctly() {
        let src = r##"
name = "Legacy"
[colors]
accent     = "#cba6f7"
link       = "#89b4fa"
success    = "#a6e3a1"
notice     = "#fab387"
bg         = "#1e1e2e"
bg_dark    = "#181825"
text       = "#cdd6f4"
muted_text = "#a6adc8"
border     = "#313244"
unread     = "#f9e2af"
teal       = "#94e2d5"
sky        = "#89dceb"
pink       = "#f5c2e7"
error      = "#f38ba8"
"##;
        let t = ColorTheme::from_toml_str(src).unwrap();
        assert_eq!(t.name, "Legacy");
        assert_eq!(t.header,  Color::Rgb(250, 179, 135)); // was notice
        assert_eq!(t.muted,   Color::Rgb(166, 173, 200)); // was muted_text
        assert_eq!(t.warning, Color::Rgb(249, 226, 175)); // was unread
        assert_eq!(t.code,    Color::Rgb(148, 226, 213)); // was teal
        // selection and code_bg fall back to border/bg_dark when absent
        assert_eq!(t.selection, Color::Rgb(49, 50, 68));  // border
        assert_eq!(t.code_bg,   Color::Rgb(24, 24, 37));  // bg_dark
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
}
