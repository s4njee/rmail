//! Credential storage: OS keychain in release, a dev-only file store otherwise.
//!
//! Release builds store secrets in the platform keychain via [`keyring`]
//! (macOS Keychain, Windows Credential Manager, Linux secret service). Debug
//! builds — i.e. `tauri dev` — use a plaintext JSON file under the user's
//! config directory instead.
//!
//! Why the dev file store: macOS Keychain Services treats each freshly-built
//! binary as a new app identity, so an unsigned dev binary re-prompts for
//! keychain access on every rebuild ("App wants to use your confidential
//! information…"). The file store keeps `tauri dev` prompt-free; it is only
//! compiled into debug builds.
//!
//! The dev file lives at `$QUILL_CREDENTIALS_FILE` if set, else
//! `{config_dir}/quill/dev-credentials.json`, written with `0600` permissions.
//! Set `QUILL_USE_KEYCHAIN=1` to force the real keychain even in debug builds.
//!
//! Existing keychain credentials are migrated into the dev file on first
//! read, so accounts configured before this change keep working in dev after
//! at most a single keychain prompt instead of being dropped.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// True when secrets should go to the dev file instead of the OS keychain.
fn use_file_store() -> bool {
    // Explicit overrides always win, so release builds can opt into the file
    // (e.g. CI) and debug builds can opt back into the keychain.
    if std::env::var_os("QUILL_USE_KEYCHAIN").is_some() {
        return false;
    }
    if std::env::var_os("QUILL_CREDENTIALS_FILE").is_some() {
        return true;
    }
    cfg!(debug_assertions)
}

/// Store a secret for `service`/`account`, replacing any existing value.
pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
    if use_file_store() {
        file_store::set(service, account, secret)
    } else {
        keyring_entry(service, account)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }
}

/// Read a stored secret for `service`/`account`. Errors if no entry exists.
pub fn get(service: &str, account: &str) -> Result<String, String> {
    if use_file_store() {
        file_store::get(service, account)
    } else {
        keyring_entry(service, account)?
            .get_password()
            .map_err(|e| e.to_string())
    }
}

/// Remove a stored secret. A missing entry is treated as success, matching
/// what the keyring backend's callers already did for `NoEntry`.
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    if use_file_store() {
        file_store::delete(service, account)
    } else {
        let entry = keyring_entry(service, account)?;
        match entry.delete_credential() {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn keyring_entry(service: &str, account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(service, account).map_err(|e| e.to_string())
}

/// Dev-only plaintext file store (see the module docs for why).
#[derive(Default, Serialize, Deserialize)]
struct CredentialFile {
    #[serde(default)]
    services: HashMap<String, HashMap<String, String>>,
}

mod file_store {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    /// Serializes read-modify-write cycles from the per-account sync loops.
    static LOCK: Mutex<()> = Mutex::new(());

    fn path() -> Result<PathBuf, String> {
        if let Some(p) = std::env::var_os("QUILL_CREDENTIALS_FILE") {
            return Ok(PathBuf::from(p));
        }
        let dir = dirs::config_dir()
            .ok_or_else(|| "could not resolve config directory".to_string())?
            .join("quill");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir.join("dev-credentials.json"))
    }

    fn load(path: &PathBuf) -> CredentialFile {
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => CredentialFile::default(),
        }
    }

    fn write(path: &PathBuf, file: &CredentialFile) -> Result<(), String> {
        let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true).mode(0o600);
            let mut f = opts.open(path).map_err(|e| e.to_string())?;
            f.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(path, json).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        let _guard = LOCK.lock().map_err(|e| e.to_string())?;
        let path = path()?;
        let mut file = load(&path);
        file.services
            .entry(service.to_string())
            .or_default()
            .insert(account.to_string(), secret.to_string());
        write(&path, &file)
    }

    pub fn get(service: &str, account: &str) -> Result<String, String> {
        let _guard = LOCK.lock().map_err(|e| e.to_string())?;
        let path = path()?;
        let file = load(&path);
        if let Some(secret) = file
            .services
            .get(service)
            .and_then(|m| m.get(account))
        {
            return Ok(secret.clone());
        }

        // Not in the dev file yet: migrate from the OS keychain once, so
        // accounts configured before this change keep working without
        // re-entering passwords. A missing keychain entry errors without
        // prompting; a present one costs a single prompt on this first read.
        let mut file = file;
        match super::keyring_entry(service, account)
            .and_then(|e| e.get_password().map_err(|e| e.to_string()))
        {
            Ok(secret) => {
                file.services
                    .entry(service.to_string())
                    .or_default()
                    .insert(account.to_string(), secret.clone());
                write(&path, &file)?;
                Ok(secret)
            }
            Err(_) => Err("no credential entry".to_string()),
        }
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        let _guard = LOCK.lock().map_err(|e| e.to_string())?;
        let path = path()?;
        let mut file = load(&path);
        if let Some(services) = file.services.get_mut(service) {
            services.remove(account);
        }
        write(&path, &file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Point the file store at a scratch file (forcing it even in a release
    /// test run) and exercise the whole set/get/delete contract.
    #[test]
    fn file_store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("quill-cred-test-{}", std::process::id()));
        let file = dir.join("creds.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("QUILL_CREDENTIALS_FILE", &file);

        // Missing entry reads as an error; deleting a missing entry is fine.
        assert!(get("svc", "acc").is_err());
        assert!(delete("svc", "acc").is_ok());

        // Round trip and overwrite.
        set("svc", "acc", "secret123").unwrap();
        assert_eq!(get("svc", "acc").unwrap(), "secret123");
        set("svc", "acc", "newsecret").unwrap();
        assert_eq!(get("svc", "acc").unwrap(), "newsecret");

        // Entries are isolated by both service and account.
        set("svc2", "acc", "other").unwrap();
        assert_eq!(get("svc2", "acc").unwrap(), "other");
        assert_eq!(get("svc", "acc").unwrap(), "newsecret");
        assert!(get("svc", "missing-account").is_err());

        // Delete is idempotent.
        delete("svc", "acc").unwrap();
        assert!(get("svc", "acc").is_err());
        delete("svc", "acc").unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }
}
