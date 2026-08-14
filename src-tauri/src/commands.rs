//! The IPC command surface (Epic 3.1).
//!
//! Thin wrappers over the store; argument and return types come from
//! `quill-store::types` (the generated TS bindings in `src/lib/ipc/` are made
//! from the same types, so the two sides of the boundary can't drift).
//!
//! **No command ever returns credential material** (AC 3.1) — credentials
//! never enter this crate at all (Epic 10.4), and the test in `tests/`
//! scans every command's return type for them.

use quill_store::sqlite::SqliteStore;
use quill_store::types::*;
use tauri::State;

#[tauri::command]
pub fn list_folders(store: State<'_, SqliteStore>) -> Vec<Folder> {
    store.folders()
}

#[tauri::command]
pub fn list_accounts(store: State<'_, SqliteStore>) -> Vec<Account> {
    store.accounts()
}

/// Total on-disk cache bytes — the footprint readout (Epic 4.3 / 11.2,
/// on-disk only per D2). Computed by the store, never on the UI thread.
#[tauri::command]
pub fn footprint(store: State<'_, SqliteStore>) -> u64 {
    store.total_disk_bytes()
}

#[tauri::command]
pub fn page_messages(store: State<'_, SqliteStore>, query: MessageQuery) -> MessagePage {
    store.page_messages(&query)
}

/// The full message for the reading pane. Any HTML body is sanitized here,
/// server-side of the IPC boundary (Epic 7.3) — the webview never sees raw
/// mail HTML.
#[tauri::command]
pub fn get_message(store: State<'_, SqliteStore>, id: MessageId) -> Option<MessageDetail> {
    let mut detail = store.get_message(id)?;
    if let Some(raw) = detail.body_html.take() {
        let sanitized = quill_store::sanitize::sanitize_html(&raw);
        detail.body_html = Some(sanitized.html);
        detail.remote_image_count = sanitized.remote_images as u32;
    }
    Some(detail)
}

/// Local path for an attachment; the frontend serves it over the asset
/// protocol (Epic 3.3) via `convertFileSrc`, never through IPC bytes.
#[tauri::command]
pub fn attachment_path(store: State<'_, SqliteStore>, id: AttachmentId) -> Option<String> {
    store
        .attachment_path(id)
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn mark_read(store: State<'_, SqliteStore>, id: MessageId, unread: bool) -> Result<(), String> {
    store.set_read(id, unread)
}

#[tauri::command]
pub fn star(store: State<'_, SqliteStore>, id: MessageId, flagged: bool) -> Result<(), String> {
    store.set_flagged(id, flagged)
}

#[tauri::command]
pub fn archive(store: State<'_, SqliteStore>, id: MessageId) -> Result<(), String> {
    store.archive(id)
}

#[tauri::command]
pub fn delete(store: State<'_, SqliteStore>, id: MessageId) -> Result<(), String> {
    store.delete(id)
}

/// Save (or update) a draft in the Drafts folder (Epic 13.2). Returns the
/// draft's message id for in-place re-saves.
#[tauri::command]
pub fn save_draft(store: State<'_, SqliteStore>, draft: Draft) -> Result<MessageId, String> {
    store.save_draft(&draft)
}

/// Outgoing mail via SMTP (Epic 12.3), with the account's keychain password.
#[tauri::command]
pub fn send(store: State<'_, SqliteStore>, outgoing: OutgoingMessage) -> Result<(), String> {
    let account = store
        .accounts()
        .into_iter()
        .find(|a| a.id == outgoing.account_id)
        .ok_or("no such account")?;
    let password = quill_mail::credentials::get_credential(&account.address)?;
    quill_mail::smtp::send_email(
        &account,
        &outgoing.to,
        &outgoing.subject,
        &outgoing.body,
        &password,
    )
}

#[tauri::command]
pub fn list_events(
    store: State<'_, SqliteStore>,
    start_ms: i64,
    end_ms: i64,
) -> Vec<CalendarEvent> {
    store.list_events(start_ms, end_ms)
}

#[tauri::command]
pub fn create_event(store: State<'_, SqliteStore>, event: CalendarEvent) -> CalendarEvent {
    store.create_event(event)
}

#[tauri::command]
pub fn update_event(store: State<'_, SqliteStore>, event: CalendarEvent) -> Result<(), String> {
    store.update_event(event)
}

#[tauri::command]
pub fn delete_event(store: State<'_, SqliteStore>, id: EventId) -> Result<(), String> {
    store.delete_event(id)
}

/// The account add form (Epic 10.4). The password goes straight into the OS
/// keychain and is then discarded — never stored in the store, never logged,
/// never returned by any command. If the keychain write fails, no account is
/// created.
#[tauri::command]
pub fn add_account(
    store: State<'_, SqliteStore>,
    info: NewAccount,
    password: String,
) -> Result<Account, String> {
    quill_mail::credentials::set_credential(&info.address, &password)?;
    let palette = ["#3b5bdb", "#0f766e", "#b4451f"];
    let color = palette[store.accounts().len() % palette.len()].to_string();
    Ok(store.create_account(&info, color))
}

/// Remove an account: its local mail and calendar data from the store, plus
/// the keychain credential. The frontend confirms first, naming exactly what
/// is deleted (10.4).
#[tauri::command]
pub fn remove_account(store: State<'_, SqliteStore>, id: AccountId) -> Result<(), String> {
    let address = store.remove_account(id)?;
    let _ = quill_mail::credentials::delete_credential(&address);
    Ok(())
}

/// "Test connection" for the add form: reach the IMAP server's port. A real
/// banner/credentials handshake lands with the sync engine in Epic 12.
#[tauri::command]
pub fn test_connection(server: String, port: u16) -> Result<(), String> {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    let addr = format!("{server}:{port}");
    let addrs: Vec<_> = addr
        .to_socket_addrs()
        .map_err(|e| format!("couldn't resolve {addr}: {e}"))?
        .collect();
    let Some(socket) = addrs.first() else {
        return Err(format!("couldn't resolve {addr}"));
    };
    let _stream = TcpStream::connect_timeout(socket, Duration::from_secs(5))
        .map_err(|e| format!("couldn't reach {addr}: {e}"))?;
    Ok(())
}
