# Brochure architecture

How the code is laid out and the patterns it relies on. Read the relevant pattern
before changing that area.

## Modules

| Path | Responsibility |
|------|----------------|
| `src/main.rs` | Terminal setup/restore (incl. panic hook), input thread, event loop |
| `src/events.rs` | Applies background results (feed fetch, article body, images) to `App`; startup cache load + initial fetch |
| `src/app/` | `App` state. `navigation` (tabs, cursors, views), `editor` (feed editor ops), `tree` (category/feed tree flattening), `freshrss` (refresh routing, sync, push) |
| `src/handlers/` | Input → state changes. One module per screen; `mouse.rs` maps clicks/wheel onto the key handlers (`mouse/editor_drag.rs`: feed editor drag-and-drop) |
| `src/ui/` | Rendering only. `content/` holds the Feeds/Saved tab panels |
| `src/models/` | Plain data types, enums, `AppEvent`, `HitAreas` |
| `src/state/` | Per-screen sub-state structs owned by `App` |
| `src/sync/` | FreshRSS: `freshrss` (pure GReader parsing/mapping/category placement), `client` (HTTP), `apply` (results → `App`), `secret` (credential store) |
| `src/storage.rs` | JSON persistence in the platform data dir, OPML import/export |
| `src/saved_io.rs` | Saved-articles import/export as Google Reader starred JSON (interoperable with other readers) |
| `src/pixel_mouse.rs` | Pixel-precise mouse reports → cells (+ fractional row) |
| `src/fetch.rs` | Shared HTTP client, feed parsing, article extraction (`dom_smoothie`), update check |

## Key patterns

- **Event loop.** Background work never touches `App` directly: tasks send an
  `AppEvent` over the MPSC channel and `main.rs` dispatches it. Keep CPU-heavy work
  (HTML conversion, image decoding) in the task or `spawn_blocking`, not the handler.
- **Mouse.** Renderers record clickable rects in `app.hit` every frame;
  `handlers/mouse.rs` reads them and, where possible, replays the equivalent key press
  so mouse and keyboard share one code path. New panels/lists should record their
  area and get a branch in `mouse.rs`; pass a `ScrollTarget` to `render_scrollbar`
  to make a scrollbar draggable.
- **Wheel scrolls, clicks select.** The wheel moves a list's viewport, never its
  cursor: it sets the list's `app.detached` flag and `sync_list_view` keeps the manual
  offset until the next key press. Rows style their own selection (no ratatui
  highlight). Note ratatui's `ListState::select(None)` also zeroes the offset. The
  sidebar/article lists select on mouse-up so a press-and-drag scrolls them instead.
- **Pixel mouse.** Where the terminal supports it, pointer reports arrive in pixels
  (`pixel_mouse.rs`); the input thread converts them to cells and passes the
  fractional row along for smooth scrollbar drags. Handlers only ever see cells.
- **Images vs overlays.** Terminal images (`SlicedImage`) keep their escape sequence in
  each image row's first cell: never draw on an image's cells. Anything drawn over the
  article (toasts, link hints) pushes its rect to `app.hit.overlays`, so `main.rs`
  repaints the images under it once it goes.
- **Link hints** (`f` / `F`) label whatever is in `app.hit.article_links` /
  `article_images` and act like the mouse click. New clickable article targets should
  be recorded there to get hints too.
- **Keys.** `handlers::handle_key` maps `j`/`k` to `↓`/`↑` everywhere except text
  inputs (`is_text_input`), so handlers only match arrows. The `?` popup's tables
  live in `ui/help.rs` — update them whenever a handler's keys change.
- **Article rendering is cached** (`ui/content/article_detail.rs`): markdown is only
  re-rendered when `render_key` changes, and the image layout only when the viewport
  height or `ImageStore::version()` changes (an image loads or fails, or the loading
  spinner turns on a tick); each frame draws just the visible lines. Article images
  live in `app.image_store` (limner's `ImageStore`): start downloads with
  `store.start(url)` (safe every frame; failed images aren't retried). Any new input
  that affects the rendered article must be added to `render_key`, or it will go stale.
- **Notifications.** Report anything to the user with `app.notify(msg)`: it shows as a
  toast (`ui/toast.rs`); a leading `✓ ` / `⚠ ` picks the success / warning style.
  Don't toast per feed during a refresh — failures go in `app.refresh_errors` and show
  in the live refresh card and the batch summary (`events::finish_fetch`).
- **Full-article fetches** all go through `handlers::article::prefetch_article_if_stub`,
  which fetches a link at most once per session (`full_fetch_tried`) and drives the
  skeleton (`full_fetch_pending`). Never write placeholder text into `content`.
- **Text selection** is stored in document rows (`TextSelection`), not screen rows: a
  panel that supports it calls `ui::selection::highlight` and `capture` while drawing.
- **Persistence.** All saves go through `storage::write_json` (atomic temp-file +
  rename). The article cache is written once per refresh batch (`events::finish_fetch`)
  and on quit — not per feed.
- **`UserData` defaults** come from the serde field defaults (`Default` deserializes
  `{}`), so every new field needs `#[serde(default…)]`.
