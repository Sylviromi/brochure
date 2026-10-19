//! Key event handling for the FreshRSS server setup wizard.
//!
//! Three steps (server → username → API password). On confirm, the config is
//! persisted, the secret goes to the secret store (never `user_data.json`),
//! and a full sync is kicked off.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::App,
    models::{AppEvent, AppState, FreshRssConfig, FreshRssStep},
    storage::{data_dir, save_user_data},
    sync::secret::SecretSource,
};

/// Handles key events for the `FreshRssSetup` wizard state.
pub(super) fn handle_freshrss_setup(app: &mut App, key: KeyEvent, tx: &UnboundedSender<AppEvent>) {
    match key.code {
        KeyCode::Esc => match app.freshrss_setup.step {
            FreshRssStep::Server => {
                app.freshrss_setup.clear_secret();
                app.state = AppState::SettingsList;
            }
            FreshRssStep::Username => app.freshrss_setup.step = FreshRssStep::Server,
            FreshRssStep::Password => app.freshrss_setup.step = FreshRssStep::Username,
        },
        KeyCode::Enter => advance(app, tx),
        _ => match app.freshrss_setup.step {
            FreshRssStep::Server => app.freshrss_setup.server_input.handle_key(key.code, None),
            FreshRssStep::Username => app.freshrss_setup.username_input.handle_key(key.code, None),
            FreshRssStep::Password => app.freshrss_setup.password_input.handle_key(key.code, None),
        },
    }
}

/// Advance the wizard, persisting + syncing on the final step.
fn advance(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    match app.freshrss_setup.step {
        FreshRssStep::Server => {
            if app.freshrss_setup.server_input.text.trim().is_empty() {
                return;
            }
            app.freshrss_setup.step = FreshRssStep::Username;
        }
        FreshRssStep::Username => {
            if app.freshrss_setup.username_input.text.trim().is_empty() {
                return;
            }
            app.freshrss_setup.step = FreshRssStep::Password;
        }
        FreshRssStep::Password => save_connection(app, tx),
    }
}

/// When editing an existing connection, a blank password means "keep the stored
/// one" — but only if the username is unchanged, since secrets are keyed by user.
fn keeps_stored_password(editing_username: Option<&str>, username: &str, password: &str) -> bool {
    password.trim().is_empty() && editing_username == Some(username)
}

/// Final wizard step: store the secret, persist the config, and start a sync.
fn save_connection(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    let setup = &mut app.freshrss_setup;
    let password = setup.password_input.text.clone();
    let username = setup.username_input.text.trim().to_string();
    let editing_username = setup.editing_username.clone();
    let keep_password = keeps_stored_password(editing_username.as_deref(), &username, &password);
    if password.trim().is_empty() && !keep_password {
        return;
    }
    let base_url = crate::sync::freshrss::normalise_base_url(&setup.server_input.text);
    setup.clear_secret();

    if keep_password {
        app.set_status("FreshRSS connection updated.");
    } else {
        match crate::sync::secret::set_password(&username, &password, &data_dir()) {
            Ok(SecretSource::Env) => app.set_status("Using API password from environment."),
            Ok(SecretSource::Keyring) => app.set_status("FreshRSS saved to OS keyring."),
            Ok(SecretSource::File) => {
                app.set_status("No OS keyring — API password saved to a locked file.")
            }
            Err(e) => {
                app.set_status(format!("⚠ Could not store API password: {e}"));
                return;
            }
        }
        // Secrets are keyed by username: drop the old user's so it doesn't linger.
        if let Some(old) = editing_username.filter(|old| *old != username) {
            crate::sync::secret::delete_password(&old, &data_dir());
        }
    }

    // Keep the existing sync category when re-running setup.
    let category_id = app.user_data.freshrss.as_ref().and_then(|c| c.category_id);
    app.user_data.freshrss = Some(FreshRssConfig {
        base_url,
        username,
        last_sync_secs: None,
        category_id,
    });
    let _ = save_user_data(&app.user_data);
    app.freshrss_auth = None;
    app.state = AppState::SettingsList;
    app.spawn_freshrss_sync(tx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SettingsItem, Tab};
    use crossterm::event::KeyModifiers;

    fn connected_app() -> App {
        let mut app = App::new();
        app.user_data.freshrss = Some(FreshRssConfig {
            base_url: "https://rss.example.com/api/greader.php".to_string(),
            username: "alice".to_string(),
            last_sync_secs: Some(1),
            category_id: None,
        });
        app
    }

    #[tokio::test]
    async fn enter_on_connected_row_opens_prefilled_setup() {
        let mut app = connected_app();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        app.select_tab(Tab::Settings);
        app.settings_selected = SettingsItem::FreshRss;
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        super::super::settings::handle_settings(&mut app, enter, &tx);

        assert_eq!(app.state, AppState::FreshRssSetup);
        let setup = &app.freshrss_setup;
        assert_eq!(
            setup.server_input.text,
            "https://rss.example.com/api/greader.php"
        );
        assert_eq!(setup.username_input.text, "alice");
        assert_eq!(setup.editing_username.as_deref(), Some("alice"));
    }

    #[test]
    fn blank_password_keeps_stored_one_only_for_same_user() {
        assert!(keeps_stored_password(Some("alice"), "alice", ""));
        assert!(!keeps_stored_password(Some("alice"), "bob", ""));
        assert!(!keeps_stored_password(None, "alice", ""));
        assert!(!keeps_stored_password(Some("alice"), "alice", "new-secret"));
    }
}
