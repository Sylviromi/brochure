# CLAUDE.md — Brochure

---

## Code Quality (non-negotiable, apply on every edit)

Whenever you read or edit a file — even for a small fix — also look for:

- **Duplicate or near-duplicate code** that belongs in a shared helper function.
- **Functions that are too long** and would be clearer if split into smaller, focused helpers.
- **Files that have grown too large** and would benefit from being split into sub-modules.
- **Obvious simplifications**: dead branches, redundant variables, needlessly verbose patterns.

Make these improvements in the same commit when they are clearly better and low-risk.
When the improvement is larger or uncertain, surface it to the user as a suggestion.

---

## After Every Change (non-negotiable)

After every code edit, run:

```
cargo fmt && cargo fix --allow-dirty && cargo clippy
```

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

**When refactoring**, split changes into focused commits — one per logical concern:
- `refactor: replace cursor macros with composable functions`
- `refactor: extract shared http_get_bytes helper, fix clippy warning`
- `refactor: add shared now_secs() timestamp helper`
- `refactor: consolidate ratatui imports, extract tree-drawing helpers`
- `refactor: deduplicate article handler read/toggle functions`
- `refactor: extract feed-list refresh helpers`
- `refactor: convert settings Left/Right if-chains to match arms`
- `refactor: use split_cursor utility in saved-category editor`

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
