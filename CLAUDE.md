# CLAUDE.md — Brochure agent guide

---

## Keeping This File Current

**Update CLAUDE.md whenever you:**

- Add, rename, or restructure a module or directory
- Change a module's sole responsibility
- Add a new persisted type or data file
- Change the testing workflow
- Establish a new non-negotiable rule
- **Change the task workflow**

**Do not let CLAUDE.md drift from the code.** If you notice a stale reference (wrong file path, removed type, changed
rule), fix it in the same commit.

---

## Task Workflow

1. **On session start**: read `TASKS.md`. Pick up the highest-priority incomplete item.
2. **Priority indicators**: tasks marked with [!] are high-priority. Always pick these before non-flagged tasks
   within the same category; otherwise take the topmost incomplete item.
3. **If anything is unclear**: stop and ask the user. Do not assume scope, layout, or behavior. Just ask directly in
   your response.
4. **On completion**: remove the item from TASKS.md and append it to `changelog/next-update.md`. The file uses
   grouped sections (`## Features`, `## UI`, `## Fixes`, `## Refactor`). Each entry must include today's date:
   `- YYYY-MM-DD: Description`. Append new entries below older ones within each section. Never commit without
   doing this first. Never read the `changelog/` files — they are an append-only archive.

**Blocked entry format** (required fields):

```
- **<Task name>** — <one-line summary>
  Question: <exact question to ask the user>
```

**Before marking any task done**: run the full test workflow and confirm the requirement is met.

---

## Test Workflow

Run in order — all must pass before invoking commit-crafter:

```bash
cargo fmt          # format code; stage any changes it makes
cargo check        # catch compile errors without a full build
cargo clippy       # lints; fix any warnings before committing
cargo test         # run the test suite
```

`cargo fmt` changes must be staged. Never invoke commit-crafter on code that fails `cargo check` or `cargo clippy`.

---

## Doc Comment Convention

Add `///` doc comments to every `pub` item (functions, structs, enums, methods). Also doc non-obvious private helpers.
Doc comments feed `rust-ast-extractor` — they are the primary signal used to understand what an item does without
opening the file.

- One sentence minimum. State **what** the function does and, if non-obvious, **why** or **when** to call it.
- For structs, describe the role of the struct and any important invariants.
- For enum variants, use `///` on each variant when the name alone is not self-explanatory.
- Keep docs concise — one or two sentences is usually enough.

**Example:**

```rust
/// Returns the path to the on-disk feeds file, creating parent dirs if missing.
pub fn feeds_path() -> PathBuf { ... }

/// Scrollable text widget state for an individual article's body.
pub struct TextScroll {
    ...
}
```

---

## Agent Orchestration (mandatory for every task)

After reading TASKS.md and selecting a task, **never implement it directly**. Instead:

### 1. Decompose the task

Break it into independent subtasks. For each subtask identify:

- What it does
- Which files it touches
- Whether it depends on another subtask

### 2. Assign model by complexity score

Score each subtask on three axes (0–3 each), sum them, and map to a model:

| Axis | 0 | 1 | 2 | 3 |
|------|---|---|---|---|
| **Scope** | One function body, no new items | Multi-fn or adds private items in one file | 1–2 files, may add pub items | 3+ files or pub API surface across modules |
| **Context** | Fully specified — no reasoning needed | Follows a pattern visible in the same file | Cross-file pattern or touches module boundary | Needs architectural or convention knowledge |
| **Risk** | UI, display logic, constants, doc comments | Business logic, handlers, standard data flow | `storage.rs`, `fetch.rs`, state transitions | `AppState`, `AppEvent`, event loop, persisted types |

**Score → Model:**

| Score | Model | Typical work |
|-------|-------|--------------|
| 0–2   | `ollama (local)` | Single-fn edits, private helpers, UI tweaks, constants |
| 3–5   | `haiku`          | Read-only research, pub fn additions, cross-file pattern follow |
| 6–7   | `sonnet`         | Multi-file logic, new handlers, state machine additions |
| 8–9   | `opus`           | Architectural decisions, large refactors, new persisted types |

**Scoring examples:**

