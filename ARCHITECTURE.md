/# ARCHITECTURE.md — rssr design reference

Human-readable reference for the rssr codebase. Keep this up to date when design details change.

---

## Project Overview

**rssr** is a terminal RSS reader written in Rust.

| Crate / Library    | Role                                       |
|--------------------|--------------------------------------------|
| `ratatui`          | TUI rendering                              |
| `tokio`            | Async runtime (multi-threaded)             |
| `crossterm`        | Terminal input / raw mode                  |
| `feed-rs`          | RSS/Atom parsing                           |
| `reqwest`          | HTTP client                                |
| `readability`      | Article extraction (Mozilla algorithm)     |
| `html2md`          | HTML → Markdown conversion                 |
| `tui-markdown`     | Markdown → Ratatui text                    |
| `ratatui-image`    | Hero image rendering (Halfblocks protocol) |
| `serde/serde_json` | Serialization                              |
| `dirs`             | Platform data directory                    |
| `quick-xml`        | OPML parsing                               |
| `regex`            | Strip markdown link syntax before render   |
| `anyhow`           | Error handling                             |
| `open`             | Open URLs in system browser                |

Entry point: `src/main.rs` bootstraps the terminal, starts the Tokio runtime, and runs the MPSC event loop.

---

## App Struct

`src/app.rs` — central state container. Passed by `&mut` to every handler and draw call.

> **Note:** read `src/app.rs` directly for the current field list; this table may lag the code.

| Field                      | Type                        | Purpose                                           |
|----------------------------|-----------------------------|---------------------------------------------------|
| `state`                    | `AppState`                  | Current navigation state                          |
| `feeds`                    | `Vec<Feed>`                 | All feeds (excluding virtual Favorites)           |
| `selected_feed`            | `usize`                     | Index into `feeds`                                |
| `selected_article`         | `usize`                     | Index into current feed's articles                |
| `status_msg`               | `String`                    | Footer status line                                |
| `input`                    | `String`                    | Text buffer for AddFeed modal                     |
| `user_data`                | `UserData`                  | Persistent read/starred state                     |
| `settings_selected`        | `SettingsItem`              | Selected item in settings menu                    |
| `image_cache`              | `HashMap<String, Protocol>` | Cached hero images by URL                         |
| `fetching_images`          | `HashSet<String>`           | Image URLs currently in-flight                    |
| `scroll_offset`            | `u16`                       | Vertical scroll position in ArticleDetail         |
| `content_line_count`       | `usize`                     | Line count of current article (for scroll cap)    |
| `selected_tab`             | `Tab`                       | Active tab (Feeds / Favorites / Settings)         |
| `favorites_sidebar_cursor` | `usize`                     | Cursor in favorites sidebar                       |
| `favorite_view_articles`   | `Vec<Article>`              | Articles shown when browsing a favorites sub-feed |
| `in_favorites_context`     | `bool`                      | True while ArticleList/Detail shows favorites     |
| `add_feed_step`            | `AddFeedStep`               | Which step of the two-step add-feed flow          |
| `add_feed_url`             | `String`                    | URL captured in step 1                            |
| `add_feed_fetched_title`   | `Option<String>`            | Auto-fetched title placeholder for step 2         |
| `add_feed_return_state`    | `AppState`                  | Where to return after AddFeed completes           |
| `add_feed_target_category` | `Option<CategoryId>`        | Category to place the new feed in                 |
| `opml_path_input`          | `String`                    | File path typed in OPML export/import screens     |
| `feeds_total`              | `usize`                     | Total feeds in the current fetch batch            |
| `feeds_pending`            | `usize`                     | Feeds still awaiting a result                     |
| `categories`               | `Vec<Category>`             | All category nodes                                |
| `sidebar_collapsed`        | `HashSet<CategoryId>`       | Categories collapsed in the sidebar               |
| `sidebar_cursor`           | `usize`                     | Cursor into the flattened sidebar tree            |
| `editor_cursor`            | `usize`                     | Cursor inside the feed editor                     |
| `editor_collapsed`         | `HashSet<CategoryId>`       | Categories collapsed in the feed editor           |
| `editor_mode`              | `FeedEditorMode`            | Normal / Moving / Renaming / NewCategory          |
| `editor_input`             | `String`                    | Text buffer for rename/new-category input         |

---

## Models

### `models/core_types.rs`

```rust
Feed    { title, url, category_id, order, unread_count*, articles*, fetched*, fetch_error* }
Article { title, description, link, is_read, is_starred, content, image_url, source_feed }
UserData { read_links: HashSet<String>, starred_articles: Vec<Article>, save_article_content }
```

`*` = `#[serde(skip)]` — runtime only, not persisted.

### `models/navigation.rs`

```
AppState: FeedList | ArticleList | ArticleDetail | AddFeed | SettingsList
          FavoriteFeedList | OPMLExportPath | OPMLImportPath | ClearData
          FeedEditor | FeedEditorRename

Tab:          Feeds | Favorites | Settings
SettingsItem: ImportOpml | ExportOpml | ClearData | SaveArticleContent
AddFeedStep:  Url | Title
FeedSource:   Feed(usize) | Favorites
```

