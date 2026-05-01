# CLAUDE.md — rssr agent guide

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

## Vault

Project knowledge lives in `vault/`. The LLM maintains it; humans drop raw inputs into `vault/raw/`.

```
vault/
├── INDEX.md          ← start here
├── raw/              ← drop inputs here (articles, code dumps, notes)
├── wiki/             ← LLM-maintained knowledge tree
│   ├── INDEX.md      ← entry point for wiki
│   ├── log.md        append-only operation log
│   ├── project/      overview
│   ├── architecture/ modules, state machine, event loop, keybindings, storage
│   ├── models/       core types, navigation, events
│   ├── ui/           theme
│   ├── behaviors/    favorites, feed-tree, add-feed flow
│   ├── workflow/     development, commits
│   └── user/         preferences
└── output/           ← LLM-generated artifacts
```

**Session start**: read `vault/wiki/INDEX.md`, navigate to relevant nodes as needed, then read `TASKS.md`.
Use vault for architectural context; don't load the full tree — navigate to what the current task needs.

---

## Task Workflow

1. **On session start**: read vault (above), then read `TASKS.md`. Pick up
   the highest-priority incomplete item.
2. **Priority indicators**: tasks marked with [!] are high-priority. Always prioritize these before non-flagged tasks
   within the same category.
3. **Pick up work**: always take the highest-priority incomplete item first.
4. **If anything is unclear**: stop and ask the user. Do not assume scope, layout, or behavior. Just ask directly in
   your response.
5. **On completion**: update TASKS.md **before committing** — move item to **Done** with a timestamp.
   Never commit without first marking the task done.

**Blocked entry format** (required fields):

```
- **<Task name>** — <one-line summary>
  Question: <exact question to ask the user>
```

**Before marking any task done**: run the full test workflow (below) and confirm the requirement is met.

---

## Agent Orchestration (mandatory for every task)

After reading TASKS.md and selecting a task, **never implement it directly**. Instead:

### 1. Decompose the task

Break it into independent subtasks. For each subtask identify:
- What it does
- Which files it touches
- Whether it depends on another subtask

### 2. Assign model by complexity

| Complexity | Model | Use for |
|------------|-------|---------|
| Trivial | `ollama (local)` | Single-function body edits, constant/string/number changes, rename within one file, add one match arm |
| Simple | `haiku` | Read-only research, single-file multi-function changes, UI tweaks |
| Medium | `sonnet` | Multi-file logic changes, new handlers, state machine additions |
| Complex | `opus` | Architectural decisions, large refactors, new persisted types + full feature |

#### Trivial routing criteria

**Use `ollama`** when ALL of the following are true:
- The change is within a single file
- No new `fn`, `struct`, `enum`, or `impl` block is created
- The instruction does not require understanding how multiple modules fit together
- The file is not `storage.rs` or `fetch.rs`
- The change does not touch `AppState`, `AppEvent`, or the state machine

**Use `haiku` minimum** when any of the above is false, or when the instruction contains phrases like "based on", "similar to", or "following the pattern of".

**Routing heuristic:** Can you fully specify the output without the model needing to reason about project architecture? If yes → ollama. If not → haiku minimum.

**Dispatch syntax for trivial tasks:**
```bash
echo '{"file":"src/path/to/file.rs","instruction":"your instruction here","context_files":[]}' | python scripts/ollama_agent.py
```

Default model is `qwen2.5-coder:7b`. Override with `--model <name>` if needed. Result is JSON `{"path": "...", "content": "..."}`. Apply with `Edit` or `Write` tool.

**Fallback:** If `ollama_agent.py` returns `{"error": "..."}`, escalate the task to `haiku`. Never silently drop the error.

### 3. Present the dispatch plan — wait for approval

Before firing any agent, output a plan table and **stop**. Do not proceed until user approves.
Use the harness table renderer — not hand-drawn ASCII art.

```
Tasks selected: <list>

Agent plan:
| # | Task | Subtask | Model  | Rationale |
|---|------|---------|--------|-----------|
| 1 | ...  | ...     | haiku  | read-only |
| 2 | ...  | ...     | sonnet | multi-file logic |

Total: X ollama, Y haiku, Z sonnet, W opus. Proceed? (y / adjust)
```

User may reply "y", adjust individual models, drop agents, or merge subtasks. Incorporate feedback before dispatching.

### 4. Dispatch agents in parallel

Use the `Agent` tool. Independent subtasks launch simultaneously. Sequential subtasks (B depends on A's output) chain.

Each agent prompt must include:
- Exact file paths to read/edit
- The specific change required
- Constraints (e.g., "do not touch `ui/`", "read-only, return findings only")
- Expected return value ("return the new struct definition" / "make the change and return nothing")

Agents do **not** inherit this session's context. Give them everything they need inline.

### 5. Review and integrate

Collect agent results. Verify no conflicts. Run the test workflow yourself (agents do not run `cargo check/test/clippy`).
Commit per the commit discipline rules.

---

## Multi-task batching

On session start, pick **multiple tasks** when sensible — not just one. Batch criteria:

- Prefer grouping tasks of similar complexity (e.g., several small UI tasks together)
- Max batch: however many can be decomposed without subtask conflicts (shared file writes)
- Present all selected tasks in the single dispatch plan table above
- One combined approval covers all selected tasks

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

## Feature Checklists

See `CHECKLISTS.md` — read it before implementing new types, AppStates, keybindings, background tasks, or persisted
data.

---

## Commit Discipline (mandatory)

Every commit must be **atomic** — one logical change per commit. Never bundle unrelated changes.

### Commit types

| Prefix      | Use for                                                                          |
|-------------|----------------------------------------------------------------------------------|
| `feat:`     | New user-visible behaviour or capability                                         |
| `fix:`      | Bug correction (logic, crash, wrong value)                                       |
| `ui:`       | Visual/layout changes with no behaviour change (colours, borders, spacing, text) |
| `refactor:` | Internal restructuring with no behaviour change                                  |
| `perf:`     | Performance improvement                                                          |
| `test:`     | Adding or fixing tests only                                                      |
| `docs:`     | Documentation files only (CLAUDE.md, README, ARCHITECTURE.md, etc.)              |
| `chore:`    | Tooling, CI, dependency, config — nothing a user would notice                    |

### Splitting rules

- **File-level**: if a single file contains hunks of different types, use `git add -p` to stage only the relevant hunks
  for each commit.
- **Minimum commits per session**: one per type touched. Changing a handler (fix), its rendering (ui), and CLAUDE.md (
  docs) = 3 commits.
- Never mix `feat`/`fix`/`ui`/`refactor` with `docs` in one commit.

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