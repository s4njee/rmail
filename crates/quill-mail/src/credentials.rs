//! Credential storage (Epic 10.4).
//!
//! Passwords live only in the OS keychain — never in the store, the config
//! file, SQLite, or any IPC response. This module is the only place the app
//! touches the keychain, and it returns `Result`s, never the secret.
//!
//! The frontend hands the password straight into the `add_account` command,
//! which forwards it here and then discards it — nothing in between stores it
//! in JS state, logs it, or returns it.

use keyring::Entry;

const SERVICE: &str = "quill-mail";

/// Store an account password in the OS keychain, keyed by account address.
pub fn set_credential(account: &str, password: &str) -> Result<(), String> {
    Entry::new(SERVICE, account)
        .map_err(|e| e.to_string())?
        .set_password(password)
        .map_err(|e| e.to_string())
}

/// Read an account's password from the keychain. The result is consumed by
/// the sync engine in-process and never crosses IPC.
pub fn get_credential(account: &str) -> Result<String, String> {
    Entry::new(SERVICE, account)
        .map_err(|e| e.to_string())?
        .get_password()
        .map_err(|e| e.to_string())
}

/// Remove an account's password from the keychain.
pub fn delete_credential(account: &str) -> Result<(), String> {
    Entry::new(SERVICE, account)
        .map_err(|e| e.to_string())?
        .delete_credential()
        .map_err(|e| e.to_string())
}
