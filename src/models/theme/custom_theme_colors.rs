//! The 16 named color slots that make up a custom theme palette.

use serde::{Deserialize, Serialize};

/// The 16 named color slots that make up a theme palette, stored as `#rrggbb` hex strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomThemeColors {
    /// Accent color — used for focused borders and highlighted UI elements.
    pub accent: String,
    /// Link color — hyperlinks and interactive text.
    pub link: String,
    /// Success/positive indicator color.
    pub success: String,
    /// Section header / popup title color.
    #[serde(alias = "notice")]
    pub header: String,
    /// Main background color.
    pub bg: String,
    /// Dark background — tab bar, footer, chrome.
    pub bg_dark: String,
    /// Primary text color.
    pub text: String,
    /// Secondary/muted text color.
    #[serde(alias = "muted_text")]
    pub muted: String,
    /// Unfocused border color (structural only).
    pub border: String,
    /// Warning / unread count / fetch status indicator color.
    #[serde(alias = "unread")]
    pub warning: String,
    /// Code syntax foreground color.
    #[serde(alias = "teal")]
    pub code: String,
    /// Sky blue accent — article metadata, secondary article info.
    pub sky: String,
    /// Pink accent — used for category color cycling.
    pub pink: String,
    /// Error/destructive action color.
    pub error: String,
    /// List item selection background color.
    pub selection: String,
    /// Inline code / code block background color.
    pub code_bg: String,
}

impl CustomThemeColors {
    /// Get a color slot's hex value by index (0–15, matching `COLOR_SLOTS` order).
    pub fn get(&self, idx: usize) -> &str {
        match idx {
            0 => &self.accent,
            1 => &self.link,
            2 => &self.success,
            3 => &self.header,
            4 => &self.bg,
            5 => &self.bg_dark,
            6 => &self.text,
            7 => &self.muted,
            8 => &self.border,
            9 => &self.warning,
            10 => &self.code,
            11 => &self.sky,
            12 => &self.pink,
            13 => &self.error,
            14 => &self.selection,
            15 => &self.code_bg,
            _ => "#000000",
        }
    }

    /// Set a color slot by index. No-op for out-of-range indices.
    pub fn set(&mut self, idx: usize, hex: String) {
        match idx {
            0 => self.accent = hex,
            1 => self.link = hex,
            2 => self.success = hex,
            3 => self.header = hex,
            4 => self.bg = hex,
            5 => self.bg_dark = hex,
            6 => self.text = hex,
            7 => self.muted = hex,
            8 => self.border = hex,
            9 => self.warning = hex,
            10 => self.code = hex,
            11 => self.sky = hex,
            12 => self.pink = hex,
            13 => self.error = hex,
            14 => self.selection = hex,
            15 => self.code_bg = hex,
            _ => {}
        }
    }

    /// Serialize to TOML text compatible with `Theme::from_toml_str`.
    pub fn to_toml(&self, name: &str) -> String {
        format!(
            "name = \"{name}\"\n\n[colors]\naccent    = \"{accent}\"\nlink      = \"{link}\"\nsuccess   = \"{success}\"\nheader    = \"{header}\"\nbg        = \"{bg}\"\nbg_dark   = \"{bg_dark}\"\ntext      = \"{text}\"\nmuted     = \"{muted}\"\nborder    = \"{border}\"\nwarning   = \"{warning}\"\ncode      = \"{code}\"\nsky       = \"{sky}\"\npink      = \"{pink}\"\nerror     = \"{error}\"\nselection = \"{selection}\"\ncode_bg   = \"{code_bg}\"\n",
            name      = name,
            accent    = self.accent,
            link      = self.link,
            success   = self.success,
            header    = self.header,
            bg        = self.bg,
            bg_dark   = self.bg_dark,
            text      = self.text,
            muted     = self.muted,
            border    = self.border,
            warning   = self.warning,
            code      = self.code,
            sky       = self.sky,
            pink      = self.pink,
            error     = self.error,
            selection = self.selection,
            code_bg   = self.code_bg,
        )
    }
}
