# AGENTS.md — Brochure

Brochure is a terminal RSS/Atom reader (Rust 2024, Ratatui + Crossterm, Tokio) with
optional FreshRSS sync. Single binary crate; no workspace.

---

## Architecture

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the module map and the key patterns
(event loop, mouse/hit areas, images vs overlays, article render cache, persistence…).
Read the relevant section before changing that area.

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
test code), and fails on any clippy warning.

Testing notes:

- `App::new()` loads from the **real** data dir. Tests may build an `App`, but must not
  exercise paths that save (marking read, toggling collapse, editing feeds…).
  `handlers/mouse/tests.rs` shows how to swap in in-memory feeds.
- To run the app without touching your own data: `XDG_DATA_HOME=/tmp/brochure-sandbox cargo run`.
- To preview the update popup with sample releases (debug builds only):
  `BROCHURE_DEMO_UPDATE=1 cargo run`.
- UI behaviour can be tested headlessly with `ratatui::backend::TestBackend` + `ui::draw`.

---

## Build & Dependencies

- `release` profile (what `cargo install` uses) is tuned for install time (thin LTO).
  CI release binaries use the `dist` profile (fat LTO, 1 codegen unit). Keep it that way.
- Before adding a dependency, check `cargo tree -d` for duplicates and prefer
  `default-features = false` with only the features used.
- `limner` comes from crates.io. Patching it to a local checkout (`[patch.crates-io]`
  in a `.cargo/config.toml`) strips its `source`/`checksum` from `Cargo.lock`, which
  breaks the `--locked` release build (`release.yml`): drop the patch and
  `git checkout Cargo.lock` before committing.

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
| `style:` | Formatting only (rustfmt, whitespace), no code change |

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

`changelog/` is gitignored (never commit it). At release time `next-update.md` becomes
`changelog/vX.Y.Z.md` plus a `changelog.json` entry (shown in the in-app Changelog tab).
