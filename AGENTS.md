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
| `src/fetch.rs` | Shared HTTP client, feed parsing, article extraction (`dom_smoothie`), update check |

Key patterns:

- **Event loop.** Background work never touches `App` directly: tasks send an
  `AppEvent` over the MPSC channel and `main.rs` dispatches it. Keep CPU-heavy work
  (HTML conversion, image decoding) in the task or `spawn_blocking`, not the handler.
- **Mouse.** Renderers record clickable rects in `app.hit` every frame;
  `handlers/mouse.rs` reads them and, where possible, replays the equivalent key press
  so mouse and keyboard share one code path. New panels/lists should record their
  area and get a branch in `mouse.rs`.
- **Keys.** `handlers::handle_key` maps `j`/`k` to `↓`/`↑` everywhere except text
  inputs (`is_text_input`), so handlers only match arrows. The `?` popup's tables
  live in `ui/help.rs` — update them whenever a handler's keys change.
- **Article rendering is cached** (`ui/content/article_detail.rs`): markdown is only
  re-rendered when `render_key` changes, and the image layout only when images load or
  the viewport height changes; each frame draws just the visible lines. Any new input
  that affects the rendered article must be added to `render_key`, or it will go stale.
- **Persistence.** All saves go through `storage::write_json` (atomic temp-file +
  rename). The article cache is written once per refresh batch (`events::finish_fetch`)
  and on quit — not per feed.
- **`UserData` defaults** come from the serde field defaults (`Default` deserializes
  `{}`), so every new field needs `#[serde(default…)]`.

---

## Code Quality (apply on every edit)

Whenever you read or edit a file — even for a small fix — also look for:

- **Duplicate or near-duplicate code** that belongs in a shared helper function.
- **Functions that are too long** and would be clearer if split into smaller, focused helpers.
- **Files that have grown too large** and would benefit from being split into sub-modules.
- **Obvious simplifications**: dead branches, redundant variables, needlessly verbose patterns.

Make these improvements in the same change when they are clearly better and low-risk.
When the improvement is larger or uncertain, surface it to the user as a suggestion.

---

## Checks (run after every change)

These mirror CI (`.github/workflows/ci.yml`), which fails on any clippy warning:

```
cargo fmt && cargo clippy --all-features -- -D warnings && cargo test --all-features
```

`cargo fix --allow-dirty` is handy for clearing unused imports, but review what it changed.

Testing notes:

- `App::new()` loads from the **real** data dir. Tests may build an `App`, but must not
  exercise paths that save (marking read, toggling collapse, editing feeds…).
  `handlers/mouse.rs` tests show how to swap in in-memory feeds.
- To run the app without touching your own data: `XDG_DATA_HOME=/tmp/brochure-sandbox cargo run`.
- UI behaviour can be tested headlessly with `ratatui::backend::TestBackend` + `ui::draw`.

---

## Build & Dependencies

- `release` profile (what `cargo install` uses) is tuned for install time (thin LTO).
  CI release binaries use the `dist` profile (fat LTO, 1 codegen unit). Keep it that way.
- Before adding a dependency, check `cargo tree -d` for duplicates and prefer
  `default-features = false` with only the features used.
- Local dev patches `limner` to `../limner` via the git-ignored `.cargo/config.toml`.
  That strips limner's `source`/`checksum` from `Cargo.lock`, which breaks `--locked`
  CI builds; `scripts/release.py` (`ensure_clean_lock`) restores them before release.

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

**When refactoring**, split changes into focused commits — one per logical concern, e.g.
`refactor: extract shared http_get_bytes helper`, `refactor: split app.rs into submodules`.

**Guidelines:**
- One file appears in at most one commit in a batch
- Each commit compiles on its own (when possible)
- Subject line is 72 chars or fewer
- Do not use `-i`/interactive flags (`rebase -i`, `add -i`, `add -p`)

---

## Changelog

**Before committing any changes**, review `git diff` for user-facing changes and
append qualifying entries to `changelog/next-update.md` under the appropriate
section (`## Features`, `## UI`, `## Fixes`, `## Refactor`).

Each entry must use today's date: `- YYYY-MM-DD: Description`. Append new
entries below older ones within each section.

**Changelog entries are user-facing only.** Write what the user sees or gains —
not how it was implemented. No library names, function names, file paths, or
internal refactoring details. If a change is invisible to the user (code
cleanup, helper extraction, deduplication), omit it entirely.

`changelog/next-update.md` is gitignored — do **not** `git add` or commit it.
`scripts/release.py` turns it into `changelog/vX.Y.Z.md` and a `changelog.json`
entry (shown in the in-app Changelog tab) at release time.
