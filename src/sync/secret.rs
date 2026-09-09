//! Credential storage for FreshRSS API passwords.
//!
//! Priority order:
//! 1. `BROCHURE_FRESHRSS_PASSWORD` env var (headless / CI).
//! 2. OS keyring via the `keyring` crate (when the `os-keyring` feature is
//!    enabled and a backend — Keychain / Credential Manager / Secret Service
//!    — is reachable).
//! 3. Fallback file `freshrss_secret` in the data dir with `0600` permissions.
//!
//! Secrets are never logged.

use std::path::PathBuf;

#[cfg(feature = "os-keyring")]
const SERVICE: &str = "brochure-freshrss";
const ENV_VAR: &str = "BROCHURE_FRESHRSS_PASSWORD";

/// Where a secret was read from (for status messages, never the secret itself).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecretSource {
    Env,
    /// OS keyring backend (only constructed with the `os-keyring` feature).
    #[allow(dead_code)]
    Keyring,
    File,
}

/// Read the API password for `username`, checking env → keyring → file.
pub fn get_password(username: &str, data_dir: &std::path::Path) -> Option<(String, SecretSource)> {
    if let Ok(pw) = std::env::var(ENV_VAR)
        && !pw.trim().is_empty()
    {
        return Some((pw, SecretSource::Env));
    }
    #[cfg(feature = "os-keyring")]
    if let Ok(entry) = keyring::Entry::new(SERVICE, username)
        && let Ok(pw) = entry.get_password()
        && !pw.is_empty()
    {
        return Some((pw, SecretSource::Keyring));
    }
    let path = secret_path(username, data_dir);
    if let Ok(pw) = std::fs::read_to_string(&path) {
        let pw = pw.trim().to_string();
        if !pw.is_empty() {
            return Some((pw, SecretSource::File));
        }
    }
    None
}

/// Persist the API password, preferring the OS keyring; falls back to a `0600` file.
/// When the env override is set it wins at read time, so nothing is written.
/// Returns where the secret effectively lives.
pub fn set_password(
    username: &str,
    password: &str,
    data_dir: &std::path::Path,
) -> anyhow::Result<SecretSource> {
    if let Ok(env) = std::env::var(ENV_VAR)
        && !env.trim().is_empty()
    {
        return Ok(SecretSource::Env);
    }
    #[cfg(feature = "os-keyring")]
    if let Ok(entry) = keyring::Entry::new(SERVICE, username)
        && entry.set_password(password).is_ok()
    {
        return Ok(SecretSource::Keyring);
    }
    let path = secret_path(username, data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, password)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(SecretSource::File)
}

/// Remove stored credentials for `username` from keyring (if enabled) and file.
pub fn delete_password(username: &str, data_dir: &std::path::Path) {
    #[cfg(feature = "os-keyring")]
    if let Ok(entry) = keyring::Entry::new(SERVICE, username) {
        let _ = entry.delete_credential();
    }
    let _ = std::fs::remove_file(secret_path(username, data_dir));
}

/// Sanitise `username` into a safe filename component.
fn secret_path(username: &str, data_dir: &std::path::Path) -> PathBuf {
    let safe: String = username
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    data_dir.join(format!("freshrss_secret_{safe}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_path_sanitises_username() {
        let dir = std::path::Path::new("/tmp");
        let p = secret_path("alice@example.com", dir);
        assert_eq!(p, PathBuf::from("/tmp/freshrss_secret_alice_example.com"));
    }

    #[test]
    fn file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("brochure-secret-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let user = "roundtrip-user";
        set_password(user, "s3cr3t", &dir).unwrap();
        // Env override is empty in tests; force file path check via direct read.
        let path = secret_path(user, &dir);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(raw, "s3cr3t");
        delete_password(user, &dir);
        assert!(!path.exists());
        let _ = std::fs::remove_dir(&dir);
    }
}
