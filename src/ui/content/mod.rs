//! Main content area rendering: three-panel layout (sidebar, article list, article detail) shared by multiple tabs.
//!
//! Submodules split the 1 000+ line content renderer by concern:
//! - `utils` — timestamp formatting, title truncation/scrolling, and age-color helpers
//! - `helpers` — article context resolution and archived/current split
//! - `sidebar` — feed/category tree sidebar
//! - `saved_sidebar` — saved-categories sidebar
//! - `article_list` — article list panel with archived sections
//! - `category_view` — category-level article preview list
//! - `article_detail` — full article body rendering with scrolling
//! - `footer` — unified one-row article/feed footer
//! - `layouts` — tab-level layout routing (Feeds tab, Saved tab, three-panel)

mod article_detail;
mod article_list;
mod category_view;
mod footer;
mod helpers;
mod layouts;
mod saved_sidebar;
mod sidebar;
pub(crate) mod utils;

pub(super) use article_detail::render_article_content;
pub(super) use layouts::{draw_feeds_tab, draw_saved_tab};