### `models/events.rs`

```rust
AppEvent::Input(KeyEvent)
AppEvent::Tick
AppEvent::FeedFetched(feed_idx: usize, Result<Vec<Article>, String>)
AppEvent::ImageFetched(url: String, Result<Vec<u8>, String>)
AppEvent::FullArticleFetched(FeedSource, art_idx: usize, Result<String, String>)
AppEvent::FeedTitleFetched(Result<String, String>)
```

### `models/mod.rs` — tree types

```rust
CategoryId = u64
Category   { id, name, parent_id: Option<CategoryId>, order }
FeedTreeItem::Category { id, depth, collapsed } | Feed { feeds_idx, depth }
FeedEditorMode: Normal | Moving { origin_render_idx } | Renaming { render_idx } | NewCategory
```

Constants: `FAVORITES_URL = "internal:favorites"`, `CONTENT_STUB_MAX_LEN = 500`

---

## Event Loop (MPSC)

**Never `.await` on the main event loop thread. Never block the UI.**

```
handlers/*.rs                  tokio::spawn (background thread)
    │                               │
    │  tokio::spawn(async { ... })  │  fetch_feed / fetch_image_bytes / fetch_readable_content
    └──────────────────────────────►│
                                    │  tx.send(AppEvent::FeedFetched(...))
                                    │  tx.send(AppEvent::ImageFetched(...))
                                    │  tx.send(AppEvent::FullArticleFetched(...))
                                    ▼
                         main.rs match loop receives, updates app state
```

---

## Navigation State Machine

```
Feeds tab                          Favorites tab         Settings tab
─────────────────────────────────  ──────────────────    ────────────────
FeedList                           FavoriteFeedList       SettingsList
  ├─ Enter (on feed) ──────────►  ArticleList              ├─ Enter ──► OPMLImportPath
  │    └─ Enter ──────────────►  ArticleDetail             ├─ Enter ──► OPMLExportPath
  │                                                        ├─ Enter ──► ClearData
  └─ [e] ─────────────────────►  FeedEditor               └─ Enter ──► (toggle SaveArticleContent)
         └─ [n]/[r] ──────────►  FeedEditorRename
         └─ [a] ──────────────►  AddFeed
```

Tab switching (`Tab` / `Shift+Tab`) works from any state and resets to the tab's root state.

State transition table:

| From             | To               | Trigger                                                     |
|------------------|------------------|-------------------------------------------------------------|
| FeedList         | ArticleList      | Enter on a feed                                             |
| ArticleList      | ArticleDetail    | Enter on an article (also spawns image + readability fetch) |
| ArticleDetail    | ArticleList      | Esc                                                         |
| ArticleList      | FeedList         | Esc                                                         |
| FavoriteFeedList | ArticleList      | Enter on a feed group                                       |
| ArticleList      | FavoriteFeedList | Esc (when in_favorites_context)                             |
| FeedList         | FeedEditor       | `e`                                                         |
| FeedEditor       | FeedEditorRename | `n` (new category) or `r` (rename)                          |
| FeedEditor       | AddFeed          | `a`                                                         |
| FeedEditorRename | FeedEditor       | Enter or Esc                                                |
| AddFeed          | (return state)   | Enter (saves feed) or Esc                                   |
| SettingsList     | OPMLImportPath   | Enter on Import OPML                                        |
| SettingsList     | OPMLExportPath   | Enter on Export OPML                                        |
| SettingsList     | ClearData        | Enter on Clear All Data                                     |
| OPMLExportPath   | SettingsList     | Enter (exports) or Esc                                      |
| OPMLImportPath   | SettingsList     | Enter (imports) or Esc                                      |
| ClearData        | SettingsList     | Enter (deletes all) or Esc                                  |

---

## Keybindings

