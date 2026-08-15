//! Credential storage (Epic 10.4).
//!
//! Passwords live only in the credential store (the OS keychain in release,
//! a dev-only file in debug — see `quill_store::credentials`) — never in the
//! store, the config file, SQLite, or any IPC response. This module is the
//! only place the app touches credentials, and it returns `Result`s, never the
//! secret.
//!
//! The frontend hands the password straight into the `add_account` command,
//! which forwards it here and then discards it — nothing in between stores it
//! in JS state, logs it, or returns it.

const SERVICE: &str = "quill-mail";

/// Store an account password, keyed by account address.
pub fn set_credential(account: &str, password: &str) -> Result<(), String> {
    quill_store::credentials::set(SERVICE, account, password)
}

/// Read an account's password. The result is consumed by the sync engine
/// in-process and never crosses IPC.
pub fn get_credential(account: &str) -> Result<String, String> {
    quill_store::credentials::get(SERVICE, account)
}

/// Remove an account's password. Missing entries are treated as success.
pub fn delete_credential(account: &str) -> Result<(), String> {
    quill_store::credentials::delete(SERVICE, account)
}
