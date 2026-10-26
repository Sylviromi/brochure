# AGENTS.md — Brochure

Brochure is a terminal RSS/Atom reader (Rust 2024, Ratatui + Crossterm, Tokio) with
optional FreshRSS sync. Single binary crate; no workspace.

---

## Architecture

| Path | Responsibility |
|------|----------------|
| `src/main.rs` | Terminal setup/restore (incl. panic hook), input thread, event loop |
| `src/events.rs` | Applies background results (feed fetch, article body, images) to `App`; startup cache load + initial fetch |
| `src/app/` | `App` state. `navigation` (tabs, cursors, views), `editor` (feed editor ops), `tree` (category/feed tree flattening), `freshrss` (refresh routing, sync, push) |
| `src/handlers/` | Input → state changes. One module per screen; `mouse.rs` maps clicks/wheel onto the key handlers |
| `src/ui/` | Rendering only. `content/` holds the Feeds/Saved tab panels |
| `src/models/` | Plain data types, enums, `AppEvent`, `HitAreas` |
| `src/state/` | Per-screen sub-state structs owned by `App` |
| `src/sync/` | FreshRSS: `freshrss` (pure GReader parsing/mapping/category placement), `client` (HTTP), `apply` (results → `App`), `secret` (credential store) |
| `src/storage.rs` | JSON persistence in the platform data dir, OPML import/export |
| `src/saved_io.rs` | Saved-articles import/export as Google Reader starred JSON (interoperable with other readers) |
| `src/fetch.rs` | Shared HTTP client, feed parsing, article extraction (`dom_smoothie`), update check |

Key patterns:

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
- **Keys.** `handlers::handle_key` maps `j`/`k` to `↓`/`↑` everywhere except text
  inputs (`is_text_input`), so handlers only match arrows. The `?` popup's tables
  live in `ui/help.rs` — update them whenever a handler's keys change.
- **Article rendering is cached** (`ui/content/article_detail.rs`): markdown is only
  re-rendered when `render_key` changes, and the image layout only when images load or
  the viewport height changes; each frame draws just the visible lines. Any new input
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

---

## Code Quality (apply on every edit)

In any file you touch, look for:

- duplicate or near-duplicate code that belongs in a shared helper;
- functions or files that have grown too long and should be split;
- dead branches, redundant variables, needlessly verbose patterns.

Fix these in the same change when the fix is clearly better and low-risk; otherwise
suggest it to the user.

---

## Checks (run after every change)

```
cargo fmt && cargo clippy --all-features --all-targets -- -D warnings && cargo test --all-features
```

CI (`.github/workflows/ci.yml`) runs the same, minus `--all-targets` (so it doesn't lint
test code), and fails on any clippy warning. `cargo fix --allow-dirty` clears unused
imports; review what it changed.

Testing notes:

- `App::new()` loads from the **real** data dir. Tests may build an `App`, but must not
  exercise paths that save (marking read, toggling collapse, editing feeds…).
  `handlers/mouse/tests.rs` shows how to swap in in-memory feeds.
- To run the app without touching your own data: `XDG_DATA_HOME=/tmp/brochure-sandbox cargo run`.
- UI behaviour can be tested headlessly with `ratatui::backend::TestBackend` + `ui::draw`.

---

## Build & Dependencies

- `release` profile (what `cargo install` uses) is tuned for install time (thin LTO).
  CI release binaries use the `dist` profile (fat LTO, 1 codegen unit). Keep it that way.
- Before adding a dependency, check `cargo tree -d` for duplicates and prefer
  `default-features = false` with only the features used.
- `limner` comes from crates.io. Patching it to a local checkout (`[patch.crates-io]`
  in a `.cargo/config.toml`) strips its `source`/`checksum` from `Cargo.lock`, which
  breaks `--locked` CI builds — drop the patch and `git checkout Cargo.lock` before
  committing.

---

## Git Commit Conventions

Use the `<type>: <lowercase description>` format:

| Type     | When to use                          |
|----------|--------------------------------------|
| `feat:`  | New user-facing feature              |
| `fix:`   | Bug fix                              |
| `chore:` | Maintenance (deps, git, CI, etc.)    |
| `refactor:` | Code restructuring with no functional change |
| `docs:`  | Documentation-only changes           |
| `test:`  | Adding or updating tests             |

- Split refactors into one commit per concern (e.g. `refactor: extract shared
  http_get_bytes helper`, `refactor: split app.rs into submodules`).
- A file appears in at most one commit per batch; each commit compiles on its own
  when possible.
- Subject line ≤ 72 chars.
- No interactive flags (`rebase -i`, `add -i`, `add -p`).

---

## Changelog

Before committing, check `git diff` for user-facing changes and append entries to
`changelog/next-update.md` under `## Features`, `## UI`, `## Fixes` or `## Refactor`,
as `- YYYY-MM-DD: Description` (today's date, below older entries).

Entries are **user-facing only**: what the user sees or gains, not how. No library,
function or file names. Changes invisible to the user (cleanup, helper extraction,
deduplication) get no entry.

`changelog/next-update.md` is gitignored — do **not** `git add` or commit it.
At release time it becomes `changelog/vX.Y.Z.md` plus a `changelog.json` entry
(shown in the in-app Changelog tab).