| Key            | Context                                                           | Action                               |
|----------------|-------------------------------------------------------------------|--------------------------------------|
| `↓` / `j`      | FeedList, ArticleList, SettingsList, FeedEditor, FavoriteFeedList | Next item                            |
| `↑` / `k`      | FeedList, ArticleList, SettingsList, FeedEditor, FavoriteFeedList | Prev item                            |
| `↓`            | ArticleDetail                                                     | Scroll down                          |
| `↑`            | ArticleDetail                                                     | Scroll up                            |
| `Enter`        | FeedList                                                          | Open feed / toggle category collapse |
| `Enter`        | ArticleList                                                       | Open article (marks read, fetches)   |
| `Enter`        | SettingsList                                                      | Execute selected setting             |
| `Enter`        | FeedEditor (Normal)                                               | Toggle collapse / start move         |
| `Enter`        | FeedEditor (Moving)                                               | Drop item at cursor                  |
| `Enter`        | FeedEditorRename                                                  | Confirm rename / new category        |
| `Enter`        | AddFeed / OPMLExportPath / OPMLImportPath                         | Confirm                              |
| `Esc`          | Any                                                               | Go back one level                    |
| `Tab`          | Any                                                               | Switch tab right                     |
| `Shift+Tab`    | Any                                                               | Switch tab left                      |
| `q`            | FeedList, ArticleList, ArticleDetail, FeedEditor                  | Quit                                 |
| `r`            | FeedList, ArticleList                                             | Refresh current feed                 |
| `e`            | FeedList                                                          | Open feed editor                     |
| `m`            | ArticleList, ArticleDetail                                        | Toggle read/unread                   |
| `s`            | ArticleList, ArticleDetail                                        | Toggle starred                       |
| `o`            | ArticleDetail                                                     | Open link in system browser          |
| `a`            | FeedEditor                                                        | Add feed (in cursor's category)      |
| `n`            | FeedEditor                                                        | New category                         |
| `r`            | FeedEditor                                                        | Rename selected item                 |
| `d`            | FeedEditor                                                        | Delete selected item                 |
| `m`            | FeedEditor                                                        | Start move mode                      |
| Char/Backspace | AddFeed, FeedEditorRename, path inputs                            | Edit text input                      |

---

## Styling — Catppuccin Mocha

Color constants live in `src/ui/mod.rs`. **Use constants, never hardcode `Color::Rgb(...)`.**

| Constant   | RGB             | Use                                               |
|------------|-----------------|---------------------------------------------------|
| `MAUVE`    | (203, 166, 247) | Active borders, article title, selected text      |
| `BLUE`     | (137, 180, 250) | Panel titles, read-dot icon                       |
| `GREEN`    | (166, 227, 161) | Status messages, success, fetch spinner           |
| `PEACH`    | (250, 179, 135) | Feed list bullet, settings section headers        |
| `YELLOW`   | (249, 226, 175) | Starred icons, unread badges, scroll %, move mode |
| `TEAL`     | (148, 226, 213) | Category palette rotation                         |
| `SKY`      | (137, 220, 235) | Category palette rotation                         |
| `PINK`     | (245, 194, 231) | Category palette rotation                         |
| `BASE`     | (30, 30, 46)    | Panel backgrounds                                 |
| `MANTLE`   | (24, 24, 37)    | Tab bar / footer background, selected-on-color fg |
| `TEXT`     | (205, 214, 244) | Body text                                         |
| `SUBTEXT0` | (166, 173, 200) | Read articles, hints, dimmed text                 |
| `SURFACE0` | (49, 50, 68)    | Selected item highlight bg, inactive borders      |

**Exception:** Danger actions (confirm-delete popup) use Catppuccin Red `Color::Rgb(243, 139, 168)` directly —
intentional, not a missing constant.

Category headers cycle through `CATEGORY_COLORS = [MAUVE, BLUE, GREEN, PEACH, YELLOW, TEAL, SKY, PINK]` by
`category.id % len`.

---

## Data Storage

All persistent data lives under the platform data directory (via the `dirs` crate):

| Platform  | Path                                  |
|-----------|---------------------------------------|
| Linux/BSD | `~/.local/share/rssr/`                |
| macOS     | `~/Library/Application Support/rssr/` |
| Windows   | `%APPDATA%\rssr\`                     |

| File              | Purpose                                    |
|-------------------|--------------------------------------------|
| `feeds.json`      | Feed list (title, url, category_id, order) |
| `user_data.json`  | Read links + starred articles + settings   |
| `articles.json`   | Article cache (url → articles map)         |
| `categories.json` | Category tree                              |

`storage::data_dir()` creates the directory on first use. Path helpers are private — callers never construct paths
directly.

Article cache stripping: when `save_article_content` is false, `content` and `image_url` are cleared before saving to
keep the cache small.

---

## Key Behaviors

### Readability fetch heuristic

When an article is opened and `article.content.len() < 500` (`CONTENT_STUB_MAX_LEN`), a `fetch_readable_content` task is
spawned. The article content is immediately set to `"⏳ Fetching full article, please wait..."` until
`AppEvent::FullArticleFetched` arrives.

### Favorites

Favorites are backed by `user_data.starred_articles`, not a virtual feed in `app.feeds`.

- `FavoriteFeedList` state shows one sidebar row per unique `article.source_feed` value.
- Entering a source-feed row populates `app.favorite_view_articles` and sets `app.in_favorites_context = true`.
- `article.source_feed` is set to the feed title at fetch time (in `on_feed_fetched`, `main.rs`).
- `in_favorites_context` is cleared when backing out to `FavoriteFeedList` or switching tabs.
- `FullArticleFetched` with `FeedSource::Favorites` means the article was opened from favorites context.

### Feed tree / categories

Feeds and categories form a tree. `visible_tree_items` in `app.rs` flattens the tree into a render list, respecting
`collapsed` state. Uncategorized feeds appear after all categories. The favorites sidebar uses
`visible_favorites_tree_items`, which filters to only feeds that have at least one starred article.

### AddFeed two-step flow

1. **Url step** — user types a URL; on Enter, a background `fetch_feed_title` task is spawned and step advances to
   Title.
2. **Title step** — fetched title shown as grey placeholder; user can override. Enter saves the feed and kicks off the
   first fetch.

`add_feed_return_state` tracks where to return (SettingsList or FeedEditor).