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
        FreshRssStep::Password => {
            let password = app.freshrss_setup.password_input.text.clone();
            if password.trim().is_empty() {
                return;
            }
            let base_url =
                crate::sync::freshrss::normalise_base_url(&app.freshrss_setup.server_input.text);
            let username = app.freshrss_setup.username_input.text.trim().to_string();
            app.freshrss_setup.clear_secret();
            // Keep the existing sync category when re-running setup.
            let category_id = app.user_data.freshrss.as_ref().and_then(|c| c.category_id);
            app.user_data.freshrss = Some(FreshRssConfig {
                base_url,
                username: username.clone(),
                last_sync_secs: None,
                category_id,
            });
            let _ = save_user_data(&app.user_data);
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
            app.freshrss_auth = None;
            app.state = AppState::SettingsList;
            app.spawn_freshrss_sync(tx);
        }
    }
}
