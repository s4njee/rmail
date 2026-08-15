//! CalDAV credential storage (OS keychain in release, dev-only file store in
//! debug builds — see `quill_store::credentials`).
//!
//! Passwords never touch SQLite, memory logs, or IPC responses.

const SERVICE: &str = "quill-caldav";

pub fn get_credential(account_address: &str) -> Result<String, String> {
    quill_store::credentials::get(SERVICE, account_address)
}

pub fn set_credential(account_address: &str, password: &str) -> Result<(), String> {
    quill_store::credentials::set(SERVICE, account_address, password)
}

pub fn delete_credential(account_address: &str) -> Result<(), String> {
    quill_store::credentials::delete(SERVICE, account_address)
}