- Change a keybinding constant → Scope 0 + Context 0 + Risk 0 = **0** → ollama
- Add private helper fn in `ui/` following local pattern → Scope 1 + Context 1 + Risk 0 = **2** → ollama
- Add new pub handler in `handlers/` → Scope 2 + Context 1 + Risk 1 = **4** → haiku
- Add variant to fetch state machine → Scope 2 + Context 2 + Risk 2 = **6** → sonnet
- Redesign `AppState` with new persisted type → Scope 3 + Context 3 + Risk 3 = **9** → opus

**Dispatch syntax for ollama tasks:**

```bash
echo '{"file":"src/path/to/file.rs","instruction":"your instruction here","context_files":[]}' | python scripts/ollama_agent.py
```

Default model is `gemma4:e4b`. Override with `--model <name>` if needed. Result is JSON
`{"path": "...", "content": "..."}`. Apply with `Edit` or `Write` tool.

**Fallback:** If `ollama_agent.py` returns `{"error": "..."}`, escalate the task to `haiku`. Never silently drop the
error.

### 3. Present the dispatch plan — wait for approval

Before firing any agent, output a plan table and **stop**. Do not proceed until user approves.
Output a markdown table exactly matching this format:

```
Tasks selected: <list>

Agent plan:
| # | Task | Subtask | S+C+R | Score | Model  | Rationale |
|---|------|---------|-------|-------|--------|-----------|
| 1 | ...  | ...     | 0+1+0 |   1   | ollama | private helper, local pattern, UI file |
| 2 | ...  | ...     | 2+1+1 |   4   | haiku  | new pub fn, cross-file pattern |
| 3 | ...  | ...     | 2+2+2 |   6   | sonnet | multi-file logic, state transition |

Total: X ollama, Y haiku, Z sonnet, W opus. Proceed? (y / adjust)
```

User may reply "y", adjust individual models, drop agents, or merge subtasks. Incorporate feedback before dispatching.

### 4. Dispatch agents in parallel

Use the `Agent` tool. Independent subtasks launch simultaneously. Sequential subtasks (B depends on A's output) chain.

**One agent per file group:** Subtasks that all edit the same file must be merged into a single agent. Never split
same-file changes across multiple agents — it causes redundant file reads and potential conflicts.

**Ollama dispatch uses `Bash`, not `Agent`:** Tasks routed to `ollama` must be dispatched with the `Bash` tool
running `ollama_agent.py`. The `Agent` tool always hits the Claude API regardless of any model label. Apply the returned
content with `Edit` or `Write`.

Each agent prompt must include:

- Exact file paths to read/edit
- The specific change required
- Constraints (e.g., "do not touch `ui/`", "read-only, return findings only")
- Expected return value ("return the new struct definition" / "make the change and return nothing")

Agents do **not** inherit this session's context. Give them everything they need inline.

### 5. Review and integrate

Collect agent results. Verify no conflicts. Run the [Test Workflow](#test-workflow) yourself — agents do not run it.
Then invoke commit-crafter.

---

## Multi-task batching

On session start, pick **multiple tasks** when sensible — not just one. Batch criteria:

- Prefer grouping tasks of similar complexity (e.g., several small UI tasks together)
- Max batch: however many can be decomposed without subtask conflicts (shared file writes)
- Present all selected tasks in the single dispatch plan table above
- One combined approval covers all selected tasks

---

## AST Cache (rust-ast-extractor)

The project is indexed with [`rust-ast-extractor`](https://github.com/TcePrepK/rust-ast-extractor). The cache lives in
`.ast-cache/` (gitignored).
**Before reading a source file**, check the cache first — it's faster and gives you signatures, docs, and line numbers
without opening the file:

```bash
# Get structured summary of a file (items, signatures, docs)
rust-ast-extractor get src/app.rs

# Get raw source of one specific item
rust-ast-extractor get src/app.rs::App
rust-ast-extractor get src/handlers/feed_list.rs::handle_feed_list_input

# Re-index after editing source files
rust-ast-extractor index src/
```

**When to use it:**

- Before asking "what does X function do?" — `get src/file.rs::fn_name` gives you the source instantly
- When planning which files to touch — `get src/file.rs` shows all items with signatures and doc comments
- After making changes — re-index so the cache stays current

**Re-index rule:** Run `rust-ast-extractor index src/` at the start of any session where you plan to edit source files,
or after any significant changes. The tool skips unchanged files, so it's fast.

---

## Module Map

Run `rust-ast-extractor dir src/` for a live index of all source files and their responsibilities.
Each file's `//!` module doc is the authoritative description — it is never out of date.
