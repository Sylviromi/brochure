# CHECKLISTS.md — rssr feature implementation checklists

Read this when implementing new types, states, keybindings, background tasks, or persisted data.

---

## New type

1. Define in `src/models/` with `#[derive(Debug)]` (+ `Serialize, Deserialize` if persisted).
2. Add to `App` struct in `src/app.rs` if runtime state.
3. Import via `crate::models::Foo` everywhere — never redeclare.

## New AppState

1. Add variant to `AppState` in `models/navigation.rs`.
2. Add `AppEvent` variant(s) in `models/events.rs` if background work needed.
3. Add key handler branch in the correct `handlers/*.rs`, wire into `handlers/mod.rs`.
4. Add draw branch in `ui/mod.rs` or the relevant `ui/*.rs`.
5. Update `App::next()`, `previous()`, `select()`, `unselect()` in `app.rs` if navigable.
6. Wire event dispatch in `main.rs` if new `AppEvent` variants added.

## New keybinding

1. Add `KeyCode` match arm in the correct `handlers/*.rs`.
2. Update footer hint string in `ui/chrome.rs` `draw_footer`.
3. Update keybindings table in `ARCHITECTURE.md`.

## New background task

1. Add `AppEvent` variant in `models/events.rs`.
2. Spawn in handler: `tokio::spawn(async move { ... tx.send(AppEvent::...) })`.
3. Handle in `main.rs` match loop.
4. Never `.await` in the main event loop thread.

## New persisted data

1. Add field to `UserData` in `models/core_types.rs` with `#[serde(default)]`.
2. Load/save only in `storage.rs`. Call `save_user_data` from handlers after mutation.
3. Update data files table in `ARCHITECTURE.md`.
