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
    fn fields(&self) -> [&String; 16] {
        [
            &self.accent,
            &self.link,
            &self.success,
            &self.header,
            &self.bg,
            &self.bg_dark,
            &self.text,
            &self.muted,
            &self.border,
            &self.warning,
            &self.code,
            &self.sky,
            &self.pink,
            &self.error,
            &self.selection,
            &self.code_bg,
        ]
    }

    fn fields_mut(&mut self) -> [&mut String; 16] {
        [
            &mut self.accent,
            &mut self.link,
            &mut self.success,
            &mut self.header,
            &mut self.bg,
            &mut self.bg_dark,
            &mut self.text,
            &mut self.muted,
            &mut self.border,
            &mut self.warning,
            &mut self.code,
            &mut self.sky,
            &mut self.pink,
            &mut self.error,
            &mut self.selection,
            &mut self.code_bg,
        ]
    }

    /// Get a color slot's hex value by index (0–15, matching `COLOR_SLOTS` order).
    pub fn get(&self, idx: usize) -> &str {
        self.fields()
            .get(idx)
            .map(|s| s.as_str())
            .unwrap_or("#000000")
    }

    /// Set a color slot by index. No-op for out-of-range indices.
    pub fn set(&mut self, idx: usize, hex: String) {
        if let Some(field) = self.fields_mut().get_mut(idx) {
            **field = hex;
        }
    }

    /// Serialize to TOML text compatible with `Theme::from_toml_str`.
    pub fn to_toml(&self, name: &str) -> String {
        format!(
            "name = \"{name}\"\n\n[colors]\naccent    = \"{accent}\"\nlink      = \"{link}\"\nsuccess   = \"{success}\"\nheader    = \"{header}\"\nbg        = \"{bg}\"\nbg_dark   = \"{bg_dark}\"\ntext      = \"{text}\"\nmuted     = \"{muted}\"\nborder    = \"{border}\"\nwarning   = \"{warning}\"\ncode      = \"{code}\"\nsky       = \"{sky}\"\npink      = \"{pink}\"\nerror     = \"{error}\"\nselection = \"{selection}\"\ncode_bg   = \"{code_bg}\"\n",
            name = name,
            accent = self.accent,
            link = self.link,
            success = self.success,
            header = self.header,
            bg = self.bg,
            bg_dark = self.bg_dark,
            text = self.text,
            muted = self.muted,
            border = self.border,
            warning = self.warning,
            code = self.code,
            sky = self.sky,
            pink = self.pink,
            error = self.error,
            selection = self.selection,
            code_bg = self.code_bg,
        )
    }
}
