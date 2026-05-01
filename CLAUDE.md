# CLAUDE.md — rssr agent guide

> **For tasks, bugs, and requests see `TASKS.md`.**

---

## Keeping This File Current

**Update CLAUDE.md whenever you:**

- Add, rename, or restructure a module or directory
- Change a module's sole responsibility
- Add a new persisted type or data file
- Change the testing workflow
- Establish a new non-negotiable rule
- **Change the task workflow or caveman mode behavior**

Update `ARCHITECTURE.md` whenever you change the state machine, keybindings, App struct shape, or any design detail a
human would want to reference.

**Do not let CLAUDE.md drift from the code.** If you notice a stale reference (wrong file path, removed type, changed
rule), fix it in the same commit.

---

## Task Workflow

Tasks live in `TASKS.md`. Categories are user-defined (create/rename/remove as needed).

1. **On session start**: read `TASKS.md`. Pick up the highest-priority incomplete item.
2. **Pick up work**: always take the highest-priority incomplete item first.
3. **If anything is unclear**: stop and ask the user. Do not assume scope, layout, or behavior. Never move items to a "
   Blocked" section — just ask directly in your response.
4. **On completion**: move item from its category to **Completed** with a timestamp (e.g.,
   `[x] - 2026-04-12 14:35 - Task name`).
5. **Maintain Completed**: keep only the last 10 entries (trim oldest).

**Blocked entry format** (required fields):

```
- [] **<Task name>** — <one-line summary>
  Question: <exact question to ask the user>
```

**Before marking any task done**: run the full test workflow (below) and confirm the requirement is met.

---

## Caveman Mode (Permanent)

**This session and all future sessions use `caveman full` mode permanently.**

Caveman reduces output tokens by ~65-75% while maintaining technical accuracy. Rules:

- **No filler** — drop "sure", "certainly", "just", "basically", "actually"
- **No pleasantries** — no "I'd be happy to", "let me help"
- **Fragments fine** — short, direct statements okay
- **Execute before explaining** — show code/result first, explain after if needed
- **Technical terms stay exact** — "polymorphism" stays "polymorphism"
- **Code blocks unchanged** — caveman speak around code, not in code
- **Error messages quoted exact** — caveman explains only
- **Auto-clarity rule** — drop caveman for: security warnings, irreversible confirmations, multi-step sequences where
  fragment ambiguity risks misread, user confusion. Resume after.

**Mode persists across conversations.** Do not revert to normal mode after a few exchanges. This is permanent for this
repository.

---

## Module Map

```
src/
├── main.rs           Bootstrap + MPSC event loop
├── app.rs            App struct + navigation methods (next/previous/select/unselect)
├── fetch.rs          All async network I/O (feeds, images, readability)
├── storage.rs        All disk I/O (feeds.json, user_data.json, articles.json, categories.json)
├── models/
│   ├── mod.rs        Constants, Category, FeedTreeItem, FeedEditorMode; re-exports all sub-modules
│   ├── core_types.rs Feed, Article, UserData structs
│   ├── feed.rs       impl Feed + impl Article (display helpers only — no I/O)
│   ├── navigation.rs AppState, Tab, SettingsItem, AddFeedStep, FeedSource
│   └── events.rs     AppEvent (MPSC channel messages)
├── handlers/
│   ├── mod.rs        handle_key dispatch
│   ├── feed_list.rs  FeedList + FavoriteFeedList key handling
│   ├── article.rs    ArticleList/Detail, read/star toggles, open article
│   ├── settings.rs   SettingsList, AddFeed, OPML paths, ClearData
│   └── feed_editor.rs FeedEditor rename/move/delete
└── ui/
    ├── mod.rs        Catppuccin Mocha color constants + draw() entry point
    ├── chrome.rs     Tab bar + footer
    ├── content.rs    Sidebar, article list, article detail (shared by Feeds + Favorites tabs)
    ├── editor.rs     Feed editor full-screen view
    ├── settings.rs   Settings menu
    └── popups.rs     Add-feed, OPML path, confirm-delete modals
```

---

## Module Rules (non-negotiable)

| Module       | Does                                         | Never does                          |
|--------------|----------------------------------------------|-------------------------------------|
| `models/`    | Type definitions + display-only impl methods | Any I/O, logic, rendering           |
| `app.rs`     | App struct fields + navigation methods       | I/O, rendering, key handling        |
| `storage.rs` | Disk I/O only                                | Network, rendering, state mutation  |
| `fetch.rs`   | Async network I/O only                       | File I/O, rendering, state mutation |
| `handlers/`  | Key routing; mutates App; spawns tasks       | File I/O (calls storage), rendering |
| `ui/`        | Ratatui rendering only                       | State mutation, any I/O             |
| `main.rs`    | Bootstrap + MPSC dispatch                    | Business logic, rendering details   |

- **`ui/` draw functions are pure** — `&mut App` is only for Ratatui widget state (e.g. `content_line_count`). Never
  call `save_*` or mutate logical state inside a draw function.
- **All disk writes go through `storage.rs`** — no `std::fs` calls anywhere else.
- **All network calls go through `fetch.rs`** — no inline `reqwest` in handlers.
- **All new shared types go into `models/`** — never redeclare a type in another module.
- **`impl` blocks on model types belong in `models/feed.rs`** — only display helpers (no I/O).
- **`app.rs` navigation methods only** — `next()`, `previous()`, `select()`, `unselect()`, tab switching.

---

## Feature Checklists

See `CHECKLISTS.md` — read it before implementing new types, AppStates, keybindings, background tasks, or persisted data.

---

## Testing Workflow (mandatory before reporting done)

```
cargo check     # zero errors
cargo test      # all tests pass
cargo clippy    # zero warnings
cargo run       # visual verify of the affected feature
```

Never report a visual feature as done without running the app.

---

## Common Pitfalls

- **Mutating App logic inside `ui/`** — draw functions are pure rendering only.
- **`std::fs` outside `storage.rs`** — all file access goes through storage.
- **`.await` in the event loop** — always use `tokio::spawn`; the loop must never block.
- **Hardcoding `Color::Rgb(...)`** — use the named constants in `ui/mod.rs`.
- **Forgetting `#[derive(Debug)]`** on new public types.
- **Adding a dep for a single utility function** — use stdlib or an existing dep.
- **Not updating CLAUDE.md or ARCHITECTURE.md after a structural change.**